// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A discharge report composition mapped out of openEHR: the composition, its encounter and the entry of each section.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use crate::support::hdr;
use crate::support::vendored_base;
use eehrxf::crosswalk::discharge::HDR_COMPOSITION;
use eehrxf::document::discharge::HDR_ENCOUNTER;
use eehrxf::mappings::HDR;
use eehrxf::receive::conform::check_resource;
use ferrobridge_testkit::fixtures::FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE_ID;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::run::Settings;
use serde_json::Value;
use std::error::Error;

use super::{
    ADMITTED, CONDITION_EU_CORE, CONTEXT, DIAGNOSIS, DISCHARGED, MEDICATION_REQUEST_EU_CORE,
    MEDICINE, NOW, PERFORMED, PROCEDURE, PROCEDURE_EU_CORE, composition, diagnosis,
    discharge_document, full_report, mapping, object, of_type, only, reference, section_entries,
    settings,
};
#[test]
fn the_shipped_project_compiles_against_the_discharge_report_template() -> Result<(), Box<dyn Error>>
{
    let mapping = mapping()?;
    let contexts: Vec<_> = mapping
        .contexts()
        .map(|context| (context.name, context.template, context.profile))
        .collect();
    assert_eq!(
        contexts,
        [(
            CONTEXT,
            FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE_ID,
            HDR_COMPOSITION
        )]
    );
    assert_eq!(HDR.contexts(), &[CONTEXT]);
    Ok(())
}

#[test]
fn the_report_maps_to_its_composition_encounter_and_condition() -> Result<(), Box<dyn Error>> {
    let run = mapping()?.to_fhir_with(
        &composition(&[diagnosis()]),
        &settings(),
        CallContext::new()
            .with_patient(reference("Patient/synthetic-patient"))
            .with_who(reference("Device/gateway")),
    )?;
    let bundle = run.bundle();
    assert!(
        of_type(bundle, "OperationOutcome")?.is_empty(),
        "the run declares no loss"
    );
    assert_eq!(
        of_type(bundle, "Provenance")?.len(),
        1,
        "one Provenance per run"
    );
    let composition = only(bundle, "Composition")?;
    assert_eq!(composition["meta"]["profile"][0], HDR_COMPOSITION);
    assert_eq!(composition["type"]["coding"][0]["code"], "34105-7");
    let encounter = only(bundle, "Encounter")?;
    assert_eq!(encounter["period"]["start"], ADMITTED);
    assert_eq!(encounter["period"]["end"], DISCHARGED);
    assert_eq!(encounter["status"], "finished");
    assert_eq!(encounter["class"]["code"], "IMP");
    check_resource(
        &hdr(HDR_ENCOUNTER)?,
        "Encounter",
        &object(bundle, "Encounter")?,
    )?;
    let condition = only(bundle, "Condition")?;
    assert_eq!(condition["code"]["coding"][0]["code"], DIAGNOSIS);
    assert_eq!(condition["onsetDateTime"], "2026-10-11T22:00:00Z");
    assert_eq!(
        condition["extension"][0]["url"],
        "http://hl7.org/fhir/StructureDefinition/condition-assertedDate"
    );
    assert_eq!(
        condition["extension"][0]["valueDateTime"],
        "2026-10-12T10:15:00Z"
    );
    assert_eq!(
        condition["subject"]["reference"],
        "Patient/synthetic-patient"
    );
    check_resource(
        &vendored_base(CONDITION_EU_CORE)?,
        "Condition",
        &object(bundle, "Condition")?,
    )?;
    let section = &composition["section"][0];
    assert_eq!(section["code"]["coding"][0]["code"], "11535-2");
    assert_eq!(
        section["entry"][0]["reference"],
        format!(
            "Condition/{}",
            condition["id"].as_str().ok_or("the condition's id")?
        )
    );
    Ok(())
}

