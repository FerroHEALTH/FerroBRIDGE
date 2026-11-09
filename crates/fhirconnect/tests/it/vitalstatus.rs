// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Observation` of a KDS vital status: the published KDS vital
//! status extension of the FHIRconnect mapping library, under a FerroBRIDGE
//! project context compiled against the published `KDS_Vitalstatus`
//! operational template.
//!
//! The published extension loads verbatim from the vendored library. Where a
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

/// The published `KDS_Vitalstatus` operational template of the testkit.
const TEMPLATE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt/kds-vitalstatus/KDS_Vitalstatus.opt"
);

/// The published files the project context loads, as the library ships them.
pub(crate) const PUBLISHED: &[&str] = &["projects/org.highmed/KDS/vitalstatus/KDS_vitalsigns.yml"];

/// The project directory this crate authored for the chain.
const PROJECT_DIR: &str = "projects/ferrobridge/kds_vitalstatus";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_kds_vitalstatus.context.yml",
    "ferrobridge_vital_status.v1.yml",
    "ferrobridge_report.v1.Observation.yml",
];

/// The context this crate authored over the chain.
const CONTEXT: &str = "ferrobridge_kds_vitalstatus.context";

/// Returns the absolute path of a file of the vendored library.
fn published(file: &str) -> String {
    format!("{CORPUS}/{file}")
}

/// Returns the absolute path of a file of the project directory.
fn project(file: &str) -> String {
    format!("{FIXTURES}/{PROJECT_DIR}/{file}")
}

/// Builds the index over the published `KDS_Vitalstatus` template.
fn kds_vitalstatus_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(&std::fs::read_to_string(TEMPLATE)?)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Loads the published files and the project directory into one set.
fn kds_vitalstatus_set() -> Result<MappingSet, Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .map(|file| published(file))
        .chain(PROJECT.iter().map(|file| project(file)))
        .collect();
    load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics).into())
}

/// Compiles the project context against the published template.
fn kds_vitalstatus_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_vitalstatus_template()?;
    let set = kds_vitalstatus_set()?;
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
fn the_kds_vitalstatus_context_compiles_against_the_published_template()
-> Result<(), Box<dyn Error>> {
    let (program, _) = kds_vitalstatus_program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Vitalstatus");
    assert_eq!(program.resource().as_str(), "Observation");
    Ok(())
}

/// Compiles the project context and returns it with the published files it
/// loads, relative to the vendored library.
pub(crate) fn kds_vitalstatus_chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, _) = kds_vitalstatus_program()?;
    Ok((PUBLISHED, program))
}

/// Returns a synthetic MII vital status `Observation`: the patient alive at
/// the last update.
fn kds_vital_status() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Observation",
        "status": "final",
        "category": [{
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/observation-category",
                "code": "survey"
            }]
        }],
        "code": {
            "coding": [{"system": "http://loinc.org", "code": "67162-8"}]
        },
        "subject": {"reference": "Patient/synthetic-1"},
        "effectiveDateTime": "2026-09-11T10:30:00+02:00",
        "issued": "2026-09-12T08:00:00+02:00",
        "valueCodeableConcept": {
            "coding": [{
                "system": "https://www.medizininformatik-initiative.de/fhir/core/modul-person/CodeSystem/Vitalstatus",
                "code": "L"
            }]
        },
        "note": [{"text": "synthetic note"}]
    })
}

/// `PutGet` over the vital status, with what the chain declares lost.
#[test]
fn putget_holds_on_the_kds_vital_status() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_vitalstatus_program()?;
    let declared = laws::putget(&program, &index, &kds_vital_status(), &laws::untouched)?;
    insta::assert_json_snapshot!("kds_vitalstatus_putget", declared);
    Ok(())
}

/// The published models, composition extension and context the project
/// context replaces or leaves out, as the library ships them.
const REPLACED: &[&str] = &[
    "model/evaluation/org.openehr/vital_status.v1.yml",
    "model/composition/org.openehr/report.v1.Observation.yml",
    "model/cluster/org.openehr/case_identification.v0.yml",
    "projects/org.highmed/KDS/vitalstatus/KDS_composition.yml",
    "projects/org.highmed/KDS/vitalstatus/KDS_Vitalstatus.context.yaml",
];

/// The published context over the published files is refused exactly; each
/// refusal is recorded on #101:
///
/// - `vital_status.v1.yml:36` to `:38` and `:67` to `:69`: `function` and
///   `performer` below a `PARTICIPATION` list, which the engine carries only
///   through `participationsFunction`; `:51`: `$archetype/perfomer`.
/// - `report.v1.Observation.yml`: the Condition model under an Observation
///   header (`onset`, `recordedDate`, `asserter`, `recorder`, `coding.code`
///   of the code `status`), with `null_flavour` below a `DV_DATE_TIME` and a
///   `PARTY_PROXY`.
/// - `case_identification.v0.yml:16`: an `Identifier` onto the
///   `DV_CODED_TEXT` `KDS_Vitalstatus` constrains.
/// - `KDS_composition.yml:40`: the code `status` onto the `DV_CODED_TEXT`
///   at0005.
#[test]
fn the_kds_vitalstatus_chain_as_published_is_refused_exactly() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(REPLACED)
        .map(|file| published(file))
        .collect();
    let codes = StaticMappingCodes::default();
    let set = load_set(files, &codes).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new("KDS_Vitalstatus.context")?,
        &kds_vitalstatus_template()?,
        &SCHEMAS,
        &codes,
    )
    .err()
    .ok_or("the vital status chain as published compiles")?;
    insta::assert_json_snapshot!("kds_vitalstatus_chain_as_published", rows(&diagnostics));
    Ok(())
}
