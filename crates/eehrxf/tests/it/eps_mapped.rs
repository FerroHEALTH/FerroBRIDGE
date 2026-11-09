// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The medication summary and the immunisations of the patient summary,
//! mapped from openEHR through the component's EPS project over FerroBRIDGE's
//! patient summary template, whose operational template the suite builds
//! from the vendored CKM archetypes.
//!
//! The project's root context maps the composition and enters each section
//! through the sub-context its section path names (FHIRconnect 1.0.0,
//! `types-of-mapping-files/sub-contexts.adoc`). A medicine recorded as a
//! medication order or a medication action in the medication summary section
//! is a medication summary entry, and a vaccine recorded as a medication
//! action in the immunisations section an immunisation, so the section, never
//! the archetype, places an entry. The document takes the run's sections,
//! held to the vendored profiles.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::crosswalk::patient_summary::{
    PATIENT_SUMMARY, PATIENT_SUMMARY_TEMPLATE, SOURCES, Source,
};
use eehrxf::document::form::slot;
use eehrxf::document::section::{EmptyReason, Section};
use eehrxf::document::{Document, DocumentError, EPS_BUNDLE};
use eehrxf::mapping::Mapping;
use eehrxf::mappings::EPS;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::conform::check_resource;
use ferrobridge_testkit::fixtures::{
    CKM_MEDICATION_ARCHETYPES, FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE,
    FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE_ID,
};
use fhir_types::codec::{Json, Value};
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::operations::run::Settings;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::template::TemplateSource;

use super::support::{emit, eps, vendored};

/// The root context of the EPS project.
const ROOT: &str = "ferrobridge_eu_eps.context";

/// The LOINC code of the medication summary section.
const MEDICATIONS: &str = "10160-0";

/// The LOINC code of the immunisations section.
const IMMUNIZATIONS: &str = "11369-6";

/// The EPS profile of a medication summary entry.
const MEDICATION_STATEMENT: &str =
    "http://hl7.eu/fhir/eps/StructureDefinition/MedicationStatement-eu-eps";

/// The EU core profile of an immunisations entry.
const IMMUNIZATION_EU_CORE: &str =
    "http://hl7.eu/fhir/base/StructureDefinition/immunization-eu-core";

/// The EPS `Composition` profile.
const EPS_COMPOSITION: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

/// The base the document names its entries under, in the `example.org`
/// domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The synthetic medicine's code.
const MEDICINE: &str = "SYN-MEDICINE-0001";

/// The synthetic vaccine's code.
const VACCINE: &str = "SYN-VACCINE-0001";

/// The ADL 2 operational template of the patient summary template.
fn template() -> Result<TemplateSource, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    for (file, source) in CKM_MEDICATION_ARCHETYPES {
        let archetype = openehr_adl::assemble::parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    let template =
        openehr_adl::assemble::parse_artefact(FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE, Dialect::Adl2)
            .map_err(|errors| format!("the template: {errors:?}"))?;
    let raw = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
        resolved_id: String::from(FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE_ID),
    })
}

/// The EPS project, its one root context compiled from its embedded files.
fn summary() -> Result<Mapping, Box<dyn Error>> {
    Ok(Mapping::compile_project(&template()?, &EPS)?)
}

/// A synthetic coded text of the local synthetic code system.
fn coded(value: &str, code: &str) -> String {
    format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "{value}", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "http://example.org/fhir/CodeSystem/ferrobridge-products" }}, "code_string": "{code}" }} }}"#
    )
}

/// The archetype details of `id`.
fn details(id: &str) -> String {
    format!(
        r#""archetype_node_id": "{id}", "archetype_details": {{ "_type": "ARCHETYPED", "archetype_id": {{ "_type": "ARCHETYPE_ID", "value": "{id}" }}, "rm_version": "1.1.0" }}"#
    )
}

/// The language, encoding and subject of an entry.
const ENTRY: &str = r#""language": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }, "code_string": "en" },
  "encoding": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "IANA_character-sets" }, "code_string": "UTF-8" },
  "subject": { "_type": "PARTY_SELF" }"#;

