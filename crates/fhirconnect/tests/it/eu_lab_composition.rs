// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 Europe Laboratory Report 2.0.0 `Composition-eu-lab` round trip: a
//! FerroBRIDGE document context over the published `KDS_Laborbericht`
//! template. The report composition is the Composition, whose one section
//! carries the test's code and references the `DiagnosticReport-eu-lab`, which
//! references its results and specimens.

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
    "projects/ferrobridge/eu_lab_composition/ferrobridge_eu_lab_composition.context.yml",
    "projects/ferrobridge/eu_lab_composition/ferrobridge_report_result.v1.Composition.yml",
    "projects/ferrobridge/eu_lab_composition/ferrobridge_eu_lab_composition.yml",
    "projects/ferrobridge/eu_lab_composition/ferrobridge_eu_lab_composition_report.yml",
    "projects/ferrobridge/eu_laboratory/ferrobridge_laboratory_test_result.v1.yml",
    "projects/ferrobridge/eu_laboratory/ferrobridge_laboratory_test_analyte.v1.yml",
    "projects/ferrobridge/eu_laboratory/ferrobridge_specimen.v1.yml",
    "projects/ferrobridge/eu_laboratory/ferrobridge_eu_laboratory_analyte.yml",
    "projects/ferrobridge/eu_laboratory/ferrobridge_eu_laboratory_specimen.yml",
];

/// The context this crate authored for the profile.
const CONTEXT: &str = "ferrobridge_eu_lab_composition.context";

/// The canonical of the document's Composition profile.
const COMPOSITION_EU_LAB: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/Composition-eu-lab";

/// The canonical of the report profile.
const DIAGNOSTIC_REPORT_EU_LAB: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/DiagnosticReport-eu-lab";

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
fn the_composition_context_compiles_against_the_published_template() -> Result<(), Box<dyn Error>> {
    let (program, _) = program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Laborbericht");
    assert_eq!(program.resource().as_str(), "Composition");
    Ok(())
}

/// A synthetic laboratory report document in the shape `Composition-eu-lab`
/// constrains: the required status, type, date, author, title and one
/// section, whose entry is the report, which references one result and one
/// specimen.
fn document() -> (serde_json::Value, Vec<(String, serde_json::Value)>) {
    let composition = serde_json::json!({
        "resourceType": "Composition",
        "meta": {"profile": [COMPOSITION_EU_LAB]},
        "identifier": {"value": "synthetic-report-0003"},
        "status": "final",
        "type": {"coding": [{
            "system": "http://loinc.org",
            "code": "11502-2",
            "display": "Laboratory report"
        }]},
        "subject": {"reference": "Patient/synthetic-patient"},
        "date": "2026-03-02T09:30:00+01:00",
        "author": [{"display": "Synthetic Laboratory"}],
        "title": "Laboratory report",
        "section": [{
            "code": {"coding": [{
                "system": "http://loinc.org",
                "code": "24331-1",
                "display": "Lipid panel - Serum or Plasma"
            }]},
            "entry": [{"reference": "DiagnosticReport/synthetic-report"}]
        }]
    });
    let report = serde_json::json!({
        "resourceType": "DiagnosticReport",
        "id": "synthetic-report",
        "meta": {"profile": [DIAGNOSTIC_REPORT_EU_LAB]},
        "identifier": [{"value": "synthetic-report-0003"}],
        "status": "final",
        "code": {"coding": [{
            "system": "http://loinc.org",
            "code": "24331-1",
            "display": "Lipid panel - Serum or Plasma"
        }]},
        "subject": {"reference": "Patient/synthetic-patient"},
        "effectiveDateTime": "2026-03-02T08:15:00+01:00",
        "result": [{"reference": "Observation/synthetic-cholesterol"}],
        "conclusion": "A synthetic conclusion."
    });
    let result = serde_json::json!({
        "resourceType": "Observation",
        "id": "synthetic-cholesterol",
        "status": "final",
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
    (
        composition,
        vec![
            (String::from("DiagnosticReport/synthetic-report"), report),
            (String::from("Observation/synthetic-cholesterol"), result),
        ],
    )
}

/// Runs the synthetic document into openEHR and back, returning the
/// Composition and the resources the run out of openEHR created.
fn round_trip() -> Result<(serde_json::Value, Vec<serde_json::Value>), Box<dyn Error>> {
    let (program, index) = program()?;
    let (composition, referenced) = document();
    let bundle = BundleReferences::new(
        referenced
            .into_iter()
            .map(|(reference, resource)| (Some(reference), Value::from_serde_json(resource)))
            .collect(),
        &NoReferences,
    );
    let into = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(composition),
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
    let composition = back
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Composition"))?;
    Ok((composition, created))
}

/// The resources of `kind` among `created`.
fn of_kind<'a>(created: &'a [serde_json::Value], kind: &str) -> Vec<&'a serde_json::Value> {
    created
        .iter()
        .filter(|resource| {
            resource
                .get("resourceType")
                .and_then(serde_json::Value::as_str)
                == Some(kind)
        })
        .collect()
}

#[test]
fn the_profile_elements_of_the_document_composition_survive_the_round_trip()
-> Result<(), Box<dyn Error>> {
    let (input, _) = document();
    let (output, created) = round_trip()?;
    assert_eq!(
        output
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(COMPOSITION_EU_LAB),
        "the composition names its profile: {output}"
    );
    for pointer in [
        "/identifier/value",
        "/status",
        "/type/coding/0",
        "/date",
        "/author/0/display",
        "/title",
        "/section/0/code/coding/0",
    ] {
        assert_eq!(
            output.pointer(pointer),
            input.pointer(pointer),
            "{pointer}: {output}"
        );
    }
    let reports = of_kind(&created, "DiagnosticReport");
    let [report] = reports.as_slice() else {
        return Err(format!("the run created {} reports, not one", reports.len()).into());
    };
    assert_eq!(
        output
            .pointer("/section/0/entry/0/reference")
            .and_then(serde_json::Value::as_str),
        report
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(|id| format!("DiagnosticReport/{id}"))
            .as_deref(),
        "the section references the report it created: {output}"
    );
    Ok(())
}

/// The report the section references reads its identifier and status out of
/// openEHR from the nodes the document's Composition writes going in.
#[test]
fn the_report_of_the_document_carries_the_report_status_and_identifier()
-> Result<(), Box<dyn Error>> {
    let (_, referenced) = document();
    let input = referenced
        .into_iter()
        .find(|(reference, _)| reference.starts_with("DiagnosticReport/"))
        .map(|(_, report)| report)
        .ok_or("the document carries no report")?;
    let (_, created) = round_trip()?;
    let reports = of_kind(&created, "DiagnosticReport");
    let [report] = reports.as_slice() else {
        return Err(format!("the run created {} reports, not one", reports.len()).into());
    };
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
    assert_eq!(of_kind(&created, "Observation").len(), 1, "one result");
    Ok(())
}
