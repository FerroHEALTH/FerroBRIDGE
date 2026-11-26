// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identifier `NamingSystem` search by unique id
//! (<https://hl7.org/fhir/R4/namingsystem.html#search>).

use crate::support::{RELEASES, client, release_path};
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::error::Error;
use ferrobridge_testkit::stubs::terminology;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mounts `response` as the answer of the search for `value` in `version`.
async fn mount(server: &MockServer, version: WireVersion, value: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(format!("/{}/NamingSystem", release_path(version))))
        .and(query_param("kind", "identifier"))
        .and(query_param("value", value))
        .respond_with(response)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_registered_unique_id_names_its_uri_in_every_release()
-> Result<(), Box<dyn std::error::Error>> {
    for &version in RELEASES {
        let server = MockServer::start().await;
        let registered =
            terminology::naming_systems(&[("NORTHLAB", "http://example.org/sid/northlab")]);
        mount(&server, version, "NORTHLAB", registered).await;
        let systems = client(&server, version)?
            .identifier_systems("NORTHLAB")
            .await?;
        assert_eq!(
            vec![String::from("http://example.org/sid/northlab")],
            systems
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_prefix_match_of_the_string_search_is_no_registration()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let registered =
        terminology::naming_systems(&[("NORTHLAB2", "http://example.org/sid/northlab2")]);
    mount(&server, WireVersion::R4, "NORTHLAB", registered).await;
    let systems = client(&server, WireVersion::R4)?
        .identifier_systems("NORTHLAB")
        .await?;
    assert_eq!(Vec::<String>::new(), systems);
    Ok(())
}

#[tokio::test]
async fn an_unregistered_unique_id_names_no_system() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    mount(
        &server,
        WireVersion::R4,
        "NORTHLAB",
        terminology::naming_systems(&[]),
    )
    .await;
    let systems = client(&server, WireVersion::R4)?
        .identifier_systems("NORTHLAB")
        .await?;
    assert_eq!(Vec::<String>::new(), systems);
    Ok(())
}

#[tokio::test]
async fn two_registrations_name_both_systems() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let registered = terminology::naming_systems(&[
        ("NORTHLAB", "http://example.org/sid/northlab"),
        ("NORTHLAB", "http://example.org/sid/north-laboratory"),
    ]);
    mount(&server, WireVersion::R4, "NORTHLAB", registered).await;
    let systems = client(&server, WireVersion::R4)?
        .identifier_systems("NORTHLAB")
        .await?;
    assert_eq!(2, systems.len());
    Ok(())
}

#[tokio::test]
async fn a_refused_search_is_a_typed_error() -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let refusal = terminology::outcome(400, "not-supported", None, "the search is not supported");
    mount(&server, WireVersion::R4, "NORTHLAB", refusal).await;
    let error = client(&server, WireVersion::R4)?
        .identifier_systems("NORTHLAB")
        .await
        .expect_err("a 400 is a refusal");
    let Error::Refused { operation, .. } = &error else {
        panic!("a refusal, got {error:?}");
    };
    assert_eq!(&ferrobridge_term::NAMING_SYSTEM_SEARCH, operation);
    Ok(())
}
