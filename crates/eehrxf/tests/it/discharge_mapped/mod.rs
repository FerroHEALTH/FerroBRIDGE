// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The discharge report mapped between openEHR and the HL7 Europe Hospital
//! Discharge Report 1.0.0 document through the shipped HDR project, over
//! FerroBRIDGE's discharge report template, whose operational template the
//! suite builds from the vendored CKM archetypes.
//!
//! A synthetic transfer of care summary of a hospital stay holds one
//! diagnosis in its diagnostic summary section. The run writes the
//! `composition-eu-hdr`, the `encounter-eu-hdr` of the stay and one
//! `condition-eu-core`; the document takes them into a `bundle-eu-hdr`,
//! held to the vendored profiles, received, and mapped back into one
//! composition.

use std::error::Error;

use eehrxf::document::Document;
use eehrxf::document::discharge::{DISCHARGE_REPORT_FORM, HDR_SLOTS};
use eehrxf::document::section::Section;
use eehrxf::mapping::Mapping;
use eehrxf::mappings::HDR;
use ferrobridge_testkit::fixtures::{
    CKM_DISCHARGE_ARCHETYPES, CKM_MEDICATION_ARCHETYPES, FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE,
    FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE_ID,
};
use fhir_types::codec::{Json, Object};
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::traverse::defaults::Defaults;
use fhirconnect::operations::run::Settings;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::template::TemplateSource;
use serde_json::Value;

/// The one context of the HDR project.
const CONTEXT: &str = "ferrobridge_eu_hdr.context";

/// The EU core profile of a diagnostic summary entry.
const CONDITION_EU_CORE: &str = "http://hl7.eu/fhir/base/StructureDefinition/condition-eu-core";

/// The base the documents name their entries under, in the `example.org`
/// domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The instant every run is stamped with, so nothing reads a clock.
const NOW: &str = "2026-10-20T12:00:00Z";

/// The synthetic diagnosis's code, in a code system under `example.org`.
const DIAGNOSIS: &str = "SYN-DIAGNOSIS-0001";

/// The synthetic procedure's code, in a code system under `example.org`.
const PROCEDURE: &str = "SYN-PROCEDURE-0001";

/// The synthetic medicine's code, in a code system under `example.org`.
const MEDICINE: &str = "SYN-MEDICINE-0001";

/// The time the synthetic procedure was completed.
const PERFORMED: &str = "2026-10-14T13:00:00Z";

/// The synthetic stay's admission.
const ADMITTED: &str = "2026-10-12T09:30:00Z";

/// The synthetic stay's discharge.
const DISCHARGED: &str = "2026-10-20T11:00:00Z";

/// The CKM archetype the discharge report template specialises.
const TRANSFER_SUMMARY: &str = "openEHR-EHR-COMPOSITION.transfer_summary.v1";

/// The ADL 2 operational template of the discharge report template.
fn template() -> Result<TemplateSource, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    for (file, source) in CKM_DISCHARGE_ARCHETYPES
        .iter()
        .chain(CKM_MEDICATION_ARCHETYPES.iter())
    {
        let archetype = openehr_adl::assemble::parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    let template =
        openehr_adl::assemble::parse_artefact(FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE, Dialect::Adl2)
            .map_err(|errors| format!("the template: {errors:?}"))?;
    let mut opt = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    // NOTE: AM Release-2.3.0 AOM2 §3.6.6, an operational template is specialised; the root's
    // parent is the CKM archetype the template specialises, which the built OPT2 leaves unstated.
    opt.parent_archetype_id
        .get_or_insert_with(|| String::from(TRANSFER_SUMMARY));
    Ok(TemplateSource::Opt2 {
        template: Box::new(opt),
        resolved_id: String::from(FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE_ID),
    })
}

/// The HDR project compiled from its embedded files.
fn mapping() -> Result<Mapping, Box<dyn Error>> {
    Ok(Mapping::compile_project(&template()?, &HDR)?)
}

/// The run settings: a synthetic engine device, a fixed instant and the
/// composition defaults a received document does not state.
fn settings() -> Settings {
    Settings::new("Device/ferrobridge-eehrxf-test", NOW)
        .with_defaults(Defaults::at(NOW).with_language("en").with_territory("NL"))
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

/// An element `at` named `name` holding `value`.
fn element(at: &str, name: &str, value: &str) -> String {
    format!(
        r#"{{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "{name}" }}, "archetype_node_id": "{at}", "value": {value} }}"#
    )
}

/// A date and time.
fn instant(value: &str) -> String {
    format!(r#"{{ "_type": "DV_DATE_TIME", "value": "{value}" }}"#)
}

/// The synthetic diagnosis, recognised during the stay.
fn diagnosis() -> String {
    let name = format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "Synthetic diagnosis", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "http://example.org/fhir/CodeSystem/ferrobridge-diagnoses" }}, "code_string": "{DIAGNOSIS}" }} }}"#
    );
    let items = [
        element("at0002", "Problem/Diagnosis name", &name),
        element(
            "at0012",
            "Body site",
            r#"{ "_type": "DV_TEXT", "value": "Synthetic body site" }"#,
        ),
        element(
            "at0077",
            "Date/time of onset",
            &instant("2026-10-11T22:00:00Z"),
        ),
        element(
            "at0003",
            "Date/time clinically recognised",
            &instant("2026-10-12T10:15:00Z"),
        ),
        element(
            "at0069",
            "Comment",
            r#"{ "_type": "DV_TEXT", "value": "A synthetic comment." }"#,
        ),
    ];
    format!(
        r#"{{
  "_type": "EVALUATION",
  "name": {{ "_type": "DV_TEXT", "value": "Problem/Diagnosis" }},
  {details},
  {ENTRY},
  "data": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "structure" }}, "archetype_node_id": "at0001", "items": [{items}] }}
}}"#,
        details = details("openEHR-EHR-EVALUATION.problem_diagnosis.v1"),
        items = items.join(","),
    )
}

