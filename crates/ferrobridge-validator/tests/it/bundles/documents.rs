// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document rules of documents.html §3.3.1, and a synthetic EPS-shaped
//! document over HL7 Europe `bundle-eu-eps` 1.0.0-ballot, read when
//! `scripts/vendor/profile-packages.sh` has fetched the packages.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Report;
use ferrobridge_validator::set::PackageSet;
use serde_json::json;

use super::bundle;
use super::patient;
use crate::eu::empty_reason;
use crate::eu::eu_set;
use crate::support::TestResult;
use crate::support::core;
use crate::support::locations;
use crate::support::validate;

/// The fullUrls of the document's entries.
const COMPOSITION: &str = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000030";
const PATIENT: &str = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000031";
const AUTHOR: &str = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000032";
const OBSERVATION: &str = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000033";
const PERFORMER: &str = "urn:uuid:4a1e8f9e-0000-4000-8000-000000000034";

/// A synthetic Composition about [`PATIENT`] by [`AUTHOR`], its one section
/// holding [`OBSERVATION`].
fn composition() -> serde_json::Value {
    json!({
        "resourceType": "Composition",
        "status": "final",
        "type": {"text": "synthetic summary"},
        "subject": {"reference": PATIENT},
        "date": "2026-01-01",
        "author": [{"reference": AUTHOR}],
        "title": "Synthetic summary",
        "section": [{"title": "Results", "entry": [{"reference": OBSERVATION}]}]
    })
}

/// A synthetic Observation about [`PATIENT`], performed by [`PERFORMER`].
fn observation() -> serde_json::Value {
    json!({
        "resourceType": "Observation", "status": "final", "code": {"text": "synthetic"},
        "subject": {"reference": PATIENT}, "performer": [{"reference": PERFORMER}]
    })
}

/// A synthetic Practitioner.
fn practitioner() -> serde_json::Value {
    json!({"resourceType": "Practitioner", "name": [{"family": "Smith"}]})
}

/// The entries of a complete document: the Composition, what it references,
/// and the performer the Observation references.
fn complete() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (COMPOSITION, composition()),
        (PATIENT, patient()),
        (AUTHOR, practitioner()),
        (OBSERVATION, observation()),
        (PERFORMER, practitioner()),
    ]
}

/// Validates the document of `entries` against the core definitions.
///
/// The Bundle carries the identifier and the timestamp R4 `bdl-9` and
/// `bdl-10` require of a document.
fn document(entries: &[(&str, serde_json::Value)]) -> TestResult<Report> {
    let set = PackageSet::load([core()])?;
    let mut document = bundle("document", entries);
    document["identifier"] = json!({"system": "urn:ietf:rfc:3986", "value": "urn:uuid:4a1e8f9e-0000-4000-8000-00000000003b"});
    document["timestamp"] = json!("2026-01-01T00:00:00Z");
    validate(&set, &document, None)
}

