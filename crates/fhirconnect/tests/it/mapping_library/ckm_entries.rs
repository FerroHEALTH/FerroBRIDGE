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
//! against the library medication statement template. The person data entry
//! and its clusters compile against the library patient template, the
//! insurance entry slotted by a FerroBRIDGE extension, and the local
//! institution episode with its location cluster against the library case
//! template. The diagnosis clusters
//! compile against the library diagnosis template, slotted by a FerroBRIDGE
//! Condition model, and the laboratory result with its specimen and analyte
//! clusters, the service request and the vital status against the library
//! laboratory template; the informed consent is refused before it compiles.
//! Each is refused
//! at load, and the suite pins every refusal by file and line; each site is
//! recorded on #101. `dosage.v2.BackboneElement.yml` raises none of them.

use core::error::Error;

use fhirconnect::engine::library::Library;
use fhirconnect::model::load::load_set;

use super::CORPUS;
use super::PROJECT;
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

/// The library patient template.
const PATIENT: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_patient.v1.0.0";

/// The library case template.
const CASE: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_case.v1.0.0";

/// The person data entry, its clusters and the insurance entry, which
/// FerroBRIDGE's extension slots.
const PATIENT_FILES: &[&str] = &[
    "model/admin_entry/org.highmed/person_data.v0.yml",
    "model/cluster/org.openehr/person.v1.yml",
    "model/cluster/org.openehr/structured_name.v1.yml",
    "model/cluster/org.openehr/address.v1.yml",
    "model/cluster/org.openehr/organisation.v1.yml",
    "model/cluster/org.openehr/death_details.v1.yml",
    "model/cluster/org.openehr/person_birth_data_iso.v0.yml",
    "model/admin_entry/org.highmed/versicherungsinformationen.v0.yml",
];

/// The local institution episode and the location cluster it slots.
const CASE_FILES: &[&str] = &[
    "model/admin_entry/org.highmed/episode_institution_local.v0.yml",
    "model/cluster/org.openehr/location.v1.yml",
];

/// The library diagnosis template.
const DIAGNOSIS: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_diagnosis.v1.0.0";

/// The library laboratory template.
const LABORATORY: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_laboratory.v1.0.0";

/// The diagnosis clusters, which FerroBRIDGE's Condition model slots.
const DIAGNOSIS_FILES: &[&str] = &[
    "model/cluster/org.openehr/anatomical_location.v1.yml",
    "model/cluster/org.openehr/problem_qualifier.v2.yml",
    "model/cluster/org.highmed/multiple_coding_icd10gm.v1.yml",
];

/// The laboratory result and its specimen and analyte clusters.
const LABORATORY_FILES: &[&str] = &[
    "model/observation/org.openehr/laboratory_test_result.v1.yml",
    "model/cluster/org.openehr/specimen.v1.yml",
    "model/cluster/org.openehr/laboratory_test_analyte.v1.yml",
];

/// The service request and the organisation cluster it slots.
const SERVICE_REQUEST: &[&str] = &[
    "model/instruction/org.openehr/service_request.v1.yml",
    "model/cluster/org.openehr/organisation.v1.yml",
];

/// The vital status evaluation.
const VITAL_STATUS: &[&str] = &["model/evaluation/org.openehr/vital_status.v1.yml"];