/// A coded text of the synthetic code system `system` under `example.org`.
fn synthetic(system: &str, value: &str, code: &str) -> String {
    format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "{value}", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "http://example.org/fhir/CodeSystem/{system}" }}, "code_string": "{code}" }} }}"#
    )
}

/// A synthetic procedure completed during the stay.
fn procedure() -> String {
    let items = [
        element(
            "at0002",
            "Procedure name",
            &synthetic("ferrobridge-procedures", "Synthetic procedure", PROCEDURE),
        ),
        element(
            "at0063",
            "Body site",
            r#"{ "_type": "DV_TEXT", "value": "Synthetic body site" }"#,
        ),
        element(
            "at0005",
            "Comment",
            r#"{ "_type": "DV_TEXT", "value": "A synthetic procedure comment." }"#,
        ),
    ];
    format!(
        r#"{{
  "_type": "ACTION",
  "name": {{ "_type": "DV_TEXT", "value": "Procedure" }},
  {details},
  {ENTRY},
  "time": {time},
  "ism_transition": {{
    "_type": "ISM_TRANSITION",
    "current_state": {{ "_type": "DV_CODED_TEXT", "value": "completed", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "532" }} }},
    "careflow_step": {{ "_type": "DV_CODED_TEXT", "value": "Procedure completed", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "local" }}, "code_string": "at0043" }} }}
  }},
  "description": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "Tree" }}, "archetype_node_id": "at0001", "items": [{items}] }}
}}"#,
        details = details("openEHR-EHR-ACTION.procedure.v1"),
        time = instant(PERFORMED),
        items = items.join(","),
    )
}

/// A synthetic medication order for after discharge.
fn order() -> String {
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
      "items": [{medicine}, {directions}, {comment}]
    }},
    "timing": {{ "_type": "DV_PARSABLE", "value": "R/2026-10-20T08:00:00Z/P1D", "formalism": "ISO8601" }},
    "action_archetype_id": "openEHR-EHR-ACTION\\.medication\\.v1"
  }}],
  "protocol": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "Tree" }}, "archetype_node_id": "at0005", "items": [] }}
}}"#,
        details = details("openEHR-EHR-INSTRUCTION.medication_order.v3"),
        medicine = element(
            "at0070",
            "Medication item",
            &synthetic("ferrobridge-products", "Synthetic tablet 500 mg", MEDICINE),
        ),
        directions = element(
            "at0009",
            "Overall directions description",
            r#"{ "_type": "DV_TEXT", "value": "One synthetic tablet each morning" }"#,
        ),
        comment = element(
            "at0167",
            "Comment",
            r#"{ "_type": "DV_TEXT", "value": "A synthetic order comment." }"#,
        ),
    )
}

/// A synthetic discharge report whose three sections each hold one entry.
fn full_report() -> String {
    composition_of(&[
        ("Diagnostic summary", &[diagnosis()]),
        ("Significant procedures", &[procedure()]),
        ("Discharge medications", &[order()]),
    ])
}

/// A synthetic discharge report of a stay from [`ADMITTED`] to
/// [`DISCHARGED`], whose diagnostic summary holds `diagnoses`.
fn composition(diagnoses: &[String]) -> String {
    composition_of(&[("Diagnostic summary", diagnoses)])
}

/// A synthetic discharge report of a stay from [`ADMITTED`] to
/// [`DISCHARGED`] holding each `(name, items)` section of the template.
fn composition_of(sections: &[(&str, &[String])]) -> String {
    let content: Vec<String> = sections
        .iter()
        .map(|(name, items)| {
            format!(
                r#"{{ "_type": "SECTION", "name": {{ "_type": "DV_TEXT", "value": "{name}" }}, {section}, "items": [{items}] }}"#,
                section = details("openEHR-EHR-SECTION.adhoc.v1"),
                items = items.join(","),
            )
        })
        .collect();
    format!(
        r#"{{
  "_type": "COMPOSITION",
  "name": {{ "_type": "DV_TEXT", "value": "Discharge report" }},
  "archetype_node_id": "openEHR-EHR-COMPOSITION.transfer_summary.v1",
  "archetype_details": {{
    "_type": "ARCHETYPED",
    "archetype_id": {{ "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.transfer_summary.v1" }},
    "template_id": {{ "_type": "TEMPLATE_ID", "value": "{FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE_ID}" }},
    "rm_version": "1.1.0"
  }},
  "uid": {{ "_type": "OBJECT_VERSION_ID", "value": "5b0c3f7e-0000-4000-8000-00000000d101::ferrobridge.example::1" }},
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
    "start_time": {start},
    "end_time": {end},
    "setting": {{ "_type": "DV_CODED_TEXT", "value": "other care", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "238" }} }}
  }},
  "content": [{content}]
}}"#,
        start = instant(ADMITTED),
        end = instant(DISCHARGED),
        content = content.join(","),
    )
}

