// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Condition` of a cause of death: the published KDS cause of death
//! chain of the FHIRconnect mapping library, under a FerroBRIDGE project
//! context compiled against the published `KDS_Todesursache` operational
//! template.
//!
//! The published extensions load verbatim from the vendored library. Where a
//! published file does not load or compile, its refusal is asserted here and
//! the project directory carries a stand-in that says what it departs from.

use core::error::Error;
use std::collections::BTreeMap;
use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::linked::Linked;
use fhirconnect::engine::linked::LinkedCompositions;
use fhirconnect::engine::seam::ReferenceError;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::template::TemplateSource;
use openehr_rm::v1_2::paths::EhrUri;

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

/// The published `KDS_Todesursache` operational template of the testkit.
const TEMPLATE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt/kds-todesursache/KDS_Todesursache.opt"
);

/// The published files the project context loads, as the library ships them.
pub(crate) const PUBLISHED: &[&str] = &[
    "model/cluster/org.openehr/problem_qualifier.v2.yml",
    "projects/org.highmed/KDS/todesursache/KDS_cause_of_death.yml",
    "projects/org.highmed/KDS/todesursache/KDS_problem_qualifier.yml",
    "projects/org.highmed/KDS/todesursache/KDS_composition.yml",
];

/// The project directory this crate authored for the chain.
const PROJECT_DIR: &str = "projects/ferrobridge/kds_todesursache";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_kds_todesursache.context.yml",
    "ferrobridge_cause_of_death.v1.yml",
    "ferrobridge_problem_qualifier_terminology_id.yml",
    "ferrobridge_report.v1.Condition.yml",
    "ferrobridge_case_identification.v0.yml",
];

/// The context this crate authored over the chain.
const CONTEXT: &str = "ferrobridge_kds_todesursache.context";

/// Returns the absolute path of a file of the vendored library.
fn published(file: &str) -> String {
    format!("{CORPUS}/{file}")
}

/// Returns the absolute path of a file of the project directory.
fn project(file: &str) -> String {
    format!("{FIXTURES}/{PROJECT_DIR}/{file}")
}

/// Builds the index over the published `KDS_Todesursache` template.
fn kds_todesursache_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(&std::fs::read_to_string(TEMPLATE)?)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Loads the published files and the project directory into one set.
fn kds_todesursache_set() -> Result<MappingSet, Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .map(|file| published(file))
        .chain(PROJECT.iter().map(|file| project(file)))
        .collect();
    load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics).into())
}

/// Compiles the project context against the published template.
fn kds_todesursache_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_todesursache_template()?;
    let set = kds_todesursache_set()?;
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
fn the_kds_todesursache_context_compiles_against_the_published_template()
-> Result<(), Box<dyn Error>> {
    let (program, _) = kds_todesursache_program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Todesursache");
    assert_eq!(program.resource().as_str(), "Condition");
    Ok(())
}

/// Compiles the project context and returns it with the published files it
/// loads, relative to the vendored library.
pub(crate) fn kds_todesursache_chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, _) = kds_todesursache_program()?;
    Ok((PUBLISHED, program))
}

/// Returns a synthetic MII cause of death `Condition`.
fn kds_cause_of_death() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Condition",
        "clinicalStatus": {
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/condition-clinical",
                "code": "active"
            }]
        },
        "code": {
            "coding": [{
                "system": "http://fhir.de/CodeSystem/bfarm/icd-10-gm",
                "version": "2025",
                "code": "I21.9"
            }]
        },
        "subject": {"reference": "Patient/synthetic-1"},
        "onsetDateTime": "2026-09-11T22:30:00+02:00",
        "recordedDate": "2026-09-12T10:00:00+02:00",
        "note": [{"text": "synthetic note"}]
    })
}

/// `PutGet` over the cause of death, with what the chain declares lost.
#[test]
fn putget_holds_on_the_kds_cause_of_death() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_todesursache_program()?;
    let declared = laws::putget(&program, &index, &kds_cause_of_death(), &laws::untouched)?;
    insta::assert_json_snapshot!("kds_todesursache_putget", declared);
    Ok(())
}

/// The reference the cause of death's `encounter` carries.
const ENCOUNTER: &str = "Encounter/synthetic-case-1";

/// The `ehr:` URI the linker answers for the case's own composition.
const CASE: &str =
    "ehr:/3a2f1c4e-0000-4000-8000-0000000000ab/compositions/d2b3c1a0-0000-4000-8000-000000000002";

/// A caller that answers every linked resource with [`CASE`].
#[derive(Debug)]
struct CaseLinker;

impl LinkedCompositions for CaseLinker {
    fn composition(&self, reference: &str, _: &Value) -> Result<Linked, ReferenceError> {
        CASE.parse::<EhrUri>()
            .map(Linked::At)
            .map_err(|source| ReferenceError::Fetch {
                reference: String::from(reference),
                source: Box::new(source),
            })
    }
}