/// A medication order of the medicine `code`.
fn order(code: &str) -> String {
    format!(
        r#"{{
  "_type": "INSTRUCTION",
  "name": {{ "_type": "DV_TEXT", "value": "Medication order" }},
  {details},
  {ENTRY},
  "narrative": {{ "_type": "DV_TEXT", "value": "Synthetic medication order" }},
  "activities": [{{
    "_type": "ACTIVITY",
    "name": {{ "_type": "DV_TEXT", "value": "Order" }},
    "archetype_node_id": "at0001",
    "description": {{
      "_type": "ITEM_TREE",
      "name": {{ "_type": "DV_TEXT", "value": "Tree" }},
      "archetype_node_id": "at0002",
      "items": [
        {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Medication item" }}, "archetype_node_id": "at0070", "value": {medicine} }},
        {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Overall directions description" }}, "archetype_node_id": "at0009", "value": {{ "_type": "DV_TEXT", "value": "One synthetic tablet each morning" }} }},
        {{ "_type": "CLUSTER", "name": {{ "_type": "DV_TEXT", "value": "Order details" }}, "archetype_node_id": "at0113", "items": [
          {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Order start date/time" }}, "archetype_node_id": "at0012", "value": {{ "_type": "DV_DATE_TIME", "value": "2026-10-01T08:00:00Z" }} }}
        ] }}
      ]
    }},
    "timing": {{ "_type": "DV_PARSABLE", "value": "R/2026-10-01T08:00:00Z/P1D", "formalism": "ISO8601" }},
    "action_archetype_id": "openEHR-EHR-ACTION\\.medication\\.v1"
  }}],
  "protocol": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "Tree" }}, "archetype_node_id": "at0005", "items": [] }}
}}"#,
        details = details("openEHR-EHR-INSTRUCTION.medication_order.v3"),
        medicine = coded("Synthetic tablet 500 mg", code),
    )
}

/// A medication action recording a dose of the vaccine `code` administered.
fn administered(code: &str) -> String {
    format!(
        r#"{{
  "_type": "ACTION",
  "name": {{ "_type": "DV_TEXT", "value": "Vaccination administered" }},
  {details},
  {ENTRY},
  "time": {{ "_type": "DV_DATE_TIME", "value": "2026-09-15T09:30:00Z" }},
  "ism_transition": {{
    "_type": "ISM_TRANSITION",
    "current_state": {{ "_type": "DV_CODED_TEXT", "value": "active", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "245" }} }},
    "careflow_step": {{ "_type": "DV_CODED_TEXT", "value": "Dose administered", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "local" }}, "code_string": "at0006" }} }}
  }},
  "description": {{
    "_type": "ITEM_TREE",
    "name": {{ "_type": "DV_TEXT", "value": "Tree" }},
    "archetype_node_id": "at0017",
    "items": [
      {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Medication item" }}, "archetype_node_id": "at0020", "value": {vaccine} }}
    ]
  }}
}}"#,
        details = details("openEHR-EHR-ACTION.medication.v1"),
        vaccine = coded("Synthetic vaccine", code),
    )
}

/// A medication action recording the medicine `code` the patient takes.
fn taken(code: &str) -> String {
    format!(
        r#"{{
  "_type": "ACTION",
  "name": {{ "_type": "DV_TEXT", "value": "Medication action" }},
  {details},
  {ENTRY},
  "time": {{ "_type": "DV_DATE_TIME", "value": "2026-09-20T08:00:00Z" }},
  "ism_transition": {{
    "_type": "ISM_TRANSITION",
    "current_state": {{ "_type": "DV_CODED_TEXT", "value": "active", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "245" }} }}
  }},
  "description": {{
    "_type": "ITEM_TREE",
    "name": {{ "_type": "DV_TEXT", "value": "Tree" }},
    "archetype_node_id": "at0017",
    "items": [
      {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Medication item" }}, "archetype_node_id": "at0020", "value": {medicine} }}
    ]
  }}
}}"#,
        details = details("openEHR-EHR-ACTION.medication.v1"),
        medicine = coded("Synthetic tablet 500 mg", code),
    )
}

/// The medication summary section of the template.
const MEDICATION_SUMMARY: (&str, &str) = ("Medication summary", "openEHR-EHR-SECTION.adhoc.v1");

/// The immunisations section of the template.
const IMMUNISATIONS: (&str, &str) = ("Immunisations", "openEHR-EHR-SECTION.immunisation_list.v0");

/// The section `(name, archetype)` of the template, holding `items`.
fn section((name, archetype): (&str, &str), items: &[String]) -> String {
    format!(
        r#"{{ "_type": "SECTION", "name": {{ "_type": "DV_TEXT", "value": "{name}" }}, {details}, "items": [{items}] }}"#,
        details = details(archetype),
        items = items.join(","),
    )
}

