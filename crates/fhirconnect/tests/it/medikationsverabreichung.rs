// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `MedicationAdministration` of a KDS medication administration:
//! the published KDS medication extension and the library's dosage model of
//! the FHIRconnect mapping library, under a FerroBRIDGE project context
//! compiled against the published `KDS_Medikamentenverabreichungen`
//! operational template.
//!
//! The published files load verbatim from the vendored library. Where a
//! published file does not load or compile, its refusal is asserted here and
//! the project directory carries a stand-in that says what it departs from.
//! The administered `Medication` is a referenced resource
//! (`types-of-mappings/concept-type/Reference.adoc`): going in the reference
//! seam serves it, and coming out the run creates it.

use core::error::Error;
use std::collections::BTreeMap;
use std::sync::Arc;

use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::library::Library;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::load::load_set;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::engine::MapReferences;
use crate::laws;
use crate::support::FIXTURES;
use crate::support::render;
use crate::support::rows;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The published `KDS_Medikamentenverabreichungen` operational template of
/// the testkit.
const TEMPLATE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt/kds-medikamentenverabreichungen/KDS_Medikamentenverabreichungen.opt"
);

/// The published files the project context loads and runs verbatim, as the
/// library ships them.
pub(crate) const PUBLISHED: &[&str] = &[
    "projects/org.highmed/KDS/medikationsverabreichung/KDS_medication.yml",
    "model/cluster/org.openehr/dosage.v2.BackboneElement.yml",
];

/// The published substance model, loaded because the medication stand-in
/// names it and reached by no mapping: `KDS_medication.v2` overwrites the
/// `ingredients` method that slots it.
const LOADED: &str = "model/cluster/org.openehr/medication.v2.substance.yml";

/// The project directory this crate authored for the chain.
const PROJECT_DIR: &str = "projects/ferrobridge/kds_medikationsverabreichung";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_kds_medikationsverabreichung.context.yml",
    "ferrobridge_kds_medikamentenverabreichung.yml",
    "ferrobridge_medication.v1.yml",
    "ferrobridge_medication.v2.yml",
    "ferrobridge_report.v1.MedicationAdministration.yml",
];

/// The context this crate authored over the chain.
const CONTEXT: &str = "ferrobridge_kds_medikationsverabreichung.context";

/// The literal reference the synthetic administration names its medication by.
const MEDICATION: &str = "Medication/synthetic-medication-1";

/// Returns the absolute path of a file of the vendored library.
fn published(file: &str) -> String {
    format!("{CORPUS}/{file}")
}

/// Returns the absolute path of a file of the project directory.
fn project(file: &str) -> String {
    format!("{FIXTURES}/{PROJECT_DIR}/{file}")
}

/// Builds the index over the published `KDS_Medikamentenverabreichungen`
/// template.
fn kds_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(&std::fs::read_to_string(TEMPLATE)?)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Loads the published files and the project directory into one set.
fn kds_set() -> Result<MappingSet, Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain([&LOADED])
        .map(|file| published(file))
        .chain(PROJECT.iter().map(|file| project(file)))
        .collect();
    load_set(files, &Library).map_err(|diagnostics| render(&diagnostics).into())
}

/// Compiles the project context against the published template.
fn kds_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_template()?;
    let set = kds_set()?;
    let program = compile(
        &set,
        &MappingName::new(CONTEXT)?,
        &index,
        &SCHEMAS,
        &Library,
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    Ok((program, index))
}

#[test]
fn the_kds_medikationsverabreichung_context_compiles_against_the_published_template()
-> Result<(), Box<dyn Error>> {
    let (program, _) = kds_program()?;
    assert_eq!(
        program.template().id().as_str(),
        "KDS_Medikamentenverabreichungen"
    );
    assert_eq!(program.resource().as_str(), "MedicationAdministration");
    Ok(())
}

/// Compiles the project context and returns it with the published files it
/// loads, relative to the vendored library.
pub(crate) fn kds_medikationsverabreichung_chain() -> Result<crate::support::Chain, Box<dyn Error>>
{
    let (program, _) = kds_program()?;
    Ok((PUBLISHED, program))
}

