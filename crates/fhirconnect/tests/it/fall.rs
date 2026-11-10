// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Encounter` of a KDS case: the published KDS case extensions of
//! the FHIRconnect mapping library, under a FerroBRIDGE project context
//! compiled against the published `KDS_Fall_einfach` operational template.
//!
//! The published extensions load verbatim from the vendored library. Where a
//! published file does not load or compile, its refusal is asserted here and
//! the project directory carries a stand-in that says what it departs from.

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
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

/// The published `KDS_Fall_einfach` operational template of the testkit.
const TEMPLATE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt/kds-fall-einfach/KDS_Fall_einfach.opt"
);

/// The published files the project context loads and runs verbatim, as the
/// library ships them.
pub(crate) const PUBLISHED: &[&str] = &[
    "projects/org.highmed/KDS/fall/KDS_fall_problem_diagnose.yml",
    "model/cluster/org.highmed/diagnoseTyp.yml",
    "model/cluster/org.openehr/organisation.v1.yml",
];

/// The published extension the project context loads verbatim and the
/// chain does not count: its `entlassungsgrund` method (lines 145 to 187)
/// gates on a `fhirCondition` alone, which `basics/Conditions.adoc` applies
/// to FHIR input only, so out of openEHR a case with an admission category
/// and no discharge reason gets an extension with a `url` and no value, which
/// FHIR R4 `ext-1` forbids (#101).
const HELD: &str = "projects/org.highmed/KDS/fall/KDS_episode_institution_local.yml";

/// The project directory this crate authored for the chain.
const PROJECT_DIR: &str = "projects/ferrobridge/kds_fall";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_kds_fall.context.yml",
    "ferrobridge_episode_institution_local.v0.yml",
    "ferrobridge_fall.v1.Encounter.yml",
    "ferrobridge_problem_diagnosis.v1.yml",
];

/// The context this crate authored over the chain.
const CONTEXT: &str = "ferrobridge_kds_fall.context";

/// Returns the absolute path of a file of the vendored library.
fn published(file: &str) -> String {
    format!("{CORPUS}/{file}")
}

/// Returns the absolute path of a file of the project directory.
fn project(file: &str) -> String {
    format!("{FIXTURES}/{PROJECT_DIR}/{file}")
}

/// Builds the index over the published `KDS_Fall_einfach` template.
fn kds_fall_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(&std::fs::read_to_string(TEMPLATE)?)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Loads the published files and the project directory into one set.
fn kds_fall_set() -> Result<MappingSet, Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain([&HELD])
        .map(|file| published(file))
        .chain(PROJECT.iter().map(|file| project(file)))
        .collect();
    load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics).into())
}

/// Compiles the project context against the published template.
fn kds_fall_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_fall_template()?;
    let set = kds_fall_set()?;
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
fn the_kds_fall_context_compiles_against_the_published_template() -> Result<(), Box<dyn Error>> {
    let (program, _) = kds_fall_program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Fall_einfach");
    assert_eq!(program.resource().as_str(), "Encounter");
    Ok(())
}

/// Compiles the project context and returns it with the published files it
/// loads, relative to the vendored library.
pub(crate) fn kds_fall_chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, _) = kds_fall_program()?;
    Ok((PUBLISHED, program))
}

/// Returns a synthetic MII case `Encounter`: an inpatient stay admitted by
/// referral and discharged regularly, with the coded reason for admission.
fn kds_encounter() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Encounter",
        "extension": [{
            "url": "http://fhir.de/StructureDefinition/Aufnahmegrund",
            "extension": [{
                "url": "ErsteUndZweiteStelle",
                "valueCoding": {
                    "system": "http://fhir.de/CodeSystem/dkgev/AufnahmegrundErsteUndZweiteStelle",
                    "code": "01"
                }
            }, {
                "url": "DritteStelle",
                "valueCoding": {
                    "system": "http://fhir.de/CodeSystem/dkgev/AufnahmegrundDritteStelle",
                    "code": "0"
                }
            }, {
                "url": "VierteStelle",
                "valueCoding": {
                    "system": "http://fhir.de/CodeSystem/dkgev/AufnahmegrundVierteStelle",
                    "code": "1"
                }
            }]
        }],
        "status": "finished",
        "class": {
            "system": "http://terminology.hl7.org/CodeSystem/v3-ActCode",
            "code": "IMP"
        },
        "subject": {"reference": "Patient/synthetic-1"},
        "period": {
            "start": "2026-09-01T08:00:00+02:00",
            "end": "2026-09-11T10:30:00+02:00"
        },
        "hospitalization": {
            "admitSource": {
                "coding": [{
                    "system": "http://fhir.de/CodeSystem/dgkev/Aufnahmeanlass",
                    "code": "E"
                }]
            },
            "dischargeDisposition": {
                "extension": [{
                    "url": "http://fhir.de/StructureDefinition/Entlassungsgrund",
                    "extension": [{
                        "url": "ErsteUndZweiteStelle",
                        "valueCoding": {
                            "system": "http://fhir.de/CodeSystem/dkgev/EntlassungsgrundErsteUndZweiteStelle",
                            "code": "01"
                        }
                    }]
                }]
            }
        }
    })
}

