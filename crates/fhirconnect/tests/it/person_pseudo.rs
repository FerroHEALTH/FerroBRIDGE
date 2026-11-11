// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Patient` of a pseudonymised KDS person: the published KDS
//! pseudonymised person cluster extension of the FHIRconnect mapping library,
//! under a FerroBRIDGE project context compiled against the published
//! `KDS_Person_Pseudonymisiert` operational template.
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
use crate::support::Recorded;
use crate::support::render;
use crate::support::rows;
use crate::support::unrecorded;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The published `KDS_Person_Pseudonymisiert` operational template of the
/// testkit.
const TEMPLATE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt/kds-person-pseudonymisiert/KDS_Person_Pseudonymisiert.opt"
);

/// The published files the project context loads and runs verbatim, as the
/// library ships them.
pub(crate) const PUBLISHED: &[&str] = &[
    "projects/org.highmed/KDS/person_pseudo/KDS_pseudo_person_cluster.yml",
    "model/cluster/org.openehr/person_birth_data_iso.v0.yml",
    "model/cluster/org.openehr/death_details.v1.yml",
    "model/cluster/org.openehr/person.v1.yml",
];

/// The published models the set loads so the two slots of `person.v1.yml`
/// (`:21`, `:40`) name a loaded model; `KDS_Person_Pseudonymisiert` fills
/// neither slot, so neither model runs.
const LOADED: &[&str] = &[
    "model/cluster/org.openehr/structured_name.v1.yml",
    "model/cluster/org.openehr/organisation.v1.yml",
];

/// The project directory this crate authored for the chain.
const PROJECT_DIR: &str = "projects/ferrobridge/kds_person_pseudo";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_kds_person_pseudo.context.yml",
    "ferrobridge_person_data.v0.yml",
    "ferrobridge_address.v1.yml",
    "ferrobridge_kds_gender_pseudo.yml",
    "../kds_person/ferrobridge_gender.v1.yml",
];

/// The context this crate authored over the chain.
const CONTEXT: &str = "ferrobridge_kds_person_pseudo.context";

/// Returns the absolute path of a file of the vendored library.
fn published(file: &str) -> String {
    format!("{CORPUS}/{file}")
}

/// Returns the absolute path of a file of the project directory.
fn project(file: &str) -> String {
    format!("{FIXTURES}/{PROJECT_DIR}/{file}")
}

/// Builds the index over the published `KDS_Person_Pseudonymisiert` template.
fn kds_person_pseudo_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(&std::fs::read_to_string(TEMPLATE)?)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Loads the published files and the project directory into one set.
fn kds_person_pseudo_set() -> Result<MappingSet, Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(LOADED)
        .map(|file| published(file))
        .chain(PROJECT.iter().map(|file| project(file)))
        .collect();
    load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics).into())
}

/// Compiles the project context against the published template.
fn kds_person_pseudo_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_person_pseudo_template()?;
    let set = kds_person_pseudo_set()?;
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
fn the_kds_person_pseudo_context_compiles_against_the_published_template()
-> Result<(), Box<dyn Error>> {
    let (program, _) = kds_person_pseudo_program()?;
    assert_eq!(
        program.template().id().as_str(),
        "KDS_Person_Pseudonymisiert"
    );
    assert_eq!(program.resource().as_str(), "Patient");
    Ok(())
}

/// Compiles the project context and returns it with the published files it
/// loads, relative to the vendored library.
pub(crate) fn kds_person_pseudo_chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, _) = kds_person_pseudo_program()?;
    Ok((PUBLISHED, program))
}

/// Returns a synthetic MII pseudonymised `Patient`: a pseudonym, the
/// administrative gender and a birth date, with no address, since no
/// mapping of the library fills the coded country `KDS_Person_Pseudonymisiert`
/// requires of one (#545).
fn kds_pseudonymised_patient() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Patient",
        "identifier": [{
            "type": {
                "coding": [{
                    "system": "http://terminology.hl7.org/CodeSystem/v3-ObservationValue",
                    "code": "PSEUDED"
                }]
            },
            "system": "https://example.org/fhir/sid/pseudonym",
            "value": "synthetic-pseudonym-1"
        }],
        "active": true,
        "gender": "female",
        "birthDate": "1970-01"
    })
}

/// `PutGet` over the pseudonymised patient, with what the chain declares
/// lost.
#[test]
fn putget_holds_on_the_kds_pseudonymised_patient() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_person_pseudo_program()?;
    let declared = laws::putget(
        &program,
        &index,
        &kds_pseudonymised_patient(),
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("kds_person_pseudo_putget", declared);
    Ok(())
}

/// The published models, extensions and context the project context
/// replaces or leaves out, as the library ships them.
const REPLACED: &[&str] = &[
    "model/admin_entry/org.highmed/person_data.v0.yml",
    "model/admin_entry/org.highmed/versicherungsinformationen.v0.yml",
    "model/cluster/org.openehr/address.v1.yml",
    "model/cluster/org.openehr/structured_name.v1.yml",
    "model/cluster/org.openehr/organisation.v1.yml",
    "model/evaluation/org.openehr/gender.v1.yml",
    "projects/org.highmed/KDS/person_pseudo/KDS_gender.yml",
    "projects/org.highmed/KDS/person_pseudo/KDS_address.yml",
];

