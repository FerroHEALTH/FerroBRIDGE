// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 Europe Laboratory Report 2.0.0 round trip: a FerroBRIDGE context
//! over the published `KDS_Laborbericht` template. The report is a
//! `DiagnosticReport-eu-lab`, and each analyte an
//! `Observation-resultslab-eu-lab` the report references, a profile that
//! derives from Base and Core 2.0.0 `medicalTestResult-eu-core`.
//!
//! The mapping library's own `eehrxf_lab` context targets Laboratory 0.1.1
//! and its template does not parse strictly (#1235), so the published
//! `KDS_Laborbericht` template, which carries the same CKM archetypes, is the
//! one this context compiles against.

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

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The published model mapping the context loads, as the library ships it.
const PUBLISHED: &[&str] = &["model/composition/org.openehr/report_result.v1.DiagnosticReport.yml"];

/// The project directory this crate authored for the profiles.
const PROJECT_DIR: &str = "projects/ferrobridge/eu_laboratory";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_eu_laboratory.context.yml",
    "ferrobridge_laboratory_test_result.v1.yml",
    "ferrobridge_laboratory_test_analyte.v1.yml",
    "ferrobridge_specimen.v1.yml",
    "ferrobridge_eu_laboratory_result.yml",
    "ferrobridge_eu_laboratory_analyte.yml",
    "ferrobridge_eu_laboratory_specimen.yml",
    "ferrobridge_eu_composition.DiagnosticReport.yml",
];

/// The context this crate authored for the profiles.
const CONTEXT: &str = "ferrobridge_eu_laboratory.context";

/// The canonical of the report profile.
const DIAGNOSTIC_REPORT_EU_LAB: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/DiagnosticReport-eu-lab";

/// The canonical of the specimen profile.
const SPECIMEN_EU_LAB: &str = "http://hl7.eu/fhir/laboratory/StructureDefinition/Specimen-eu-lab";

/// The canonical of the result profile.
const OBSERVATION_RESULTSLAB_EU_LAB: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/Observation-resultslab-eu-lab";

/// Builds the index over the published `KDS_Laborbericht` template.
fn kds_laborbericht_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(ferrobridge_testkit::fixtures::KDS_LABORBERICHT_OPT)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Compiles the context against the published template.
fn eu_laboratory_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_laborbericht_template()?;
    let files: Vec<String> = PUBLISHED
        .iter()
        .map(|file| format!("{CORPUS}/{file}"))
        .chain(
            PROJECT
                .iter()
                .map(|file| format!("{FIXTURES}/{PROJECT_DIR}/{file}")),
        )
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
fn the_eu_laboratory_context_compiles_against_the_published_template() -> Result<(), Box<dyn Error>>
{
    let (program, _) = eu_laboratory_program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Laborbericht");
    assert_eq!(program.resource().as_str(), "DiagnosticReport");
    insta::assert_snapshot!("eu_laboratory_program", program.to_string());
    Ok(())
}

/// A synthetic serum specimen in the shape `Specimen-eu-lab` constrains: the
/// required `type`, the collection time and method, the accession
/// identifier and the received time.
fn specimen() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Specimen",
        "id": "synthetic-serum",
        "meta": {"profile": [SPECIMEN_EU_LAB]},
        "accessionIdentifier": {
            "system": "http://example.org/fhir/sid/ferrobridge-specimens",
            "value": "synthetic-specimen-0001"
        },
        "type": {"coding": [{
            "system": "http://snomed.info/sct",
            "code": "119364003",
            "display": "Serum specimen"
        }]},
        "receivedTime": "2026-03-02T07:40:00+01:00",
        "collection": {
            "collectedDateTime": "2026-03-02T07:05:00+01:00",
            "method": {"coding": [{
                "system": "http://snomed.info/sct",
                "code": "28520004",
                "display": "Venipuncture for blood test"
            }]}
        }
    })
}

/// A synthetic laboratory report with one quantitative result, in the shape
/// the two profiles constrain: the report's code, status, effective time and
/// conclusion, and the result's code, status, value, interpretation and
/// method.
fn report() -> (serde_json::Value, serde_json::Value) {
    let report = serde_json::json!({
        "resourceType": "DiagnosticReport",
        "meta": {"profile": [DIAGNOSTIC_REPORT_EU_LAB]},
        "identifier": [{
            "system": "http://example.org/fhir/sid/ferrobridge-lab-reports",
            "value": "synthetic-report-0001"
        }],
        "status": "final",
        "code": {"coding": [{
            "system": "http://loinc.org",
            "code": "24331-1",
            "display": "Lipid panel - Serum or Plasma"
        }]},
        "subject": {"reference": "Patient/synthetic-patient"},
        "effectiveDateTime": "2026-03-02T08:15:00+01:00",
        "specimen": [{"reference": "Specimen/synthetic-serum"}],
        "result": [{"reference": "Observation/synthetic-cholesterol"}],
        "conclusion": "A synthetic conclusion."
    });
    let result = serde_json::json!({
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
        },
        "interpretation": [{"coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/v3-ObservationInterpretation",
            "code": "N",
            "display": "Normal"
        }]}],
        "method": {"coding": [{
            "system": "http://example.org/fhir/CodeSystem/ferrobridge-lab-methods",
            "code": "SYN-ENZ",
            "display": "Synthetic enzymatic method"
        }]}
    });
    (report, result)
}

