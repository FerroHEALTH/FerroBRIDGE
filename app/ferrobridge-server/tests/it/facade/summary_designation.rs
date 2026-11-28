// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The designation the summary's terminology seam asks for: a
//! `CodeSystem/$lookup` that sends the requested language as its
//! `displayLanguage` and reads the designation in that language
//! (<https://hl7.org/fhir/R4/codesystem-operation-lookup.html>).

use std::error::Error;
use std::time::Duration;

use eehrxf::terminology::TerminologyServer;
use ferrobridge_server::facade::summary::terminology::Server;
use ferrobridge_term::client::Client;
use ferrobridge_term::config::{Config, RetryPolicy, WireVersion};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A synthetic code system; `example.org` is reserved for examples
/// (RFC 6761 §6.5).
const SYSTEM: &str = "http://example.org/fhir/CodeSystem/synthetic";

/// Returns the seam over the R4 terminology server `server` mocks.
fn seam(server: &MockServer) -> Result<Server, Box<dyn Error>> {
    let config = Config::new(format!("{}/r4", server.uri()).parse()?, WireVersion::R4)
        .with_timeout(Duration::from_secs(5))
        .with_retry(RetryPolicy {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        });
    Ok(Server::new(Client::new(config)?))
}

/// Mounts a `$lookup` answer with an English display and a Dutch
/// designation.
async fn mount(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/r4/CodeSystem/$lookup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Parameters",
            "parameter": [
                {"name": "name", "valueString": "Synthetic"},
                {"name": "display", "valueString": "Synthetic finding"},
                {"name": "designation", "part": [
                    {"name": "language", "valueCode": "nl"},
                    {"name": "value", "valueString": "Synthetische bevinding"}
                ]}
            ]
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_designation_is_asked_in_its_language_and_read_in_it() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount(&server).await;
    let found = seam(&server)?.designation(SYSTEM, "S-1", "nl").await?;
    assert_eq!(found.as_deref(), Some("Synthetische bevinding"));
    let requests = server
        .received_requests()
        .await
        .ok_or("the recorded requests")?;
    let body: serde_json::Value =
        serde_json::from_slice(&requests.first().ok_or("one request")?.body)?;
    let language = body["parameter"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|parameter| parameter["name"] == "displayLanguage")
        .ok_or("a displayLanguage parameter")?;
    assert_eq!(language["valueCode"], "nl");
    Ok(())
}

#[tokio::test]
async fn a_language_the_server_holds_no_designation_in_answers_none() -> Result<(), Box<dyn Error>>
{
    let server = MockServer::start().await;
    mount(&server).await;
    let found = seam(&server)?.designation(SYSTEM, "S-1", "de").await?;
    assert_eq!(found, None);
    Ok(())
}
