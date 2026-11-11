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
use fhirconnect::model::load::load_set_from_text;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::laws;
use crate::support::SHIPPED;
use crate::support::render;

/// The files the context loads from the shipped laboratory project, the
/// context first.
const SHIPPED_LABORATORY: &[&str] = &[
    "ferrobridge_eu_laboratory.context.yml",
    "ferrobridge_eu_laboratory_result.yml",
    "ferrobridge_laboratory_test_result.v1.yml",
    "ferrobridge_laboratory_test_analyte.v1.yml",
    "ferrobridge_specimen.v1.yml",
    "ferrobridge_eu_laboratory_analyte.yml",
    "ferrobridge_eu_laboratory_specimen.yml",
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
    let files: Vec<String> = SHIPPED_LABORATORY
        .iter()
        .map(|file| format!("{SHIPPED}/laboratory/{file}"))
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

/// An extension of the analyte that names `tail` below the polymorphic
/// analyte result, `items[at0001]`, which `KDS_Laborbericht` admits as a
/// `DV_QUANTITY`, a `DV_CODED_TEXT` or a `DV_TEXT`.
fn tail_extension(tail: &str) -> String {
    format!(
        "grammar: FHIRConnect/v1.0.0
type: extension
metadata:
  name: ferrobridge_analyte_tail
  version: 1.0.0
spec:
  system: FHIR
  version: R4
  extends: CLUSTER.laboratory_test_analyte.v1
mappings:
  - name: \"tail\"
    extension: \"add\"
    with:
      fhir: \"$resource.note.text\"
      openehr: \"$archetype/items[at0001]/value/{tail}\"
"
    )
}

/// Compiles the shipped context with the analyte extension `extension`
/// beside the shipped ones.
fn compiled_with_tail(extension: &str) -> Result<Arc<Program>, Vec<String>> {
    let index = kds_laborbericht_template().map_err(|error| vec![error.to_string()])?;
    let mut files: Vec<(String, String)> = SHIPPED_LABORATORY
        .iter()
        .map(|file| {
            let path = format!("{SHIPPED}/laboratory/{file}");
            std::fs::read_to_string(&path).map(|text| (path, text))
        })
        .collect::<Result<_, _>>()
        .map_err(|error| vec![error.to_string()])?;
    if let Some((_, context)) = files.first_mut() {
        *context = context.replace(
            "    - \"ferrobridge_eu_laboratory_specimen\"",
            "    - \"ferrobridge_eu_laboratory_specimen\"\n    - \"ferrobridge_analyte_tail\"",
        );
    }
    files.push((
        String::from("ferrobridge_analyte_tail.yml"),
        String::from(extension),
    ));
    let set = load_set_from_text(files, &StaticMappingCodes::default()).map_err(|diagnostics| {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message().to_owned())
            .collect::<Vec<_>>()
    })?;
    let name = MappingName::new(CONTEXT).map_err(|error| vec![error.to_string()])?;
    compile(
        &set,
        &name,
        &index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message().to_owned())
            .collect()
    })
}

// NOTE: openEHR RM data_types §DV_ORDERED: `normal_range` is an attribute of an ordered value,
// so of the three alternatives the quantity alone carries it, and a path below it compiles.
#[test]
fn a_tail_one_value_alternative_carries_selects_that_alternative() -> Result<(), Box<dyn Error>> {
    let program = compiled_with_tail(&tail_extension("normal_range/lower/units"))
        .map_err(|diagnostics| diagnostics.join("; "))?;
    let rendered = program.to_string();
    assert!(
        rendered.contains("DV_QUANTITY tail normal_range/lower/units"),
        "the quantity alternative carries the tail: {rendered}"
    );
    Ok(())
}

