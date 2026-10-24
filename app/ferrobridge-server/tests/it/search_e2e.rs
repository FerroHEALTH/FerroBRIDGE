// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Search against a real CDR, behind the `FERROBRIDGE_E2E` gate.
//!
//! The `wiremock` suite pins the AQL each parameter writes and every refusal;
//! this case proves the projection runs on a CDR: two synthetic `Condition`s
//! commit through the facade, and `GET Condition?patient=…` pages them back
//! across a page boundary in a stable order, mapped through the same program.

use axum::Router;
use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::Settings;
use ferrobridge_server::facade::ehr::Policy;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::search;
use ferrobridge_server::health::Registry;
use ferrobridge_server::state::AppState;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::fixtures;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error;
use std::sync::Arc;
use tower::ServiceExt as _;

/// The fixture directory the case loads its mapping set and its search
/// declaration from.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures");

/// The profile the fixture context claims.
const PROFILE: &str = "http://example.org/fhir/StructureDefinition/ferrobridge-facade-diagnosis";

/// The namespace a literal subject reference is keyed in.
const NAMESPACE: &str = "http://example.org/fhir";

/// The FHIR service base the facade answers under.
const BASE: &str = "http://ferrobridge.invalid/fhir/";

#[tokio::test]
async fn a_patient_search_pages_the_committed_conditions_back_from_a_real_cdr()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let cdr = containers::cdr().await?;
    crate::facade_e2e::upload_template(cdr.base_url()).await?;
    let client = CdrClient::new(&CdrConfig::new(cdr.base_url().parse()?))?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let catalog = search::load(std::path::Path::new(FIXTURES), &compiled)?;
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let facade = Arc::new(
        Facade::new(
            compiled,
            store,
            client,
            Settings {
                base_url: String::from(BASE.trim_end_matches('/')),
                ehr_policy: Policy::CreateOnFirstWrite,
                subject: crate::support::subject_link(NAMESPACE),
                system_id: String::from("ferrobridge.e2e"),
                language: String::from("en"),
                territory: String::from("GB"),
                ehr_status_system: None,
            },
        )
        .with_search(catalog, search::Limits::default()),
    );

    let mut created = Vec::new();
    for (sender, code) in [("e2e-search-1", "SYN-001"), ("e2e-search-2", "SYN-002")] {
        let (status, body) = call(app(&facade), post_condition(sender, code)?).await?;
        assert_eq!(StatusCode::CREATED, status, "{body}");
        created.push(
            body["id"]
                .as_str()
                .ok_or("the create carries an id")?
                .to_owned(),
        );
    }

    let (status, first) = get(&facade, "Condition?patient=e2e-search-subject&_count=1").await?;
    assert_eq!(StatusCode::OK, status, "{first}");
    assert_eq!(1, matched(&first).len(), "{first}");
    let next = link(&first, "next").ok_or("two matches over a page of one leave a next page")?;
    let (status, second) = get(&facade, next.strip_prefix(BASE).unwrap_or(next)).await?;
    assert_eq!(StatusCode::OK, status, "{second}");
    assert_eq!(1, matched(&second).len(), "{second}");
    assert_eq!(None, link(&second, "next"));
    let mut found: Vec<String> = [&first, &second]
        .iter()
        .flat_map(|page| matched(page))
        .filter_map(|id| id.map(str::to_owned))
        .collect();
    found.sort();
    created.sort();
    assert_eq!(
        created, found,
        "the pages answer each committed Condition once"
    );

    let (status, all) = get(&facade, "Condition?patient=e2e-search-subject").await?;
    assert_eq!(StatusCode::OK, status, "{all}");
    assert_eq!(Some(2), all["total"].as_u64(), "{all}");

    let (status, none) = get(&facade, "Condition?patient=nobody").await?;
    assert_eq!(StatusCode::OK, status, "{none}");
    assert_eq!(Some(0), none["total"].as_u64());

    let (status, refused) = get(&facade, "Condition?patient=e2e-search-subject&severity=x").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{refused}");
    Ok(())
}

