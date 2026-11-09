// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A client built from an `[<lane>.endpoint]` table through
//! `ferrohealth-smart`'s outbound client (`Client::from_endpoint`): its
//! credential, its timeout and a base that cannot carry a path.
//!
//! No specification governs the table: our own design.

use crate::support::release_path;
use ferrobridge_term::client::Client;
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::outcome::LookupOutcome;
use ferrobridge_testkit::fixtures;
use ferrobridge_testkit::stubs::terminology;
use ferrohealth_smart::client::PresentedTls;
use ferrohealth_smart::config::TlsBuilder;
use ferrohealth_smart::config::endpoint::EndpointSection;
use std::sync::Arc;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer};

/// Builds the platform's default TLS for a table that names no material.
#[derive(Debug)]
struct NoMaterial;

impl TlsBuilder for NoMaterial {
    fn build(
        &self,
        _key: &str,
        _section: &ferrohealth_config::tls::TlsClientSection,
    ) -> Result<PresentedTls, Box<dyn std::error::Error + Send + Sync>> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();
        Ok(PresentedTls::new(Arc::new(config), None))
    }
}

/// Returns the endpoint table `json` writes.
fn endpoint(json: &serde_json::Value) -> Result<EndpointSection, serde_json::Error> {
    serde_json::from_value(json.clone())
}

/// The `$lookup` route of the R4 server under the stub.
fn route() -> String {
    format!("/{}/CodeSystem/$lookup", release_path(WireVersion::R4))
}

/// Looks the synthetic member up through `client`.
async fn lookup(client: &Client) -> Result<LookupOutcome, ferrobridge_term::error::Error> {
    client
        .lookup(
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_MEMBER_CODE,
            None,
        )
        .await
}

#[tokio::test]
async fn an_endpoint_client_carries_its_basic_credential() -> Result<(), Box<dyn std::error::Error>>
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route()))
        .respond_with(terminology::lookup(
            "FerroBridgeSyntheticSource",
            fixtures::TERM_MEMBER_DISPLAY,
            None,
        ))
        .mount(&server)
        .await;
    let table = endpoint(&serde_json::json!({
        "url": format!("{}/{}", server.uri(), release_path(WireVersion::R4)),
        "credentials": {"user": "bridge", "password": "s3cr3t-password"},
    }))?;
    let client =
        Client::from_endpoint(&table, "terminology.endpoint", &NoMaterial, WireVersion::R4)?;
    lookup(&client).await?;
    assert_eq!(
        vec!["Basic YnJpZGdlOnMzY3IzdC1wYXNzd29yZA==".to_owned()],
        crate::support::request_header(&server, 0, "authorization").await?,
        "the table's credential travels"
    );
    assert!(
        !format!("{client:?}").contains("s3cr3t-password"),
        "the password stays out of the rendering"
    );
    Ok(())
}

#[tokio::test]
async fn an_endpoint_client_is_bounded_by_the_tables_timeout()
-> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route()))
        .respond_with(
            terminology::lookup("FerroBridgeSyntheticSource", "Alpha finding", None)
                .set_delay(Duration::from_secs(2)),
        )
        .mount(&server)
        .await;
    let table = endpoint(&serde_json::json!({
        "url": format!("{}/{}", server.uri(), release_path(WireVersion::R4)),
        "timeout": "50ms",
        "retry": {"max_attempts": 1, "initial_backoff": "1ms", "max_backoff": "1ms"},
    }))?;
    let client =
        Client::from_endpoint(&table, "terminology.endpoint", &NoMaterial, WireVersion::R4)?;
    let error = lookup(&client).await.expect_err("the server was too slow");
    assert_eq!("timeout", error.kind());
    Ok(())
}

#[test]
fn an_endpoint_without_a_url_is_refused_naming_the_table() {
    let error = Client::from_endpoint(
        &EndpointSection::default(),
        "terminology.endpoint",
        &NoMaterial,
        WireVersion::R4,
    )
    .expect_err("a table without a url");
    assert_eq!("endpoint", error.kind());
    assert!(
        error.to_string().contains("terminology.endpoint"),
        "{error}"
    );
}