// NOTE: openEHR RM data_types §DV_TEXT: `DV_CODED_TEXT` inherits `value`, so two alternatives
// carry the tail and the path stays ambiguous.
#[test]
fn a_tail_several_value_alternatives_carry_stays_ambiguous() {
    let refused = compiled_with_tail(&tail_extension("value"))
        .err()
        .unwrap_or_default();
    assert!(
        refused
            .iter()
            .any(|message| message.contains("names 3 nodes")),
        "the ambiguous tail is refused: {refused:?}"
    );
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
            "code": "laboratory",
            "display": "Laboratory"
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

/// The library's `EEHRxF` lab analyte extension, which counts once its
/// context runs.
const LAB_ANALYTE: &[&str] = &["projects/org.openehr/EEHRxF/lab/bundle/lab_analyte.yml"];

/// The context FerroBRIDGE authored over the lab analyte extension.
const LAB_ANALYTE_CONTEXT: &str = "ferrobridge_library_lab_analyte.context";

/// Compiles the laboratory context with the library's lab analyte extension
/// applied after the shipped ones.
///
/// `eehrxf_lab_analyte` slots the analyte cluster into itself through
/// `hasMember`, and FHIRconnect v1.0.0 does not say where such a recursion
/// ends. The compiler unfolds it as deep as the template instantiates it
/// (our own design, `docs/architecture.md`): `KDS_Laborbericht` nests no
/// analyte in an analyte, so the self-slot is left out at the first level.
fn lab_analyte_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_laborbericht_template()?;
    let files: Vec<String> = SHIPPED_LABORATORY
        .iter()
        .skip(1)
        .map(|file| format!("{SHIPPED}/laboratory/{file}"))
        .chain(LAB_ANALYTE.iter().map(|file| {
            format!(
                "{}/../../docs/specs/fhirconnect-mapping-lib/{file}",
                env!("CARGO_MANIFEST_DIR")
            )
        }))
        .chain([format!(
            "{}/tests/fixtures/projects/ferrobridge/library/{LAB_ANALYTE_CONTEXT}.yml",
            env!("CARGO_MANIFEST_DIR")
        )])
        .collect();
    let set = load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics))?;
    let program = compile(
        &set,
        &MappingName::new(LAB_ANALYTE_CONTEXT)?,
        &index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    Ok((program, index))
}

/// Compiles the lab analyte context and runs the synthetic report through
/// it both ways, and returns it with the published file it reaches.
pub(crate) fn lab_analyte_chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, index) = lab_analyte_program()?;
    round_trip(&program, &index)?;
    Ok((LAB_ANALYTE, program))
}

/// The recursive self-slot ends where the template stops nesting the
/// analyte, with no refusal and no warning, and the report comes back out
/// with the one result it referenced.
#[test]
fn the_self_slotting_lab_analyte_unfolds_as_deep_as_the_template() -> Result<(), Box<dyn Error>> {
    let (program, index) = lab_analyte_program()?;
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    let Outbound { results, .. } = round_trip(&program, &index)?;
    assert_eq!(results.len(), 1, "{results:?}");
    Ok(())
}

/// A slot of an archetype the chain does not hold, at a place the template
/// lacks below the analyte it does instantiate, is still refused: only a
/// recursion ends where the template stops.
#[test]
fn a_slot_off_the_recursion_at_a_missing_node_is_still_refused() {
    let extension = "grammar: FHIRConnect/v1.0.0
type: extension
metadata:
  name: ferrobridge_analyte_tail
  version: 1.0.0
spec:
  system: FHIR
  version: R4
  extends: CLUSTER.laboratory_test_analyte.v1
mappings:
  - name: \"specimenBelowAnalyte\"
    extension: \"add\"
    with:
      fhir: \"$resource.specimen\"
      openehr: \"$archetype/items[openEHR-EHR-CLUSTER.specimen.v1]\"
    slotArchetype: \"CLUSTER.specimen.v1\"
";
    let Err(refusals) = compiled_with_tail(extension) else {
        panic!("a specimen below the analyte compiles");
    };
    assert!(
        refusals
            .iter()
            .any(|refusal| refusal.contains("has no node at")),
        "{refusals:?}"
    );
}