/// A synthetic composition of the patient summary template
/// whose sections hold `sections`.
fn composition(sections: &[String]) -> String {
    format!(
        r#"{{
  "_type": "COMPOSITION",
  "name": {{ "_type": "DV_TEXT", "value": "Patient summary" }},
  "archetype_node_id": "openEHR-EHR-COMPOSITION.health_summary.v1",
  "archetype_details": {{
    "_type": "ARCHETYPED",
    "archetype_id": {{ "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.health_summary.v1" }},
    "template_id": {{ "_type": "TEMPLATE_ID", "value": "{PATIENT_SUMMARY_TEMPLATE}" }},
    "rm_version": "1.1.0"
  }},
  "uid": {{ "_type": "OBJECT_VERSION_ID", "value": "5a1c3e20-0000-4000-8000-000000001323::ferrobridge.example::1" }},
  "language": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }}, "code_string": "en" }},
  "territory": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "ISO_3166-1" }}, "code_string": "NL" }},
  "category": {{
    "_type": "DV_CODED_TEXT",
    "value": "event",
    "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "433" }}
  }},
  "composer": {{ "_type": "PARTY_IDENTIFIED", "name": "Synthetic composer" }},
  "context": {{
    "_type": "EVENT_CONTEXT",
    "start_time": {{ "_type": "DV_DATE_TIME", "value": "2026-10-01T08:00:00Z" }},
    "setting": {{ "_type": "DV_CODED_TEXT", "value": "other care", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "238" }} }}
  }},
  "content": [{content}]
}}"#,
        content = sections.join(","),
    )
}

/// The composition holding one medicine as an order and one vaccine as an
/// administered action, each in its template section.
fn medicine_and_vaccine() -> String {
    composition(&[
        section(MEDICATION_SUMMARY, &[order(MEDICINE)]),
        section(IMMUNISATIONS, &[administered(VACCINE)]),
    ])
}

/// The run settings: a synthetic engine device and a fixed instant.
fn settings() -> Settings {
    Settings::new("Device/ferrobridge-eehrxf-test", "2026-10-06T10:00:00Z")
}

/// The call context naming the document's subject and its gateway device.
fn call(subject: &str, device: &str) -> CallContext {
    CallContext::new()
        .with_patient(reference(subject))
        .with_who(reference(device))
}

/// The literal reference `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}

/// The resources one run of `mapping` over `composition` answers, its
/// `OperationOutcome` left out, refusing a run that declares a loss other than
/// an element the project does not read: the synthetic order's
/// `ACTIVITY.timing`, a `DV_PARSABLE` no FHIRconnect data-type page pairs with
/// R4 `Timing`.
fn run(
    mapping: &Mapping,
    composition: &str,
    context: CallContext,
) -> Result<Vec<Resource>, Box<dyn Error>> {
    let (answer, warnings) = mapping.to_fhir_with_warnings(composition, &settings(), context)?;
    if let Some(warning) = warnings
        .iter()
        .find(|warning| !matches!(warning.loss(), Loss::Unread { .. }))
    {
        return Err(format!("the run declared a loss: {warning}").into());
    }
    Ok(answer
        .bundle()
        .entry
        .iter()
        .filter_map(|entry| entry.resource.clone())
        .filter(|resource| !matches!(resource, Resource::OperationOutcome(_)))
        .collect())
}

/// The codes of `resource`'s medicine or vaccine.
fn codes(resource: &Resource) -> Result<Vec<String>, Box<dyn Error>> {
    let object = serde_json::to_value(resource)?;
    let concept = if object["resourceType"] == "Immunization" {
        &object["vaccineCode"]
    } else {
        &object["medicationCodeableConcept"]
    };
    Ok(concept["coding"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|coding| coding["code"].as_str().map(ToOwned::to_owned))
        .collect())
}

/// The resources of `resources` of the type `name`.
fn of_type<'a>(resources: &'a [Resource], name: &str) -> Result<Vec<&'a Resource>, Box<dyn Error>> {
    let mut found = Vec::new();
    for resource in resources {
        if Json::to_json(resource)?.get("resourceType") == Some(&Value::String(name.to_owned())) {
            found.push(resource);
        }
    }
    Ok(found)
}

