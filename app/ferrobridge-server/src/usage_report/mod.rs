// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What FerroBRIDGE puts into the installation report of
//! `ferrohealth-usage-report`.
//!
//! The crate carries `FerroPULSE`'s envelope v2, the reporter, the instance id
//! under `[usage_report] state_dir` and the window the request log fills.
//! FerroBRIDGE adds its `status` (the enabled lanes, the mapping counts as
//! ranges and the upstream health, [`status::Status`]), its request groups by
//! matched route template ([`grouping`]), and its counters: the HL7 v2
//! acknowledgment codes and the mapping failures. A report carries no
//! clinical content, no identifier other than the instance id, no mapping
//! name, no URL and no request path. No specification governs any of this:
//! our own design.

pub mod status;

use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrobridge_hl7v2::ack::Code;
use ferrohealth_usage_report::envelope::{Key, LicenceGrant, Product, report_git_sha};
use ferrohealth_usage_report::reporter::{Identity, Notice, ProductSection};
use ferrohealth_usage_report::section::UsageReportSection;
use ferrohealth_usage_report::window::Window;
use http::Method;
use tokio::task::JoinHandle;

use crate::state::AppState;
use crate::usage_report::status::{LaneToken, MappingBucket, Status};

/// What the boot line says about the report.
pub const NOTICE: Notice = Notice {
    contents: "instance id, version, licence type, deployment, uptime, host size, enabled lanes, \
               mapping counts as ranges, upstream health and coarse request latency per route \
               group",
    switch: "FERROBRIDGE__USAGE_REPORT__ENABLED=false",
    state_dir_variable: "FERROBRIDGE__USAGE_REPORT__STATE_DIR",
    url: "https://ferrobridge.eu/docs/operate/usage-report.html",
};

/// The group of an HL7 v2 message the face answered or dropped, timed from
/// its frame to its acknowledgment.
pub const HL7V2_MESSAGE: &str = "hl7v2_message";

/// The counter of the units a FHIRconnect program refused to map: a facade
/// or HL7 v2 ingest and an operation call.
pub const MAPPING_FAILURES: &str = "mapping_failures";

/// Returns the identity of this build.
///
/// The server installs no commercial licence, so it runs under what the
/// Business Source License grants everyone, the non-commercial grant.
#[must_use]
pub fn identity() -> Identity {
    Identity {
        version: crate::state::VERSION.to_owned(),
        git_sha: report_git_sha(crate::build_info::COMMIT),
        licence: LicenceGrant::NonCommercial,
    }
}

/// Returns an empty window that groups requests with [`grouping`].
#[must_use]
pub fn window() -> Window {
    Window::new(grouping)
}

/// Logs the boot line and, when the report runs, starts its background task
/// over `state`.
///
/// `ci` is the CI variable the process environment sets, when one is
/// (`ferrohealth_usage_report::section::ci_of`). `None` when the report is
/// off; the report never fails boot.
pub async fn start(
    section: &UsageReportSection,
    state: Arc<AppState>,
    started: Instant,
    ci: Option<&'static str>,
) -> Option<JoinHandle<()>> {
    let window = Arc::clone(state.usage_window());
    ferrohealth_usage_report::reporter::start(
        section,
        &NOTICE,
        identity(),
        state,
        window,
        started,
        ci,
    )
    .await
}

impl ProductSection for AppState {
    type Status = Status;
    const PRODUCT: Product = Product::Bridge;

    fn status(&self) -> impl Future<Output = Status> + Send {
        status_of(self)
    }
}

/// Returns the status of this process as of now: the lanes from the boot
/// summary and each upstream's readiness check.
async fn status_of(state: &AppState) -> Status {
    let mut lanes: Vec<LaneToken> = state
        .lanes()
        .iter()
        .filter(|lane| lane.enabled)
        .filter_map(|lane| LaneToken::of(lane.name))
        .collect();
    lanes.sort_unstable();
    lanes.dedup();
    let mappings = |name: &str| {
        state
            .lanes()
            .iter()
            .find(|lane| lane.name == name && lane.enabled)
            .and_then(|lane| lane.mappings)
            .map(|counts| MappingBucket::of(counts.programs))
    };
    let report = state.health().readiness(state.lifecycle()).await;
    let up = |name: &str| {
        report
            .indicators
            .iter()
            .find(|indicator| indicator.name == name)
            .map(|indicator| indicator.state == ferrohealth_health::document::State::Up)
    };
    Status {
        lanes,
        auth: state.auth().is_some(),
        facade_mappings: mappings("facade"),
        operations_mappings: mappings("operations"),
        cdr_ok: up("cdr"),
        terminology_ok: up("terminology"),
        cdm_ok: up("cdm"),
        identity_store_ok: up("identity-store"),
        hl7v2_listener_ok: up("hl7v2-listener"),
    }
}

