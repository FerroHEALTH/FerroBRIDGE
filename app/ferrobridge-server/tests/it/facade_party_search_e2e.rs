// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The party search through the facade against the reference CDR, which
//! serves Cadasto's experimental AQL demographic extension, behind the
//! `FERROBRIDGE_E2E` gate.
//!
//! The parties are created through the facade over the ITS-REST 1.1.0
//! Demographic API, DEVELOPMENT, and then found again by `identifier`,
//! `name`, `_id` and `_count` with queries rooted at `PERSON`,
//! `ORGANISATION` and `ROLE`, the roots the reference CDR serves. A query
//! rooted at a class it refuses answers as the facade documents a CDR that
//! does not serve the query: `501 not-supported`. The R4 search rules each
//! case encodes are cited on the case
//! (<https://hl7.org/fhir/R4/search.html>). No specification governs a party
//! search beyond them: our own design.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::error::Error as StdError;
use std::sync::Arc;

use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::Settings;
use ferrobridge_server::facade::ehr::Policy;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::search::identifier::EXTENSION;
use ferrobridge_server::facade::search::identifier::IdentifierSearch;
use ferrobridge_server::facade::search::party::NameSearch;
use ferrobridge_server::facade::search::party::PartyQueries;
use ferrobridge_server::facade::search::party::PartySearch;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::containers::Cdr;
use ferrobridge_testkit::fixtures;
use http::Request;
use http::StatusCode;

use crate::facade_demographic_e2e::with_body;
use crate::facade_e2e::app;
use crate::facade_e2e::call;
use crate::facade_e2e::upload_published;

/// The demographic fixture mapping set.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures_demographic");

/// The base URL the facade writes into its links.
const BASE_URL: &str = "http://ferrobridge.invalid/fhir";

/// The identifier system every synthetic party carries.
const SYSTEM: &str = "http://example.org/fhir/sid/synthetic";

/// The identifier the two synthetic patients share.
const PATIENT_IDENTIFIER: &str = "SYN-PAT-0001";

/// The identifier of the synthetic practitioner.
const PRACTITIONER_IDENTIFIER: &str = "SYN-PRA-0001";

/// The identifier of the synthetic organization.
const ORGANIZATION_IDENTIFIER: &str = "SYN-ORG-0001";

/// The identifier of the synthetic practitioner role.
const ROLE_IDENTIFIER: &str = "SYN-ROL-0001";

/// The resource types the fixture set serves, with the class each is rooted
/// at.
const SEARCHED: [(&str, &str); 4] = [
    ("Patient", "PERSON"),
    ("Practitioner", "PERSON"),
    ("Organization", "ORGANISATION"),
    ("PractitionerRole", "ROLE"),
];

/// Returns the identifier query rooted at `class`, over the details
/// identifier of the testkit's party templates.
fn query_of(class: &str) -> String {
    format!(
        "SELECT p/uid/value FROM {class} p WHERE p/details[at0001]/items[at0002]/value/id = \
         $identifier ORDER BY p/uid/value ASC"
    )
}

/// Returns the name query rooted at `class`, comparing the identity name with
/// the three case variants of the value as prefix patterns.
fn name_query_of(class: &str) -> String {
    format!(
        "SELECT p/uid/value FROM {class} p WHERE \
         p/identities[at0003]/details[at0004]/items[at0005]/value/value LIKE $name OR \
         p/identities[at0003]/details[at0004]/items[at0005]/value/value LIKE $name_lower OR \
         p/identities[at0003]/details[at0004]/items[at0005]/value/value LIKE $name_capitalised \
         ORDER BY p/uid/value ASC"
    )
}

/// Returns the search of every `(type, class)` pair, with a `name` query on
/// `Patient` and `Practitioner`, the two R4 types of this search that define
/// `name` over a person (<https://hl7.org/fhir/R4/patient.html#search>).
fn searching(types: &[(&str, &str)]) -> Result<PartySearch, Box<dyn StdError>> {
    let mut queries = BTreeMap::new();
    for &(resource_type, class) in types {
        let name = matches!(resource_type, "Patient" | "Practitioner")
            .then(|| NameSearch::new(&name_query_of(class)))
            .transpose()?;
        let identifier = IdentifierSearch::new(&query_of(class), None)?;
        queries.insert(
            String::from(resource_type),
            PartyQueries::new(Some(identifier), name),
        );
    }
    Ok(PartySearch::new(queries))
}

/// Starts the reference CDR and uploads the testkit's party templates.
async fn started() -> Result<Cdr, Box<dyn StdError>> {
    let cdr = containers::cdr().await?;
    for opt in [
        fixtures::PERSON_OPT,
        fixtures::ORGANISATION_OPT,
        fixtures::ROLE_OPT,
    ] {
        upload_published(cdr.base_url(), opt).await?;
    }
    Ok(cdr)
}

