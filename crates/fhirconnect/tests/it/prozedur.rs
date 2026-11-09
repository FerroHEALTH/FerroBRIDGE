// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Procedure` of a KDS procedure: the published KDS procedure chain
//! of the FHIRconnect mapping library, under a FerroBRIDGE project context
//! compiled against the published `KDS_Prozedur` operational template.
//!
//! The published extensions load verbatim from the vendored library. Where a
//! published file does not load or compile, its refusal is asserted here and
//! the project directory carries a stand-in that says what it departs from.

use core::error::Error;
use std::sync::Arc;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::model::load::MappingSet;
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
use crate::support::rows;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The published `KDS_Prozedur` operational template of the testkit.
const TEMPLATE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt/kds-prozedur/KDS_Prozedur.opt"
);

/// The published files the project context loads, as the library ships them.
pub(crate) const PUBLISHED: &[&str] = &[
    "projects/org.highmed/KDS/procedure/KDS_procedure.v1.yml",
    "projects/org.highmed/KDS/procedure/KDS_anatomical_location.yml",
];

/// The project directory this crate authored for the chain.
const PROJECT_DIR: &str = "projects/ferrobridge/kds_prozedur";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_kds_prozedur.context.yml",
    "ferrobridge_procedure.v1.yml",
    "ferrobridge_report.v1.Procedure.yml",
    "ferrobridge_anatomical_location.v1.yml",
    "ferrobridge_procedure_name.yml",
];

/// The context this crate authored over the chain.
const CONTEXT: &str = "ferrobridge_kds_prozedur.context";

/// Returns the absolute path of a file of the vendored library.
fn published(file: &str) -> String {
    format!("{CORPUS}/{file}")
}

/// Returns the absolute path of a file of the project directory.
fn project(file: &str) -> String {
    format!("{FIXTURES}/{PROJECT_DIR}/{file}")
}

/// Builds the index over the published `KDS_Prozedur` template.
fn kds_prozedur_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(&std::fs::read_to_string(TEMPLATE)?)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Loads the published files and the project directory into one set.
fn kds_prozedur_set() -> Result<MappingSet, Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .map(|file| published(file))
        .chain(PROJECT.iter().map(|file| project(file)))
        .collect();
    load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics).into())
}

/// Compiles the project context against the published template.
fn kds_prozedur_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_prozedur_template()?;
    let set = kds_prozedur_set()?;
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
fn the_kds_prozedur_context_compiles_against_the_published_template() -> Result<(), Box<dyn Error>>
{
    let (program, _) = kds_prozedur_program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Prozedur");
    assert_eq!(program.resource().as_str(), "Procedure");
    Ok(())
}

/// Compiles the project context and returns it with the published files it
/// loads, relative to the vendored library.
pub(crate) fn kds_prozedur_chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, _) = kds_prozedur_program()?;
    Ok((PUBLISHED, program))
}

/// Returns a synthetic MII procedure: an OPS-coded appendectomy on the
/// right side, with its intent extension.
fn kds_procedure() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Procedure",
        "extension": [{
            "url": "https://www.medizininformatik-initiative.de/fhir/core/modul-prozedur/StructureDefinition/Durchfuehrungsabsicht",
            "valueCoding": {"system": "http://snomed.info/sct", "code": "262202000"}
        }],
        "status": "completed",
        "category": {
            "coding": [{"system": "http://snomed.info/sct", "code": "387713003"}]
        },
        "code": {
            "coding": [{
                "system": "http://fhir.de/CodeSystem/bfarm/ops",
                "version": "2025",
                "code": "5-470.11"
            }]
        },
        "subject": {"reference": "Patient/synthetic-1"},
        "performedDateTime": "2026-09-11T10:30:00+02:00",
        "bodySite": [{
            "coding": [{"system": "http://snomed.info/sct", "code": "66754008"}]
        }],
        "note": [{"text": "synthetic note"}]
    })
}

/// `PutGet` over the procedure, with what the chain declares lost.
#[test]
fn putget_holds_on_the_kds_procedure() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_prozedur_program()?;
    let declared = laws::putget(&program, &index, &kds_procedure(), &laws::untouched)?;
    insta::assert_json_snapshot!("kds_prozedur_putget", declared);
    Ok(())
}

/// The published models, composition extension and context the project
/// context replaces or leaves out, as the library ships them.
const REPLACED: &[&str] = &[
    "model/composition/org.openehr/report.v1.Procedure.yml",
    "model/cluster/org.openehr/anatomical_location.v1.yml",
    "model/cluster/org.openehr/case_identification.v0.yml",
    "projects/org.highmed/KDS/procedure/KDS_composition.yml",
    "projects/org.highmed/KDS/procedure/procedure.context.yaml",
];

/// The published procedure model, which the whole set refuses at load.
const PROCEDURE: &str = "model/action/org.openehr/procedure.v1.yml";

/// The published chain loaded as the library ships it is refused at load:
/// `procedure.v1.yml` conditions `effectivePeriodContext` on
/// `$composition/context`, a child of its `with` path (#101, #1118).
#[test]
fn the_published_procedure_model_is_refused_at_load() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(REPLACED)
        .chain(&[PROCEDURE])
        .map(|file| published(file))
        .collect();
    let diagnostics = load_set(files, &StaticMappingCodes::default())
        .err()
        .ok_or("the published procedure model loads")?;
    insta::assert_json_snapshot!("kds_prozedur_model_at_load", rows(&diagnostics));
    Ok(())
}

/// The published context over the published files, with only the procedure
/// model replaced by its stand-in less the name overwrite, is refused
/// exactly; each refusal is recorded on #101.
#[test]
fn the_kds_prozedur_chain_as_published_is_refused_exactly() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(REPLACED)
        .map(|file| published(file))
        .chain([project("ferrobridge_procedure.v1.yml")])
        .collect();
    let codes = StaticMappingCodes::default();
    let set = load_set(files, &codes).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new("KDS_Prozedur.context")?,
        &kds_prozedur_template()?,
        &SCHEMAS,
        &codes,
    )
    .err()
    .ok_or("the procedure chain as published compiles")?;
    insta::assert_json_snapshot!("kds_prozedur_chain_as_published", rows(&diagnostics));
    Ok(())
}
