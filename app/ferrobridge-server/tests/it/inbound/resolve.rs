// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A webhook source's declared resolution: the calls below the tenant's
//! base read the entity, the declared map renders the answers into an R4
//! `Patient`, a `404` on the first call reads as removed, and every other
//! failure parks or runs again.

use std::error::Error;
use std::sync::Arc;

use ferrobridge_operator_api::sources::{Kind, Write};
use ferrobridge_server::inbound::apply::{Resolve, Resolved};
use ferrobridge_server::inbound::resolve::Declared;
use ferrobridge_server::retry::Failure;
use ferrobridge_server::sources::{Source, Sources};
use ferrobridge_server::structuremap::Set;
use ferrobridge_server::structuremap::openapi::logical_model;
use ferrobridge_server::structuremap::parse::parse;
use fhir_types::codec::Value;
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

/// The directory of the map and `OpenAPI` fixtures.
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/it/structuremap/fixtures"
);

/// The map the source declares.
const MAP: &str = "http://ferrobridge.example/StructureMap/client-patient";

/// The webhook source document whose tenant `t1` lies below `base`.
fn document(base: &str, answer_limit: usize) -> serde_json::Value {
    json!({
        "verify": {"kind": "mtls", "subjects": ["CN=ons"]},
        "events": {"entity": "/id", "tenant": "/tenant", "dedupe": {"pointers": ["/id"]}},
        "tenants": [{"tenant": "t1", "identifier_system": "urn:example:tenant:t1", "base": base}],
        "resolve": {
            "calls": [
                {"path": "/v0/clients/{entity}", "headers": {"X-Field-Whitelist": "id,firstName,lastName,dateOfBirth,gender,deathDate"}},
                {"path": "/v0/clients/{entity}/bsn"}
            ],
            "removed_on": 404,
            "map": MAP,
            "answer_limit": answer_limit
        }
    })
}

/// Returns the source of [`document`].
fn source(base: &str, answer_limit: usize) -> Result<Source, Box<dyn Error>> {
    Ok(Source::fixed(&Write {
        name: String::from("ons"),
        kind: Kind::Webhook,
        enabled: true,
        document: document(base, answer_limit),
    })?)
}

/// Returns sources over a store never reached, without a running map.
fn sources() -> Result<Arc<Sources>, Box<dyn Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://nobody:nothing@127.0.0.1:1/none?sslmode=disable")?;
    Ok(Arc::new(Sources::new(pool, None)))
}

/// Returns the map of [`MAP`] with the Client and Bsn logical models.
fn set() -> Result<Set, Box<dyn Error>> {
    let mut set = Set::default();
    set.maps.push(parse(&std::fs::read_to_string(format!(
        "{FIXTURES}/ons-patient.map"
    ))?)?);
    let openapi: Value = serde_json::from_str(&std::fs::read_to_string(format!(
        "{FIXTURES}/ons-openapi.json"
    ))?)?;
    for name in ["Client", "Bsn"] {
        let url = format!("http://ferrobridge.example/StructureDefinition/{name}");
        set.logical.add(&logical_model(&openapi, name, &url)?)?;
    }
    Ok(set)
}

/// Returns sources running [`set`].
fn running() -> Result<Arc<Sources>, Box<dyn Error>> {
    let sources = sources()?;
    sources.structuremaps().replace(set()?);
    Ok(sources)
}