/// The resources the section of `resources`' one `Composition` whose LOINC
/// code is `code` names as its entries, in its order.
fn section_entries<'a>(
    resources: &'a [Resource],
    code: &str,
) -> Result<Vec<&'a Resource>, Box<dyn Error>> {
    let composition = of_type(resources, "Composition")?;
    let [composition] = composition.as_slice() else {
        return Err(format!("{} compositions", composition.len()).into());
    };
    let composition = serde_json::to_value(composition)?;
    let named: Vec<String> = composition["section"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|section| section["code"]["coding"][0]["code"] == code)
        .flat_map(|section| section["entry"].as_array().cloned().unwrap_or_default())
        .filter_map(|entry| entry["reference"].as_str().map(ToOwned::to_owned))
        .collect();
    let mut found = Vec::new();
    for reference in named {
        let resource = resources
            .iter()
            .find(|resource| {
                serde_json::to_value(resource).is_ok_and(|object| {
                    format!(
                        "{}/{}",
                        object["resourceType"].as_str().unwrap_or_default(),
                        object["id"].as_str().unwrap_or_default()
                    ) == reference
                })
            })
            .ok_or_else(|| format!("the section names {reference}, which the run holds not"))?;
        found.push(resource);
    }
    Ok(found)
}

#[test]
fn a_medication_order_maps_to_the_medication_summary_entry() -> Result<(), Box<dyn Error>> {
    let composition = composition(&[section(MEDICATION_SUMMARY, &[order(MEDICINE)])]);
    let resources = run(
        &summary()?,
        &composition,
        call("Patient/synthetic-subject", "Device/gateway"),
    )?;
    let statements = section_entries(&resources, MEDICATIONS)?;
    assert_eq!(1, statements.len(), "one statement per order");
    assert_eq!(1, of_type(&resources, "MedicationStatement")?.len());
    let statement = statements.first().ok_or("the statement")?;
    assert_eq!(vec![MEDICINE.to_owned()], codes(statement)?);
    let object = Json::to_json(*statement)?;
    assert_eq!(object.get("status"), Some(&Value::String("unknown".into())));
    check_resource(
        &vendored(MEDICATION_STATEMENT)?,
        "MedicationStatement",
        &object,
    )?;
    assert_eq!(
        1,
        of_type(&resources, "Provenance")?.len(),
        "one Provenance per run"
    );
    Ok(())
}

#[test]
fn a_medication_action_under_the_medication_summary_is_a_medication_summary_entry()
-> Result<(), Box<dyn Error>> {
    let composition = composition(&[section(MEDICATION_SUMMARY, &[taken(MEDICINE)])]);
    let resources = run(
        &summary()?,
        &composition,
        call("Patient/synthetic-subject", "Device/gateway"),
    )?;
    let statements = section_entries(&resources, MEDICATIONS)?;
    assert_eq!(1, statements.len(), "one statement per action");
    assert_eq!(0, of_type(&resources, "Immunization")?.len());
    let statement = statements.first().ok_or("the statement")?;
    assert_eq!(vec![MEDICINE.to_owned()], codes(statement)?);
    let object = Json::to_json(*statement)?;
    assert_eq!(object.get("status"), Some(&Value::String("active".into())));
    assert_eq!(
        object.get("effectiveDateTime"),
        Some(&Value::String("2026-09-20T08:00:00Z".into()))
    );
    check_resource(
        &vendored(MEDICATION_STATEMENT)?,
        "MedicationStatement",
        &object,
    )?;
    Ok(())
}

#[test]
fn a_vaccine_and_a_medicine_are_each_placed_by_their_template_section() -> Result<(), Box<dyn Error>>
{
    let composition = medicine_and_vaccine();
    let resources = run(
        &summary()?,
        &composition,
        call("Patient/synthetic-subject", "Device/gateway"),
    )?;
    let statements = section_entries(&resources, MEDICATIONS)?;
    let immunizations = section_entries(&resources, IMMUNIZATIONS)?;
    assert_eq!(1, statements.len(), "the medicine alone is a statement");
    assert_eq!(
        1,
        immunizations.len(),
        "the vaccine alone is an immunisation"
    );
    assert_eq!(1, of_type(&resources, "MedicationStatement")?.len());
    assert_eq!(1, of_type(&resources, "Immunization")?.len());
    let statement = statements.first().ok_or("the statement")?;
    let immunization = immunizations.first().ok_or("the immunisation")?;
    assert_eq!(vec![MEDICINE.to_owned()], codes(statement)?);
    assert_eq!(vec![VACCINE.to_owned()], codes(immunization)?);
    let object = Json::to_json(*immunization)?;
    assert_eq!(
        object.get("status"),
        Some(&Value::String("completed".into()))
    );
    check_resource(&vendored(IMMUNIZATION_EU_CORE)?, "Immunization", &object)?;
    Ok(())
}

