// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one-subject rule of `$toopenehr`, with each reference resolved to the
//! Bundle entry it addresses by the R4 rules
//! (<https://hl7.org/fhir/R4/bundle.html#references>, §2.36.4.1).

use core::error::Error;
use std::path::Path;
use std::path::PathBuf;

use fhir_types::codec::Json;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::seam::Seams;
use fhirconnect::operations::contract::ToOpenehrRequest;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::run;
use serde_json::json;

use crate::operations::MINIMAL_PROFILE;
use crate::operations::condition;
use crate::operations::program_set;
use crate::operations::settings;

/// The context the EPS documents run through, which claims the EPS Patient
/// profile.
const EPS_CONTEXT: &str = "ferrobridge_eps_patient";

/// A synthetic `urn:uuid` an entry is addressed by.
const PATIENT_UUID: &str = "urn:uuid:5e0c3a1d-0000-4000-8000-00000000a001";

/// A second synthetic `urn:uuid`, for a second patient.
const OTHER_UUID: &str = "urn:uuid:5e0c3a1d-0000-4000-8000-00000000a002";

/// Returns a `document`-shaped Bundle whose entries carry the `fullUrl`
/// beside each resource when one is given.
fn addressed(entries: Vec<(Option<&str>, serde_json::Value)>) -> Result<Bundle, Box<dyn Error>> {
    let entry: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|(full_url, resource)| match full_url {
            Some(url) => json!({ "fullUrl": url, "resource": resource }),
            None => json!({ "resource": resource }),
        })
        .collect();
    parsed(&json!({ "resourceType": "Bundle", "type": "collection", "entry": entry }))
}

/// Reads a JSON Bundle into the typed model.
fn parsed(document: &serde_json::Value) -> Result<Bundle, Box<dyn Error>> {
    let fhir_types::codec::Value::Object(object) =
        fhir_types::codec::Value::from_serde_json(document.clone())
    else {
        return Err(Box::<dyn Error>::from("the bundle is not a JSON object"));
    };
    Ok(Bundle::from_json(
        &object,
        &mut fhir_types::codec::Path::root("Bundle"),
    )?)
}

/// Returns a synthetic `Patient` with `id`.
fn patient(id: &str) -> serde_json::Value {
    json!({ "resourceType": "Patient", "id": id })
}

/// Returns a synthetic `Patient` with `id` at version `version`.
fn patient_at(id: &str, version: &str) -> serde_json::Value {
    json!({ "resourceType": "Patient", "id": id, "meta": { "versionId": version } })
}

/// Returns the synthetic `Condition` of the minimal context with `subject`
/// as its literal subject reference.
fn condition_of(subject: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut resource = condition(MINIMAL_PROFILE)?;
    if let Some(object) = resource.as_object_mut() {
        object.insert(String::from("subject"), json!({ "reference": subject }));
    }
    Ok(resource)
}

/// Runs `$toopenehr` over `bundle` through the minimal context.
fn run_minimal(bundle: Bundle) -> Result<Result<(), OperationError>, Box<dyn Error>> {
    let set = program_set(&["ferrobridge_diagnose_minimal"])?;
    let request = ToOpenehrRequest::new(bundle);
    Ok(run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request).map(|_| ()))
}

/// Returns the subjects a refusal names, or fails when the run was no
/// several-subjects refusal.
fn refused_subjects(outcome: Result<(), OperationError>) -> Result<Vec<String>, Box<dyn Error>> {
    match outcome {
        Err(OperationError::SeveralSubjects { subjects }) => Ok(subjects),
        Err(other) => Err(format!("expected a several-subjects refusal, got {other}").into()),
        Ok(()) => Err("expected a several-subjects refusal, the run succeeded".into()),
    }
}

#[test]
fn a_urn_uuid_reference_resolves_to_the_entry_it_addresses() -> Result<(), Box<dyn Error>> {
    // bundle.html §2.36.4.1: an absolute reference matches an entry's fullUrl.
    let bundle = addressed(vec![
        (
            Some("urn:uuid:5e0c3a1d-0000-4000-8000-00000000c001"),
            condition_of(PATIENT_UUID)?,
        ),
        (Some(PATIENT_UUID), patient("synthetic-patient")),
    ])?;
    run_minimal(bundle)??;
    Ok(())
}

#[test]
fn a_urn_oid_reference_resolves_to_the_entry_it_addresses() -> Result<(), Box<dyn Error>> {
    let bundle = addressed(vec![
        (
            Some("urn:oid:2.999.1.1"),
            condition_of("urn:oid:2.999.1.2")?,
        ),
        (Some("urn:oid:2.999.1.2"), patient("synthetic-patient")),
    ])?;
    run_minimal(bundle)??;
    Ok(())
}

#[test]
fn an_absolute_url_reference_resolves_to_the_entry_it_addresses() -> Result<(), Box<dyn Error>> {
    let bundle = addressed(vec![
        (
            Some("http://example.org/fhir/Condition/c1"),
            condition_of("http://example.org/fhir/Patient/p1")?,
        ),
        (Some("http://example.org/fhir/Patient/p1"), patient("p1")),
    ])?;
    run_minimal(bundle)??;
    Ok(())
}

