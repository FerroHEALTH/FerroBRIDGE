// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A party context over an id-coded ADL 2 template, over a `wiremock` CDR:
//! the reference CDR's professional `PERSON`, which it serves only on the
//! `adl2` template route (ITS-REST 1.1.0 Definition API), mapped onto
//! `Practitioner` with paths that name its id-codes.
//!
//! ADL 2.4 admits the id-code coding system beside the at-code one
//! (<https://specifications.openehr.org/releases/AM/latest/ADL2.html>, §4.2.5
//! Node Identifier and Coding Systems), and an archetype path predicate is a
//! shortcut for `[@archetype_node_id = '…']` (BASE Release 1.2.0,
//! Architecture Overview, §Paths and Locators), so the mapping names the
//! id-codes the template and its data carry.

use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_testkit::fixtures::FERROEHR_PROFESSIONAL_PERSON_ADL2;
use ferrobridge_testkit::fixtures::FERROEHR_PROFESSIONAL_PERSON_ID;
use http::Request;
use http::StatusCode;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::assemble::parse_artefact;
use openehr_adl::opt::create_opt;
use openehr_adl::parse::Dialect;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::app_over;
use super::call;
use super::demographic::with_body;
use super::handle;
use super::settings;

/// The mapping set of the professional `Practitioner`.
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/it/fixtures_professional"
);

/// The object id of the synthetic professional.
const UID: &str = "a1b2c3d4-0000-4000-8000-0000000000aa";

/// The `OBJECT_VERSION_ID` of the synthetic professional's first version.
const VERSION: &str = "a1b2c3d4-0000-4000-8000-0000000000aa::synthetic.example::1";

/// Returns the operational template the reference CDR serves for the
/// professional `PERSON`, in AOM2 canonical JSON.
fn served_template() -> Result<String, Box<dyn Error>> {
    let archetype = parse_artefact(FERROEHR_PROFESSIONAL_PERSON_ADL2, Dialect::Adl2)
        .map_err(|errors| format!("{errors:?}"))?;
    let raw = create_opt(&archetype, &ArchetypeRepository::default())?;
    Ok(openehr_its::json::to_canonical_json(&raw.template))
}

/// Returns the stub CDR, serving the template on the `adl2` route alone, and
/// a facade over the professional mapping set with `[cdr] demographic` on.
async fn harness() -> Result<(MockServer, Arc<Facade>), Box<dyn Error>> {
    let cdr = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "/definition/template/adl1.4/{FERROEHR_PROFESSIONAL_PERSON_ID}"
        )))
        .respond_with(ResponseTemplate::new(404))
        .mount(&cdr)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "/definition/template/adl2/{FERROEHR_PROFESSIONAL_PERSON_ID}"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{FERROEHR_PROFESSIONAL_PERSON_ID}\""))
                .set_body_string(served_template()?),
        )
        .mount(&cdr)
        .await;
    let base = format!("{}/", cdr.uri()).parse()?;
    let client = CdrClient::new(&CdrConfig::new(base).with_demographic(true))?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    programs::check_parties(&compiled, true)?;
    let store = Arc::new(MemoryStore::default());
    Ok((
        cdr,
        Arc::new(Facade::new(compiled, handle(&store), client, settings())),
    ))
}