#[test]
fn two_diagnoses_are_two_entries_of_the_diagnostic_summary() -> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    let (mut document, subject, device) =
        discharge_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d107")?;
    let run = mapping.to_fhir_with(
        &composition(&[diagnosis(), diagnosis()]),
        &Settings::new(device.clone(), NOW),
        CallContext::new()
            .with_patient(reference(&subject))
            .with_who(reference(&device)),
    )?;
    let sections = document.with_mapped_composition(run.bundle())?;
    let [section] = sections.as_slice() else {
        return Err(format!("{} sections, not one", sections.len()).into());
    };
    assert_eq!(section.slot.slice, "sectionDiagnosticSummary");
    assert_eq!(section.entries.len(), 2, "{:?}", section.entries);
    assert_ne!(section.entries.first(), section.entries.last());
    Ok(())
}

/// A `Bundle` of `resources`, each FHIR JSON.
#[test]
fn the_report_maps_its_procedure_and_discharge_medication() -> Result<(), Box<dyn Error>> {
    let run = mapping()?.to_fhir_with(
        &full_report(),
        &settings(),
        CallContext::new()
            .with_patient(reference("Patient/synthetic-patient"))
            .with_who(reference("Device/gateway")),
    )?;
    let bundle = run.bundle();
    // NOTE: R4 Dosage.timing is a Timing, and the order's ACTIVITY.timing a DV_PARSABLE no
    // FHIRconnect data-type page pairs with it, so the run declares the timing it holds.
    let outcomes = of_type(bundle, "OperationOutcome")?;
    let declared: Vec<&Value> = outcomes
        .iter()
        .flat_map(|outcome| outcome["issue"].as_array().into_iter().flatten())
        .collect();
    assert_eq!(
        declared.len(),
        1,
        "the run declares the timing alone: {declared:?}"
    );
    assert!(
        declared[0]["details"]["coding"][0]["code"]
            .as_str()
            .is_some_and(|path| path.ends_with("/activities[at0001]/timing")),
        "{declared:?}"
    );
    assert_eq!(
        of_type(bundle, "Provenance")?.len(),
        1,
        "one Provenance per run"
    );
    let procedure = only(bundle, "Procedure")?;
    assert_eq!(procedure["meta"]["profile"][0], PROCEDURE_EU_CORE);
    assert_eq!(procedure["status"], "completed");
    assert_eq!(procedure["code"]["coding"][0]["code"], PROCEDURE);
    assert_eq!(procedure["performedDateTime"], PERFORMED);
    assert_eq!(procedure["bodySite"][0]["text"], "Synthetic body site");
    assert_eq!(
        procedure["note"][0]["text"],
        "A synthetic procedure comment."
    );
    check_resource(
        &vendored_base(PROCEDURE_EU_CORE)?,
        "Procedure",
        &object(bundle, "Procedure")?,
    )?;
    let request = only(bundle, "MedicationRequest")?;
    assert_eq!(request["meta"]["profile"][0], MEDICATION_REQUEST_EU_CORE);
    assert_eq!(request["status"], "unknown");
    assert_eq!(request["intent"], "order");
    assert_eq!(
        request["medicationCodeableConcept"]["coding"][0]["code"],
        MEDICINE
    );
    assert_eq!(
        request["dosageInstruction"][0]["text"],
        "One synthetic tablet each morning"
    );
    assert_eq!(request["note"][0]["text"], "A synthetic order comment.");
    check_resource(
        &vendored_base(MEDICATION_REQUEST_EU_CORE)?,
        "MedicationRequest",
        &object(bundle, "MedicationRequest")?,
    )?;
    let composition = only(bundle, "Composition")?;
    let id = |resource: &Value| resource["id"].as_str().map(ToOwned::to_owned);
    assert_eq!(
        section_entries(&composition, "10185-7"),
        [format!(
            "Procedure/{}",
            id(&procedure).ok_or("the procedure's id")?
        )]
    );
    assert_eq!(
        section_entries(&composition, "75311-1"),
        [format!(
            "MedicationRequest/{}",
            id(&request).ok_or("the request's id")?
        )]
    );
    Ok(())
}