/// `PutGet` over the case, with what the chain declares lost.
#[test]
fn putget_holds_on_the_kds_encounter() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_fall_program()?;
    let declared = laws::putget(&program, &index, &kds_encounter(), &laws::untouched)?;
    insta::assert_json_snapshot!("kds_fall_putget", declared);
    Ok(())
}

/// The published models, extensions and context the project context
/// replaces or leaves out, as the library ships them.
const REPLACED: &[&str] = &[
    "model/admin_entry/org.highmed/episode_institution_local.v0.yml",
    "model/composition/org.highmed/fall.v1.Encounter.yml",
    "model/evaluation/org.openehr/problem_diagnosis.v1.yml",
    "model/cluster/org.highmed/kontaktTyp.yml",
    "model/cluster/org.highmed/organization.v0.yml",
    "model/cluster/org.openehr/location.v1.yml",
    "model/cluster/org.openehr/anatomical_location.v1.yml",
    "projects/org.highmed/KDS/diagnose/KDS_anatomical_location.yml",
    "projects/org.highmed/KDS/fall/KDS_composition.yml",
    "projects/org.highmed/KDS/fall/KDS_fall_einfach.context.yaml",
];

/// The published context over the published files, with the
/// `KDS_anatomical_location` extension it names loaded from the diagnosis
/// project, is refused exactly; each refusal is recorded on #101:
///
/// - `episode_institution_local.v0.yml:63` to `:74`: `location.identifier`,
///   `physicalType` and `status` read below `Encounter.period`, written to
///   `items[at0024]` and `items[at0028]`, which sit below `items[at0018]`.
/// - `location.v1.yml`: `identifier`, `physicalType`, `system` and `code`
///   read on the Encounter, and the code `status` onto a `DV_CODED_TEXT`.
/// - `fall.v1.Encounter.yml:55` to `:82`: `null_flavour` below the
///   `DV_DATE_TIME` start time and the `PARTY_PROXY` composer.
/// - `problem_diagnosis.v1.yml:45`, `:47`, `:71`, `:73`: the participation
///   `function` written as a `DV_TEXT` whole and the asserter mapped onto
///   the `PARTICIPATION` itself.
/// - `anatomical_location.v1.yml:27`, `:33`: the `Condition` narrative onto
///   a `DV_TEXT` and `coding` read on the `Condition` (#173).
/// - `KDS_anatomical_location.yml:15`: `value` read on a `CodeableConcept`.
/// - `KDS_composition.yml:25`, `:31`: the code `status` and an `Identifier`
///   onto `DV_CODED_TEXT` nodes.
#[test]
fn the_kds_fall_chain_as_published_is_refused_exactly() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain([&HELD])
        .chain(REPLACED)
        .map(|file| published(file))
        .collect();
    let codes = StaticMappingCodes::default();
    let set = load_set(files, &codes).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new("KDS_fall_einfach.context")?,
        &kds_fall_template()?,
        &SCHEMAS,
        &codes,
    )
    .err()
    .ok_or("the case chain as published compiles")?;
    insta::assert_json_snapshot!("kds_fall_chain_as_published", rows(&diagnostics));
    Ok(())
}

/// The published `entlassungsgrund` method gates on a `fhirCondition` alone,
/// which `basics/Conditions.adoc` applies to FHIR input only, so out of
/// openEHR a case with an admission category and no discharge reason gets an
/// extension with a `url` and no value, and the run refuses it under FHIR R4
/// `ext-1` (<https://hl7.org/fhir/R4/extensibility.html>, #101).
#[test]
fn a_case_with_no_discharge_reason_is_refused_out_of_openehr_naming_ext_1()
-> Result<(), Box<dyn Error>> {
    let (program, index) = kds_fall_program()?;
    let mut encounter = kds_encounter();
    let hospitalization = encounter
        .get_mut("hospitalization")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("the synthetic case has a hospitalization")?;
    hospitalization
        .remove("dischargeDisposition")
        .ok_or("the synthetic case has a discharge reason")?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(encounter),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    )?;
    let error = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &Seams::default(),
        &CallContext::new(),
    )
    .err()
    .ok_or("an extension with a url and no value breaks ext-1")?;
    let EngineError::Invariant { ref breach, .. } = error else {
        return Err(format!("expected the ext-1 refusal, got {error}").into());
    };
    assert_eq!(breach.key(), "ext-1");
    assert_eq!(
        breach.location(),
        "Encounter.hospitalization.dischargeDisposition.extension[0].extension[0]"
    );
    Ok(())
}
