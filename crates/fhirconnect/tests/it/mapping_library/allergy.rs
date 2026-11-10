// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The library's IPS allergy extension, applied by a FerroBRIDGE context
//! against the library allergy template built from the CKM archetypes.
//!
//! The published `IPS_allergy.context.yml` lists `IPS_allergy_section.yml`,
//! which writes a path under `$reference` (#1436), so FerroBRIDGE's context
//! starts at the adverse reaction risk model that `IPS_adverse_reaction.yml`
//! extends. The published model does not compile (#101), so the project
//! directory carries a stand-in under the published name, and the extension
//! runs verbatim over it.

use core::error::Error;
use std::sync::Arc;

use fhirconnect::model::load::load_set;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;

use super::{CORPUS, PROJECT, program, template};
use crate::laws;

/// The library allergy template.
const ALLERGY: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_allergy.v1.0.0";

/// The context FerroBRIDGE authored over the allergy extension.
const CONTEXT: &str = "ferrobridge_library_allergy.context";

/// The published files the program reaches verbatim.
const FILES: &[&str] = &["projects/org.openehr/IPS/allergy/IPS_adverse_reaction.yml"];

/// The stand-in for the published adverse reaction risk model.
const STAND_IN: &str = "ferrobridge_adverse_reaction_risk.v2.yml";

/// Compiles the allergy context against the library allergy template.
fn allergy() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    program(ALLERGY, CONTEXT, FILES, &[STAND_IN])
}

/// Returns a synthetic IPS allergy: a food allergy, active and confirmed,
/// with a low criticality and an onset date.
fn allergy_intolerance() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "AllergyIntolerance",
        "clinicalStatus": {"coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical",
            "code": "active",
            "display": "Active"
        }]},
        "verificationStatus": {"coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-verification",
            "code": "confirmed",
            "display": "Confirmed"
        }]},
        "type": "allergy",
        "category": ["food"],
        "criticality": "low",
        "code": {"text": "Synthetic peanut"},
        "onsetDateTime": "2026-09-14T08:00:00Z",
        "note": [{"text": "Synthetic allergy"}]
    })
}

/// Compiles the context and runs it in both directions over the synthetic
/// allergy, and returns it with the published file it reaches: the
/// extension counts once it runs.
pub(super) fn chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, index) = allergy()?;
    laws::putget(&program, &index, &allergy_intolerance(), &laws::untouched)?;
    Ok((FILES, program))
}

#[test]
fn the_allergy_extension_compiles_onto_allergy_intolerance() -> Result<(), Box<dyn Error>> {
    let (program, _) = allergy()?;
    assert_eq!(program.resource().as_str(), "AllergyIntolerance");
    let model = program
        .models()
        .iter()
        .find(|model| model.name().as_str() == "EVALUATION.adverse_reaction_risk.v2")
        .ok_or("the program compiles no adverse reaction risk")?;
    assert_eq!(
        model
            .extensions()
            .iter()
            .map(MappingName::as_str)
            .collect::<Vec<_>>(),
        ["IPS_adverse_reaction"]
    );
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

/// The allergy goes into openEHR and comes back out with every element it
/// carried: `type` and `category` through the text values the IPS extension
/// writes, the codes through the stand-in model.
#[test]
fn putget_holds_on_a_synthetic_ips_allergy() -> Result<(), Box<dyn Error>> {
    let (program, index) = allergy()?;
    let declared = laws::putget(&program, &index, &allergy_intolerance(), &laws::untouched)?;
    insta::assert_json_snapshot!("library_ips_allergy_putget", declared);
    Ok(())
}

/// The published model under the same context is refused exactly, each
/// refusal a library defect recorded on #101:
///
/// - lines 42, 68, 106, 132, 158, 190, 212 and 234: a string written onto the
///   `TERMINOLOGY_ID` object, where `manual.adoc` writes
///   `defining_code/terminology_id/value`;
/// - lines 49, 75, 113, 139 and 165: a manual condition's `targetRoot`
///   `coding` under a mapping that already names `coding`
///   (`basics/FollowedBy.adoc`, §Path construction);
/// - lines 274 and 294: `Age` and the `Range` of `SimpleQuantity` bounds onto
///   `DV_DURATION` alternatives, which no data-type page pairs;
/// - line 324 and the slotted `adverse_reaction_event.v1.yml`: the cluster
///   writes `$resource.substance` and its siblings, elements of the root
///   resource, and the template's slot holds no such cluster.
#[test]
fn the_published_allergy_model_is_refused_exactly() -> Result<(), Box<dyn Error>> {
    let index = template(ALLERGY)?;
    let files: Vec<String> = [
        "projects/org.openehr/IPS/allergy/IPS_adverse_reaction.yml",
        "model/evaluation/org.openehr/adverse_reaction_risk.v2.yml",
        "model/cluster/org.openehr/adverse_reaction_event.v1.yml",
    ]
    .iter()
    .map(|file| format!("{CORPUS}/{file}"))
    .chain([format!("{PROJECT}/{CONTEXT}.yml")])
    .collect();
    let set = load_set(files, &fhirconnect::engine::library::Library)
        .map_err(|diagnostics| crate::resolve::render(&diagnostics))?;
    let diagnostics = compile(
        &set,
        &MappingName::new(CONTEXT)?,
        &index,
        &fhir_types::r4::schema::SCHEMAS,
        &fhirconnect::engine::library::Library,
    )
    .err()
    .ok_or("the published allergy model compiles")?;
    let mut rows: Vec<String> = diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{}:{} {}",
                diagnostic
                    .file()
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                diagnostic
                    .position()
                    .map_or_else(String::new, |position| position.line().to_string()),
                diagnostic.code()
            )
        })
        .collect();
    rows.sort();
    rows.dedup();
    insta::assert_json_snapshot!("library_ips_allergy_as_published", rows);
    Ok(())
}

/// The category leaf of the CKM archetype is a `DV_CODED_TEXT` with a
/// `DV_TEXT` alternative, an open value set (Simplified Formats master04
/// §Open Value-Sets and the `|other` Suffix), and the substance leaf a
/// `DV_TEXT`, which has no value set to open.
#[test]
fn the_category_leaf_admits_free_text_and_the_substance_leaf_does_not() -> Result<(), Box<dyn Error>>
{
    let index = template(ALLERGY)?;
    let leaf = |code: &str| {
        index
            .nodes()
            .find(|node| {
                node.aql_path()
                    .as_str()
                    .ends_with(&format!("items[{code}]/value"))
            })
            .ok_or_else(|| format!("the template holds no {code} value"))
    };
    assert!(index.admits_free_text(leaf("at0120")?)?);
    assert!(!index.admits_free_text(leaf("at0002")?)?);
    Ok(())
}

/// Going into openEHR the IPS category `food` is the `DV_TEXT` `Food` the
/// extension's manual writes, held at the open coded leaf, and the
/// criticality the `DV_CODED_TEXT` the model's manual writes.
#[test]
fn the_ips_category_goes_in_as_free_text() -> Result<(), Box<dyn Error>> {
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
    let flat = serde_json::to_value(inbound.value().value())?;
    let text = flat.to_string();
    assert!(
        text.contains(r#"{"_type":"DV_TEXT","value":"Food"}"#),
        "{text}"
    );
    assert!(text.contains(r#""code_string":"at0102""#), "{text}");
    Ok(())
}