/// Returns a facade over `cdr` searching as `search` configures.
async fn facade_over(cdr: &Cdr, search: PartySearch) -> Result<Arc<Facade>, Box<dyn StdError>> {
    let client = CdrClient::new(&CdrConfig::new(cdr.base_url().parse()?).with_demographic(true))?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    programs::check_parties(&compiled, true)?;
    programs::check_party_search(&compiled, Some(&search))?;
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let settings = Settings {
        base_url: String::from(BASE_URL),
        ehr_policy: Policy::Existing,
        subject: crate::support::subject_link("http://example.org/fhir"),
        system_id: String::from("ferrobridge.e2e"),
        language: String::from("en"),
        territory: String::from("GB"),
        ehr_status_system: None,
    };
    Ok(Arc::new(
        Facade::new(compiled, store, client, settings).with_party_search(Some(search)),
    ))
}

/// Returns a `Patient` or `Practitioner` carrying `identifier` and `name`.
fn person(resource_type: &str, identifier: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "resourceType": resource_type,
        "identifier": [{"system": SYSTEM, "value": identifier}],
        "name": [{"text": name}]
    })
}

/// Creates `body` through the facade and returns the id it was given.
async fn create(
    facade: &Arc<Facade>,
    body: &serde_json::Value,
) -> Result<String, Box<dyn StdError>> {
    let resource_type = body["resourceType"].as_str().ok_or("a resource type")?;
    let created = call(
        app(facade),
        with_body(
            http::Method::POST,
            &format!("/fhir/{resource_type}"),
            body,
            None,
        )?,
    )
    .await?;
    assert_eq!(
        StatusCode::CREATED,
        created.0,
        "{resource_type}: {}",
        created.1
    );
    Ok(created.1["id"]
        .as_str()
        .ok_or("the created resource carries an id")?
        .to_owned())
}

/// Sends `GET /fhir/{path}`.
async fn get(
    facade: &Arc<Facade>,
    path: &str,
) -> Result<(StatusCode, serde_json::Value), Box<dyn StdError>> {
    call(
        app(facade),
        Request::get(format!("/fhir/{path}")).body(Body::empty())?,
    )
    .await
}

/// Runs a search that has to succeed and returns its searchset.
async fn searchset(
    facade: &Arc<Facade>,
    path: &str,
) -> Result<serde_json::Value, Box<dyn StdError>> {
    let (status, body) = get(facade, path).await?;
    assert_eq!(StatusCode::OK, status, "{path}: {body}");
    assert_eq!(body["type"], "searchset", "{path}: {body}");
    Ok(body)
}