#[test]
fn one_archetype_in_two_sections_is_placed_by_the_section_that_holds_it()
-> Result<(), Box<dyn Error>> {
    let composition = composition(&[
        section(MEDICATION_SUMMARY, &[taken(MEDICINE)]),
        section(IMMUNISATIONS, &[administered(VACCINE)]),
    ]);
    let resources = run(
        &summary()?,
        &composition,
        call("Patient/synthetic-subject", "Device/gateway"),
    )?;
    let statements = section_entries(&resources, MEDICATIONS)?;
    let immunizations = section_entries(&resources, IMMUNIZATIONS)?;
    assert_eq!(1, statements.len(), "{statements:?}");
    assert_eq!(1, immunizations.len(), "{immunizations:?}");
    assert_eq!(
        vec![MEDICINE.to_owned()],
        codes(statements.first().ok_or("the statement")?)?
    );
    assert_eq!(
        vec![VACCINE.to_owned()],
        codes(immunizations.first().ok_or("the immunisation")?)?
    );
    assert_eq!(1, of_type(&resources, "MedicationStatement")?.len());
    assert_eq!(1, of_type(&resources, "Immunization")?.len());
    Ok(())
}

#[test]
fn a_vaccine_alone_writes_no_medication_summary_entry() -> Result<(), Box<dyn Error>> {
    let composition = composition(&[section(IMMUNISATIONS, &[administered(VACCINE)])]);
    let resources = run(
        &summary()?,
        &composition,
        call("Patient/synthetic-subject", "Device/gateway"),
    )?;
    assert!(
        section_entries(&resources, MEDICATIONS)?.is_empty(),
        "an action in the immunisations section never feeds the medication summary"
    );
    assert_eq!(0, of_type(&resources, "MedicationStatement")?.len());
    assert_eq!(1, section_entries(&resources, IMMUNIZATIONS)?.len());
    Ok(())
}

/// The template overlay of FerroBRIDGE's patient summary template that
/// specialises the section archetype `section`.
fn overlay(section: &str) -> Option<&'static str> {
    FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE
        .split("template_overlay")
        .skip(1)
        .find(|part| part.contains(&format!("specialize\n    {section}\n")))
}

#[test]
fn every_source_names_the_template_section_that_holds_it() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        PATIENT_SUMMARY_TEMPLATE,
        FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE_ID
    );
    for Source {
        path,
        template,
        section,
        archetype,
    } in SOURCES
    {
        assert_eq!(*template, PATIENT_SUMMARY_TEMPLATE, "{path}");
        let overlay = overlay(section).ok_or_else(|| format!("{path}: no overlay of {section}"))?;
        assert!(
            overlay.contains(&format!(", {archetype}]")),
            "{path}: the template's {section} holds {archetype}"
        );
        let row = PATIENT_SUMMARY
            .row(path)
            .ok_or_else(|| format!("{path} is a crosswalk row"))?;
        assert!(!row.contexts.is_empty(), "{path} admits a mapped entry");
    }
    Ok(())
}