/// The published project, with the context and the files the set refuses
/// replaced by the project context, the gender evaluation started at and
/// the published gender, address and pseudonymised cluster extensions
/// applied, is refused exactly; each refusal is recorded on #101:
///
/// - `person_data.v0.yml:75`: `link.type.value` read on a `Reference`.
/// - `gender.v1.yml:17`: the code `gender` onto a `DV_CODED_TEXT`.
/// - `KDS_gender.yml:34`, `:45`, `:46`: `other_context` and the insurance
///   entry, which the template does not carry; the insurance model the
///   entry slots then raises `versicherungsinformationen.v0.yml:19`.
/// - `KDS_address.yml:34`: the code `Address.type` onto a `DV_CODED_TEXT`;
///   `:47`, `:70`, `:92`: the manual `defining_code/terminology_id`, an
///   object; `:55`, `:60`, `:78`, `:82`, `:100`, `:104`: `code` read on the
///   code `Address.type`.
#[test]
fn the_kds_person_pseudo_chain_as_published_is_refused_exactly() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!(
        "kds_person_pseudo_chain_as_published",
        person_pseudo_chain_as_published()?
    );
    Ok(())
}

/// The #101 comment that records each refusal of the pseudonymised person
/// chain as published.
const RECORDED: Recorded = &[
    ("KDS_address.yml:34", 6_771_393_133),
    ("KDS_address.yml:47", 6_771_393_133),
    ("KDS_address.yml:55", 6_771_393_133),
    ("KDS_address.yml:60", 6_771_393_133),
    ("KDS_address.yml:70", 6_771_393_133),
    ("KDS_address.yml:78", 6_771_393_133),
    ("KDS_address.yml:82", 6_771_393_133),
    ("KDS_address.yml:92", 6_771_393_133),
    ("KDS_address.yml:100", 6_771_393_133),
    ("KDS_address.yml:104", 6_771_393_133),
    ("KDS_gender.yml:34", 6_771_393_133),
    ("KDS_gender.yml:45", 6_771_393_133),
    ("KDS_gender.yml:46", 6_771_393_133),
    ("gender.v1.yml:17", 6_651_397_610),
    ("person_data.v0.yml:75", 6_804_630_244),
    ("versicherungsinformationen.v0.yml:19", 6_806_390_482),
];

/// Every refusal the pseudonymised person chain as published raises is a
/// library defect recorded on #101, and every record names a refusal the
/// chain still raises ([`RECORDED`]).
#[test]
fn every_refusal_of_the_kds_person_pseudo_chain_is_recorded() -> Result<(), Box<dyn Error>> {
    let (missing, stale) = unrecorded(&person_pseudo_chain_as_published()?, RECORDED);
    assert!(
        missing.is_empty(),
        "refused with no #101 record: {missing:?}"
    );
    assert!(
        stale.is_empty(),
        "recorded and no longer refused: {stale:?}"
    );
    Ok(())
}

/// Compiles the pseudonymised person chain as the library publishes it and
/// returns its refusals as rows.
fn person_pseudo_chain_as_published() -> Result<Vec<String>, Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(REPLACED)
        .map(|file| published(file))
        .chain([project(
            "ferrobridge_kds_person_pseudo_published.context.yml",
        )])
        .collect();
    let codes = StaticMappingCodes::default();
    let set = load_set(files, &codes).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new("ferrobridge_kds_person_pseudo_published.context")?,
        &kds_person_pseudo_template()?,
        &SCHEMAS,
        &codes,
    )
    .err()
    .ok_or("the pseudonymised person chain as published compiles")?;
    Ok(rows(&diagnostics))
}

/// An open slot leaves out only the archetypes it admits: with the
/// structured name slot of `person.v1.yml` (`:20`) renamed to an OBSERVATION,
/// which no `CLUSTER` slot of `KDS_Person_Pseudonymisiert` admits, the path
/// is refused as every other node identity the template does not carry is.
#[test]
fn a_slot_path_the_open_slot_does_not_admit_is_refused() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let source = std::fs::read_to_string(published("model/cluster/org.openehr/person.v1.yml"))?;
    let written = "$archetype/items[openEHR-EHR-CLUSTER.structured_name.v1]";
    if source.matches(written).count() != 1 {
        return Err("the published person cluster no longer writes its name slot once".into());
    }
    let renamed = directory.path().join("person.v1.yml");
    std::fs::write(
        &renamed,
        source.replace(
            written,
            "$archetype/items[openEHR-EHR-OBSERVATION.blood_pressure.v2]",
        ),
    )?;
    let files: Vec<String> = PUBLISHED
        .iter()
        .filter(|file| !file.ends_with("/person.v1.yml"))
        .chain(LOADED)
        .map(|file| published(file))
        .chain([renamed.to_string_lossy().into_owned()])
        .chain(PROJECT.iter().map(|file| project(file)))
        .collect();
    let codes = StaticMappingCodes::default();
    let set = load_set(files, &codes).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new(CONTEXT)?,
        &kds_person_pseudo_template()?,
        &SCHEMAS,
        &codes,
    )
    .err()
    .ok_or("a slot path the template's slot does not admit compiles")?;
    let found = rows(&diagnostics);
    assert!(
        found.contains(&String::from(
            "person.v1.yml:20 error fc-unknown-template-node"
        )),
        "{found:?}"
    );
    Ok(())
}
