// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 Europe Base and Core 2.0.0 `patient-eu-core` round trip: a
//! FerroBRIDGE context over the published `KDS_Person` template, loading the
//! person chain `projects/ferrobridge/kds_person` loads, with this crate's
//! gender extension naming the profile in place of the published
//! `KDS_gender.v1`.
//!
//! `patient-eu-core` requires `name` and `birthDate`; the synthetic
//! `Patient` carries both, with `gender`, and each must come back out of
//! openEHR as it went in, under the profile's canonical.

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
use crate::person::kds_person_template;
use crate::support::FIXTURES;
use crate::support::render;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The published files the context loads, as the library ships them: the
/// person chain without `KDS_gender.v1`.
const PUBLISHED: &[&str] = &[
    "model/admin_entry/org.highmed/person_data.v0.yml",
    "model/admin_entry/org.highmed/versicherungsinformationen.v0.yml",
    "model/cluster/org.openehr/structured_name.v1.yml",
    "model/cluster/org.openehr/person_birth_data_iso.v0.yml",
    "model/cluster/org.openehr/death_details.v1.yml",
    "model/cluster/org.openehr/organisation.v1.yml",
    "projects/org.highmed/KDS/person/KDS_Composition.yml",
];

/// The stand-ins of `projects/ferrobridge/kds_person` the context loads.
const KDS_PERSON: &[&str] = &[
    "ferrobridge_person.v1.yml",
    "ferrobridge_address.v1.yml",
    "ferrobridge_person.v1.Patient.yml",
    "ferrobridge_gender.v1.yml",
    "ferrobridge_kds_admin_entry_person.yml",
    "ferrobridge_kds_cluster_person.yml",
    "ferrobridge_kds_address.yml",
    "ferrobridge_kds_structured_name.yml",
];

/// The files of this crate's EU patient directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_eu_patient.context.yml",
    "ferrobridge_eu_gender.yml",
];

/// The context this crate authored for the profile.
const CONTEXT: &str = "ferrobridge_eu_patient.context";

/// The canonical of the profile.
const PATIENT_EU_CORE: &str = "http://hl7.eu/fhir/base/StructureDefinition/patient-eu-core";

/// Compiles the context against the published template.
fn eu_patient_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_person_template()?;
    let files: Vec<String> = PUBLISHED
        .iter()
        .map(|file| format!("{CORPUS}/{file}"))
        .chain(
            KDS_PERSON
                .iter()
                .map(|file| format!("{FIXTURES}/projects/ferrobridge/kds_person/{file}")),
        )
        .chain(
            PROJECT
                .iter()
                .map(|file| format!("{FIXTURES}/projects/ferrobridge/eu_patient/{file}")),
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

/// A synthetic `Patient` carrying the elements `patient-eu-core` requires.
fn patient() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Patient",
        "meta": {"profile": [PATIENT_EU_CORE]},
        "identifier": [{
            "system": "http://example.org/fhir/sid/ferrobridge-patient",
            "value": "synthetic-eu-patient-0001"
        }],
        "name": [{"use": "official", "family": "Synthetic", "given": ["Ada"]}],
        "gender": "female",
        "birthDate": "1970-01-01"
    })
}

#[test]
fn the_eu_patient_context_compiles_against_the_published_template() -> Result<(), Box<dyn Error>> {
    let (program, _) = eu_patient_program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Person");
    assert_eq!(program.resource().as_str(), "Patient");
    insta::assert_snapshot!("eu_patient_program", program.to_string());
    Ok(())
}

#[test]
fn the_profile_elements_of_patient_eu_core_survive_the_round_trip() -> Result<(), Box<dyn Error>> {
    let (program, index) = eu_patient_program()?;
    let input = patient();
    let into = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(input.clone()),
        &Seams::default(),
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
    let output = back
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Patient"))?;
    assert_eq!(
        output
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(PATIENT_EU_CORE),
        "the resource names the profile: {output}"
    );
    assert_eq!(
        output.get("birthDate"),
        input.get("birthDate"),
        "the required birthDate: {output}"
    );
    assert_eq!(
        output.pointer("/name/0/family"),
        input.pointer("/name/0/family"),
        "the required name, its family part: {output}"
    );
    assert_eq!(
        output.pointer("/name/0/given"),
        input.pointer("/name/0/given"),
        "the required name, its given part: {output}"
    );
    assert_eq!(
        output.get("gender"),
        input.get("gender"),
        "gender: {output}"
    );
    Ok(())
}

/// `PutGet` over the synthetic patient, its declared set pinned.
#[test]
fn putget_holds_on_the_eu_patient() -> Result<(), Box<dyn Error>> {
    let (program, index) = eu_patient_program()?;
    let declared = laws::putget(&program, &index, &patient(), &laws::untouched)?;
    insta::assert_json_snapshot!("eu_patient_putget", declared);
    Ok(())
}