/// Returns the synthetic case `Encounter` the cause of death names.
fn kds_case() -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Encounter",
        "id": "synthetic-case-1",
        "status": "finished",
        "class": {"system": "http://terminology.hl7.org/CodeSystem/v3-ActCode", "code": "IMP"},
        "identifier": [{
            "type": {"coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/v2-0203",
                "code": "VN"
            }]},
            "system": "http://example.org/fhir/sid/case",
            "value": "synthetic-case-1"
        }]
    }))
}

/// The published `encounterMapping` (`KDS_composition.yml:32`) links the case
/// cluster to the composition of the Encounter its `reference` entered
/// (`concept-mappings.adoc` §Linked mappings), and going out the encounter
/// reference names that composition, with no copy of the Encounter created.
#[test]
fn the_published_encounter_link_targets_the_case_composition() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_todesursache_program()?;
    let mut condition = kds_cause_of_death();
    if let Some(object) = condition.as_object_mut() {
        object.insert(
            String::from("encounter"),
            serde_json::json!({"reference": ENCOUNTER}),
        );
    }
    let mut held = BTreeMap::new();
    held.insert(String::from(ENCOUNTER), kds_case());
    let references = MapReferences(held);
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(condition),
        &Seams::default()
            .with_references(&references)
            .with_linked(&CaseLinker),
        &laws::defaults(),
        &CallContext::new(),
    )?;
    let cluster = index.node(&AqlPath::new(
        "/context/other_context[at0001]/items[openEHR-EHR-CLUSTER.case_identification.v0]",
    ))?;
    let written = index
        .read(inbound.value(), cluster, &[])?
        .ok_or("the case cluster was written")?;
    let targets: Vec<&str> = written
        .get("links")
        .and_then(serde_json::Value::as_array)
        .map(|links| {
            links
                .iter()
                .filter_map(|link| link.pointer("/target/value")?.as_str())
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(targets, [CASE]);
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    assert_eq!(
        outbound
            .value()
            .get("encounter")
            .and_then(|encounter| encounter.get("reference"))
            .and_then(Value::as_str),
        Some(CASE)
    );
    assert!(outbound.created().is_empty(), "{:?}", outbound.created());
    Ok(())
}

/// The published files of the chain the stand-ins replace, as the library
/// ships them, with the published context.
const REPLACED: &[&str] = &[
    "model/composition/org.openehr/report.v1.Condition.yml",
    "model/cluster/org.openehr/case_identification.v0.yml",
    "projects/org.highmed/KDS/todesursache/KDS_Todesursache.context.yaml",
];

/// The published cause of death model, which the whole set refuses at load.
const CAUSE_OF_DEATH: &str = "model/evaluation/org.openehr/cause_of_death.v1.yml";

/// The published chain loaded as the library ships it is refused at load:
/// `cause_of_death.v1.yml:17` carries `extension: add` in a model (#630).
#[test]
fn the_published_cause_of_death_model_is_refused_at_load() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(REPLACED)
        .chain(&[CAUSE_OF_DEATH])
        .map(|file| published(file))
        .collect();
    let diagnostics = load_set(files, &StaticMappingCodes::default())
        .err()
        .ok_or("the published cause of death model loads")?;
    assert_eq!(
        rows(&diagnostics),
        ["cause_of_death.v1.yml:17 error fc-extension-method-outside-model"]
    );
    Ok(())
}

/// The published context over the published files, with only the cause of
/// death model replaced by its stand-in, is refused exactly; each refusal is
/// recorded on #101:
///
/// - `problem_qualifier.v2.yml:28` and its four siblings: a string written
///   onto the `TERMINOLOGY_ID` object.
/// - `report.v1.Condition.yml:62` to `:96` and `:136` to `:140`:
///   `null_flavour` below a `DV_DATE_TIME` and a `PARTY_PROXY`, in part
///   under two methods named `contextStartTimeEmpty`.
/// - `report.v1.Condition.yml:113` and `:115`: `function` and `performer` below a
///   `PARTICIPATION` list, which the engine carries only through
///   `participationsFunction`.
/// - `case_identification.v0.yml:16`: an `Identifier` onto the
///   `DV_CODED_TEXT` `KDS_Todesursache` constrains.
#[test]
fn the_kds_todesursache_chain_as_published_is_refused_exactly() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(REPLACED)
        .map(|file| published(file))
        .chain([project("ferrobridge_cause_of_death.v1.yml")])
        .collect();
    let codes = StaticMappingCodes::default();
    let set = load_set(files, &codes).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new("KDS_death.context")?,
        &kds_todesursache_template()?,
        &SCHEMAS,
        &codes,
    )
    .err()
    .ok_or("the todesursache chain as published compiles")?;
    insta::assert_json_snapshot!("kds_todesursache_chain_as_published", rows(&diagnostics));
    Ok(())
}
