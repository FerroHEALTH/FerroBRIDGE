// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The problems, the allergies and intolerances and the history of
//! procedures of the patient summary, mapped from the CKM
//! `EVALUATION.problem_diagnosis.v1`, `EVALUATION.adverse_reaction_risk.v2`
//! and `ACTION.procedure.v1` each held by the ad hoc section named for it,
//! with their SNOMED CT codes kept (xShare Yellow Button Bronze statement 9).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::document::Document;
use eehrxf::document::IPS_BUNDLE;
use eehrxf::document::form::IPS_PATIENT_SUMMARY_FORM;
use eehrxf::document::form::PATIENT_SUMMARY_FORM;
use eehrxf::document::form::{Form, SectionRule};
use eehrxf::document::section::{EmptyReason, Section};
use eehrxf::receive::ReceivedDocument;
use fhir_types::r4::device::Device;
use fhir_types::r4::resource::Resource;

use super::eps_mapped::{
    BASE, ENTRY, call, composition, details, patient, run, section, section_entries, settings,
    summary,
};
use super::support::{emit, profile_set};

/// The SNOMED CT system.
const SNOMED: &str = "http://snomed.info/sct";

/// The problem list section of the template.
pub(crate) const PROBLEM_LIST: (&str, &str) = ("Problem list", "openEHR-EHR-SECTION.adhoc.v1");

/// The allergies and intolerances section of the template.
pub(crate) const ALLERGIES: (&str, &str) =
    ("Allergies and intolerances", "openEHR-EHR-SECTION.adhoc.v1");

/// The history of procedures section of the template.
pub(crate) const PROCEDURES: (&str, &str) =
    ("History of procedures", "openEHR-EHR-SECTION.adhoc.v1");

/// The LOINC code of the problems section.
pub(crate) const PROBLEMS_CODE: &str = "11450-4";

/// The LOINC code of the allergies section.
pub(crate) const ALLERGIES_CODE: &str = "48765-2";

/// The LOINC code of the procedures section.
pub(crate) const PROCEDURES_CODE: &str = "47519-4";

/// SNOMED CT 38341003 |Hypertensive disorder, systemic arterial|.
pub(crate) const HYPERTENSION: &str = "38341003";

/// SNOMED CT 764146007 |Penicillin|.
pub(crate) const PENICILLIN: &str = "764146007";

/// SNOMED CT 80146002 |Appendectomy|.
pub(crate) const APPENDECTOMY: &str = "80146002";

/// A SNOMED CT coded text.
fn snomed(value: &str, code: &str) -> String {
    format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "{value}", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "{SNOMED}" }}, "code_string": "{code}" }} }}"#
    )
}

/// An element `node` named `name` holding `value`.
fn element(node: &str, name: &str, value: &str) -> String {
    format!(
        r#"{{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "{name}" }}, "archetype_node_id": "{node}", "value": {value} }}"#
    )
}

/// A problem or diagnosis coded `code`.
pub(crate) fn problem(code: &str) -> String {
    format!(
        r#"{{
  "_type": "EVALUATION",
  "name": {{ "_type": "DV_TEXT", "value": "Problem/Diagnosis" }},
  {details},
  {ENTRY},
  "data": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "structure" }}, "archetype_node_id": "at0001", "items": [{name}, {onset}] }}
}}"#,
        details = details("openEHR-EHR-EVALUATION.problem_diagnosis.v1"),
        name = element(
            "at0002",
            "Problem/Diagnosis name",
            &snomed("Hypertension", code)
        ),
        onset = element(
            "at0077",
            "Date/time of onset",
            r#"{ "_type": "DV_DATE_TIME", "value": "2020-03-01T00:00:00Z" }"#
        ),
    )
}

/// An active risk of an adverse reaction to the substance `code`.
pub(crate) fn allergy(code: &str) -> String {
    format!(
        r#"{{
  "_type": "EVALUATION",
  "name": {{ "_type": "DV_TEXT", "value": "Adverse reaction risk" }},
  {details},
  {ENTRY},
  "data": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "Tree" }}, "archetype_node_id": "at0001", "items": [{substance}, {status}] }}
}}"#,
        details = details("openEHR-EHR-EVALUATION.adverse_reaction_risk.v2"),
        substance = element("at0002", "Substance", &snomed("Penicillin", code)),
        status = element(
            "at0130",
            "Active/inactive status",
            r#"{ "_type": "DV_CODED_TEXT", "value": "Active", "defining_code": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "local" }, "code_string": "at0131" } }"#
        ),
    )
}