/// Returns a synthetic MII medication administration: one tablet of a
/// referenced medication, given and completed, with its reason and dose.
fn kds_administration() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "MedicationAdministration",
        "status": "completed",
        "medicationReference": {"reference": MEDICATION},
        "subject": {"reference": "Patient/synthetic-1"},
        "effectiveDateTime": "2026-09-01T08:00:00+02:00",
        "reasonCode": [{"coding": [{"display": "Arterielle Hypertonie"}]}],
        "note": [{"text": "Mit Wasser eingenommen"}],
        "dosage": {
            "text": "1 Tablette morgens",
            "dose": {
                "value": 1,
                "unit": "1",
                "system": "http://unitsofmeasure.org",
                "code": "1"
            }
        }
    })
}

/// Returns the synthetic medication the administration references: a
/// tablet with one active ingredient and its strength, in the unit `1` the
/// template admits.
fn kds_medication() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Medication",
        "code": {"coding": [{"display": "Ramipril 5 mg Tabletten"}]},
        "form": {
            "coding": [{
                "system": "http://standardterms.edqm.eu",
                "code": "10219000",
                "display": "Tablet"
            }]
        },
        "ingredient": [{
            "itemCodeableConcept": {
                "text": "Ramipril",
                "coding": [{"display": "Active"}]
            },
            "strength": {
                "numerator": {
                    "value": 5,
                    "unit": "1",
                    "system": "http://unitsofmeasure.org",
                    "code": "1"
                },
                "denominator": {
                    "value": 1,
                    "unit": "1",
                    "system": "http://unitsofmeasure.org",
                    "code": "1"
                }
            }
        }]
    })
}

/// `PutGet` over the administration and its referenced medication, with what
/// the chain declares lost: the administration through
/// [`laws::putget_with`], and the medication the run creates coming out
/// against the one the seam served going in.
#[test]
fn putget_holds_on_the_kds_medication_administration() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_program()?;
    let references = MapReferences(BTreeMap::from([(
        String::from(MEDICATION),
        Value::from_serde_json(kds_medication()),
    )]));
    let seams = Seams::default()
        .with_references(&references)
        .with_functions(&Library);
    let declared = laws::putget_with(
        &program,
        &index,
        &kds_administration(),
        &laws::untouched,
        &seams,
    )?;
    insta::assert_json_snapshot!("kds_medikationsverabreichung_putget", declared);

    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(kds_administration()),
        &seams,
        &laws::defaults(),
        &CallContext::new(),
    )?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &seams,
        &CallContext::new(),
    )?;
    let created = outbound
        .created()
        .iter()
        .map(|resource| resource.to_serde_json(&mut Path::root("Medication")))
        .collect::<Result<Vec<serde_json::Value>, _>>()?;
    insta::assert_json_snapshot!("kds_medikationsverabreichung_medication", created);
    Ok(())
}

/// The published models, extensions and context the project context
/// replaces or leaves out, as the library ships them.
const REPLACED: &[&str] = &[
    "model/action/org.openehr/medication.v1.yml",
    "model/cluster/org.openehr/medication.v2.yml",
    "model/cluster/org.openehr/case_identification.v0.yml",
    "projects/org.highmed/KDS/medikationsverabreichung/KDS_medikamentenverabreichung.yml",
    "projects/org.highmed/KDS/medikationsverabreichung/KDS_composition.yml",
    "projects/org.highmed/KDS/medikationsverabreichung/KDS_medication_substance.yml",
    "projects/org.highmed/KDS/medikationsverabreichung/KDS_medikationsverabreichung.context.yaml",
];

