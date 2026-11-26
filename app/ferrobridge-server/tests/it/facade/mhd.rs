// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The IHE MHD 4.2.3 Document Responder: Find Document References [ITI-67],
//! Find Document Lists [ITI-66] and Retrieve Document [ITI-68] over the
//! document one Composition context assembles.

use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::endpoint::Endpoints;
use ferrobridge_server::identity::PersonId;
use ferrobridge_server::identity::store::MemoryStore;
use ferrobridge_server::identity::store::Store;
use ferrobridge_server::terminology::Translator;
use http::Request;
use http::StatusCode;
use http::header;
use wiremock::matchers;
use wiremock::{Mock, ResponseTemplate};

use super::BASE_URL;
use super::EHR_ID;
use super::Harness;
use super::VERSION_ONE;
use super::call;
use super::facade_over;
use super::handle;
use super::raw;
use super::settings;

/// The fixture directory holding the Composition context.
const MHD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/mhd");

/// The profile the Composition context maps.
const PROFILE: &str = "http://example.org/fhir/StructureDefinition/ferrobridge-mhd";

/// The pseudonym the suite's EHR is keyed by.
const PSEUDONYM: &str = "p-1";

/// The patient the suite's EHR belongs to.
const SUBJECT: &str = "Patient/p-1";

/// The CONTRIBUTION that committed the composition.
const CONTRIBUTION: &str = "7b0a4c2e-0000-4000-8000-00000000000c";

/// The system that committed it.
const SYSTEM_ID: &str = "ferrobridge.test";

/// Returns a harness whose facade serves the Document Responder over the
/// fixture context, its subject bound to the suite's EHR, and whose CDR
/// lists the one composition.
async fn responder() -> Result<Harness, Box<dyn Error>> {
    responder_over(MHD, "ferrobridge_mhd.context").await
}

/// Returns the [`responder`] harness over the Composition context `context`
/// under `fixtures`.
async fn responder_over(fixtures: &str, context: &str) -> Result<Harness, Box<dyn Error>> {
    let store = Arc::new(MemoryStore::default());
    let (cdr, facade) = facade_over(
        fixtures,
        settings(),
        None,
        handle(&store),
        Translator::default(),
    )
    .await;
    let facade = Arc::try_unwrap(facade)
        .map_err(|_shared| "the facade is held once")?
        .with_mhd(true)
        .with_endpoints(Some(Endpoints::default()));
    let harness = Harness {
        cdr,
        store,
        facade: Arc::new(facade),
    };
    harness
        .store
        .record_ehr(
            &PersonId::new("http://example.org/fhir", PSEUDONYM)?,
            &EhrId::new(EHR_ID)?,
        )
        .await?;
    let composition = composition(&harness, context);
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/query/aql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "q": "synthetic",
            "columns": [
                {"name": "ehr_id", "path": "/ehr_id/value"},
                {"name": "version_uid", "path": "/uid/value"},
                {"name": "contribution", "path": "/contribution/id/value"},
                {"name": "time_committed", "path": "/commit_audit/time_committed/value"},
                {"name": "system_id", "path": "/commit_audit/system_id"},
                {"name": "composition", "path": ""}
            ],
            "rows": [[EHR_ID, VERSION_ONE, CONTRIBUTION, "2026-09-15T09:00:00Z", SYSTEM_ID, composition]]
        })))
        .mount(&harness.cdr)
        .await;
    super::stub::mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    Ok(harness)
}

/// Returns the canonical composition the synthetic Composition maps to under
/// `context`, at [`VERSION_ONE`].
fn composition(harness: &Harness, context: &str) -> serde_json::Value {
    let mut composition = harness.composition_under(
        context,
        &serde_json::json!({
            "resourceType": "Composition",
            "meta": {"profile": [PROFILE]},
            "status": "final",
            "type": {"coding": [{"system": "http://loinc.org", "code": "11488-4"}]},
            "date": "2026-09-15T09:00:00Z",
            "author": [{"display": "Synthetic Author"}],
            "title": "Synthetic report",
            "contained": [{
                "resourceType": "Condition",
                "id": "problem",
                "code": {"text": "Synthetic diagnosis"}
            }],
            "section": [{"entry": [{"reference": "#problem"}]}]
        }),
    );
    composition["uid"] = serde_json::json!({"_type": "OBJECT_VERSION_ID", "value": VERSION_ONE});
    composition
}