/// The literal reference `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}

/// The resources of `bundle` of the type `kind`, as JSON.
fn of_type(bundle: &Bundle, kind: &str) -> Result<Vec<Value>, Box<dyn Error>> {
    let mut found = Vec::new();
    for resource in bundle
        .entry
        .iter()
        .filter_map(|entry| entry.resource.as_ref())
    {
        let value = serde_json::to_value(resource)?;
        if value["resourceType"] == kind {
            found.push(value);
        }
    }
    Ok(found)
}

/// The first resource of `bundle` of the type `kind`, as the codec reads it.
fn object(bundle: &Bundle, kind: &str) -> Result<Object, Box<dyn Error>> {
    for resource in bundle
        .entry
        .iter()
        .filter_map(|entry| entry.resource.as_ref())
    {
        let object = Json::to_json(resource)?;
        if object.get("resourceType") == Some(&fhir_types::codec::Value::String(kind.to_owned())) {
            return Ok(object);
        }
    }
    Err(format!("the run holds no {kind}").into())
}

/// The one resource of `bundle` of the type `kind`.
fn only(bundle: &Bundle, kind: &str) -> Result<Value, Box<dyn Error>> {
    let mut found = of_type(bundle, kind)?;
    match (found.pop(), found.is_empty()) {
        (Some(one), true) => Ok(one),
        _ => Err(format!("the run holds not one {kind}").into()),
    }
}

/// A discharge report document, named `identifier`, of the synthetic
/// patient, by the synthetic gateway and operator, answering the document
/// and the references of its subject and its gateway device.
fn discharge_document(identifier: &str) -> Result<(Document, String, String), Box<dyn Error>> {
    let mut document = Document::of(
        &DISCHARGE_REPORT_FORM,
        BASE,
        identifier.to_owned(),
        String::from(NOW),
        String::from("Discharge report"),
    );
    let subject = document.with_patient(Patient {
        id: Some(String::from("synthetic-patient")),
        name: vec![HumanName {
            family: Some("Synthetic".into()),
            ..HumanName::default()
        }],
        birth_date: Some("1970-01-01".into()),
        ..Patient::default()
    })?;
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
    document.with_language(String::from("en"));
    Ok((document, subject, device))
}

/// The document of the run of `mapping` over `composition`, named
/// `identifier`, in the text it is sent as.
fn sent(mapping: &Mapping, composition: &str, identifier: &str) -> Result<String, Box<dyn Error>> {
    let (mut document, subject, device) = discharge_document(identifier)?;
    let run = mapping.to_fhir_with(
        composition,
        &Settings::new(device.clone(), NOW),
        CallContext::new()
            .with_patient(reference(&subject))
            .with_who(reference(&device)),
    )?;
    // NOTE: composition-eu-hdr cmp-hdr-2 requires the discharge summary or the hospital course; the
    // crosswalk has the document assembly write the course's narrative, which the system supplies.
    let course = HDR_SLOTS
        .iter()
        .find(|slot| slot.slice == "sectionHospitalCourse")
        .ok_or("the hospital course slot")?;
    document.with_section(Section::new(
        course,
        vec![String::from("A synthetic hospital course.")],
    ))?;
    for section in document.with_mapped_composition(run.bundle())? {
        let paragraph = format!("{}.", section.slot.title);
        document.with_section(section.into_section(vec![paragraph]))?;
    }
    Ok(serde_json::to_string(&document.into_bundle()?)?)
}

/// The EU core profile of a significant procedures entry.
const PROCEDURE_EU_CORE: &str = "http://hl7.eu/fhir/base/StructureDefinition/procedure-eu-core";

/// The EU core profile of a discharge medications entry.
const MEDICATION_REQUEST_EU_CORE: &str =
    "http://hl7.eu/fhir/base/StructureDefinition/medicationRequest-eu-core";

/// The `entry` references of the section of `composition` coded `code`.
fn section_entries(composition: &Value, code: &str) -> Vec<String> {
    composition["section"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|section| section["code"]["coding"][0]["code"] == code)
        .flat_map(|section| section["entry"].as_array().cloned().unwrap_or_default())
        .filter_map(|entry| entry["reference"].as_str().map(ToOwned::to_owned))
        .collect()
}

mod composition;
mod document;
mod received;