#[test]
fn a_document_holding_what_its_composition_references_passes() -> TestResult {
    let report = document(&complete())?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn a_document_missing_a_resource_its_composition_references_is_refused() -> TestResult {
    let mut entries = complete();
    entries.retain(|(url, _)| *url != OBSERVATION);
    let report = document(&entries)?;
    assert_eq!(
        locations(&report, MessageId::DocumentReferenceMissing),
        ["Bundle.entry[0].resource.section[0].entry[0]"]
    );
    Ok(())
}

#[test]
fn a_document_carrying_an_unreferenced_resource_is_refused() -> TestResult {
    let mut entries = complete();
    entries.push((
        "urn:uuid:4a1e8f9e-0000-4000-8000-000000000035",
        practitioner(),
    ));
    let report = document(&entries)?;
    assert_eq!(
        locations(&report, MessageId::DocumentEntryUnreferenced),
        ["Bundle.entry[5]"]
    );
    Ok(())
}

#[test]
fn a_stylesheet_binary_and_the_provenance_of_the_document_may_ride_along() -> TestResult {
    let mut entries = complete();
    entries.push((
        "urn:uuid:4a1e8f9e-0000-4000-8000-000000000036",
        json!({"resourceType": "Binary", "contentType": "text/css", "data": "Ym9keXt9"}),
    ));
    entries.push((
        "urn:uuid:4a1e8f9e-0000-4000-8000-000000000037",
        json!({"resourceType": "Provenance", "target": [{"reference": COMPOSITION}],
               "recorded": "2026-01-01T00:00:00Z", "agent": [{"who": {"reference": AUTHOR}}]}),
    ));
    let report = document(&entries)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    entries.push((
        "urn:uuid:4a1e8f9e-0000-4000-8000-000000000038",
        json!({"resourceType": "Binary", "contentType": "application/pdf", "data": "JVBERg=="}),
    ));
    let report = document(&entries)?;
    assert_eq!(
        locations(&report, MessageId::DocumentEntryUnreferenced),
        ["Bundle.entry[7]"]
    );
    Ok(())
}

/// The Bundle profile of the European patient summary.
const EPS_BUNDLE: &str = "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps";

/// The Patient profile the patient slice of [`EPS_BUNDLE`] names.
const EPS_PATIENT: &str = "http://hl7.eu/fhir/eps/StructureDefinition/patient-eu-eps";

#[test]
fn an_eps_document_whose_patient_breaks_its_slice_profile_is_reported_at_that_entry() -> TestResult
{
    let Some(set) = eu_set()? else { return Ok(()) };
    let sections: Vec<serde_json::Value> = ["11450-4", "48765-2", "10160-0", "47519-4", "46264-8"]
        .iter()
        .map(|code| {
            json!({
                "title": code,
                "code": {"coding": [{"system": "http://loinc.org", "code": code}]},
                "text": {"status": "generated", "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">x</div>"},
                "emptyReason": empty_reason()
            })
        })
        .collect();
    let composition = json!({
        "resourceType": "Composition",
        "identifier": {"system": "urn:ietf:rfc:3986", "value": "urn:uuid:4a1e8f9e-0000-4000-8000-000000000039"},
        "status": "final",
        "type": {"coding": [{"system": "http://loinc.org", "code": "60591-5"}]},
        "subject": {"reference": PATIENT},
        "date": "2026-01-01",
        "author": [{"reference": AUTHOR}],
        "title": "Patient summary",
        "section": sections
    });
    let entries = |birth_date: bool| {
        let mut subject = json!({
            "resourceType": "Patient", "meta": {"profile": [EPS_PATIENT]},
            "identifier": [{"system": "urn:example:patients", "value": "1"}],
            "name": [{"family": "Doe", "given": ["Ada"]}], "gender": "female", "birthDate": "1970-01-01"
        });
        if !birth_date {
            subject
                .as_object_mut()
                .map(|object| object.remove("birthDate"));
        }
        let mut document = bundle(
            "document",
            &[
                (COMPOSITION, composition.clone()),
                (PATIENT, subject),
                (AUTHOR, practitioner()),
            ],
        );
        document["identifier"] = json!({"system": "urn:ietf:rfc:3986", "value": "urn:uuid:4a1e8f9e-0000-4000-8000-00000000003a"});
        document["timestamp"] = json!("2026-01-01T00:00:00Z");
        document
    };
    let report = validate(&set, &entries(true), Some(EPS_BUNDLE))?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    // R4 profiling.html#discriminator: under a `profile` discriminator the broken Patient is in no
    // slice, so the slice minimum is reported at the Bundle and the claim at the entry.
    let report = validate(&set, &entries(false), Some(EPS_BUNDLE))?;
    assert_eq!(
        locations(&report, MessageId::ClaimedProfileMismatch),
        ["Bundle.entry[1].resource"]
    );
    let at_patient: Vec<String> = report
        .issues()
        .iter()
        .filter(|issue| issue.expression.starts_with("Bundle.entry[1].resource"))
        .map(|issue| issue.message.clone())
        .collect();
    assert!(
        at_patient
            .iter()
            .any(|message| message.contains("birthDate")),
        "{:#?}",
        report.issues()
    );
    Ok(())
}