/// Returns the synthetic professional as the CDR holds it, its nodes named by
/// the template's id-codes.
fn professional() -> serde_json::Value {
    serde_json::json!({
        "_type": "PERSON",
        "name": {"_type": "DV_TEXT", "value": "Healthcare professional"},
        "archetype_node_id": "openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1",
        "uid": {"_type": "OBJECT_VERSION_ID", "value": VERSION},
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": {"_type": "ARCHETYPE_ID", "value": "openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1"},
            "template_id": {"_type": "TEMPLATE_ID", "value": FERROEHR_PROFESSIONAL_PERSON_ID},
            "rm_version": "1.1.0"
        },
        "details": {
            "_type": "ITEM_TREE",
            "name": {"_type": "DV_TEXT", "value": "Person identifiers"},
            "archetype_node_id": "id2",
            "items": [{
                "_type": "ELEMENT",
                "name": {"_type": "DV_TEXT", "value": "Identifier main data"},
                "archetype_node_id": "id3",
                "value": {"_type": "DV_IDENTIFIER", "issuer": "http://example.org/fhir/sid/login", "assigner": "Synthetic assigner", "id": "SYN-PRO-0001", "type": "login"}
            }]
        },
        "identities": [{
            "_type": "PARTY_IDENTITY",
            "name": {"_type": "DV_TEXT", "value": "Person name"},
            "archetype_node_id": "id4",
            "details": {
                "_type": "ITEM_TREE",
                "name": {"_type": "DV_TEXT", "value": "Name"},
                "archetype_node_id": "id5",
                "items": [{
                    "_type": "ELEMENT",
                    "name": {"_type": "DV_TEXT", "value": "Full name"},
                    "archetype_node_id": "id6",
                    "value": {"_type": "DV_TEXT", "value": "Synthetic Professional"}
                }]
            }
        }]
    })
}

#[tokio::test]
async fn a_practitioner_creates_a_person_named_by_the_id_codes() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness().await?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/demographic/person"))
        .respond_with(ResponseTemplate::new(204).insert_header("ETag", format!("\"{VERSION}\"")))
        .mount(&cdr)
        .await;
    let practitioner = serde_json::json!({
        "resourceType": "Practitioner",
        "identifier": [{"system": "http://example.org/fhir/sid/login", "value": "SYN-PRO-0001"}],
        "name": [{"text": "Synthetic Professional"}]
    });
    let (status, body) = call(
        app_over(&facade),
        with_body(http::Method::POST, "/fhir/Practitioner", &practitioner)?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    assert_eq!(UID, body["id"]);
    let requests = cdr.received_requests().await.unwrap_or_default();
    let sent = requests
        .iter()
        .find(|request| request.method.as_str() == "POST")
        .ok_or("the CDR received the person")?;
    let sent: serde_json::Value = serde_json::from_slice(&sent.body)?;
    assert_eq!("PERSON", sent["_type"], "{sent}");
    assert_eq!(
        FERROEHR_PROFESSIONAL_PERSON_ID, sent["archetype_details"]["template_id"]["value"],
        "{sent}"
    );
    assert_eq!("id2", sent["details"]["archetype_node_id"], "{sent}");
    assert_eq!(
        "id3", sent["details"]["items"][0]["archetype_node_id"],
        "{sent}"
    );
    assert_eq!(
        "SYN-PRO-0001", sent["details"]["items"][0]["value"]["id"],
        "{sent}"
    );
    assert_eq!("id4", sent["identities"][0]["archetype_node_id"], "{sent}");
    assert_eq!(
        "id6", sent["identities"][0]["details"]["items"][0]["archetype_node_id"],
        "{sent}"
    );
    Ok(())
}

#[tokio::test]
async fn a_practitioner_reads_from_an_id_coded_person() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness().await?;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("/demographic/person/{UID}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{VERSION}\""))
                .set_body_json(professional()),
        )
        .mount(&cdr)
        .await;
    let (status, body) = call(
        app_over(&facade),
        Request::get(format!("/fhir/Practitioner/{UID}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!("Practitioner", body["resourceType"]);
    assert_eq!(UID, body["id"]);
    assert_eq!("Synthetic Professional", body["name"][0]["text"], "{body}");
    assert_eq!("SYN-PRO-0001", body["identifier"][0]["value"], "{body}");
    // RM 1.1.0 types DV_IDENTIFIER.type as a free String; with no known system it is the
    // concept's R4 text, never a system-less coding (#2038).
    assert_eq!(
        serde_json::json!({"text": "login"}),
        body["identifier"][0]["type"],
        "{body}"
    );
    Ok(())
}
