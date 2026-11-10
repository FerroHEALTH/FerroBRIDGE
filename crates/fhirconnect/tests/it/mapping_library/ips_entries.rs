// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The library's IPS adverse reaction extension and its clinical evidence
//! cluster, each compiled through a FerroBRIDGE context against the library
//! IPS entries template built from the CKM archetypes.
//!
//! The published `IPS_allergy.context.yml` and `IPS_problem.context.yml` are
//! sub-contexts entered only from `IPS.context.yml`, and each lists a section
//! extension that writes a path under `$reference` (#1436). So the
//! `IPS_adverse_reaction` extension runs under a FerroBRIDGE root context at
//! the adverse reaction risk evaluation, whose published model and reaction
//! event cluster write `defining_code/terminology_id` in their `manual` rows
//! where `types-of-mappings/concept-type/manual.adoc` writes
//! `defining_code/terminology_id/value` (#101); the project directory
//! carries stand-ins under the published names. The extension writes free
//! text at the open coded category and reaction mechanism, which the run
//! holds as `DV_TEXT` (#1861).
//!
//! The clinical evidence cluster, which only `IPS_problem_diagnosis.yml`
//! slots, is slotted by a FerroBRIDGE Condition model the same way. Its
//! `fhirConfig` names a `BackboneElement`, so its `$resource.code` is the code
//! of the `Condition.evidence` the slot hands over (`basics/Variables.adoc`).

use core::error::Error;
use std::sync::Arc;

use fhirconnect::resolve::program::Program;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;

use super::models;
use super::program;
use crate::laws;

/// The library IPS entries template.
const IPS_ENTRIES: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_ips_entries.v1.0.0";

/// The published extension the allergy context applies verbatim.
const ALLERGY: &[&str] = &["projects/org.openehr/IPS/allergy/IPS_adverse_reaction.yml"];

/// The stand-ins and the context of the allergy chain, relative to the
/// project directory.
const ALLERGY_FIXTURES: &[&str] = &[
    "ferrobridge_adverse_reaction_risk.v2.yml",
    "ferrobridge_adverse_reaction_event.v1.yml",
];

/// The context FerroBRIDGE authored over the adverse reaction risk model.
const ALLERGY_CONTEXT: &str = "ferrobridge_library_allergy.context";

/// The published clinical evidence cluster.
const EVIDENCE: &[&str] = &["model/cluster/org.openehr/clinical_evidence.v1.yml"];

/// The context FerroBRIDGE authored over its Condition model.
const EVIDENCE_CONTEXT: &str = "ferrobridge_library_evidence.context";

/// Compiles the allergy context against the library IPS entries template.
fn allergy() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    program(IPS_ENTRIES, ALLERGY_CONTEXT, ALLERGY, ALLERGY_FIXTURES)
}

/// Compiles the evidence context against the library IPS entries template.
fn evidence() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    program(
        IPS_ENTRIES,
        EVIDENCE_CONTEXT,
        EVIDENCE,
        &["ferrobridge_problem_evidence.yml"],
    )
}

/// Compiles both contexts and returns each with the published files it
/// reaches.
pub(super) fn chains() -> Result<Vec<crate::support::Chain>, Box<dyn Error>> {
    Ok(vec![(ALLERGY, allergy()?.0), (EVIDENCE, evidence()?.0)])
}

#[test]
fn the_ips_adverse_reaction_extension_applies_to_the_evaluation() -> Result<(), Box<dyn Error>> {
    let (program, _) = allergy()?;
    assert_eq!(program.resource().as_str(), "AllergyIntolerance");
    let evaluation = program
        .models()
        .iter()
        .find(|model| model.name().as_str() == "EVALUATION.adverse_reaction_risk.v2")
        .ok_or("the program compiles no adverse reaction risk")?;
    assert_eq!(
        evaluation
            .extensions()
            .iter()
            .map(MappingName::as_str)
            .collect::<Vec<_>>(),
        ["IPS_adverse_reaction"]
    );
    Ok(())
}

#[test]
fn the_clinical_evidence_cluster_compiles_under_the_condition() -> Result<(), Box<dyn Error>> {
    let (program, _) = evidence()?;
    assert_eq!(program.resource().as_str(), "Condition");
    assert_eq!(
        models(&program),
        [
            "ferrobridge_problem_evidence",
            "CLUSTER.clinical_evidence.v1"
        ]
    );
    Ok(())
}

