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
use std::error::Error;
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
use ferrobridge_server::facade::search::party::NameSearch;
use ferrobridge_server::facade::search::party::PartyQueries;
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

/// Returns the name query of `class`, comparing the three case variants of
/// the value as prefix patterns.
fn name_query_of(class: &str) -> String {
    format!(
        "SELECT p/uid/value FROM {class} p WHERE \
         p/identities[at0003]/details[at0004]/items[at0005]/value/value LIKE $name OR \
         p/identities[at0003]/details[at0004]/items[at0005]/value/value LIKE $name_lower OR \
         p/identities[at0003]/details[at0004]/items[at0005]/value/value LIKE $name_capitalised \
         ORDER BY p/uid/value ASC"
    )
}

/// Whether the suite configures a `name` query for `resource_type`: the two
/// R4 types of this search that define `name` over a person
/// (<https://hl7.org/fhir/R4/patient.html#search>).
fn named(resource_type: &str) -> bool {
    matches!(resource_type, "Patient" | "Practitioner")
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
                    PartyQueries::new(
                        Some(
                            IdentifierSearch::new(&query_of(class), None)
                                .expect("the query binds the identifier"),
                        ),
                        named(resource_type).then(|| {
                            NameSearch::new(&name_query_of(class))
                                .expect("the query binds the name")
                        }),
                    ),
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
) -> Result<(MockServer, Arc<Facade>), Box<dyn Error>> {
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
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
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
async fn each_party_type_is_searched_by_identifier() -> Result<(), Box<dyn Error>> {
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
async fn a_page_with_a_row_left_links_the_next_page() -> Result<(), Box<dyn Error>> {
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
async fn a_cdr_that_does_not_serve_the_extension_is_a_501() -> Result<(), Box<dyn Error>> {
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
async fn a_type_no_query_is_configured_for_offers_no_search() -> Result<(), Box<dyn Error>> {
    let (_cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    let (status, body) = get(&facade, "Organization?identifier=SYN-0001").await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    assert_eq!(first_issue(&body)["code"], "not-supported");
    Ok(())
}

#[tokio::test]
async fn an_unsupported_parameter_or_combination_is_refused() -> Result<(), Box<dyn Error>> {
    let (_cdr, facade) = harness(Some(searching(&["Patient", "Organization"]))).await?;
    for path in [
        "Patient?birthdate=1990-01-01",
        "Patient",
        "Patient?identifier=SYN-0001,SYN-0002",
        "Patient?identifier=SYN-0001&identifier=SYN-0002",
        "Patient?identifier=SYN-0001&name=Synthetic",
        "Patient?name:exact=Synthetic",
        "Patient?name=Syn*",
        "Organization?name=Synthetic",
    ] {
        let (status, body) = get(&facade, path).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{path}: {body}");
        assert_eq!(first_issue(&body)["code"], "not-supported", "{path}");
    }
    Ok(())
}

#[tokio::test]
async fn a_count_that_is_no_number_is_invalid() -> Result<(), Box<dyn Error>> {
    let (_cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    let (status, body) = get(&facade, "Patient?identifier=SYN-0001&_count=many").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    assert_eq!(first_issue(&body)["code"], "invalid", "{body}");
    Ok(())
}

/// R4 search.html §3.1.1.3: `Prefer: handling=lenient` asks the server to
/// ignore a parameter it does not support.
#[tokio::test]
async fn a_lenient_search_ignores_another_parameter_and_reports_it() -> Result<(), Box<dyn Error>>
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
-> Result<(), Box<dyn Error>> {
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
            let mut declared = vec![
                serde_json::json!({
                    "name": "_id",
                    "definition": "http://hl7.org/fhir/SearchParameter/Resource-id",
                    "type": "token"
                }),
                serde_json::json!({
                    "name": "identifier",
                    "definition": format!("http://hl7.org/fhir/SearchParameter/{resource_type}-identifier"),
                    "type": "token",
                    "documentation": format!("Resolved through the {EXTENSION}")
                }),
            ];
            if named(resource_type) {
                declared.push(serde_json::json!({
                    "name": "name",
                    "definition": format!("http://hl7.org/fhir/SearchParameter/{resource_type}-name"),
                    "type": "string",
                    "documentation": format!("Resolved through the {EXTENSION}")
                }));
            }
            assert_eq!(
                entry["searchParam"],
                serde_json::Value::Array(declared),
                "{entry}"
            );
        } else {
            assert!(entry.get("searchParam").is_none(), "{entry}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_search_of_a_type_no_party_context_serves_does_not_boot() -> Result<(), Box<dyn Error>>
{
    let search = PartySearch::new(BTreeMap::from([(
        String::from("Group"),
        PartyQueries::new(Some(IdentifierSearch::new(&query_of("GROUP"), None)?), None),
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

/// The canonical `PERSON` the CDR holds under `uid`.
fn person(uid: &str) -> serde_json::Value {
    party("PERSON", "ferrobridge_person", "ferrobridge.person.v1", uid)
}

/// Returns the parties the CDR was asked for over the Demographic API.
async fn party_reads(cdr: &MockServer) -> usize {
    cdr.received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path().starts_with("/demographic/"))
        .count()
}

/// R4 search.html §3.1.1.4.8: a string matches a field that "equals or
/// starts with the supplied parameter value, after both have been normalized
/// by case"; the dialect's `LIKE` is case-sensitive, so three case variants
/// are bound.
#[tokio::test]
async fn a_name_search_binds_three_case_variants_of_a_prefix() -> Result<(), Box<dyn Error>> {
    for resource_type in ["Patient", "Practitioner"] {
        let (cdr, facade) = harness(Some(searching(&[resource_type]))).await?;
        mount_rows(&cdr, &[UID]).await;
        mount_party(&cdr, "person", &person(UID), UID).await;
        let (status, body) = get(&facade, &format!("{resource_type}?name=syn")).await?;
        assert_eq!(StatusCode::OK, status, "{resource_type}: {body}");
        assert_eq!(body["entry"].as_array().map(Vec::len), Some(1), "{body}");
        let sent = queries(&cdr).await;
        assert_eq!(sent[0]["q"], name_query_of("PERSON"));
        assert_eq!(sent[0]["query_parameters"]["name"], "syn*");
        assert_eq!(sent[0]["query_parameters"]["name_lower"], "syn*");
        assert_eq!(sent[0]["query_parameters"]["name_capitalised"], "Syn*");
    }
    Ok(())
}

/// R4 search.html §3.1.1.4.10: `[parameter]=[code]` matches "irrespective
/// of the value of the system property", so two parties are two entries.
#[tokio::test]
async fn a_bare_identifier_two_parties_carry_answers_both() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    mount_rows(&cdr, &[UID, OTHER]).await;
    mount_party(&cdr, "person", &person(UID), UID).await;
    mount_party(&cdr, "person", &person(OTHER), OTHER).await;
    let (status, body) = get(&facade, "Patient?identifier=SYN-0001").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let ids: Vec<&str> = body["entry"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .filter_map(|entry| entry["resource"]["id"].as_str())
        .collect();
    assert_eq!(ids, [UID, OTHER], "{body}");
    Ok(())
}

/// R4 search.html §3.1.1.4.10: `[system]|[code]` matches the system too.
#[tokio::test]
async fn an_identifier_with_a_system_runs_the_system_query() -> Result<(), Box<dyn Error>> {
    let system_query = "SELECT p/uid/value FROM PERSON p WHERE \
                        p/details[at0001]/items[at0002]/value/id = $identifier AND \
                        p/details[at0001]/items[at0002]/value/issuer = $system";
    let search = PartySearch::new(BTreeMap::from([(
        String::from("Patient"),
        PartyQueries::new(
            Some(IdentifierSearch::new(
                &query_of("PERSON"),
                Some(system_query),
            )?),
            None,
        ),
    )]));
    let (cdr, facade) = harness(Some(search)).await?;
    mount_rows(&cdr, &[UID]).await;
    mount_party(&cdr, "person", &person(UID), UID).await;
    let (status, body) = get(
        &facade,
        "Patient?identifier=http%3A%2F%2Fexample.org%2Fsid%7CSYN-0001",
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let sent = queries(&cdr).await;
    assert_eq!(sent[0]["q"], system_query);
    assert_eq!(
        sent[0]["query_parameters"]["system"],
        "http://example.org/sid"
    );
    assert_eq!(sent[0]["query_parameters"]["identifier"], "SYN-0001");
    Ok(())
}

/// R4 search.html §3.1.1.4.1: `_id` names the one resource with that id.
#[tokio::test]
async fn an_id_reads_the_one_party_it_names_and_runs_no_query() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    mount_party(&cdr, "person", &person(UID), UID).await;
    let (status, body) = get(&facade, &format!("Patient?_id={UID}")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let entries = body["entry"].as_array().ok_or("entries")?;
    assert_eq!(entries.len(), 1, "{body}");
    assert_eq!(entries[0]["resource"]["id"], UID);
    assert!(queries(&cdr).await.is_empty(), "_id ran a query");
    Ok(())
}

#[tokio::test]
async fn an_id_naming_no_party_of_the_type_matches_nothing() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness(Some(searching(&["Patient", "Organization"]))).await?;
    mount_party(
        &cdr,
        "organisation",
        &party(
            "ORGANISATION",
            "ferrobridge_organisation",
            "another.template.v1",
            UID,
        ),
        UID,
    )
    .await;
    for path in [
        format!("Patient?_id={OTHER}"),
        format!("Organization?_id={UID}"),
        String::from("Patient?_id=not%20a%20uid"),
    ] {
        let (status, body) = get(&facade, &path).await?;
        assert_eq!(StatusCode::OK, status, "{path}: {body}");
        assert!(
            body["entry"].as_array().is_none_or(Vec::is_empty),
            "{path}: {body}"
        );
    }
    Ok(())
}

/// R4 search.html §3.1.1.5.3: "if `_count` has the value 0, this shall be
/// treated the same as `_summary=count`: the server returns a bundle that
/// reports the total number of resources that match in Bundle.total, but
/// with no entries, and no prev/next/last links."
#[tokio::test]
async fn a_count_of_zero_answers_the_total_and_no_entry() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    mount_rows(&cdr, &[UID, OTHER]).await;
    mount_party(&cdr, "person", &person(UID), UID).await;
    let (status, body) = get(&facade, "Patient?identifier=SYN-0001&_count=0").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(body["total"], 2, "{body}");
    assert!(body.get("entry").is_none(), "{body}");
    let relations: Vec<&str> = body["link"]
        .as_array()
        .ok_or("links")?
        .iter()
        .filter_map(|link| link["relation"].as_str())
        .collect();
    assert_eq!(relations, ["self"], "{body}");
    let sent = queries(&cdr).await;
    assert!(
        sent[0].get("fetch").is_none_or(serde_json::Value::is_null),
        "{sent:?}"
    );
    assert_eq!(party_reads(&cdr).await, 0, "a count reads no party");
    let (status, body) = get(&facade, &format!("Patient?_id={UID}&_count=0")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(body["total"], 1, "{body}");
    Ok(())
}

/// R4 search.html §3.1.1.5.3: "Servers SHALL NOT return more resources than
/// requested", and the facade answers no more than its largest page.
#[tokio::test]
async fn a_count_above_the_largest_page_is_capped() -> Result<(), Box<dyn Error>> {
    let (cdr, facade) = harness(Some(searching(&["Patient"]))).await?;
    mount_rows(&cdr, &[]).await;
    let (status, body) = get(&facade, "Patient?identifier=SYN-0001&_count=100000").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let largest = ferrobridge_server::facade::search::Limits::default().max_count;
    assert_eq!(queries(&cdr).await[0]["fetch"], largest + 1);
    Ok(())
}