/// The report and the resources a run out of openEHR writes.
struct Outbound {
    /// The report.
    report: serde_json::Value,
    /// The results it created.
    results: Vec<serde_json::Value>,
    /// The specimens it created.
    specimens: Vec<serde_json::Value>,
}

/// Runs the synthetic report into openEHR and back.
fn round_trip(program: &Program, index: &WebTemplateIndex) -> Result<Outbound, Box<dyn Error>> {
    let (report, result) = report();
    let bundle = BundleReferences::new(
        vec![
            (
                Some(String::from("Observation/synthetic-cholesterol")),
                Value::from_serde_json(result),
            ),
            (
                Some(String::from("Specimen/synthetic-serum")),
                Value::from_serde_json(specimen()),
            ),
        ],
        &NoReferences,
    );
    let into = to_openehr(
        program,
        &SCHEMAS,
        index,
        &Value::from_serde_json(report),
        &Seams::default().with_references(&bundle),
        &laws::defaults(),
        &CallContext::new(),
    )?;
    let back = to_fhir(
        program,
        &SCHEMAS,
        index,
        into.value(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    let created = back
        .created()
        .iter()
        .map(|created| created.to_serde_json(&mut fhir_types::codec::Path::root("Resource")))
        .collect::<Result<Vec<_>, _>>()?;
    let of_type = |kind: &str| -> Vec<serde_json::Value> {
        created
            .iter()
            .filter(|resource| {
                resource
                    .get("resourceType")
                    .and_then(serde_json::Value::as_str)
                    == Some(kind)
            })
            .cloned()
            .collect()
    };
    Ok(Outbound {
        report: back
            .value()
            .to_serde_json(&mut fhir_types::codec::Path::root("DiagnosticReport"))?,
        results: of_type("Observation"),
        specimens: of_type("Specimen"),
    })
}

#[test]
fn the_profile_elements_of_the_laboratory_report_survive_the_round_trip()
-> Result<(), Box<dyn Error>> {
    let (program, index) = eu_laboratory_program()?;
    let (input, _) = report();
    let Outbound {
        report, results, ..
    } = round_trip(&program, &index)?;
    assert_eq!(
        report
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(DIAGNOSTIC_REPORT_EU_LAB),
        "the report names its profile: {report}"
    );
    for pointer in [
        "/identifier/0/value",
        "/status",
        "/code/coding/0",
        "/effectiveDateTime",
        "/conclusion",
    ] {
        assert_eq!(
            report.pointer(pointer),
            input.pointer(pointer),
            "{pointer}: {report}"
        );
    }
    let [ref result] = *results else {
        return Err(format!(
            "the run created {} results, not one: {results:?}",
            results.len()
        )
        .into());
    };
    assert_eq!(
        report
            .pointer("/result/0/reference")
            .and_then(serde_json::Value::as_str),
        result
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(|id| format!("Observation/{id}"))
            .as_deref(),
        "the report references the result it created: {report}"
    );
    Ok(())
}

#[test]
fn the_profile_elements_of_the_laboratory_result_survive_the_round_trip()
-> Result<(), Box<dyn Error>> {
    let (program, index) = eu_laboratory_program()?;
    let (_, input) = report();
    let Outbound { results, .. } = round_trip(&program, &index)?;
    let [ref result] = *results else {
        return Err(format!("the run created {} results, not one", results.len()).into());
    };
    assert_eq!(
        result
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(OBSERVATION_RESULTSLAB_EU_LAB),
        "the result names its profile: {result}"
    );
    for pointer in [
        "/status",
        "/category/0/coding/0",
        "/code/coding/0",
        "/valueQuantity/value",
        "/valueQuantity/code",
        "/interpretation/0/coding/0/code",
        "/method/coding/0/code",
    ] {
        assert_eq!(
            result.pointer(pointer),
            input.pointer(pointer),
            "{pointer}: {result}"
        );
    }
    Ok(())
}

#[test]
fn the_profile_elements_of_the_laboratory_specimen_survive_the_round_trip()
-> Result<(), Box<dyn Error>> {
    let (program, index) = eu_laboratory_program()?;
    let input = specimen();
    let Outbound {
        report, specimens, ..
    } = round_trip(&program, &index)?;
    let [ref specimen] = *specimens else {
        return Err(format!("the run created {} specimens, not one", specimens.len()).into());
    };
    assert_eq!(
        specimen
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(SPECIMEN_EU_LAB),
        "the specimen names its profile: {specimen}"
    );
    for pointer in [
        "/type/coding/0",
        "/accessionIdentifier/value",
        "/receivedTime",
        "/collection/collectedDateTime",
        "/collection/method/coding/0",
    ] {
        assert_eq!(
            specimen.pointer(pointer),
            input.pointer(pointer),
            "{pointer}: {specimen}"
        );
    }
    assert_eq!(
        report
            .pointer("/specimen/0/reference")
            .and_then(serde_json::Value::as_str),
        specimen
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(|id| format!("Specimen/{id}"))
            .as_deref(),
        "the report references the specimen it created: {report}"
    );
    Ok(())
}