/// Returns the `(type, id)` of every entry of `searchset`.
fn found(searchset: &serde_json::Value) -> BTreeSet<(String, String)> {
    searchset["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    (
                        entry["resource"]["resourceType"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                        entry["resource"]["id"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the set holding `id` of `resource_type` for each of `ids`.
fn of(resource_type: &str, ids: &[&str]) -> BTreeSet<(String, String)> {
    ids.iter()
        .map(|id| (String::from(resource_type), String::from(*id)))
        .collect()
}

/// Returns the path of the `next` link of `searchset`, if it has one.
fn next_of(searchset: &serde_json::Value) -> Option<String> {
    searchset["link"]
        .as_array()?
        .iter()
        .find(|link| link["relation"] == "next")?["url"]
        .as_str()?
        .strip_prefix(BASE_URL)
        .map(|path| path.trim_start_matches('/').to_owned())
}

/// The ids of the parties the search test creates.
struct Seeded {
    /// The first patient, `Alpha Synthetic`.
    alpha: String,
    /// The second patient, `Bravo Synthetic`, sharing the first one's
    /// identifier.
    bravo: String,
    /// The practitioner, `Charlie Synthetic`.
    charlie: String,
    /// The organization.
    organization: String,
    /// The practitioner role, whose practitioner is `charlie`.
    role: String,
}

/// Creates the synthetic parties through the facade.
async fn seed(facade: &Arc<Facade>) -> Result<Seeded, Box<dyn StdError>> {
    let alpha = create(
        facade,
        &person("Patient", PATIENT_IDENTIFIER, "Alpha Synthetic"),
    )
    .await?;
    let bravo = create(
        facade,
        &person("Patient", PATIENT_IDENTIFIER, "Bravo Synthetic"),
    )
    .await?;
    let charlie = create(
        facade,
        &person("Practitioner", PRACTITIONER_IDENTIFIER, "Charlie Synthetic"),
    )
    .await?;
    let organization = create(
        facade,
        &serde_json::json!({
            "resourceType": "Organization",
            "identifier": [{"system": SYSTEM, "value": ORGANIZATION_IDENTIFIER}],
            "name": "Delta Synthetic"
        }),
    )
    .await?;
    let role = create(
        facade,
        &serde_json::json!({
            "resourceType": "PractitionerRole",
            "identifier": [{"system": SYSTEM, "value": ROLE_IDENTIFIER}],
            "code": [{"text": "Echo Synthetic"}],
            "practitioner": {"reference": format!("Practitioner/{charlie}")}
        }),
    )
    .await?;
    Ok(Seeded {
        alpha,
        bravo,
        charlie,
        organization,
        role,
    })
}

#[tokio::test]
async fn the_party_searches_find_the_parties_the_reference_cdr_holds()
-> Result<(), Box<dyn StdError>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let cdr = started().await?;
    let facade = facade_over(&cdr, searching(&SEARCHED)?).await?;
    let Seeded {
        alpha,
        bravo,
        charlie,
        organization,
        role,
    } = seed(&facade).await?;

    // NOTE: R4 search.html §3.1.1.4.10, a bare token value matches whatever the
    // system, so both patients carrying the value are found.
    for (path, expected) in [
        (
            format!("Patient?identifier={PATIENT_IDENTIFIER}"),
            of("Patient", &[&alpha, &bravo]),
        ),
        (
            format!("Practitioner?identifier={PRACTITIONER_IDENTIFIER}"),
            of("Practitioner", &[&charlie]),
        ),
        (
            format!("Organization?identifier={ORGANIZATION_IDENTIFIER}"),
            of("Organization", &[&organization]),
        ),
        (
            format!("PractitionerRole?identifier={ROLE_IDENTIFIER}"),
            of("PractitionerRole", &[&role]),
        ),
        (
            String::from("Patient?identifier=SYN-NONE-0001"),
            BTreeSet::new(),
        ),
    ] {
        let body = searchset(&facade, &path).await?;
        assert_eq!(expected, found(&body), "{path}: {body}");
    }

    // NOTE: R4 search.html §3.1.1.4.8, a string matches a name that starts with
    // the value after both are normalized by case.
    for (path, expected) in [
        (String::from("Patient?name=alpha"), of("Patient", &[&alpha])),
        (String::from("Patient?name=BRAVO"), of("Patient", &[&bravo])),
        (
            String::from("Practitioner?name=Charlie%20Syn"),
            of("Practitioner", &[&charlie]),
        ),
        (String::from("Patient?name=zulu"), BTreeSet::new()),
    ] {
        let body = searchset(&facade, &path).await?;
        assert_eq!(expected, found(&body), "{path}: {body}");
    }

    for (resource_type, id) in [
        ("Patient", &alpha),
        ("Practitioner", &charlie),
        ("Organization", &organization),
        ("PractitionerRole", &role),
    ] {
        let path = format!("{resource_type}?_id={id}");
        let body = searchset(&facade, &path).await?;
        assert_eq!(of(resource_type, &[id]), found(&body), "{path}: {body}");
    }

    // NOTE: R4 search.html §3.1.1.5.3, `_count=0` is treated as `_summary=count`,
    // so the searchset carries the total and no entry.
    let counted = searchset(
        &facade,
        &format!("Patient?identifier={PATIENT_IDENTIFIER}&_count=0"),
    )
    .await?;
    assert_eq!(counted["total"], 2, "{counted}");
    assert!(found(&counted).is_empty(), "{counted}");

    let first = searchset(
        &facade,
        &format!("Patient?identifier={PATIENT_IDENTIFIER}&_count=1"),
    )
    .await?;
    assert_eq!(found(&first).len(), 1, "{first}");
    let next = next_of(&first).ok_or("the first page links the next one")?;
    let second = searchset(&facade, &next).await?;
    assert_eq!(found(&second).len(), 1, "{second}");
    assert_eq!(next_of(&second), None, "{second}");
    let paged: BTreeSet<_> = found(&first).union(&found(&second)).cloned().collect();
    assert_eq!(of("Patient", &[&alpha, &bravo]), paged);

    let deleted = call(
        app(&facade),
        Request::delete(format!("/fhir/Patient/{bravo}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::NO_CONTENT, deleted.0, "{}", deleted.1);
    let after = searchset(&facade, &format!("Patient?identifier={PATIENT_IDENTIFIER}")).await?;
    assert_eq!(of("Patient", &[&alpha]), found(&after), "{after}");
    Ok(())
}

/// RFC 9110 §15.6.2: the reference CDR refuses a query rooted at `GROUP`,
/// `AGENT` or `PARTY_RELATIONSHIP`, so the facade cannot serve a search
/// configured over one and answers `501 not-supported` naming the extension,
/// never an empty searchset.
#[tokio::test]
async fn a_root_the_reference_cdr_refuses_answers_501() -> Result<(), Box<dyn StdError>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let cdr = started().await?;
    for class in ["GROUP", "AGENT", "PARTY_RELATIONSHIP"] {
        let facade = facade_over(&cdr, searching(&[("Organization", class)])?).await?;
        let (status, body) = get(
            &facade,
            &format!("Organization?identifier={ORGANIZATION_IDENTIFIER}"),
        )
        .await?;
        assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "{class}: {body}");
        assert_eq!(body["resourceType"], "OperationOutcome", "{class}: {body}");
        assert_eq!(body["issue"][0]["code"], "not-supported", "{class}: {body}");
        let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap_or_default();
        assert!(diagnostics.contains(EXTENSION), "{class}: {diagnostics}");
    }
    Ok(())
}
