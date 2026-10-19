// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search on the four party types by `identifier`, through Cadasto's
//! experimental AQL demographic extension, over a `wiremock` CDR that serves
//! the extension's `POST /query/aql` and the ITS-REST 1.1.0 Demographic API,
//! DEVELOPMENT. The queries are the forms the extension's guide documents or
//! Cadasto observed as stable
//! (<https://docs.cadasto.io/docs/guides/aql-archetype-query-language>,
//! §Querying demographic data). No specification governs a party search: our
//! own design.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::programs::LoadError;
use ferrobridge_server::facade::search::identifier::EXTENSION;
use ferrobridge_server::facade::search::identifier::IdentifierSearch;
use ferrobridge_server::facade::search::party::PartySearch;
use http::Request;
use http::StatusCode;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::app_over;
use super::super::call;
use super::super::first_issue;
use super::super::handle;
use super::super::settings;

/// The demographic fixture mapping set.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures_demographic");

/// The object id of the synthetic party every case finds.
const UID: &str = "a1b2c3d4-0000-4000-8000-000000000001";

/// The object id of a second synthetic party.
const OTHER: &str = "a1b2c3d4-0000-4000-8000-000000000002";

/// The `ITS-REST` query route.
const AQL: &str = "/query/aql";

/// The party types the fixture set serves: the resource type, the RM class,
/// the archetype concept, the template, and the Demographic API path.
const PARTIES: [(&str, &str, &str, &str, &str); 4] = [
    (
        "Patient",
        "PERSON",
        "ferrobridge_person",
        "ferrobridge.person.v1",
        "person",
    ),
    (
        "Practitioner",
        "PERSON",
        "ferrobridge_person",
        "ferrobridge.person.v1",
        "person",
    ),
    (
        "Organization",
        "ORGANISATION",
        "ferrobridge_organisation",
        "ferrobridge.organisation.v1",
        "organisation",
    ),
    (
        "PractitionerRole",
        "ROLE",
        "ferrobridge_role",
        "ferrobridge.role.v1",
        "role",
    ),
];

/// Returns the identifier query of `class`, in the dialect's documented shape.
fn query_of(class: &str) -> String {
    format!(
        "SELECT p/uid/value FROM {class} p WHERE p/details[at0001]/items[at0002]/value/id = \
         $identifier ORDER BY p/uid/value ASC"
    )
}

/// Returns the search of every type in `types`.
fn searching(types: &[&str]) -> PartySearch {
    PartySearch::new(
        PARTIES
            .iter()
            .filter(|(resource_type, ..)| types.contains(resource_type))
            .map(|&(resource_type, class, ..)| {
                (
                    String::from(resource_type),
                    IdentifierSearch::new(&query_of(class), None)
                        .expect("the query binds the identifier"),
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )
}

/// Returns the canonical party of `class` the CDR holds under `uid`.
fn party(class: &str, concept: &str, template: &str, uid: &str) -> serde_json::Value {
    let mut party = serde_json::json!({
        "_type": class,
        "name": {"_type": "DV_TEXT", "value": "Synthetic party"},
        "archetype_node_id": format!("openEHR-DEMOGRAPHIC-{class}.{concept}.v1"),
        "uid": {"_type": "OBJECT_VERSION_ID", "value": format!("{uid}::synthetic.example::1")},
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": {"_type": "ARCHETYPE_ID", "value": format!("openEHR-DEMOGRAPHIC-{class}.{concept}.v1")},
            "template_id": {"_type": "TEMPLATE_ID", "value": template},
            "rm_version": "1.1.0"
        },
        "identities": [{
            "_type": "PARTY_IDENTITY",
            "name": {"_type": "DV_TEXT", "value": "Identity"},
            "archetype_node_id": "at0003",
            "details": {
                "_type": "ITEM_TREE",
                "name": {"_type": "DV_TEXT", "value": "Identity details"},
                "archetype_node_id": "at0004",
                "items": [{
                    "_type": "ELEMENT",
                    "name": {"_type": "DV_TEXT", "value": "Name"},
                    "archetype_node_id": "at0005",
                    "value": {"_type": "DV_TEXT", "value": "Synthetic Party"}
                }]
            }
        }]
    });
    if class == "ROLE"
        && let Some(object) = party.as_object_mut()
    {
        object.insert(
            String::from("performer"),
            serde_json::json!({
                "_type": "PARTY_REF",
                "namespace": "demographic",
                "type": "ACTOR",
                "id": {"_type": "HIER_OBJECT_ID", "value": OTHER}
            }),
        );
    }
    party
}

/// Returns the stub CDR and a facade over the demographic fixture set,
/// searching the party types `search` configures.
async fn harness(
    search: Option<PartySearch>,
) -> Result<(MockServer, Arc<Facade>), Box<dyn StdError>> {
    let cdr = MockServer::start().await;
    for (id, opt) in [
        (
            "ferrobridge.person.v1",
            ferrobridge_testkit::fixtures::PERSON_OPT,
        ),
        (
            "ferrobridge.organisation.v1",
            ferrobridge_testkit::fixtures::ORGANISATION_OPT,
        ),
        (
            "ferrobridge.role.v1",
            ferrobridge_testkit::fixtures::ROLE_OPT,
        ),
    ] {
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("/definition/template/adl1.4/{id}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "application/xml")
                    .set_body_string(opt),
            )
            .mount(&cdr)
            .await;
    }
    let base = format!("{}/", cdr.uri()).parse()?;
    let client = CdrClient::new(&CdrConfig::new(base).with_demographic(true))?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    programs::check_parties(&compiled, true)?;
    programs::check_party_search(&compiled, search.as_ref())?;
    let store = Arc::new(MemoryStore::new());
    let facade =
        Facade::new(compiled, handle(&store), client, settings()).with_party_search(search);
    Ok((cdr, Arc::new(facade)))
}

/// Answers every query with one row per uid of `uids`, as versioned uids.
async fn mount_rows(cdr: &MockServer, uids: &[&str]) {
    let rows: Vec<serde_json::Value> = uids
        .iter()
        .map(|uid| serde_json::json!([format!("{uid}::synthetic.example::1")]))
        .collect();
    Mock::given(matchers::method("POST"))
        .and(matchers::path(AQL))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "q": "synthetic",
            "columns": [{"name": "uid", "path": "/uid/value"}],
            "rows": rows
        })))
        .mount(cdr)
        .await;
}

