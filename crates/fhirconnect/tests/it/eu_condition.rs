// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 Europe Base and Core 2.0.0 `condition-eu-core` round trip: a
//! FerroBRIDGE context over the published `KDS_Diagnose` template, whose
//! diagnosis and qualifier archetypes are CKM's, with the published model
//! mappings loaded verbatim and this crate's extensions for the profile.
//!
//! The synthetic `Condition` carries the elements the profile constrains:
//! the required `code`, the `assertedDate` extension slice, the
//! `onsetDateTime` slice, and `clinicalStatus` and `verificationStatus`
//! under their required bindings. Each must come back out of openEHR as it
//! went in, and the resource must name the profile.

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;

use crate::laws;
use crate::roundtrip::kds_template;
use crate::support::FIXTURES;
use crate::support::render;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The published model mappings the context loads, as the library ships
/// them.
const PUBLISHED: &[&str] = &[
    "model/evaluation/org.openehr/problem_diagnosis.v1.yml",
    "model/cluster/org.openehr/problem_qualifier.v2.yml",
    "model/cluster/org.openehr/anatomical_location.v1.yml",
    "model/composition/org.openehr/report.v1.Condition.yml",
];

/// The project directory this crate authored for the profile.
const PROJECT_DIR: &str = "projects/ferrobridge/eu_condition";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_eu_condition.context.yml",
    "ferrobridge_eu_condition.yml",
    "ferrobridge_eu_problem_qualifier.yml",
    "ferrobridge_eu_composition.Condition.yml",
];

/// The context this crate authored for the profile.
const CONTEXT: &str = "ferrobridge_eu_condition.context";

/// The canonical of the profile.
const CONDITION_EU_CORE: &str = "http://hl7.eu/fhir/base/StructureDefinition/condition-eu-core";

/// Compiles the context against the published template.
fn eu_condition_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_template()?;
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

/// A synthetic `Condition` carrying the elements `condition-eu-core`
/// constrains.
fn condition() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Condition",
        "meta": {"profile": [CONDITION_EU_CORE]},
        "extension": [{
            "url": "http://hl7.org/fhir/StructureDefinition/condition-assertedDate",
            "valueDateTime": "2026-03-02T09:30:00+01:00"
        }],
        "clinicalStatus": {"coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/condition-clinical",
            "code": "active"
        }]},
        "verificationStatus": {"coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/condition-ver-status",
            "code": "confirmed",
            "display": "Confirmed"
        }]},
        "code": {"coding": [{
            "system": "http://snomed.info/sct",
            "code": "38341003",
            "display": "Hypertensive disorder, systemic arterial"
        }]},
        "subject": {"reference": "Patient/synthetic-patient"},
        "onsetDateTime": "2026-02-20T00:00:00+01:00",
        "recordedDate": "2026-03-02T09:30:00+01:00"
    })
}

/// Runs the condition into openEHR and back, returning the FHIR resource the
/// second leg writes.
fn round_trip(
    program: &Program,
    index: &WebTemplateIndex,
    resource: serde_json::Value,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let into = to_openehr(
        program,
        &SCHEMAS,
        index,
        &Value::from_serde_json(resource),
        &Seams::default(),
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
    Ok(back
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?)
}

#[test]
fn the_eu_condition_context_compiles_against_the_published_template() -> Result<(), Box<dyn Error>>
{
    let (program, _) = eu_condition_program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Diagnose");
    assert_eq!(program.resource().as_str(), "Condition");
    insta::assert_snapshot!("eu_condition_program", program.to_string());
    Ok(())
}

#[test]
fn the_profile_elements_of_condition_eu_core_survive_the_round_trip() -> Result<(), Box<dyn Error>>
{
    let (program, index) = eu_condition_program()?;
    let input = condition();
    let output = round_trip(&program, &index, input.clone())?;
    assert_eq!(
        output
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(CONDITION_EU_CORE),
        "the resource names the profile: {output}"
    );
    assert_eq!(
        output.pointer("/code/coding/0"),
        input.pointer("/code/coding/0"),
        "the required code: {output}"
    );
    let asserted = output
        .get("extension")
        .and_then(serde_json::Value::as_array)
        .and_then(|extensions| {
            extensions.iter().find(|extension| {
                extension.get("url").and_then(serde_json::Value::as_str)
                    == Some("http://hl7.org/fhir/StructureDefinition/condition-assertedDate")
            })
        })
        .ok_or("the assertedDate extension slice comes back")?;
    assert_eq!(
        asserted.get("valueDateTime"),
        input.pointer("/extension/0/valueDateTime"),
        "the assertedDate value: {output}"
    );
    assert_eq!(
        output.get("onsetDateTime"),
        input.get("onsetDateTime"),
        "the onsetDateTime slice: {output}"
    );
    assert_eq!(
        output.pointer("/clinicalStatus/coding/0/code"),
        input.pointer("/clinicalStatus/coding/0/code"),
        "clinicalStatus under its required binding: {output}"
    );
    assert_eq!(
        output.pointer("/verificationStatus/coding/0"),
        input.pointer("/verificationStatus/coding/0"),
        "verificationStatus under its required binding: {output}"
    );
    Ok(())
}

/// `PutGet` over the synthetic condition: what the two legs lose is
/// declared, and the declared set is pinned.
#[test]
fn putget_holds_on_the_eu_condition() -> Result<(), Box<dyn Error>> {
    let (program, index) = eu_condition_program()?;
    let declared = laws::putget(&program, &index, &condition(), &laws::untouched)?;
    insta::assert_json_snapshot!("eu_condition_putget", declared);
    Ok(())
}