/// Mounts the client `4711` and its BSN on `server` below `/t1`.
async fn mount_client(server: &MockServer) {
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/t1/v0/clients/4711"))
        .and(matchers::header(
            "X-Field-Whitelist",
            "id,firstName,lastName,dateOfBirth,gender,deathDate",
        ))
        .and(matchers::header("Accept", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 4711, "firstName": "Testa", "lastName": "Voorbeeld",
            "dateOfBirth": "1950-02-03", "gender": "F"
        })))
        .mount(server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/t1/v0/clients/4711/bsn"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({"clientId": 4711, "number": "999990019", "idVerified": true}),
            ),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn the_calls_below_the_tenant_base_render_a_patient() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_client(&server).await;
    let source = source(&format!("{}/t1", server.uri()), 65_536)?;
    let resolved = Declared::new(running()?)
        .patient(&source, &source.parsed.tenants()[0], "4711")
        .await
        .map_err(|failed| failed.reason)?;
    let Resolved::Patient(patient) = resolved else {
        return Err("the client resolves to a Patient".into());
    };
    assert_eq!(
        json!({
            "resourceType": "Patient",
            "identifier": [
                {"system": "urn:example:tenant:t1", "value": "4711"},
                {"system": "http://fhir.nl/fhir/NamingSystem/bsn", "value": "999990019"}
            ],
            "name": [{"family": "Voorbeeld", "given": ["Testa"]}],
            "birthDate": "1950-02-03",
            "gender": "female"
        }),
        serde_json::to_value(&patient)?
    );
    Ok(())
}

#[tokio::test]
async fn a_404_on_the_first_call_reads_as_removed() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/t1/v0/clients/4711"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let source = source(&format!("{}/t1", server.uri()), 65_536)?;
    let resolved = Declared::new(running()?)
        .patient(&source, &source.parsed.tenants()[0], "4711")
        .await
        .map_err(|failed| failed.reason)?;
    assert_eq!(Resolved::Removed, resolved);
    assert_eq!(
        1,
        server.received_requests().await.unwrap_or_default().len(),
        "the BSN call is not made for a removed client"
    );
    Ok(())
}

#[tokio::test]
async fn a_refusal_parks_naming_its_status_and_an_outage_runs_again() -> Result<(), Box<dyn Error>>
{
    for (status, failure, reason) in [
        (403, Failure::Poisoned, "source-answered-403"),
        (410, Failure::Poisoned, "source-answered-410"),
        (503, Failure::Transient, "source-unavailable"),
        (429, Failure::Transient, "source-unavailable"),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let source = source(&format!("{}/t1", server.uri()), 65_536)?;
        let failed = Declared::new(running()?)
            .patient(&source, &source.parsed.tenants()[0], "4711")
            .await
            .expect_err("a failure status fails the resolution");
        assert_eq!((failure, reason), (failed.failure, failed.reason.as_ref()));
    }
    let source = source("http://127.0.0.1:1/t1", 65_536)?;
    let failed = Declared::new(running()?)
        .patient(&source, &source.parsed.tenants()[0], "4711")
        .await
        .expect_err("no answer fails the resolution");
    assert_eq!(
        (Failure::Transient, "source-unavailable"),
        (failed.failure, failed.reason.as_ref())
    );
    Ok(())
}

#[tokio::test]
async fn an_answer_past_the_limit_parks() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_client(&server).await;
    let source = source(&format!("{}/t1", server.uri()), 16)?;
    let failed = Declared::new(running()?)
        .patient(&source, &source.parsed.tenants()[0], "4711")
        .await
        .expect_err("the answer is larger than 16 bytes");
    assert_eq!(
        (Failure::Poisoned, "answer-too-large"),
        (failed.failure, failed.reason.as_ref())
    );
    Ok(())
}

#[tokio::test]
async fn a_map_the_revision_does_not_run_parks() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    mount_client(&server).await;
    let source = source(&format!("{}/t1", server.uri()), 65_536)?;
    let failed = Declared::new(sources()?)
        .patient(&source, &source.parsed.tenants()[0], "4711")
        .await
        .expect_err("no map runs");
    assert_eq!(
        (Failure::Poisoned, "map-unknown"),
        (failed.failure, failed.reason.as_ref())
    );
    Ok(())
}

#[test]
fn a_write_naming_a_map_the_revision_does_not_run_is_refused() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let _entered = runtime.enter();
    let sources = sources()?;
    let document = document("https://api.example/t1", 65_536);
    let refused = sources
        .validate(Kind::Webhook, &document)
        .expect_err("no map runs yet");
    assert!(refused.0.contains(MAP), "{refused}");
    sources.structuremaps().replace(set()?);
    sources.validate(Kind::Webhook, &document)?;
    Ok(())
}