/// Answers `GET` on `path` against the harness.
async fn get(
    harness: &Harness,
    path: &str,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    call(harness.app(), Request::get(path).body(Body::empty())?).await
}

/// Returns the resources a searchset matched.
fn matched(bundle: &serde_json::Value) -> Vec<&serde_json::Value> {
    bundle["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["search"]["mode"] == "match")
                .map(|entry| &entry["resource"])
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the URL of the searchset's link `relation`.
fn link<'a>(bundle: &'a serde_json::Value, relation: &str) -> Option<&'a str> {
    bundle["link"].as_array().and_then(|links| {
        links
            .iter()
            .find(|link| link["relation"] == relation)
            .and_then(|link| link["url"].as_str())
    })
}

#[tokio::test]
async fn find_document_references_describes_the_patients_document() -> Result<(), Box<dyn Error>> {
    let harness = responder().await?;
    let (status, bundle) = get(
        &harness,
        &format!("/fhir/DocumentReference?patient={SUBJECT}&status=current"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{bundle}");
    let found = matched(&bundle);
    assert_eq!(1, found.len(), "{bundle}");
    let reference = found[0];
    let id = reference["id"]
        .as_str()
        .ok_or("the description has an id")?;
    assert_eq!(
        Some(format!("{BASE_URL}/Composition/{id}/$document").as_str()),
        reference["content"][0]["attachment"]["url"].as_str(),
        "ITI TF-2: 3.67.4.2.2.1, the full URL ITI-68 retrieves: {bundle}"
    );
    assert_eq!(Some("current"), reference["status"].as_str(), "{bundle}");
    assert_eq!(
        Some("34133-9"),
        reference["category"][0]["coding"][0]["code"].as_str(),
        "{bundle}"
    );
    assert_eq!(
        Some("11488-4"),
        reference["type"]["coding"][0]["code"].as_str(),
        "{bundle}"
    );
    Ok(())
}

#[tokio::test]
async fn each_parameter_matches_the_element_the_description_fills() -> Result<(), Box<dyn Error>> {
    let harness = responder().await?;
    for (query, expected) in [
        ("category=http://loinc.org|34133-9", 1),
        ("category=http://loinc.org|11488-4", 0),
        ("type=11488-4", 1),
        ("status=superseded", 0),
        ("date=ge2000-01-01", 1),
        ("date=lt2000-01-01", 0),
        ("period=2026", 0),
        ("setting=394802001", 0),
        ("author.family=Author", 0),
        ("creation=ge2000", 0),
        ("event=a", 0),
        ("facility=a", 0),
        ("format=a", 0),
        ("security-label=N", 0),
        ("related=DocumentReference/x", 0),
    ] {
        let (status, bundle) = get(
            &harness,
            &format!("/fhir/DocumentReference?patient={SUBJECT}&{query}"),
        )
        .await?;
        assert_eq!(StatusCode::OK, status, "{query}: {bundle}");
        assert_eq!(expected, matched(&bundle).len(), "{query}: {bundle}");
    }
    Ok(())
}

#[tokio::test]
async fn an_unsupported_parameter_is_ignored_and_named_unless_strict_handling_asks()
-> Result<(), Box<dyn Error>> {
    let harness = responder().await?;
    let path = format!("/fhir/DocumentReference?patient={SUBJECT}&_sort=date");
    let (status, bundle) = get(&harness, &path).await?;
    assert_eq!(StatusCode::OK, status, "{bundle}");
    assert_eq!(1, matched(&bundle).len(), "{bundle}");
    let own = link(&bundle, "self").ok_or("a self link")?;
    assert!(
        own.contains("patient=") && !own.contains("_sort"),
        "R4 search.html §3.1.1.6: self names the applied parameters: {own}"
    );
    let outcome = bundle["entry"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["search"]["mode"] == "outcome")
        })
        .ok_or_else(|| format!("no outcome entry in {bundle}"))?;
    assert_eq!(
        Some("_sort"),
        outcome["resource"]["issue"][0]["expression"][0].as_str(),
        "{bundle}"
    );
    let (status, refused) = call(
        harness.app(),
        Request::get(&path)
            .header("prefer", "handling=strict")
            .body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{refused}");
    Ok(())
}

#[tokio::test]
async fn a_search_with_no_patient_is_refused() -> Result<(), Box<dyn Error>> {
    let harness = responder().await?;
    let (status, body) = get(&harness, "/fhir/DocumentReference?status=current").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn a_post_search_answers_as_the_get() -> Result<(), Box<dyn Error>> {
    let harness = responder().await?;
    let (status, bundle) = call(
        harness.app(),
        Request::post("/fhir/DocumentReference/_search")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("patient={SUBJECT}&status=current")))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{bundle}");
    assert_eq!(1, matched(&bundle).len(), "{bundle}");
    Ok(())
}

#[tokio::test]
async fn a_description_reads_by_the_compositions_id() -> Result<(), Box<dyn Error>> {
    let harness = responder().await?;
    let (_, bundle) = get(
        &harness,
        &format!("/fhir/DocumentReference?patient={SUBJECT}"),
    )
    .await?;
    let id = matched(&bundle)
        .first()
        .and_then(|reference| reference["id"].as_str())
        .ok_or("a description")?
        .to_owned();
    let (status, read) = get(&harness, &format!("/fhir/DocumentReference/{id}")).await?;
    assert_eq!(StatusCode::OK, status, "{read}");
    assert_eq!(Some(id.as_str()), read["id"].as_str(), "{read}");
    let (status, _) = get(&harness, "/fhir/DocumentReference/unknown").await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    Ok(())
}

#[tokio::test]
async fn the_responder_answers_in_xml_when_asked() -> Result<(), Box<dyn Error>> {
    let harness = responder().await?;
    let response = raw(
        harness.app(),
        Request::get(format!("/fhir/DocumentReference?patient={SUBJECT}"))
            .header(header::ACCEPT, "application/fhir+xml")
            .body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some("application/fhir+xml"),
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
    );
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let text = String::from_utf8(bytes.to_vec())?;
    assert!(text.starts_with("<Bundle"), "{text}");
    Ok(())
}

#[tokio::test]
async fn retrieve_document_answers_json_or_xml_and_refuses_any_other_type()
-> Result<(), Box<dyn Error>> {
    // ITI TF-2: 3.68.4.1.3: the document in the requested media type, or an error.
    let harness = responder().await?;
    let (_, bundle) = get(
        &harness,
        &format!("/fhir/DocumentReference?patient={SUBJECT}"),
    )
    .await?;
    let url = matched(&bundle)
        .first()
        .and_then(|reference| reference["content"][0]["attachment"]["url"].as_str())
        .ok_or("a document URL")?
        .to_owned();
    let path = url.trim_start_matches(BASE_URL.trim_end_matches("/fhir"));
    let (status, document) = get(&harness, path).await?;
    assert_eq!(StatusCode::OK, status, "{document}");
    assert_eq!(Some("document"), document["type"].as_str(), "{document}");
    for (accept, expected) in [
        ("application/fhir+xml", StatusCode::OK),
        ("application/pdf", StatusCode::NOT_ACCEPTABLE),
    ] {
        let response = raw(
            harness.app(),
            Request::get(path)
                .header(header::ACCEPT, accept)
                .body(Body::empty())?,
        )
        .await?;
        assert_eq!(expected, response.status(), "{accept}");
    }
    Ok(())
}

#[tokio::test]
async fn find_document_lists_answers_the_contribution_as_a_submission_set()
-> Result<(), Box<dyn Error>> {
    let harness = responder().await?;
    let (status, bundle) = get(
        &harness,
        &format!("/fhir/List?patient={SUBJECT}&code=submissionset&status=current"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{bundle}");
    let found = matched(&bundle);
    assert_eq!(1, found.len(), "{bundle}");
    let list = found[0];
    assert_eq!(Some(CONTRIBUTION), list["id"].as_str(), "{bundle}");
    assert!(
        list["entry"][0]["item"]["reference"]
            .as_str()
            .is_some_and(|reference| reference.starts_with("DocumentReference/")),
        "{bundle}"
    );
    for (query, expected) in [
        ("code=folder", 0),
        (&*format!("sourceId={SYSTEM_ID}"), 1),
        ("designationType=a", 0),
        ("source.family=a", 0),
        ("date=2026-09-15", 1),
    ] {
        let (status, bundle) =
            get(&harness, &format!("/fhir/List?patient={SUBJECT}&{query}")).await?;
        assert_eq!(StatusCode::OK, status, "{query}: {bundle}");
        assert_eq!(expected, matched(&bundle).len(), "{query}: {bundle}");
    }
    let (status, read) = get(&harness, &format!("/fhir/List/{CONTRIBUTION}")).await?;
    assert_eq!(StatusCode::OK, status, "{read}");
    assert_eq!(Some("working"), read["mode"].as_str(), "{read}");
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_instantiates_the_document_responder() -> Result<(), Box<dyn Error>>
{
    let harness = responder().await?;
    let (status, statement) = get(&harness, "/fhir/metadata").await?;
    assert_eq!(StatusCode::OK, status, "{statement}");
    assert!(
        statement["instantiates"].as_array().is_some_and(|all| {
            all.iter().any(|one| one
                == "https://profiles.ihe.net/ITI/MHD/CapabilityStatement/IHE.MHD.DocumentResponder")
        }),
        "{statement}"
    );
    let resources = statement["rest"][0]["resource"]
        .as_array()
        .ok_or("resources")?;
    for kind in ["DocumentReference", "List"] {
        let resource = resources
            .iter()
            .find(|resource| resource["type"] == kind)
            .ok_or_else(|| format!("no {kind} in {statement}"))?;
        assert!(
            resource["searchParam"]
                .as_array()
                .is_some_and(|params| params
                    .iter()
                    .any(|param| param["name"] == "patient.identifier")),
            "{kind}: {statement}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn binary_read_is_neither_claimed_nor_served() -> Result<(), Box<dyn Error>> {
    // R4 CapabilityStatement.instantiates: a server may implement a subset of
    // what it instantiates and states its full capability details. ITI-68
    // retrieves the `attachment.url`, the `$document` URL (ITI TF-2: 3.68.4.1.2).
    let harness = responder().await?;
    let (status, statement) = get(&harness, "/fhir/metadata").await?;
    assert_eq!(StatusCode::OK, status, "{statement}");
    let resources = statement["rest"][0]["resource"]
        .as_array()
        .ok_or("resources")?;
    assert!(
        resources
            .iter()
            .all(|resource| resource["type"] != "Binary"),
        "{statement}"
    );
    let composition = resources
        .iter()
        .find(|resource| resource["type"] == "Composition")
        .ok_or_else(|| format!("no Composition in {statement}"))?;
    assert!(
        composition["operation"]
            .as_array()
            .is_some_and(|operations| operations
                .iter()
                .any(|operation| operation["name"] == "document")),
        "{statement}"
    );
    let (status, outcome) = get(&harness, "/fhir/Binary/any").await?;
    assert!(status.is_client_error(), "{status}: {outcome}");
    assert_eq!(Some("OperationOutcome"), outcome["resourceType"].as_str());
    Ok(())
}

#[tokio::test]
async fn a_composition_with_no_category_is_refused_naming_it() -> Result<(), Box<dyn Error>> {
    // Draft EEHRxF Annex III Table 1: documentCategory is required.
    let harness = responder_over(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/document"),
        "ferrobridge_document.context",
    )
    .await?;
    let (status, body) = get(
        &harness,
        &format!("/fhir/DocumentReference?patient={SUBJECT}"),
    )
    .await?;
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, status, "{body}");
    assert!(
        body["issue"][0]["diagnostics"]
            .as_str()
            .is_some_and(|text| text.starts_with("Composition/") && text.contains("category")),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn the_responder_is_an_endpoint_with_a_coded_payload_in_both_formats()
-> Result<(), Box<dyn Error>> {
    let harness = responder().await?;
    let (status, endpoint) = get(&harness, "/fhir/Endpoint/mhd").await?;
    assert_eq!(StatusCode::OK, status, "{endpoint}");
    assert_eq!(
        Some("urn:ihe:iti:xds:2017:mimeTypeSufficient"),
        endpoint["payloadType"][0]["coding"][0]["code"].as_str(),
        "{endpoint}"
    );
    assert_eq!(
        serde_json::json!(["application/fhir+json", "application/fhir+xml"]),
        endpoint["payloadMimeType"],
        "{endpoint}"
    );
    Ok(())
}