/// The declaration of the KDS search case: the coded diagnosis, read under
/// the repeated `problem_diagnosis` entry.
const KDS_SEARCH: &str = "context: ferrobridge_kds_diagnose.context\nparameters:\n  code:\n    type: token\n    node: { class: EVALUATION, archetype: openEHR-EHR-EVALUATION.problem_diagnosis.v1 }\n    path: \"data[at0001]/items[at0002,'Kodierte Diagnose']/value\"\n";

#[tokio::test]
async fn a_crossed_code_and_system_pair_matches_no_composition_on_a_real_cdr()
-> Result<(), Box<dyn Error>> {
    // #526 and R4 search.html#token: `[system]|[code]` matches one Coding, so a
    // composition holding diagnosis 1 (S1, X) and diagnosis 2 (S2, Y) answers
    // `S2|X` with nothing, and each diagnosis that matches is one resource.
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let cdr = containers::cdr().await?;
    crate::facade_e2e::upload_kds_template(cdr.base_url()).await?;
    let client = CdrClient::new(&CdrConfig::new(cdr.base_url().parse()?))?;
    let directory = tempfile::tempdir()?;
    crate::kds::write_mappings(directory.path())?;
    std::fs::write(directory.path().join("kds.search.yml"), KDS_SEARCH)?;
    let set = programs::read_set(directory.path())?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let catalog = search::load(directory.path(), &compiled)?;
    let memory = Arc::new(MemoryStore::new());
    let store: Arc<dyn Store> = memory.clone();
    let facade = Arc::new(
        Facade::new(
            compiled,
            store,
            client.clone(),
            Settings {
                base_url: String::from(BASE.trim_end_matches('/')),
                ehr_policy: Policy::CreateOnFirstWrite,
                subject: crate::support::subject_link(NAMESPACE),
                system_id: String::from("ferrobridge.e2e"),
                language: String::from("de"),
                territory: String::from("DE"),
                ehr_status_system: None,
            },
        )
        .with_search(catalog, search::Limits::default()),
    );

    let input: serde_json::Value = serde_json::from_str(fixtures::KDS_DIAGNOSE_CONDITION)
        .map_err(|error| format!("the KDS Condition fixture is not JSON: {error}"))?;
    let (status, created) = call(
        app(&facade),
        Request::post("/fhir/Condition")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(input.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "{created}");
    let id = ferrobridge_server::facade::identity::FhirResourceId::new(
        created["id"].as_str().ok_or("the create carries an id")?,
    )?;
    let binding = memory
        .binding_of("Condition", &id)?
        .ok_or("the create recorded its binding")?;
    let ehr_id = ferrobridge_server::cdr::ids::EhrId::new(&binding.ehr_id)?;
    let (system_one, code_one) =
        commit_two_diagnoses(&client, &ehr_id, &binding.versioned_object_uid).await?;

    let search = |system: &str, code: &str| {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("code", &format!("{system}|{code}"))
            .finish();
        format!("Condition?{query}")
    };
    let (status, crossed) = get(&facade, &search("SYNTHETIC-B", &code_one)).await?;
    assert_eq!(StatusCode::OK, status, "{crossed}");
    assert_eq!(
        Some(0),
        crossed["total"].as_u64(),
        "the crossed pair matched: {crossed}"
    );
    let (status, own) = get(&facade, &search("SYNTHETIC-B", "SYN-B2")).await?;
    assert_eq!(StatusCode::OK, status, "{own}");
    assert_eq!(Some(1), own["total"].as_u64(), "{own}");
    // #530 and R4 search.html §3.1.1: the match on the second diagnosis
    // answers that diagnosis, with an id of its own.
    let second = own["entry"][0]["resource"].clone();
    assert_eq!(
        Some("SYN-B2"),
        second["code"]["coding"][0]["code"].as_str(),
        "the match on the second diagnosis answered another: {own}"
    );
    let second_id = second["id"].as_str().ok_or("the match carries an id")?;
    assert_ne!(id.as_str(), second_id, "two entries answered one id");
    let (status, read) = get(&facade, &format!("Condition/{second_id}")).await?;
    assert_eq!(StatusCode::OK, status, "{read}");
    assert_eq!(
        Some("SYN-B2"),
        read["code"]["coding"][0]["code"].as_str(),
        "{read}"
    );
    let (status, both) = get(&facade, &search(&system_one, &code_one)).await?;
    assert_eq!(StatusCode::OK, status, "{both}");
    assert_eq!(Some(2), both["total"].as_u64(), "{both}");
    let either = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("code", &format!("{code_one},SYN-B2"))
        .finish();
    let (status, either) = get(&facade, &format!("Condition?{either}")).await?;
    assert_eq!(StatusCode::OK, status, "{either}");
    assert_eq!(
        Some(3),
        either["total"].as_u64(),
        "each matching diagnosis is one resource: {either}"
    );
    assert_eq!(3, matched(&either).len(), "{either}");
    Ok(())
}

/// Reads the composition `versioned_object_uid` holds, commits a copy that
/// adds a second diagnosis coded `SYNTHETIC-B|SYN-B2`, and returns the first
/// diagnosis's terminology id and code.
async fn commit_two_diagnoses(
    client: &CdrClient,
    ehr_id: &ferrobridge_server::cdr::ids::EhrId,
    versioned_object_uid: &str,
) -> Result<(String, String), Box<dyn Error>> {
    let answered = client
        .composition(
            ehr_id,
            &openehr_base::v1_3::base_types::identification::uid_based_id::UidBasedId::HierObjectId(
                openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId::new(
                    versioned_object_uid,
                )?,
            ),
            None,
        )
        .await?;
    let read_status = answered.upstream.status();
    let openehr_its::rest::generated::ehr::client::CompositionGetOutcome::Ok { body, .. } =
        answered.outcome
    else {
        return Err(format!(
            "GET composition {versioned_object_uid} answered {read_status}, not the committed composition"
        )
        .into());
    };
    let mut document: serde_json::Value =
        serde_json::from_str(&openehr_its::json::to_canonical_json(&body)).map_err(|error| {
            format!("the composition the CDR served does not render as canonical JSON: {error}")
        })?;
    let first = document["content"][0].clone();
    let (system_one, code_one) = coded(&first).ok_or("the first diagnosis is coded")?;
    let mut second = first.clone();
    recode(&mut second, "SYNTHETIC-B", "SYN-B2").ok_or("the second diagnosis is coded")?;
    // The copy is a second entry, so it carries no `LOCATABLE.uid` of the first.
    if let Some(entry) = second.as_object_mut() {
        entry.remove("uid");
    }
    if let Some(content) = document["content"].as_array_mut() {
        content.push(second);
    }
    if let Some(object) = document.as_object_mut() {
        object.remove("uid");
    }
    let two = openehr_its::json::from_canonical_json::<
        openehr_rm::v1_2::composition::composition::Composition,
    >(&document.to_string())
    .map_err(|error| {
        format!("the composition with two diagnoses is no canonical COMPOSITION: {error}")
    })?;
    let commit = ferrobridge_server::facade::commit::context(
        ferrobridge_server::facade::commit::Change::Creation,
        "ferrobridge.e2e",
        fixtures::KDS_DIAGNOSE_TEMPLATE_ID,
    )?;
    let committed = client
        .create_composition(
            ehr_id,
            &two,
            &commit,
            ferrobridge_server::cdr::Prefer::Minimal,
        )
        .await?;
    assert!(
        matches!(
            committed.outcome,
            openehr_its::rest::generated::ehr::client::CompositionCreateOutcome::Created { .. }
        ),
        "POST composition answered {}: {}, not the commit of the composition with two diagnoses",
        committed.upstream.status(),
        committed.upstream.body()
    );
    Ok((system_one, code_one))
}

/// Returns the item of a diagnosis entry that carries the coded diagnosis.
fn diagnosis(entry: &serde_json::Value) -> Option<&serde_json::Value> {
    entry["data"]["items"]
        .as_array()?
        .iter()
        .find(|item| item["archetype_node_id"].as_str() == Some("at0002"))
}

/// Returns the terminology id and the code of a diagnosis entry.
fn coded(entry: &serde_json::Value) -> Option<(String, String)> {
    let code = &diagnosis(entry)?["value"]["defining_code"];
    Some((
        code["terminology_id"]["value"].as_str()?.to_owned(),
        code["code_string"].as_str()?.to_owned(),
    ))
}

/// Recodes a diagnosis entry to `system` and `code`.
fn recode(entry: &mut serde_json::Value, system: &str, code: &str) -> Option<()> {
    let items = entry["data"]["items"].as_array_mut()?;
    let item = items
        .iter_mut()
        .find(|item| item["archetype_node_id"].as_str() == Some("at0002"))?;
    item["value"]["defining_code"]["terminology_id"]["value"] = serde_json::json!(system);
    item["value"]["defining_code"]["code_string"] = serde_json::json!(code);
    item["value"]["value"] = serde_json::json!("Synthetic diagnosis two");
    Some(())
}

/// Returns a create of the synthetic `Condition` under the sender id
/// `sender`, coded `code`, its subject a literal reference.
fn post_condition(sender: &str, code: &str) -> Result<Request<Body>, Box<dyn Error>> {
    let mut resource: serde_json::Value = serde_json::from_str(fixtures::R4_CONDITION)
        .map_err(|error| format!("the synthetic Condition fixture is not JSON: {error}"))?;
    resource["id"] = serde_json::json!(sender);
    resource["meta"] = serde_json::json!({ "profile": [PROFILE] });
    resource["code"]["coding"][0]["code"] = serde_json::json!(code);
    resource["subject"] = serde_json::json!({ "reference": "Patient/e2e-search-subject" });
    Ok(Request::post("/fhir/Condition")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(resource.to_string()))?)
}