/// A completed procedure coded `code`.
pub(crate) fn procedure(code: &str) -> String {
    format!(
        r#"{{
  "_type": "ACTION",
  "name": {{ "_type": "DV_TEXT", "value": "Procedure" }},
  {details},
  {ENTRY},
  "time": {{ "_type": "DV_DATE_TIME", "value": "2019-06-12T10:00:00Z" }},
  "ism_transition": {{
    "_type": "ISM_TRANSITION",
    "current_state": {{ "_type": "DV_CODED_TEXT", "value": "completed", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "532" }} }},
    "careflow_step": {{ "_type": "DV_CODED_TEXT", "value": "Procedure completed", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "local" }}, "code_string": "at0043" }} }}
  }},
  "description": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "Tree" }}, "archetype_node_id": "at0001", "items": [{name}] }}
}}"#,
        details = details("openEHR-EHR-ACTION.procedure.v1"),
        name = element("at0002", "Procedure name", &snomed("Appendectomy", code)),
    )
}

/// The composition holding one problem, one allergy and one procedure, each
/// in its section.
fn one_of_each() -> String {
    composition(&[
        section(PROBLEM_LIST, &[problem(HYPERTENSION)]),
        section(ALLERGIES, &[allergy(PENICILLIN)]),
        section(PROCEDURES, &[procedure(APPENDECTOMY)]),
    ])
}

/// The SNOMED CT codes of `resource`'s `code`.
fn snomed_codes(resource: &Resource) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(serde_json::to_value(resource)?["code"]["coding"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|coding| coding["system"] == SNOMED)
        .filter_map(|coding| coding["code"].as_str().map(ToOwned::to_owned))
        .collect())
}

#[test]
fn each_section_maps_its_entry_with_its_snomed_ct_code() -> Result<(), Box<dyn Error>> {
    let resources = run(
        &summary()?,
        &one_of_each(),
        call("Patient/synthetic-subject", "Device/gateway"),
    )?;
    for (code, kind, snomed) in [
        (PROBLEMS_CODE, "Condition", HYPERTENSION),
        (ALLERGIES_CODE, "AllergyIntolerance", PENICILLIN),
        (PROCEDURES_CODE, "Procedure", APPENDECTOMY),
    ] {
        let entries = section_entries(&resources, code)?;
        let [entry] = entries.as_slice() else {
            return Err(format!("{code}: {} entries", entries.len()).into());
        };
        let object = serde_json::to_value(entry)?;
        assert_eq!(kind, object["resourceType"], "{code}");
        assert_eq!(vec![snomed.to_owned()], snomed_codes(entry)?, "{code}");
    }
    Ok(())
}

/// Returns the document of `form` over [`one_of_each`] and every other section the form requires
/// `nilknown`.
fn document_of(form: &'static Form) -> Result<String, Box<dyn Error>> {
    let mut document = Document::of(
        form,
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-000000002020"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Patient Summary"),
    );
    let subject = document.with_patient(patient())?;
    let device = document.with_author(&Resource::Device(Box::new(Device {
        id: Some(String::from("gateway")),
        ..Device::default()
    })))?;
    let answer = summary()?.to_fhir_with(&one_of_each(), &settings(), call(&subject, &device))?;
    let mut placed = Vec::new();
    for mapped in document.with_mapped_composition(answer.bundle())? {
        placed.push(mapped.slot.slice);
        document.with_section(mapped.into_section(vec![String::from("One synthetic entry.")]))?;
    }
    assert_eq!(
        placed,
        ["sectionProblems", "sectionAllergies", "sectionProceduresHx"]
    );
    let required: Vec<&str> = form
        .rules
        .iter()
        .flat_map(|rule| match rule {
            SectionRule::Required { slices } => slices.to_vec(),
            _ => Vec::new(),
        })
        .collect();
    for slot in form
        .slots
        .iter()
        .filter(|slot| required.contains(&slot.slice) && !placed.contains(&slot.slice))
    {
        document.with_section(
            Section::new(slot, vec![String::from("Nothing recorded.")])
                .empty_because(EmptyReason::NilKnown),
        )?;
    }
    Ok(serde_json::to_string(&document.into_bundle()?)?)
}

#[test]
fn the_summary_with_one_of_each_validates_in_both_forms() -> Result<(), Box<dyn Error>> {
    let Some(set) = profile_set()? else {
        return Ok(());
    };
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    for (form, name) in [
        (
            &IPS_PATIENT_SUMMARY_FORM,
            "eehrxf-ips-summary-problem-allergy-procedure",
        ),
        (
            &PATIENT_SUMMARY_FORM,
            "eehrxf-eps-summary-problem-allergy-procedure",
        ),
    ] {
        let text = document_of(form)?;
        runtime
            .block_on(ReceivedDocument::read(&text)?.validate(
                set,
                form.bundle,
                None::<&ferrobridge_term::client::Client>,
            ))
            .map_err(|error| format!("{}: {error:?}", form.bundle))?;
        if form.bundle == IPS_BUNDLE {
            let root: serde_json::Value = serde_json::from_str(&text)?;
            assert_eq!(root["meta"]["profile"][0], IPS_BUNDLE);
        }
        emit(name, &text)?;
    }
    Ok(())
}

