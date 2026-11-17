// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The library's entry models and their clusters, each compiled verbatim
//! through a FerroBRIDGE context against a library template built from the
//! CKM archetypes they name (#1963).
//!
//! The adverse reaction risk evaluation with its reaction event cluster and
//! the problem diagnosis evaluation compile against the library IPS entries
//! template, and the medication action with its dosage and medication
//! clusters against the library medication action template. Each is refused
//! at load, and the suite pins every refusal by file and line; each site is
//! recorded on #101. `dosage.v2.BackboneElement.yml` raises none of them.

use core::error::Error;

use super::compiled;
use super::rows;

/// The library IPS entries template.
const IPS_ENTRIES: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_ips_entries.v1.0.0";

/// The library medication action template.
const MEDICATION_ACTION: &str =
    "openEHR-EHR-COMPOSITION.ferrobridge_library_medication_action.v1.0.0";

/// The adverse reaction risk evaluation and the reaction event cluster it
/// slots.
const ADVERSE_REACTION: &[&str] = &[
    "model/evaluation/org.openehr/adverse_reaction_risk.v2.yml",
    "model/cluster/org.openehr/adverse_reaction_event.v1.yml",
];

/// The problem diagnosis evaluation and the anatomical location cluster it
/// names in its body site slot, which the template leaves unfilled.
const PROBLEM_DIAGNOSIS: &[&str] = &[
    "model/evaluation/org.openehr/problem_diagnosis.v1.yml",
    "model/cluster/org.openehr/anatomical_location.v1.yml",
];

/// The medication action and the clusters it slots.
const MEDICATION: &[&str] = &[
    "model/action/org.openehr/medication.v1.yml",
    "model/cluster/org.openehr/dosage.v2.BackboneElement.yml",
    "model/cluster/org.openehr/medication.v2.yml",
    "model/cluster/org.openehr/medication.v2.substance.yml",
];

/// Compiles the FerroBRIDGE `context` over the published `files` against
/// the library template `id` and returns its refusals as rows.
fn refused(id: &str, context: &str, files: &[&str]) -> Result<Vec<String>, Box<dyn Error>> {
    let Err(diagnostics) = compiled(id, context, files, &[])? else {
        return Err(format!("{context} compiles").into());
    };
    Ok(rows(&diagnostics))
}

/// `adverse_reaction_risk.v2.yml` and `adverse_reaction_event.v1.yml` write
/// `defining_code/terminology_id`, an object, in their `manual` rows where
/// `types-of-mappings/concept-type/manual.adoc` writes
/// `defining_code/terminology_id/value`, name a `coding` below a `Coding`,
/// and map the `code` `AllergyIntolerance.category` onto a `DV_CODED_TEXT`,
/// which `DV_CODED_TEXT.adoc` pairs with `CodeableConcept` and `Coding` only.
#[test]
fn the_adverse_reaction_models_are_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!(
        "ckm_adverse_reaction_refusals",
        refused(
            IPS_ENTRIES,
            "ferrobridge_library_adverse_reaction_risk.context",
            ADVERSE_REACTION
        )?
    );
    Ok(())
}

/// `problem_diagnosis.v1.yml:45` and `:71` write the text `asserter` at a
/// participation's `function`, a `DV_TEXT`, and `:47` and `:73` map a
/// `Reference` onto the `PARTICIPATION` itself.
#[test]
fn the_problem_diagnosis_is_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!(
        "ckm_problem_diagnosis_refusals",
        refused(
            IPS_ENTRIES,
            "ferrobridge_library_problem_diagnosis.context",
            PROBLEM_DIAGNOSIS
        )?
    );
    Ok(())
}

/// `medication.v1.yml:15` to `:30` is a hierarchy over the events of an
/// `OBSERVATION`, which an `ACTION` does not have; `:47` names
/// `MedicationAdministration.actor`, which R4 does not define; `:100` to
/// `:102` name the strength numerator and denominator as direct items of
/// the medication cluster, where the CKM archetype nests them under
/// `at0152`; `:108` and `:113` name the slot nodes `at0138` and `at0141`,
/// which a filled slot carries as the filler's archetype id; and `:197` to
/// `:533` write `defining_code/terminology_id` as above.
#[test]
fn the_medication_action_is_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!(
        "ckm_medication_action_refusals",
        refused(
            MEDICATION_ACTION,
            "ferrobridge_library_medication_action.context",
            MEDICATION
        )?
    );
    Ok(())
}