/// The informed consent and the study clusters it slots.
const CONSENT: &[&str] = &[
    "model/action/org.openehr/informed_consent.v0.yml",
    "model/cluster/org.highmed/study_participation.v1.yml",
    "model/cluster/org.highmed/study_details.v1.yml",
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
    json_snapshot!(
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
    json_snapshot!(
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
    json_snapshot!(
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
/// `DV_DURATION.adoc` pairs with `Duration` only. The repeating time of day
/// port of `timingToDaily` at `timing_daily.v1.yml:21` raises none.
#[test]
fn the_medication_statement_is_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    json_snapshot!(
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

/// `address.v1.yml` writes `defining_code/terminology_id` in its `manual`
/// rows (`:54`, `:77`, `:99`) and reads `code` on the `code`
/// `Address.type` (`:62`, `:67`, `:85`, `:89`, `:107`, `:111`), and
/// `person_data.v0.yml:75` writes a `link` below a `Reference`; the person,
/// name, organisation, death details, birth data and insurance models raise
/// none of their own.
#[test]
fn the_patient_clusters_are_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    json_snapshot!(
        "ckm_patient_refusals",
        refused_with(
            PATIENT,
            "ferrobridge_library_patient.context",
            PATIENT_FILES,
            &["ferrobridge_person_insurance.yml"]
        )?
    );
    Ok(())
}

/// `episode_institution_local.v0.yml:63`, `:67` and `:73` name `location`,
/// `physicalType` and `status` below an `Encounter.period`, and `:64` and
/// `:74` name nodes the archetype does not have at the entry's root;
/// `location.v1.yml:29`, `:69` and `:110` map the `Identifier` of the
/// referenced `Location` onto a `DV_TEXT`, which `DV_TEXT.adoc` pairs with
/// `String`, `Coding` and `CodeableConcept` only.
#[test]
fn the_case_entry_is_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    json_snapshot!(
        "ckm_case_refusals",
        refused(CASE, "ferrobridge_library_case.context", CASE_FILES)?
    );
    Ok(())
}

/// `problem_qualifier.v2.yml` and `multiple_coding_icd10gm.v1.yml` write
/// `defining_code/terminology_id` in their `manual` rows, and the multiple
/// coding cluster reads `value` on a `Coding` (`:30`, `:56`, `:95`);
/// `anatomical_location.v1.yml` raises none slotted on `Condition.bodySite`.
#[test]
fn the_diagnosis_clusters_are_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    json_snapshot!(
        "ckm_diagnosis_refusals",
        refused_with(
            DIAGNOSIS,
            "ferrobridge_library_diagnosis.context",
            DIAGNOSIS_FILES,
            &["ferrobridge_problem_diagnosis_clusters.yml"]
        )?
    );
    Ok(())
}

/// The laboratory result maps participations through raw paths, names
/// `perfomer`, reads `text` on a `Coding`, writes
/// `defining_code/terminology_id`, and walks `null_flavour` below the
/// `DV_CODED_TEXT` value of `items[at0073]` (`:383` to `:392`); the specimen
/// and analyte clusters write `defining_code/terminology_id` too. The
/// analyte cluster is slotted by FerroBRIDGE's extension, the append the KDS
/// laboratory report extension makes.
#[test]
fn the_laboratory_result_is_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    json_snapshot!(
        "ckm_laboratory_result_refusals",
        refused_with(
            LABORATORY,
            "ferrobridge_library_laboratory_result.context",
            LABORATORY_FILES,
            &["ferrobridge_laboratory_result_analyte.yml"]
        )?
    );
    Ok(())
}

/// `service_request.v1.yml:34` maps `$composition/provider`, which a
/// `COMPOSITION` does not have; its requester organisation at `:62` sits in
/// a protocol slot whose CKM requester admits only `CLUSTER.person.v1`, so
/// the template leaves it out and the method is ignored with a warning.
#[test]
fn the_service_request_is_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    json_snapshot!(
        "ckm_service_request_refusals",
        refused(
            LABORATORY,
            "ferrobridge_library_service_request.context",
            SERVICE_REQUEST
        )?
    );
    Ok(())
}

/// `vital_status.v1.yml` maps participations through raw paths and names
/// `perfomer` at `:51`.
#[test]
fn the_vital_status_is_refused_as_pinned() -> Result<(), Box<dyn Error>> {
    json_snapshot!(
        "ckm_vital_status_refusals",
        refused(
            LABORATORY,
            "ferrobridge_library_vital_status.context",
            VITAL_STATUS
        )?
    );
    Ok(())
}

/// `informed_consent.v0.yml:170` slots `CLUSTER.study_participation.v1`,
/// which slots `CLUSTER.study_details.v1`, whose `mappings` is `null`
/// (#101), so the set refuses before anything compiles.
#[test]
fn the_informed_consent_is_refused_at_load() -> Result<(), Box<dyn Error>> {
    let paths: Vec<String> = CONSENT
        .iter()
        .map(|file| format!("{CORPUS}/{file}"))
        .chain([format!(
            "{PROJECT}/ferrobridge_library_informed_consent.context.yml"
        )])
        .collect();
    let Err(diagnostics) = load_set(paths, &Library) else {
        return Err("the informed consent set loads".into());
    };
    json_snapshot!("ckm_informed_consent_refusals", rows(&diagnostics));
    Ok(())
}