/// Returns the group of a request from its method and the route template it
/// matched, or `None` for a request the report does not count.
///
/// `template` is `None` for a request no route matched, which is `other`.
/// The health routes and the root document are probes and are not counted.
/// A template's parameter segments are matched by position and never read.
#[must_use]
pub fn grouping(method: &Method, template: Option<&str>) -> Option<Key> {
    let Some(template) = template else {
        return Key::new("other");
    };
    if template == "/" || template == "/health" || template.starts_with("/health/") {
        return None;
    }
    let segments: Vec<&str> = template.split('/').filter(|s| !s.is_empty()).collect();
    let group = match segments.as_slice() {
        ["fhir", "$tofhir" | "tofhir"] => "operations_to_fhir",
        ["fhir", "$toopenehr" | "toopenehr"] => "operations_to_openehr",
        ["fhir", "metadata"] => "facade_metadata",
        ["fhir"] if *method == Method::POST => "facade_transaction",
        ["fhir", "{resource_type}"] => match *method {
            Method::GET => "facade_search",
            Method::POST => "facade_write",
            _ => "other",
        },
        ["fhir", "{resource_type}", "_search"] => "facade_search",
        ["fhir", "{resource_type}", "$validate"] => "facade_validate",
        ["fhir", "{resource_type}", "{id}"] => match *method {
            Method::GET => "facade_read",
            Method::PUT => "facade_write",
            _ => "other",
        },
        ["fhir", "{resource_type}", "{id}", "_history", "{vid}"] => "facade_read",
        _ => "other",
    };
    Key::new(group)
}

/// Records one HL7 v2 message the face handled in `elapsed`, and the
/// acknowledgment code it answered with, `None` when it sent none.
pub fn record_message(window: &Window, elapsed: Duration, code: Option<Code>) {
    if let Some(group) = Key::new(HL7V2_MESSAGE) {
        window.record_work(group, elapsed);
    }
    if let Some(code) = code {
        record_acknowledgment(window, code);
    }
}

/// Records one HL7 v2 acknowledgment, a message's or the `AR` of a frame the
/// listener could not read.
pub fn record_acknowledgment(window: &Window, code: Code) {
    let counter = match code {
        Code::Accept => "hl7v2_aa",
        Code::Error => "hl7v2_ae",
        Code::Reject => "hl7v2_ar",
    };
    if let Some(key) = Key::new(counter) {
        window.count(key);
    }
}

/// Records one unit a FHIRconnect program refused to map.
pub fn record_mapping_failure(window: &Window) {
    if let Some(key) = Key::new(MAPPING_FAILURES) {
        window.count(key);
    }
}

#[cfg(test)]
mod tests {
    use super::grouping;
    use http::Method;

    fn group(method: &Method, template: Option<&str>) -> Option<String> {
        grouping(method, template).map(|key| key.as_str().to_owned())
    }

    #[test]
    fn a_probe_is_not_counted_and_an_unmatched_request_is_other() {
        assert_eq!(None, group(&Method::GET, Some("/health/readiness")));
        assert_eq!(None, group(&Method::GET, Some("/health")));
        assert_eq!(None, group(&Method::GET, Some("/")));
        assert_eq!(Some(String::from("other")), group(&Method::GET, None));
    }

    #[test]
    fn each_facade_and_operation_route_has_its_group() {
        for (method, template, expected) in [
            (Method::POST, "/fhir/$tofhir", "operations_to_fhir"),
            (Method::POST, "/fhir/toopenehr", "operations_to_openehr"),
            (Method::GET, "/fhir/metadata", "facade_metadata"),
            (Method::POST, "/fhir", "facade_transaction"),
            (Method::GET, "/fhir/{resource_type}", "facade_search"),
            (Method::POST, "/fhir/{resource_type}", "facade_write"),
            (
                Method::POST,
                "/fhir/{resource_type}/_search",
                "facade_search",
            ),
            (
                Method::POST,
                "/fhir/{resource_type}/$validate",
                "facade_validate",
            ),
            (Method::GET, "/fhir/{resource_type}/{id}", "facade_read"),
            (Method::PUT, "/fhir/{resource_type}/{id}", "facade_write"),
            (
                Method::GET,
                "/fhir/{resource_type}/{id}/_history/{vid}",
                "facade_read",
            ),
            (Method::GET, "/operator/mappings", "other"),
        ] {
            assert_eq!(
                Some(String::from(expected)),
                group(&method, Some(template)),
                "{method} {template}"
            );
        }
    }
}