/// Mounts `GET /demographic/{path}/{uid}` answering the party.
async fn mount_party(cdr: &MockServer, path: &str, body: &serde_json::Value, uid: &str) {
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("/demographic/{path}/{uid}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{uid}::synthetic.example::1\""))
                .set_body_json(body),
        )
        .mount(cdr)
        .await;
}

/// Sends `GET /fhir/{path}`.
async fn get(
    facade: &Arc<Facade>,
    path: &str,
) -> Result<(StatusCode, serde_json::Value), Box<dyn StdError>> {
    call(
        app_over(facade),
        Request::get(format!("/fhir/{path}")).body(Body::empty())?,
    )
    .await
}

/// Returns the queries the CDR received.
async fn queries(cdr: &MockServer) -> Vec<serde_json::Value> {
    cdr.received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == AQL)
        .filter_map(|request| serde_json::from_slice(&request.body).ok())
        .collect()
}

#[tokio::test]
async fn each_party_type_is_searched_by_identifier() -> Result<(), Box<dyn StdError>> {
    for (resource_type, class, concept, template, path) in PARTIES {
        let (cdr, facade) = harness(Some(searching(&[resource_type]))).await?;
        mount_rows(&cdr, &[UID]).await;
        mount_party(&cdr, path, &party(class, concept, template, UID), UID).await;
        let (status, body) = get(&facade, &format!("{resource_type}?identifier=SYN-0001")).await?;
        assert_eq!(StatusCode::OK, status, "{resource_type}: {body}");
        assert_eq!(body["type"], "searchset", "{body}");
        let entries = body["entry"].as_array().ok_or("a searchset with entries")?;
        assert_eq!(entries.len(), 1, "{resource_type}: {body}");
        assert_eq!(entries[0]["resource"]["resourceType"], resource_type);
        assert_eq!(entries[0]["resource"]["id"], UID);
        let sent = queries(&cdr).await;
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0]["q"], query_of(class));
        assert_eq!(sent[0]["query_parameters"]["identifier"], "SYN-0001");
        assert_eq!(sent[0]["offset"], 0);
    }
    Ok(())
}

#[tokio::test]
async fn a_page_with_a_row_left_links_the_next_page() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    mount_rows(&cdr, &[UID, OTHER]).await;
    let (class, concept, template) = ("PERSON", "ferrobridge_person", "ferrobridge.person.v1");
    mount_party(&cdr, "person", &party(class, concept, template, UID), UID).await;
    let (status, body) = get(&facade, "Patient?identifier=SYN-0001&_count=1").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(body["entry"].as_array().map(Vec::len), Some(1), "{body}");
    let next = body["link"]
        .as_array()
        .and_then(|links| links.iter().find(|link| link["relation"] == "next"))
        .and_then(|link| link["url"].as_str())
        .ok_or("the page links the next one")?;
    assert!(next.contains("_cursor="), "{next}");
    let sent = queries(&cdr).await;
    assert_eq!(sent[0]["fetch"], 2, "{sent:?}");
    Ok(())
}

