// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FHIR `Patient` through the facade: written as the `KDS_Person`
//! composition the KDS person chain maps it to, and read back from it.

use axum::body::Body;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::Settings;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error;
use std::sync::Arc;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::EHR_ID;
use super::Harness;
use super::VERSION_ONE;
use super::call;
use super::client;
use super::get_tagged;
use super::handle;
use super::settings;
use super::stub::mount_create;
use super::stub::mount_ehr;
use super::stub::mount_read;

/// The context the person mapping set compiles under.
const CONTEXT: &str = "ferrobridge_kds_person.context";

/// Returns a harness whose facade serves the KDS person mapping set over a
/// CDR that serves the published `KDS_Person` template.
async fn person_harness() -> Result<(Harness, tempfile::TempDir), Box<dyn Error>> {
    person_harness_with(settings()).await
}

/// Returns the harness of [`person_harness`] with the facade settings
/// `facade_settings`.
async fn person_harness_with(
    facade_settings: Settings,
) -> Result<(Harness, tempfile::TempDir), Box<dyn Error>> {
    let cdr = MockServer::start().await;
    let facade_settings = crate::support::served_identity(&cdr, facade_settings).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "/definition/template/adl1.4/{}",
            ferrobridge_testkit::fixtures::KDS_PERSON_TEMPLATE_ID
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/xml")
                .set_body_string(ferrobridge_testkit::fixtures::KDS_PERSON_OPT),
        )
        .mount(&cdr)
        .await;
    let directory = tempfile::tempdir()?;
    crate::kds::write_person_mappings(directory.path())?;
    let set = programs::read_set(directory.path())?;
    let templates = programs::fetch_templates(&set, &client(&cdr)).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let store = Arc::new(MemoryStore::default());
    let facade = Arc::new(Facade::new(
        compiled,
        handle(&store),
        client(&cdr),
        facade_settings,
    ));
    Ok((Harness { cdr, store, facade }, directory))
}

/// Returns the synthetic MII Person `Patient` of the testkit.
fn patient() -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(serde_json::from_str(
        ferrobridge_testkit::fixtures::KDS_PERSON_PATIENT,
    )?)
}

/// Returns the canonical composition `resource` maps to under the person
/// context.
fn person_composition(
    harness: &Harness,
    resource: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let loaded = harness
        .facade
        .programs()
        .by_context(CONTEXT)
        .cloned()
        .ok_or("the person context compiled")?;
    let document: fhir_types::codec::Value = serde_json::from_value(resource.clone())?;
    let built = ferrobridge_server::facade::engine::inbound(
        loaded.program(),
        loaded.index(),
        &document,
        "2026-09-15T09:00:00Z",
        &settings(),
        &fhirconnect::engine::terminology::NoTerminology,
        &fhirconnect::engine::demographics::IssuerDemographics::new(crate::facade::BASE_URL)
            .expect("the test base issues identities"),
    )?;
    Ok(built.into_value().into_value())
}

/// Returns a `POST [base]/Patient` carrying `body`.
fn post_patient(body: &serde_json::Value) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post("/fhir/Patient")
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(body.to_string()))?)
}

#[tokio::test]
async fn a_patient_is_stored_as_the_kds_person_composition_of_its_own_ehr()
-> Result<(), Box<dyn Error>> {
    let (harness, _directory) = person_harness().await?;
    let resource = patient()?;
    assert_eq!(
        Some(crate::kds::PERSON_PROFILE),
        resource["meta"]["profile"][0].as_str(),
        "the synthetic Patient claims the profile the person context maps"
    );
    mount_ehr(&harness.cdr).await;
    mount_create(
        &harness.cdr,
        VERSION_ONE,
        &person_composition(&harness, &resource)?,
    )
    .await;
    let (status, body) = call(harness.app(), post_patient(&resource)?).await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");

    let lookup = harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|request| request.method.as_str() == "GET" && request.url.path() == "/ehr")
        .ok_or("the facade looked the EHR up by subject")?;
    let query: Vec<(String, String)> = lookup.url.query_pairs().into_owned().collect();
    assert!(
        query.contains(&(
            String::from("subject_id"),
            String::from("synthetic-patient-0001")
        )),
        "the Patient is keyed by its first identifier: {query:?}"
    );
    assert!(
        query.contains(&(
            String::from("subject_namespace"),
            String::from("http://example.org/fhir/sid/ferrobridge-patient")
        )),
        "{query:?}"
    );

    let sent = harness
        .sent("POST", &format!("/ehr/{EHR_ID}/composition"))
        .await
        .ok_or("the create sent a composition")?;
    assert_eq!(
        Some("KDS_Person"),
        sent["archetype_details"]["template_id"]["value"].as_str(),
        "{sent}"
    );
    let text = sent.to_string();
    for value in ["Synthetisch", "Beispiel", "female", "1970-01-01"] {
        assert!(text.contains(value), "the composition carries {value}");
    }
    Ok(())
}

