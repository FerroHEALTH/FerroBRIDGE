// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access-record lane: the crate boundary of its two audit crates, the
//! HL7 v2 face's import record, and the Audit Record Repository stub every
//! recording case sends to over the ITI-20 ATX: FHIR Feed.

mod boundary;
mod hl7v2;

use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ehds_logging::map::CategoryMap;
use ferrobridge_server::audit::AccessLog;
use ferrobridge_server::audit::Settings;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;
use ferrohealth_smart::config::endpoint::{EndpointSection, RetrySection};
use ihe_atna::atna::feed::FeedRepository;
use ihe_atna::atna::spool::Bounds;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

/// The FHIR base the facade of these cases answers under.
const BASE_URL: &str = "http://ferrobridge.invalid/fhir";

/// Starts an Audit Record Repository that creates every `AuditEvent` it is
/// sent (ITI TF-2 §3.20.4.3.2).
pub(crate) async fn repository() -> MockServer {
    let arr = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/AuditEvent"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&arr)
        .await;
    arr
}

/// Opens an access-record lane that delivers to `arr` from a spool in
/// `directory` of at most `max_bytes`, classifying by `categories`, for a
/// facade over the CDR at `cdr`, and starts its delivery.
pub(crate) fn access_log(
    arr: &MockServer,
    directory: &Path,
    cdr: &url::Url,
    categories: CategoryMap,
    max_bytes: u64,
) -> Result<Arc<AccessLog>, Box<dyn Error>> {
    let client = OutboundClient::build(
        &EndpointSection {
            url: SecretUrl::new(arr.uri()),
            timeout: ferrohealth_config::units::Duration::from_secs(5),
            retry: RetrySection {
                max_attempts: 1,
                initial_backoff: ferrohealth_config::units::Duration::from_millis(100),
                max_backoff: ferrohealth_config::units::Duration::from_secs(1),
            },
            ..EndpointSection::default()
        },
        "audit.endpoint",
        &ServerTls,
    )?;
    let settings = Settings {
        feed: FeedRepository::cleartext(client)?,
        spool: directory.join("spool"),
        bounds: Bounds {
            max_messages: 100,
            max_bytes,
            write_timeout: Duration::from_secs(2),
        },
        source_id: String::from("ferrobridge.test"),
        site: None,
        categories,
    };
    let log = Arc::new(AccessLog::open(&settings, &BASE_URL.parse()?, cdr)?);
    drop(log.spawn());
    Ok(log)
}

/// Returns the `AuditEvent`s `arr` received, waiting until it holds `count`.
pub(crate) async fn records(
    arr: &MockServer,
    count: usize,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    for _ in 0..200 {
        let records: Vec<serde_json::Value> = arr
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path() == "/AuditEvent")
            .filter_map(|request| serde_json::from_slice(&request.body).ok())
            .collect();
        if records.len() >= count {
            return Ok(records);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Err(format!("the repository received fewer than {count} records").into())
}
