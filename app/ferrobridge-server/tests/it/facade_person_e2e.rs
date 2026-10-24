// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FHIR `Patient` through the facade against a real CDR, behind the
//! `FERROBRIDGE_E2E` gate: the published `KDS_Person` template is uploaded,
//! the synthetic MII Person `Patient` commits as its composition in the EHR
//! the Patient keys, and reads back through the same program.

use axum::body::Body;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::Settings;
use ferrobridge_server::facade::ehr::Policy;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::programs;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::fixtures;
use http::Request;
use http::StatusCode;
use http::header;
use std::error::Error;
use std::sync::Arc;

use crate::facade_e2e::app;
use crate::facade_e2e::call;
use crate::facade_e2e::upload_published;

#[tokio::test]
async fn the_facade_round_trips_the_kds_patient_through_a_real_cdr() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let cdr = containers::cdr().await?;
    upload_published(cdr.base_url(), fixtures::KDS_PERSON_OPT).await?;
    let client = CdrClient::new(&CdrConfig::new(cdr.base_url().parse()?))?;
    let directory = tempfile::tempdir()?;
    crate::kds::write_person_mappings(directory.path())?;
    let set = programs::read_set(directory.path())?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let programs = programs::compile_set(&set, &templates)?;
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let facade = Arc::new(Facade::new(
        programs,
        store,
        client,
        Settings {
            base_url: String::from("http://ferrobridge.invalid/fhir"),
            ehr_policy: Policy::CreateOnFirstWrite,
            subject: crate::support::subject_link("http://example.org/fhir"),
            system_id: String::from("ferrobridge.e2e"),
            language: String::from("de"),
            territory: String::from("DE"),
            ehr_status_system: None,
        },
    ));
    let input: serde_json::Value = serde_json::from_str(fixtures::KDS_PERSON_PATIENT)?;
    let created = call(
        app(&facade),
        Request::post("/fhir/Patient")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(input.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, created.0, "{}", created.1);
    let id = created.1["id"]
        .as_str()
        .ok_or("the created resource carries an id")?
        .to_owned();
    let read = call(
        app(&facade),
        Request::get(format!("/fhir/Patient/{id}")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, read.0, "{}", read.1);
    for element in ["name", "gender", "birthDate", "deceasedBoolean"] {
        assert_eq!(
            input[element], read.1[element],
            "{element} did not survive the round trip: {}",
            read.1
        );
    }
    assert_eq!(
        input["managingOrganization"]["display"], read.1["managingOrganization"]["display"],
        "{}",
        read.1
    );
    Ok(())
}