#[tokio::test]
async fn a_stored_kds_person_composition_reads_back_as_the_patient() -> Result<(), Box<dyn Error>> {
    let (harness, _directory) = person_harness().await?;
    let resource = patient()?;
    mount_ehr(&harness.cdr).await;
    mount_create(
        &harness.cdr,
        VERSION_ONE,
        &person_composition(&harness, &resource)?,
    )
    .await;
    let (status, created) = call(harness.app(), post_patient(&resource)?).await?;
    assert_eq!(StatusCode::CREATED, status, "{created}");
    let id = created["id"].as_str().ok_or("the create answers an id")?;
    let stored = harness
        .sent("POST", &format!("/ehr/{EHR_ID}/composition"))
        .await
        .ok_or("the create sent a composition")?;
    mount_read(&harness.cdr, VERSION_ONE, &stored).await;

    let (status, tag, read) = get_tagged(&harness, &format!("/fhir/Patient/{id}")).await?;
    assert_eq!(StatusCode::OK, status, "{read}");
    assert_eq!(Some("W/\"1\""), tag.as_deref());
    assert_eq!(Some("Patient"), read["resourceType"].as_str());
    assert_eq!(Some(id), read["id"].as_str());
    assert_eq!(Some("female"), read["gender"].as_str(), "{read}");
    assert_eq!(Some("1970-01-01"), read["birthDate"].as_str(), "{read}");
    assert_eq!(Some(false), read["deceasedBoolean"].as_bool(), "{read}");
    assert_eq!(
        Some("Synthetic managing organization"),
        read["managingOrganization"]["display"].as_str(),
        "{read}"
    );
    let names = read["name"].as_array().ok_or("the Patient reads names")?;
    let official = names
        .iter()
        .find(|name| name["use"] == "official")
        .ok_or("the official name reads back")?;
    assert_eq!(Some("Synthetisch"), official["family"].as_str(), "{read}");
    assert_eq!(Some("Erika"), official["given"][0].as_str(), "{read}");
    assert_eq!(Some("Dr."), official["prefix"][0].as_str(), "{read}");
    let maiden = names
        .iter()
        .find(|name| name["use"] == "maiden")
        .ok_or("the maiden name reads back")?;
    assert_eq!(Some("Beispiel"), maiden["family"].as_str(), "{read}");
    assert_eq!(
        Some("synthetic-patient-0001"),
        read["identifier"][0]["value"].as_str(),
        "{read}"
    );
    Ok(())
}

/// Posts `resource` and returns the status, the body, and the query of the
/// EHR lookup by subject when there was one, with the identity source
/// answering as `pix` says when it is set.
async fn post_patient_with(
    resource: &serde_json::Value,
    pix: Option<ResponseTemplate>,
) -> Result<(Harness, StatusCode, serde_json::Value, Vec<(String, String)>), Box<dyn Error>> {
    let (harness, _directory) = person_harness().await?;
    if let Some(answer) = pix {
        Mock::given(matchers::method("POST"))
            .and(matchers::path(crate::support::PIX_PATH))
            .respond_with(answer)
            .with_priority(1)
            .mount(&harness.cdr)
            .await;
    }
    mount_ehr(&harness.cdr).await;
    mount_create(
        &harness.cdr,
        VERSION_ONE,
        &person_composition(&harness, resource)?,
    )
    .await;
    let (status, body) = call(harness.app(), post_patient(resource)?).await?;
    let query = harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|request| request.method.as_str() == "GET" && request.url.path() == "/ehr")
        .map(|lookup| lookup.url.query_pairs().into_owned().collect())
        .unwrap_or_default();
    Ok((harness, status, body, query))
}

