// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The guide's `IF CX.4 IN` identifier registry condition on CX.4 to
//! `system`: a universal id names its system, any other assigning authority
//! is searched as an identifier `NamingSystem`.

use ferrobridge_hl7v2::map::{MapError, Mapped, Outcome, map};
use ferrobridge_testkit::stubs::terminology;
use wiremock::{Mock, ResponseTemplate};

use crate::fixtures;
use crate::map::{resources, tables};
use crate::support;

/// The system the registry holds for the assigning authority `NORTHHOSP`.
const NORTHHOSP: &str = "http://example.org/fhir/sid/northhosp-mrn";

/// An admission whose PID-3 is `pid_3`.
fn admission(pid_3: &str) -> Vec<u8> {
    let pid = format!("PID|1||{pid_3}||Doe^Anna^^^^^L||19920304|F");
    fixtures::message(&[
        b"MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTHCLINIC|20260925090000+0200||ADT^A01^ADT_A01|MSG00002|P|2.5.1",
        b"EVN||20260925085900+0200",
        pid.as_bytes(),
        b"PV1|1|I|WARD1^101^A",
    ])
}

/// Maps `message` against a registry holding `registered`, returning the
/// run and the registry searches the server received.
async fn mapped_against(message: &[u8], registered: &[(&str, &str)]) -> (Mapped, usize) {
    let tables = tables();
    let (server, client) = tables.serve().await;
    support::mount_registry(&server, registered).await;
    let mapped = map(&support::parsed(message), support::corpus(), Some(&client))
        .await
        .expect("the message maps");
    let searches = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/r4/NamingSystem")
        .count();
    (mapped, searches)
}

/// The `system` of the Patient's first identifier.
fn system(mapped: Mapped) -> Option<String> {
    let bundle = mapped.into_bundle();
    let patient = resources(&bundle, "Patient").into_iter().next()?;
    patient
        .get("identifier")?
        .as_array()?
        .first()?
        .get("system")?
        .as_str()
        .map(String::from)
}

#[tokio::test]
async fn a_registered_assigning_authority_names_the_identifier_system() {
    let (mapped, searches) = mapped_against(
        &admission("PAT-0002^^^NORTHHOSP^MR"),
        &[("NORTHHOSP", NORTHHOSP)],
    )
    .await;
    assert_eq!(1, searches);
    assert!(
        !mapped
            .outcomes()
            .iter()
            .any(|outcome| matches!(outcome, Outcome::Unregistered { .. })),
        "{:?}",
        mapped.outcomes()
    );
    assert_eq!(Some(String::from(NORTHHOSP)), system(mapped));
}

#[tokio::test]
async fn an_unregistered_assigning_authority_leaves_the_system_unwritten_and_counted() {
    let (mapped, searches) = mapped_against(&admission("PAT-0002^^^NORTHHOSP^MR"), &[]).await;
    assert_eq!(1, searches);
    let unregistered: Vec<&str> = mapped
        .outcomes()
        .iter()
        .filter_map(|outcome| match outcome {
            Outcome::Unregistered { at, authority, .. } => {
                assert_eq!(("PID", Some(3)), (at.segment.as_str(), at.field));
                Some(authority.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(vec!["NORTHHOSP"], unregistered);
    assert_eq!(None, system(mapped));
}

#[tokio::test]
async fn an_iso_universal_id_names_its_oid_with_no_registry_search() {
    let (mapped, searches) = mapped_against(&admission("PAT-0002^^^&2.999.1&ISO^MR"), &[]).await;
    assert_eq!(0, searches);
    assert_eq!(Some(String::from("urn:oid:2.999.1")), system(mapped));
}

#[tokio::test]
async fn two_registrations_of_one_authority_leave_the_system_unwritten() {
    let tables = tables();
    let (server, client) = tables.serve().await;
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/r4/NamingSystem"))
        .respond_with(terminology::naming_systems(&[
            ("NORTHHOSP", NORTHHOSP),
            ("NORTHHOSP", "http://example.org/fhir/sid/other"),
        ]))
        .with_priority(1)
        .mount(&server)
        .await;
    let mapped = map(
        &support::parsed(&admission("PAT-0002^^^NORTHHOSP^MR")),
        support::corpus(),
        Some(&client),
    )
    .await
    .expect("the message maps");
    assert!(
        mapped.outcomes().iter().any(|outcome| matches!(
            outcome,
            Outcome::RegistryAmbiguous { systems, .. } if systems.len() == 2
        )),
        "{:?}",
        mapped.outcomes()
    );
    assert_eq!(None, system(mapped));
}

#[tokio::test]
async fn a_refused_registry_search_refuses_the_message_with_the_upstream_status() {
    let tables = tables();
    let (server, client) = tables.serve().await;
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/r4/NamingSystem"))
        .respond_with(ResponseTemplate::new(503))
        .with_priority(1)
        .mount(&server)
        .await;
    let refused = map(
        &support::parsed(&admission("PAT-0002^^^NORTHHOSP^MR")),
        support::corpus(),
        Some(&client),
    )
    .await;
    let Err(MapError::Registry { authority, source }) = refused else {
        panic!("a registry refusal, got {refused:?}");
    };
    assert_eq!("NORTHHOSP", authority);
    assert_eq!("server-failure", source.kind());
}

#[tokio::test]
async fn with_no_terminology_server_the_registry_row_is_counted() {
    let mapped = map(
        &support::parsed(&admission("PAT-0002^^^NORTHHOSP^MR")),
        support::corpus(),
        None,
    )
    .await
    .expect("the message maps");
    assert!(mapped.outcomes().iter().any(|outcome| matches!(
        outcome,
        Outcome::NoTerminology { row, .. } if row.target == "system"
    )));
}