#[test]
fn a_relative_reference_resolves_against_a_restful_full_url() -> Result<(), Box<dyn Error>> {
    // bundle.html §2.36.4.1: a relative reference takes the root of the
    // referring entry's RESTful fullUrl.
    let bundle = addressed(vec![
        (
            Some("http://example.org/fhir/Condition/c1"),
            condition_of("Patient/p1")?,
        ),
        (Some("http://example.org/fhir/Patient/p1"), patient("p1")),
    ])?;
    run_minimal(bundle)??;
    Ok(())
}

#[test]
fn a_relative_reference_against_another_server_is_another_subject() -> Result<(), Box<dyn Error>> {
    // The referring entry's root makes `Patient/p1` a resource of
    // b.example.org, which is not the Patient entry of a.example.org.
    let bundle = addressed(vec![
        (
            Some("http://b.example.org/fhir/Condition/c1"),
            condition_of("Patient/p1")?,
        ),
        (Some("http://a.example.org/fhir/Patient/p1"), patient("p1")),
    ])?;
    let subjects = refused_subjects(run_minimal(bundle)?)?;
    assert_eq!(
        subjects,
        vec![
            String::from("Patient/p1"),
            String::from("http://a.example.org/fhir/Patient/p1"),
        ]
    );
    Ok(())
}

#[test]
fn a_relative_reference_without_full_urls_resolves_by_type_and_id() -> Result<(), Box<dyn Error>> {
    let bundle = addressed(vec![
        (None, condition_of("Patient/p1")?),
        (None, patient("p1")),
    ])?;
    run_minimal(bundle)??;
    Ok(())
}

#[test]
fn a_relative_reference_inside_urn_entries_resolves_by_type_and_id() -> Result<(), Box<dyn Error>> {
    let bundle = addressed(vec![
        (
            Some("urn:uuid:5e0c3a1d-0000-4000-8000-00000000c001"),
            condition_of("Patient/synthetic-patient")?,
        ),
        (Some(PATIENT_UUID), patient("synthetic-patient")),
    ])?;
    run_minimal(bundle)??;
    Ok(())
}

#[test]
fn a_version_specific_reference_matches_the_version_id() -> Result<(), Box<dyn Error>> {
    // bundle.html §2.36.4.1: the version leaves the URL and is matched
    // against `Resource.meta.versionId`.
    let bundle = addressed(vec![
        (None, condition_of("Patient/p1/_history/2")?),
        (None, patient_at("p1", "2")),
    ])?;
    run_minimal(bundle)??;
    Ok(())
}

#[test]
fn a_version_specific_reference_to_another_version_is_another_subject() -> Result<(), Box<dyn Error>>
{
    let bundle = addressed(vec![
        (None, condition_of("Patient/p1/_history/3")?),
        (None, patient_at("p1", "2")),
    ])?;
    let subjects = refused_subjects(run_minimal(bundle)?)?;
    assert_eq!(
        subjects,
        vec![
            String::from("Patient/p1/_history/3"),
            String::from("Patient/p1")
        ]
    );
    Ok(())
}

#[test]
fn references_resolving_to_two_patient_entries_are_refused() -> Result<(), Box<dyn Error>> {
    let mut second = condition_of(OTHER_UUID)?;
    if let Some(object) = second.as_object_mut() {
        object.insert(String::from("id"), json!("another"));
    }
    let bundle = addressed(vec![
        (
            Some("urn:uuid:5e0c3a1d-0000-4000-8000-00000000c001"),
            condition_of(PATIENT_UUID)?,
        ),
        (
            Some("urn:uuid:5e0c3a1d-0000-4000-8000-00000000c002"),
            second,
        ),
        (Some(PATIENT_UUID), patient("first")),
        (Some(OTHER_UUID), patient("second")),
    ])?;
    let subjects = refused_subjects(run_minimal(bundle)?)?;
    assert_eq!(
        subjects,
        vec![String::from(PATIENT_UUID), String::from(OTHER_UUID)]
    );
    Ok(())
}

/// Returns the examples directory of the fetched EPS package, `None` when
/// the package is absent outside CI.
///
/// Under CI (`CI=true`) an absent package is an error, so the lane cannot
/// pass by skipping the published documents.
fn eps_examples() -> Result<Option<PathBuf>, Box<dyn Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/fhir-packages/hl7.fhir.eu.eps#1.0.0-ballot/package/example");
    if root.is_dir() {
        return Ok(Some(root));
    }
    if std::env::var("CI").is_ok_and(|value| value == "true") {
        return Err(format!(
            "the EPS package is absent from {} under CI; run scripts/vendor/profile-packages.sh before the tests",
            root.display()
        )
        .into());
    }
    Ok(None)
}

#[test]
fn both_published_eps_documents_map() -> Result<(), Box<dyn Error>> {
    // HL7 Europe's EPS examples address every entry by a urn:uuid fullUrl and
    // reference the Patient entry by it.
    let Some(examples) = eps_examples()? else {
        return Ok(());
    };
    let set = program_set(&[EPS_CONTEXT])?;
    for name in [
        "Bundle-EPSExampleBundle01NoProblemsMedicationAllergies.json",
        "Bundle-Instance-Bundle-1a24e60d-9b12-4109-a50a-07249a4f21c3.json",
    ] {
        let text = std::fs::read_to_string(examples.join(name))?;
        let bundle = parsed(&serde_json::from_str(&text)?)?;
        let request = ToOpenehrRequest::new(bundle);
        run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request)
            .map_err(|error| format!("{name} does not map: {error:?}"))?;
    }
    Ok(())
}