/// The `subject_id` and `subject_namespace` of an EHR lookup query.
fn subject_of(query: &[(String, String)]) -> (Option<&str>, Option<&str>) {
    let value = |key: &str| {
        query
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    (value("subject_id"), value("subject_namespace"))
}

#[tokio::test]
async fn a_patient_keys_its_ehr_by_its_iti_83_pseudonym() -> Result<(), Box<dyn Error>> {
    let (harness, status, body, query) = post_patient_with(&patient()?, None).await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    assert_eq!(
        (
            Some(crate::support::PSEUDONYM),
            Some(settings().subject.namespace())
        ),
        subject_of(&query),
        "{query:?}"
    );
    let asked = harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|request| request.url.path() == crate::support::PIX_PATH)
        .ok_or("the identity source is asked")?;
    assert_eq!("POST", asked.method.as_str());
    assert_eq!(None, asked.url.query(), "no identifier sits in the URL");
    let parameters: serde_json::Value = serde_json::from_slice(&asked.body)?;
    let text = parameters.to_string();
    assert!(text.contains("synthetic-patient-0001"), "{text}");
    assert!(text.contains(settings().subject.namespace()), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_patient_with_no_identifier_in_an_accepted_domain_is_refused_naming_the_domains()
-> Result<(), Box<dyn Error>> {
    let mut resource = patient()?;
    resource["identifier"] = serde_json::json!([
        {"system": "urn:oid:2.999.77", "value": "elsewhere-1"}
    ]);
    let (_harness, status, body, query) = post_patient_with(&resource, None).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert!(
        body.to_string().contains(crate::support::SOURCE_DOMAIN),
        "the refusal names the accepted domains: {body}"
    );
    assert!(query.is_empty(), "no EHR is looked up: {query:?}");
    Ok(())
}

#[tokio::test]
async fn an_identifier_without_a_system_is_refused() -> Result<(), Box<dyn Error>> {
    let mut resource = patient()?;
    resource["identifier"] = serde_json::json!([{"value": "bare-1"}]);
    let (_harness, status, body, query) = post_patient_with(&resource, None).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert!(query.is_empty(), "no EHR is looked up: {query:?}");
    Ok(())
}

#[tokio::test]
async fn an_identity_source_failure_is_a_typed_refusal_and_never_another_subject()
-> Result<(), Box<dyn Error>> {
    for (answer, expected) in [
        (500, StatusCode::SERVICE_UNAVAILABLE),
        (503, StatusCode::SERVICE_UNAVAILABLE),
        (401, StatusCode::BAD_GATEWAY),
    ] {
        let (_harness, status, body, query) =
            post_patient_with(&patient()?, Some(ResponseTemplate::new(answer))).await?;
        assert_eq!(expected, status, "{answer}: {body}");
        assert!(
            body.to_string().contains(&answer.to_string()),
            "the refusal names the upstream status: {body}"
        );
        assert!(query.is_empty(), "no EHR is looked up: {query:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_patient_the_identity_source_does_not_know_is_refused() -> Result<(), Box<dyn Error>> {
    let not_found = ResponseTemplate::new(404).set_body_json(serde_json::json!({
        "resourceType": "OperationOutcome",
        "issue": [{"severity": "error", "code": "not-found"}]
    }));
    let (_harness, status, body, query) = post_patient_with(&patient()?, Some(not_found)).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert!(query.is_empty(), "no EHR is looked up: {query:?}");
    Ok(())
}

#[test]
fn the_first_accepted_domain_in_list_order_is_asked() -> Result<(), Box<dyn Error>> {
    let identity = crate::support::identity_source("http://127.0.0.1:9/fhir", "urn:oid:2.999.9");
    let source = identity.source_of(&[
        (Some("urn:oid:2.999.9"), "second"),
        (Some(crate::support::SOURCE_DOMAIN), "first"),
    ])?;
    assert_eq!(crate::support::SOURCE_DOMAIN, source.system());
    Ok(())
}