/// Returns a synthetic IPS allergy: a confirmed, active food allergy with one
/// reaction event.
fn allergy_intolerance() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "AllergyIntolerance",
        "clinicalStatus": {
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical",
                "code": "active",
                "display": "Active"
            }]
        },
        "verificationStatus": {
            "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-verification",
                "code": "confirmed",
                "display": "Confirmed"
            }]
        },
        "type": "allergy",
        "category": ["food"],
        "criticality": "high",
        "code": {
            "coding": [{
                "system": "http://snomed.info/sct",
                "code": "227493005",
                "display": "Cashew nuts"
            }]
        },
        "patient": {"reference": "Patient/synthetic-1"},
        "reaction": [{
            "manifestation": [{
                "coding": [{
                    "system": "http://snomed.info/sct",
                    "code": "39579001",
                    "display": "Anaphylaxis"
                }]
            }],
            "description": "Synthetic reaction",
            "severity": "severe"
        }]
    })
}

/// Returns a synthetic condition with one piece of clinical evidence.
fn evidenced_condition() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Condition",
        "code": {
            "coding": [{
                "system": "http://snomed.info/sct",
                "code": "38341003",
                "display": "Hypertensive disorder"
            }]
        },
        "subject": {"reference": "Patient/synthetic-1"},
        "evidence": [{
            "code": [{
                "coding": [{
                    "system": "http://snomed.info/sct",
                    "code": "271649006",
                    "display": "Systolic blood pressure"
                }]
            }]
        }]
    })
}

/// `PutGet` over the synthetic allergy, with what the chain declares lost.
#[test]
fn putget_holds_on_a_synthetic_ips_allergy() -> Result<(), Box<dyn Error>> {
    let (program, index) = allergy()?;
    let declared = laws::putget(&program, &index, &allergy_intolerance(), &laws::untouched)?;
    insta::assert_json_snapshot!("library_ips_allergy_putget", declared);
    Ok(())
}

/// `PutGet` over the synthetic condition, with what the chain declares lost.
#[test]
fn putget_holds_on_a_synthetic_evidenced_condition() -> Result<(), Box<dyn Error>> {
    let (program, index) = evidence()?;
    let declared = laws::putget(&program, &index, &evidenced_condition(), &laws::untouched)?;
    insta::assert_json_snapshot!("library_evidence_putget", declared);
    Ok(())
}

/// A `manual` entry writing free text at a coded text node whose value set
/// is closed is refused at load (`types-of-mappings/concept-type/manual.adoc`,
/// Simplified Formats §Open Value-Sets and the `|other` Suffix): the template
/// constrains the clinical evidence result to a `DV_CODED_TEXT` alone.
#[test]
fn free_text_at_a_closed_coded_node_is_refused_at_load() -> Result<(), Box<dyn Error>> {
    let Err(diagnostics) = super::compiled(
        IPS_ENTRIES,
        "ferrobridge_library_closed_value_set.context",
        EVIDENCE,
        &[
            "ferrobridge_problem_evidence.yml",
            "ferrobridge_closed_value_set.yml",
        ],
    )?
    else {
        return Err("free text at a closed coded node compiles".into());
    };
    let codes: Vec<&str> = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.severity() == openehr_mapping_core::diagnostic::Severity::Error
        })
        .map(|diagnostic| diagnostic.code().as_str())
        .collect();
    assert_eq!(
        codes,
        ["fc-closed-value-set"],
        "{}",
        crate::resolve::render(&diagnostics)
    );
    Ok(())
}

/// Free text at an open coded node is held as a `DV_TEXT`, which FLAT writes
/// under `|other`, and a coded row beside it as a `DV_CODED_TEXT` (Simplified
/// Formats §Open Value-Sets and the `|other` Suffix).
#[test]
fn free_text_at_an_open_coded_node_is_held_as_a_dv_text() -> Result<(), Box<dyn Error>> {
    let (program, index) = allergy()?;
    let inbound = fhirconnect::engine::traverse::to_openehr(
        &program,
        &fhir_types::r4::schema::SCHEMAS,
        &index,
        &fhir_types::codec::Value::from_serde_json(allergy_intolerance()),
        &fhirconnect::engine::seam::Seams::default(),
        &laws::defaults(),
        &fhirconnect::engine::context::CallContext::new(),
    )?;
    let flat = index.flatten(inbound.value())?;
    let at = |key: &str| flat.get(key).and_then(serde_json::Value::as_str);
    assert_eq!(
        at("library_ips_entries/adverse_reaction_risk/category|other"),
        Some("Food")
    );
    assert_eq!(
        at("library_ips_entries/adverse_reaction_risk/category|code"),
        None
    );
    assert_eq!(
        at("library_ips_entries/adverse_reaction_risk/reaction_mechanism|other"),
        Some("Allergy")
    );
    assert_eq!(
        at("library_ips_entries/adverse_reaction_risk/active_inactive_status|code"),
        Some("at0131")
    );
    Ok(())
}