#[test]
fn every_source_feeds_its_row_from_its_section() -> Result<(), Box<dyn Error>> {
    assert_eq!(EPS.contexts(), &[ROOT], "the project compiles its root");
    let mapping = summary()?;
    for Source {
        path,
        section: held_by,
        archetype,
        ..
    } in SOURCES
    {
        let (holder, code) = if *held_by == MEDICATION_SUMMARY.1 {
            (MEDICATION_SUMMARY, MEDICATIONS)
        } else if *held_by == IMMUNISATIONS.1 {
            (IMMUNISATIONS, IMMUNIZATIONS)
        } else {
            return Err(format!("{path}: {held_by} is no section of the suite").into());
        };
        let item = match *archetype {
            "openEHR-EHR-INSTRUCTION.medication_order.v3" => order(MEDICINE),
            "openEHR-EHR-ACTION.medication.v1" if code == MEDICATIONS => taken(MEDICINE),
            "openEHR-EHR-ACTION.medication.v1" => administered(VACCINE),
            other => return Err(format!("{path}: no synthetic {other}").into()),
        };
        let resources = run(
            &mapping,
            &composition(&[section(holder, &[item])]),
            call("Patient/synthetic-subject", "Device/gateway"),
        )?;
        let entries = section_entries(&resources, code)?;
        let [entry] = entries.as_slice() else {
            return Err(format!("{path}: {} entries", entries.len()).into());
        };
        let profiles: Vec<String> = serde_json::to_value(entry)?["meta"]["profile"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|profile| profile.as_str().map(ToOwned::to_owned))
            .collect();
        let row = PATIENT_SUMMARY.row(path).ok_or("the source's row")?;
        assert!(
            profiles
                .iter()
                .any(|profile| row.contexts.contains(&profile.as_str())),
            "{path}: {profiles:?} is admitted by {:?}",
            row.contexts
        );
    }
    Ok(())
}

/// The synthetic patient.
fn patient() -> Patient {
    Patient {
        id: Some(String::from("synthetic-subject")),
        name: vec![HumanName {
            text: Some("Synthetic Subject".into()),
            ..HumanName::default()
        }],
        birth_date: Some("1970-01-01".into()),
        ..Patient::default()
    }
}

/// The section of the slice `slice` holding the first resource each of
/// `added` names, or empty when `added` names none.
fn section_of(
    slice: &str,
    paragraph: &str,
    added: Option<&String>,
) -> Result<Section, Box<dyn Error>> {
    let section = Section::new(
        slot(slice).ok_or_else(|| format!("the {slice} slot"))?,
        vec![String::from(paragraph)],
    );
    Ok(match added {
        Some(url) => section.with_entry(url.clone()),
        None => section.empty_because(EmptyReason::NilKnown),
    })
}

#[test]
fn the_summary_holds_the_medicine_and_the_vaccine_in_their_sections() -> Result<(), Box<dyn Error>>
{
    let composition = medicine_and_vaccine();
    let mut document = Document::new(
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-000000001323"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Patient Summary"),
    );
    let subject = document.with_patient(patient())?;
    let device = document.with_author(&Resource::Device(Box::new(Device {
        id: Some(String::from("gateway")),
        device_name: vec![DeviceDeviceName {
            name: "Synthetic gateway".into(),
            r#type: "user-friendly-name".into(),
            ..DeviceDeviceName::default()
        }],
        ..Device::default()
    })))?;
    document.with_author(&Resource::Organization(Box::new(Organization {
        id: Some(String::from("operator")),
        name: Some("Synthetic operator".into()),
        ..Organization::default()
    })))?;
    let answer = summary()?.to_fhir_with(&composition, &settings(), call(&subject, &device))?;
    let mut placed = Vec::new();
    for mapped in document.with_mapped_composition(answer.bundle())? {
        placed.push(mapped.slot.slice);
        let paragraph = match mapped.slot.slice {
            "sectionMedications" => "One synthetic medicine.",
            _ => "One synthetic vaccine.",
        };
        document.with_section(mapped.into_section(vec![String::from(paragraph)]))?;
    }
    assert_eq!(placed, ["sectionMedications", "sectionImmunizations"]);
    for slice in [
        "sectionProblems",
        "sectionAllergies",
        "sectionProceduresHx",
        "sectionMedicalDevices",
    ] {
        document.with_section(section_of(slice, "Nothing recorded.", None)?)?;
    }
    let bundle = document.into_bundle()?;
    let text = serde_json::to_string(&bundle)?;
    let received = ReceivedDocument::read(&text)?;
    received.check(&eps(EPS_BUNDLE)?, &eps(EPS_COMPOSITION)?)?;
    let root: serde_json::Value = serde_json::from_str(&text)?;
    let sections = root["entry"][0]["resource"]["section"]
        .as_array()
        .ok_or("the sections")?;
    let entries = |code: &str| -> Vec<String> {
        sections
            .iter()
            .filter(|section| section["code"]["coding"][0]["code"] == code)
            .flat_map(|section| section["entry"].as_array().cloned().unwrap_or_default())
            .filter_map(|entry| entry["reference"].as_str().map(ToOwned::to_owned))
            .collect()
    };
    let medications = entries("10160-0");
    let immunizations = entries("11369-6");
    assert_eq!(1, medications.len(), "{medications:?}");
    assert_eq!(1, immunizations.len(), "{immunizations:?}");
    assert!(
        medications
            .iter()
            .all(|entry| entry.contains("MedicationStatement"))
    );
    assert!(
        immunizations
            .iter()
            .all(|entry| entry.contains("Immunization"))
    );
    emit("eehrxf-summary-medicine-and-vaccine", &text)?;
    Ok(())
}

