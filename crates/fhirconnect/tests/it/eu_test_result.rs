// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 Europe Base and Core 2.0.0 `medicalTestResult-eu-core` round trip:
//! a FerroBRIDGE context over the published `KDS_Laborbericht` template. The
//! laboratory test result is the Observation, and each analyte a member
//! `Observation-resultslab-eu-lab`, a profile derived from
//! `medicalTestResult-eu-core`, that it references.

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::BundleReferences;
use fhirconnect::engine::seam::NoReferences;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::laws;
use crate::support::FIXTURES;
use crate::support::render;

/// The files this context loads, the context first, each under the
/// project directory that holds it.
const FILES: &[&str] = &[
    "projects/ferrobridge/eu_test_result/ferrobridge_eu_test_result.context.yml",
    "projects/ferrobridge/eu_test_result/ferrobridge_laboratory_test_result.v1.Observation.yml",
    "projects/ferrobridge/eu_test_result/ferrobridge_eu_test_result.yml",
    "projects/ferrobridge/eu_laboratory/ferrobridge_laboratory_test_analyte.v1.yml",
    "projects/ferrobridge/eu_laboratory/ferrobridge_eu_laboratory_analyte.yml",
];

/// The context this crate authored for the profile.
const CONTEXT: &str = "ferrobridge_eu_test_result.context";

/// The canonical of the test result profile.
const MEDICAL_TEST_RESULT_EU_CORE: &str =
    "http://hl7.eu/fhir/base/StructureDefinition/medicalTestResult-eu-core";

/// The canonical of the member profile.
const OBSERVATION_RESULTSLAB_EU_LAB: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/Observation-resultslab-eu-lab";

/// Compiles the context against the published `KDS_Laborbericht` template.
fn program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(ferrobridge_testkit::fixtures::KDS_LABORBERICHT_OPT)?;
    let index = WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(opt)))?;
    let files: Vec<String> = FILES
        .iter()
        .map(|file| format!("{FIXTURES}/{file}"))
        .collect();
    let set = load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics))?;
    let program = compile(
        &set,
        &MappingName::new(CONTEXT)?,
        &index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    Ok((program, index))
}

#[test]
fn the_test_result_context_compiles_against_the_published_template() -> Result<(), Box<dyn Error>> {
    let (program, _) = program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Laborbericht");
    assert_eq!(program.resource().as_str(), "Observation");
    Ok(())
}

/// A synthetic lipid panel in the shape `medicalTestResult-eu-core`
/// constrains: the required category, subject and effective time, the code,
/// the status and the business identifier, with one member result.
fn panel() -> (serde_json::Value, serde_json::Value) {
    let panel = serde_json::json!({
        "resourceType": "Observation",
        "meta": {"profile": [MEDICAL_TEST_RESULT_EU_CORE]},
        "identifier": [{"value": "synthetic-report-0002"}],
        "status": "final",
        "category": [{"coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/observation-category",
            "code": "laboratory"
        }]}],
        "code": {"coding": [{
            "system": "http://loinc.org",
            "code": "24331-1",
            "display": "Lipid panel - Serum or Plasma"
        }]},
        "subject": {"reference": "Patient/synthetic-patient"},
        "effectiveDateTime": "2026-03-02T08:15:00+01:00",
        "hasMember": [{"reference": "Observation/synthetic-cholesterol"}]
    });
    let member = serde_json::json!({
        "resourceType": "Observation",
        "id": "synthetic-cholesterol",
        "meta": {"profile": [OBSERVATION_RESULTSLAB_EU_LAB]},
        "status": "final",
        "category": [{"coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/observation-category",
            "code": "laboratory"
        }]}],
        "code": {"coding": [{
            "system": "http://loinc.org",
            "code": "2093-3",
            "display": "Cholesterol [Mass/volume] in Serum or Plasma"
        }]},
        "subject": {"reference": "Patient/synthetic-patient"},
        "valueQuantity": {
            "value": 182,
            "unit": "mg/dL",
            "system": "http://unitsofmeasure.org",
            "code": "mg/dL"
        }
    });
    (panel, member)
}

/// Runs the synthetic panel into openEHR and back, returning the panel and
/// the members the run out of openEHR created.
fn round_trip() -> Result<(serde_json::Value, Vec<serde_json::Value>), Box<dyn Error>> {
    let (program, index) = program()?;
    let (panel, member) = panel();
    let bundle = BundleReferences::new(
        vec![(
            Some(String::from("Observation/synthetic-cholesterol")),
            Value::from_serde_json(member),
        )],
        &NoReferences,
    );
    let into = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(panel),
        &Seams::default().with_references(&bundle),
        &laws::defaults(),
        &CallContext::new(),
    )?;
    let back = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        into.value(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    let created = back
        .created()
        .iter()
        .map(|created| created.to_serde_json(&mut fhir_types::codec::Path::root("Resource")))
        .collect::<Result<Vec<_>, _>>()?;
    let panel = back
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Observation"))?;
    Ok((panel, created))
}

#[test]
fn the_profile_elements_of_the_test_result_survive_the_round_trip() -> Result<(), Box<dyn Error>> {
    let (input, _) = panel();
    let (output, members) = round_trip()?;
    assert_eq!(
        output
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(MEDICAL_TEST_RESULT_EU_CORE),
        "the result names its profile: {output}"
    );
    for pointer in [
        "/identifier/0/value",
        "/status",
        "/category/0/coding/0",
        "/code/coding/0",
        "/effectiveDateTime",
    ] {
        assert_eq!(
            output.pointer(pointer),
            input.pointer(pointer),
            "{pointer}: {output}"
        );
    }
    let [ref member] = *members else {
        return Err(format!("the run created {} members, not one", members.len()).into());
    };
    assert_eq!(
        output
            .pointer("/hasMember/0/reference")
            .and_then(serde_json::Value::as_str),
        member
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(|id| format!("Observation/{id}"))
            .as_deref(),
        "the result references the member it created: {output}"
    );
    Ok(())
}

#[test]
fn the_member_of_the_test_result_survives_the_round_trip() -> Result<(), Box<dyn Error>> {
    let (_, input) = panel();
    let (_, members) = round_trip()?;
    let [ref member] = *members else {
        return Err(format!("the run created {} members, not one", members.len()).into());
    };
    assert_eq!(
        member
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(OBSERVATION_RESULTSLAB_EU_LAB),
        "the member names its profile: {member}"
    );
    for pointer in [
        "/status",
        "/category/0/coding/0",
        "/code/coding/0",
        "/valueQuantity/value",
        "/valueQuantity/code",
    ] {
        assert_eq!(
            member.pointer(pointer),
            input.pointer(pointer),
            "{pointer}: {member}"
        );
    }
    Ok(())
}
