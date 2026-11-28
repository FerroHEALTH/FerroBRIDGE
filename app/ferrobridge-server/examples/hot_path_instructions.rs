// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Runs one hot path's in-process work once, for an instruction count under
//! callgrind (`scripts/perf/instructions.sh`, #2249).
//!
//! The paths are the facade's mapping of the synthetic Condition to its
//! composition (`fhir_to_openehr`, a write), of that composition back to
//! FHIR (`openehr_to_fhir`, a read, a search row and an outbound delivery),
//! and the HL7 v2 face's parse and acknowledgment of the perf gate's ORU^R01
//! (`hl7v2_ack`). Only the `measured` call is counted.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use ferrobridge_hl7v2::ack::{Code, Stamp};
use ferrobridge_hl7v2::decode::Charset;
use ferrobridge_hl7v2::inbound::{self, Received};
use ferrobridge_hl7v2::parse::structure::structure_for;
use ferrobridge_server::facade::{Custodian, Settings, engine, programs};
use ferrobridge_server::identity::ehr::Policy;
use ferrobridge_server::identity::subject::SubjectLink;
use ferrobridge_testkit::fixtures;
use fhirconnect::engine::demographics::IssuerDemographics;
use fhirconnect::engine::seam::identity::DerivedIdentity;
use fhirconnect::engine::terminology::NoTerminology;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

/// The facade suite's mapping set.
const MAPPINGS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/repeated");

/// The context the synthetic Condition maps through.
const CONTEXT: &str = "ferrobridge_facade.context";

/// The profile that context claims.
const PROFILE: &str = "http://example.org/fhir/StructureDefinition/ferrobridge-facade-diagnosis";

/// The base the facade issues identities under.
const BASE: &str = "http://ferrobridge.invalid/fhir";

/// The perf gate's ORU^R01.
const ORU: &str = include_str!("../../../perf/messages/oru-r01.hl7");

/// Runs `path` and keeps its result opaque to the optimizer.
#[inline(never)]
fn measured<R>(path: impl FnOnce() -> R) -> R {
    std::hint::black_box(path())
}

/// Runs `path` through [`measured`] on a thread of its own, which allocates
/// from a fresh glibc arena, so the count does not depend on the heap that
/// loading the mappings left behind (#2305).
///
/// The thread allocates once before the measured call, so the arena and the
/// thread's cache are set up outside the count. A panic on the thread carries
/// on in the caller.
fn on_fresh_heap<R: Send>(path: impl FnOnce() -> R + Send) -> R {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                drop(std::hint::black_box(Box::new(0_u8)));
                measured(path)
            })
            .join()
            .unwrap_or_else(|payload| std::panic::resume_unwind(payload))
    })
}

/// Returns the facade settings the facade suite runs with, over an identity
/// source no path reaches.
fn settings() -> Result<Settings, Box<dyn Error>> {
    let section = ferrobridge_server::config::section::identity_source::IdentitySource {
        cache: None,
        endpoint: ferrohealth_smart::config::endpoint::EndpointSection {
            url: ferrohealth_secret::secret_url::SecretUrl::new("http://127.0.0.1:9/fhir"),
            ..ferrohealth_smart::config::endpoint::EndpointSection::default()
        },
        pseudonym_domain: String::from("http://example.org/fhir"),
        source_domains: vec![String::from("http://example.org/fhir")],
    };
    let source = ferrobridge_server::config::identity_source::resolve(&section)?;
    Ok(Settings {
        base_url: String::from(BASE),
        ehr_policy: Policy::Existing,
        subject: SubjectLink::new(Arc::new(source)),
        system_id: String::from("ferrobridge.test"),
        language: String::from("en"),
        territory: String::from("GB"),
        ehr_status_system: None,
        custodian: Custodian {
            name: String::from("Synthetic Hospital"),
            system: String::from("urn:oid:2.999.1.1"),
            value: String::from("synthetic-hospital"),
        },
    })
}

/// Returns the program of [`CONTEXT`] with its template index.
fn loaded() -> Result<programs::Loaded, Box<dyn Error>> {
    let mut indexes = BTreeMap::new();
    for opt in [
        fixtures::DIAGNOSE_OPT,
        fixtures::DIAGNOSES_OPT,
        fixtures::DIAGNOSE_SECTIONED_OPT,
    ] {
        let index = WebTemplateIndex::build(&TemplateSource::opt14(opt)?)?;
        indexes.insert(index.template_id().to_owned(), Arc::new(index));
    }
    let set = programs::read_set(Path::new(MAPPINGS))?;
    let compiled = programs::compile_set(&set, &indexes)?;
    Ok(compiled
        .by_context(CONTEXT)
        .cloned()
        .ok_or("the facade context compiles")?)
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: hot_path_instructions <fhir_to_openehr|openehr_to_fhir|hl7v2_ack>")?;
    if path == "hl7v2_ack" {
        let message = ORU.replace('\n', "\r");
        let stamp = Stamp {
            control_id: "ACK00001",
            timestamp: "20260925090001+0200",
        };
        let reply = on_fresh_heap(|| {
            match inbound::receive(message.as_bytes(), Charset::Ascii, structure_for, stamp) {
                Received::Parsed(parsed) => parsed.answer(Code::Accept, &[], stamp).ok(),
                _ => None,
            }
        });
        reply.ok_or("the ORU^R01 parses and is acknowledged")?;
        return Ok(());
    }
    let settings = settings()?;
    let loaded = loaded()?;
    let demographics = IssuerDemographics::new(BASE)?;
    let mut resource: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(fixtures::R4_CONDITION)?;
    resource.insert(
        String::from("meta"),
        serde_json::json!({ "profile": [PROFILE] }),
    );
    let document: fhir_types::codec::Value =
        serde_json::from_value(serde_json::Value::Object(resource))?;
    let write = || {
        engine::inbound(
            loaded.program(),
            loaded.index(),
            &document,
            "2026-09-15T09:00:00Z",
            &settings,
            &NoTerminology,
            &demographics,
        )
    };
    match path.as_str() {
        "fhir_to_openehr" => {
            on_fresh_heap(write)?;
        }
        "openehr_to_fhir" => {
            let composition = write()?.into_value();
            on_fresh_heap(|| {
                engine::outbound(
                    loaded.program(),
                    loaded.index(),
                    &composition,
                    &DerivedIdentity,
                    &NoTerminology,
                    &demographics,
                )
            })?;
        }
        other => return Err(format!("unknown path `{other}`").into()),
    }
    Ok(())
}