#[test]
fn a_document_takes_one_section_of_a_run_and_nothing_another_section_names()
-> Result<(), Box<dyn Error>> {
    let mut document = Document::new(
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-000000001324"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Patient Summary"),
    );
    let subject = document.with_patient(patient())?;
    let answer = summary()?.to_fhir_with(
        &medicine_and_vaccine(),
        &settings(),
        call(&subject, "Device/gateway"),
    )?;
    let section = document
        .with_mapped_section(answer.bundle(), "sectionMedications")?
        .ok_or("the run holds a medication summary")?;
    assert_eq!(section.slot.slice, "sectionMedications");
    assert_eq!(1, section.entries.len(), "{:?}", section.entries);
    let entry = section.entries.first().ok_or("the entry")?;
    assert!(entry.contains("/MedicationStatement/"), "{entry}");
    document.with_section(section.into_section(vec![String::from("One synthetic medicine.")]))?;
    for slice in [
        "sectionProblems",
        "sectionAllergies",
        "sectionProceduresHx",
        "sectionMedicalDevices",
    ] {
        document.with_section(section_of(slice, "Nothing recorded.", None)?)?;
    }
    let bundle = serde_json::to_value(document.into_bundle()?)?;
    let kinds: Vec<&str> = bundle["entry"]
        .as_array()
        .ok_or("the entries")?
        .iter()
        .filter_map(|entry| entry["resource"]["resourceType"].as_str())
        .collect();
    assert!(!kinds.contains(&"Immunization"), "{kinds:?}");
    let provenance = bundle["entry"]
        .as_array()
        .ok_or("the entries")?
        .iter()
        .find(|entry| entry["resource"]["resourceType"] == "Provenance")
        .ok_or("the run's Provenance")?;
    let targets = provenance["resource"]["target"]
        .as_array()
        .ok_or("the targets")?;
    assert!(
        targets.iter().all(|target| target["reference"]
            .as_str()
            .is_some_and(|reference| reference.contains("MedicationStatement"))),
        "{targets:?}"
    );
    Ok(())
}

#[test]
fn a_run_with_no_section_in_the_slot_gives_none() -> Result<(), Box<dyn Error>> {
    let mut document = Document::new(
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-000000001325"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Patient Summary"),
    );
    let subject = document.with_patient(patient())?;
    let answer = summary()?.to_fhir_with(
        &composition(&[section(MEDICATION_SUMMARY, &[order(MEDICINE)])]),
        &settings(),
        call(&subject, "Device/gateway"),
    )?;
    assert!(
        document
            .with_mapped_section(answer.bundle(), "sectionImmunizations")?
            .is_none()
    );
    assert!(
        matches!(
            document.with_mapped_section(answer.bundle(), "sectionUnknown"),
            Err(DocumentError::Unslotted { .. })
        ),
        "a slice the form has no slot for is refused"
    );
    Ok(())
}

// NOTE: no specification governs this: our own design; a spawned thread gets the 2 MiB
// default stack (std::thread "Stack size"); the template build, compile and run fit in it.
#[test]
fn the_project_compiles_and_maps_on_a_default_stack_thread() -> Result<(), Box<dyn Error>> {
    let spawned = std::thread::Builder::new().spawn(|| -> Result<usize, String> {
        let source = template().map_err(|error| error.to_string())?;
        let project = EPS.with_context(ROOT).map_err(|error| error.to_string())?;
        let mapping =
            Mapping::compile_project(&source, &project).map_err(|error| error.to_string())?;
        let composition = medicine_and_vaccine();
        let resources = run(
            &mapping,
            &composition,
            call("Patient/synthetic-subject", "Device/gateway"),
        )
        .map_err(|error| error.to_string())?;
        Ok(resources.len())
    })?;
    let mapped = spawned
        .join()
        .map_err(|_panicked| "the mapping thread panicked")??;
    assert!(mapped > 0, "the run mapped the composition");
    Ok(())
}