/// RFC 9110 §15.6.2: a CDR without the extension refuses the query, and the
/// server cannot serve the search.
#[tokio::test]
async fn a_cdr_that_does_not_serve_the_extension_is_a_501() -> Result<(), Box<dyn StdError>> {
    let (cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(AQL))
        .respond_with(
            ResponseTemplate::new(400)
                .insert_header("Content-Type", "application/json")
                .set_body_json(serde_json::json!({
                    "message": "FROM source class `PERSON` is not in scope"
                })),
        )
        .mount(&cdr)
        .await;
    let (status, body) = get(&facade, "Patient?identifier=SYN-0001").await?;
    assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "{body}");
    let issue = first_issue(&body);
    assert_eq!(issue["code"], "not-supported", "{body}");
    let diagnostics = issue["diagnostics"].as_str().ok_or("diagnostics")?;
    assert!(diagnostics.contains(EXTENSION), "{diagnostics}");
    assert!(diagnostics.contains("not in scope"), "{diagnostics}");
    Ok(())
}

#[tokio::test]
async fn a_type_no_query_is_configured_for_offers_no_search() -> Result<(), Box<dyn StdError>> {
    let (_cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    let (status, body) = get(&facade, "Organization?identifier=SYN-0001").await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    assert_eq!(first_issue(&body)["code"], "not-supported");
    Ok(())
}

#[tokio::test]
async fn a_parameter_other_than_identifier_is_refused() -> Result<(), Box<dyn StdError>> {
    let (_cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    for path in [
        "Patient?name=Synthetic",
        "Patient",
        "Patient?identifier=SYN-0001,SYN-0002",
        "Patient?identifier=SYN-0001&identifier=SYN-0002",
    ] {
        let (status, body) = get(&facade, path).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{path}: {body}");
        assert_eq!(first_issue(&body)["code"], "not-supported", "{path}");
    }
    Ok(())
}

/// R4 search.html §3.1.1.3: `Prefer: handling=lenient` asks the server to
/// ignore a parameter it does not support.
#[tokio::test]
async fn a_lenient_search_ignores_another_parameter_and_reports_it() -> Result<(), Box<dyn StdError>>
{
    let (cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    mount_rows(&cdr, &[UID]).await;
    let (class, concept, template) = ("PERSON", "ferrobridge_person", "ferrobridge.person.v1");
    mount_party(&cdr, "person", &party(class, concept, template, UID), UID).await;
    let (status, body) = call(
        app_over(&facade),
        Request::get("/fhir/Patient?identifier=SYN-0001&name=Synthetic")
            .header("Prefer", "handling=lenient")
            .body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let entries = body["entry"].as_array().ok_or("entries")?;
    assert_eq!(entries.len(), 2, "a match and the outcome: {body}");
    let outcome = entries
        .iter()
        .find(|entry| entry["resource"]["resourceType"] == "OperationOutcome")
        .ok_or("the ignored parameter is reported")?;
    assert_eq!(outcome["resource"]["issue"][0]["code"], "not-supported");
    Ok(())
}

#[tokio::test]
async fn the_statement_declares_search_on_the_configured_types_only()
-> Result<(), Box<dyn StdError>> {
    let (_cdr, facade) = harness(Some(searching(&["Patient", "Organization"]))).await?;
    let (status, body) = get(&facade, "metadata").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let resources = body["rest"][0]["resource"]
        .as_array()
        .ok_or("the statement lists resources")?;
    for (resource_type, ..) in PARTIES {
        let entry = resources
            .iter()
            .find(|entry| entry["type"] == resource_type)
            .ok_or(format!("{resource_type} is declared"))?;
        let searched = entry["interaction"]
            .as_array()
            .ok_or("interactions")?
            .iter()
            .any(|interaction| interaction["code"] == "search-type");
        let wanted = matches!(resource_type, "Patient" | "Organization");
        assert_eq!(searched, wanted, "{resource_type}: {entry}");
        if wanted {
            assert_eq!(
                entry["searchParam"],
                serde_json::json!([{
                    "name": "identifier",
                    "definition": format!("http://hl7.org/fhir/SearchParameter/{resource_type}-identifier"),
                    "type": "token",
                    "documentation": format!("Resolved through the {EXTENSION}")
                }]),
                "{entry}"
            );
        } else {
            assert!(entry.get("searchParam").is_none(), "{entry}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_search_of_a_type_no_party_context_serves_does_not_boot() -> Result<(), Box<dyn StdError>>
{
    let search = PartySearch::new(BTreeMap::from([(
        String::from("Group"),
        IdentifierSearch::new(&query_of("GROUP"), None)?,
    )]));
    let refused = harness(Some(search)).await;
    let error = refused
        .err()
        .ok_or("the search of an unserved type booted")?;
    assert!(
        matches!(
            error.downcast_ref::<LoadError>(),
            Some(LoadError::UnservedPartySearch { resource_type }) if resource_type == "Group"
        ),
        "{error}"
    );
    Ok(())
}