/// Returns the logical id of every match of a searchset.
fn matched(body: &serde_json::Value) -> Vec<Option<&str>> {
    body["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["search"]["mode"].as_str() == Some("match"))
                .map(|entry| entry["resource"]["id"].as_str())
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the `url` of the link whose relation is `relation`.
fn link<'a>(body: &'a serde_json::Value, relation: &str) -> Option<&'a str> {
    body["link"]
        .as_array()?
        .iter()
        .find(|entry| entry["relation"].as_str() == Some(relation))
        .and_then(|entry| entry["url"].as_str())
}

/// Sends `GET /fhir/{path_and_query}`.
async fn get(
    facade: &Arc<Facade>,
    path_and_query: &str,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    call(
        app(facade),
        Request::get(format!("/fhir/{path_and_query}")).body(Body::empty())?,
    )
    .await
}

/// Returns the application over `facade`.
///
/// The body ceiling is the facade suite's, 1 MiB: the shared test settings
/// cap a body at 1024 bytes, which the KDS `Condition` exceeds, so its create
/// would meet the `413` of the ceiling rather than the facade.
fn app(facade: &Arc<Facade>) -> Router {
    let state = AppState::with_health(Registry::default()).with_facade(Arc::clone(facade));
    ferrobridge_server::router(
        Arc::new(state),
        &ferrobridge_server::config::ServerSettings {
            body_limit: 1024 * 1024,
            request_timeout: core::time::Duration::from_secs(30),
            ..crate::support::settings()
        },
    )
}

/// Sends `request` and reads the status and the JSON body.
///
/// A body that is not JSON is an error naming the call, the status, the
/// `Content-Type` and the start of the body, so a failure says which answer
/// it met.
async fn call(
    app: Router,
    request: Request<Body>,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    let call = format!("{} {}", request.method(), request.uri());
    let response = app.oneshot(request).await?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("no Content-Type")
        .to_owned();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let body = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "{call} answered {status} ({content_type}) with a body that is not JSON ({error}): {:?}",
            String::from_utf8_lossy(bytes.get(..bytes.len().min(200)).unwrap_or_default())
        )
    })?;
    Ok((status, body))
}
