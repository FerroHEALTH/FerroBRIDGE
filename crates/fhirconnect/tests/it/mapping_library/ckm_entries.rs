// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The library's entry models and their clusters, each compiled verbatim
//! through a FerroBRIDGE context against a library template built from the
//! CKM archetypes they name (#1963).
//!
//! The adverse reaction risk evaluation with its reaction event cluster and
//! the problem diagnosis evaluation compile against the library IPS entries
//! template, and the medication action with its dosage and medication
//! clusters against the library medication action template, and the
//! medication statement with its dosage, timing and medication clusters
//! against the library medication statement template. Each is refused
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

/// The library medication statement template.
const MEDICATION_STATEMENT: &str =
    "openEHR-EHR-COMPOSITION.ferrobridge_library_medication_statement.v1.0.0";

/// The medication statement and the clusters it slots.
const STATEMENT: &[&str] = &[
    "model/observation/org.openehr/medication_statement.v0.yml",
    "model/cluster/org.openehr/dosage.v2.yml",
    "model/cluster/org.openehr/timing_daily.v1.yml",
    "model/cluster/org.openehr/timing_non_daily.yml",
    "model/cluster/org.openehr/medication.v2.yml",
    "model/cluster/org.openehr/medication.v2.substance.yml",
];

/// The non-daily timing cluster, which FerroBRIDGE's medication statement
/// timing model slots where the CKM medication statement admits it.
const NON_DAILY: &[&str] = &["model/cluster/org.openehr/timing_non_daily.yml"];

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
    refused_with(id, context, files, &[])
}

/// Compiles as [`refused`] does with the FerroBRIDGE `fixtures` beside the
/// published `files`.
fn refused_with(
    id: &str,
    context: &str,
    files: &[&str],
    fixtures: &[&str],
) -> Result<Vec<String>, Box<dyn Error>> {
    let Err(diagnostics) = compiled(id, context, files, fixtures)? else {
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

/// `medication_statement.v0.yml:76` names `items[at0033]`, which the CKM
/// medication statement does not define either; `dosage.v2.yml:97` and
/// `:98` slot `CLUSTER.timing_nondaily.v1` in the dosage cluster, whose CKM
/// archetype admits only the daily timing cluster there, so
/// `timing_non_daily.yml:21`, `:27` and `:32` name nodes below a slot the
/// template cannot fill; `timing_daily.v1.yml:26`
/// reads `when` as an `Enumeration`, and `:29` and `timing_non_daily.yml:29`
/// map the `unsignedInt` offset onto a `DV_DURATION`, which
/// `DV_DURATION.adoc` pairs with `Duration` only. `timing_daily.v1.yml:21`
/// is FerroBRIDGE's `timingToDaily`, whose time of day port holds one value
/// while the CKM cluster repeats it (#1972).
#[test]
fn the_medication_statement_is_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    insta::assert_json_snapshot!(
        "ckm_medication_statement_refusals",
        refused(
            MEDICATION_STATEMENT,
            "ferrobridge_library_medication_statement.context",
            STATEMENT
        )?
    );
    Ok(())
}

/// Slotted where the CKM medication statement admits it, the non-daily
/// timing cluster refuses only on its offset (`timing_non_daily.yml:29`).
#[test]
fn the_non_daily_timing_is_refused_on_its_offset() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        refused_with(
            MEDICATION_STATEMENT,
            "ferrobridge_library_medication_timing.context",
            NON_DAILY,
            &["ferrobridge_medication_statement_timing.yml"]
        )?,
        ["timing_non_daily.yml:29 fc-unpaired-types"]
    );
    Ok(())
}