/// The published context over the published files, with the composition
/// stand-in in place of the model the corpus set refuses at
/// `report.v1.MedicationAdministration.yml:60` (`fc-condition-target-root-mismatch`),
/// is refused exactly; each refusal is recorded on #101:
///
/// - `medication.v1.yml:15` to `:30`: a hierarchy over `data[at0001]/events[at0002]`,
///   which an ACTION does not have, split on `MedicationAdministration.dosage.timing`,
///   which R4 does not define.
/// - `medication.v1.yml:47`: `MedicationAdministration.actor`, which R4 does
///   not define.
/// - `medication.v1.yml:77` and `medication.v2.yml:72`, `:85`: a `reference`
///   written into a choice without naming its type.
/// - `medication.v1.yml` ISM transition rows and the composition's
///   `category` rows: `defining_code/terminology_id` where
///   `types-of-mappings/concept-type/manual.adoc` writes
///   `defining_code/terminology_id/value`.
/// - `medication.v2.yml:25`: the Ratio `Medication.amount` onto a
///   `DV_QUANTITY`.
/// - `KDS_medikamentenverabreichung.yml:38` to `:54`: a condition in an
///   extension method naming nodes the template excludes.
/// - `KDS_composition.yml:12`, `:18` and `case_identification.v0.yml:16`: an
///   `Identifier` and the code `status` onto `DV_CODED_TEXT` nodes.
#[test]
fn the_kds_medikationsverabreichung_chain_as_published_is_refused_exactly()
-> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain([&LOADED])
        .chain(REPLACED)
        .map(|file| published(file))
        .chain([project(
            "ferrobridge_report.v1.MedicationAdministration.yml",
        )])
        .collect();
    let set = load_set(files, &Library).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new("KDS_medikationsverabreichung.context")?,
        &kds_template()?,
        &SCHEMAS,
        &Library,
    )
    .err()
    .ok_or("the medication administration chain as published compiles")?;
    insta::assert_json_snapshot!(
        "kds_medikationsverabreichung_chain_as_published",
        rows(&diagnostics)
    );
    Ok(())
}

/// The published files the substance context loads and runs verbatim.
pub(crate) const SUBSTANCE_PUBLISHED: &[&str] = &[
    "projects/org.highmed/KDS/medikationsverabreichung/KDS_medication_substance.yml",
    "model/cluster/org.openehr/medication.v2.substance.yml",
];

/// The files of the project directory the substance context loads, the
/// context first.
const SUBSTANCE_PROJECT: &[&str] = &[
    "ferrobridge_kds_substance.context.yml",
    "ferrobridge_kds_substance.yml",
];

/// Compiles the substance context against the published template.
fn kds_substance_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_template()?;
    let files: Vec<String> = SUBSTANCE_PUBLISHED
        .iter()
        .map(|file| published(file))
        .chain(SUBSTANCE_PROJECT.iter().map(|file| project(file)))
        .collect();
    let set = load_set(files, &Library).map_err(|diagnostics| render(&diagnostics))?;
    let program = compile(
        &set,
        &MappingName::new("ferrobridge_kds_substance.context")?,
        &index,
        &SCHEMAS,
        &Library,
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    Ok((program, index))
}

/// Compiles the substance context and returns it with the published files
/// it loads, relative to the vendored library.
pub(crate) fn kds_substance_chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, _) = kds_substance_program()?;
    Ok((SUBSTANCE_PUBLISHED, program))
}

#[test]
fn the_kds_substance_extension_applies_to_the_substance_cluster() -> Result<(), Box<dyn Error>> {
    let (program, _) = kds_substance_program()?;
    assert_eq!(program.resource().as_str(), "Substance");
    let substance = program
        .models()
        .iter()
        .find(|model| model.name().as_str() == "CLUSTER.medication.v2.substance")
        .ok_or("the program compiles no substance cluster")?;
    assert_eq!(
        substance
            .extensions()
            .iter()
            .map(MappingName::as_str)
            .collect::<Vec<_>>(),
        ["KDS_medication.v2.substance"]
    );
    Ok(())
}

/// `PutGet` over a synthetic substance: the KDS extension writes the coding
/// display as the cluster's name.
#[test]
fn putget_holds_on_a_kds_substance() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_substance_program()?;
    let substance = serde_json::json!({
        "resourceType": "Substance",
        "code": {"coding": [{"display": "Ramipril"}]}
    });
    let declared = laws::putget(&program, &index, &substance, &laws::untouched)?;
    insta::assert_json_snapshot!("kds_substance_putget", declared);
    Ok(())
}
