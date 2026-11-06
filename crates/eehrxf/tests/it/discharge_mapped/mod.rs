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

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::category::Category;
use eehrxf::crosswalk::discharge::HDR_COMPOSITION;
use eehrxf::document::discharge::{DISCHARGE_REPORT_FORM, HDR_BUNDLE, HDR_ENCOUNTER};
use eehrxf::document::{Document, DocumentError};
use eehrxf::mapping::Mapping;
use eehrxf::mappings::HDR;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::conform::check_resource;
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
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::operations::run::Settings;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::template::TemplateSource;
use serde_json::Value;

use super::support::{emit, hdr, vendored_base};

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
    for section in document.with_mapped_composition(run.bundle())? {
        let paragraph = format!("{}.", section.slot.title);
        document.with_section(section.into_section(vec![paragraph]))?;
    }
    Ok(serde_json::to_string(&document.into_bundle()?)?)
}

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
fn the_mapped_report_is_a_discharge_document_its_profiles_constrain() -> Result<(), Box<dyn Error>>
{
    let text = sent(
        &mapping()?,
        &composition(&[diagnosis()]),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d102",
    )?;
    let document = ReceivedDocument::read(&text)?;
    assert_eq!(document.category()?, Category::Discharge);
    document.check(&hdr(HDR_BUNDLE)?, &hdr(HDR_COMPOSITION)?)?;
    let root: Value = serde_json::from_str(&text)?;
    let composition = &root["entry"][0]["resource"];
    let encounter = composition["encounter"]["reference"]
        .as_str()
        .ok_or("the composition names its encounter")?;
    let section = composition["section"]
        .as_array()
        .and_then(|sections| {
            sections
                .iter()
                .find(|section| section["code"]["coding"][0]["code"] == "11535-2")
        })
        .ok_or("the diagnostic summary section")?;
    let entry = section["entry"][0]["reference"]
        .as_str()
        .ok_or("the section's entry")?;
    let entries = root["entry"].as_array().ok_or("the entries")?;
    let resolve = |reference: &str| {
        entries.iter().find(|held| {
            held["fullUrl"]
                .as_str()
                .is_some_and(|url| url.ends_with(&format!("/{reference}")) || url == reference)
        })
    };
    let condition = resolve(entry).ok_or("the entry resolves")?;
    assert_eq!(
        condition["resource"]["code"]["coding"][0]["code"],
        DIAGNOSIS
    );
    let stay = resolve(encounter).ok_or("the encounter resolves")?;
    assert_eq!(stay["resource"]["meta"]["profile"][0], HDR_ENCOUNTER);
    assert_eq!(stay["resource"]["period"]["start"], ADMITTED);
    emit("eehrxf-discharge-mapped", &text)?;
    Ok(())
}

#[test]
fn a_received_discharge_report_maps_into_one_composition_that_keeps_it()
-> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    let text = sent(
        &mapping,
        &composition(&[diagnosis()]),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d103",
    )?;
    let received = mapping.to_openehr(&ReceivedDocument::read(&text)?, &settings())?;
    assert_eq!(
        received.template_id(),
        FERROBRIDGE_DISCHARGE_REPORT_TEMPLATE_ID
    );
    let outcome = serde_json::to_value(received.outcome().ok_or("the run declares its losses")?)?;
    let declared: Vec<&str> = outcome["issue"]
        .as_array()
        .ok_or("the issues")?
        .iter()
        .filter_map(|issue| issue["details"]["text"].as_str())
        .collect();
    assert_eq!(
        declared,
        [
            "the mapping metaURL did not run: its unidirectional marker names the other direction",
            "the mapping status did not run: its unidirectional marker names the other direction",
            "the mapping date did not run: its unidirectional marker names the other direction",
            "the mapping author did not run: its unidirectional marker names the other direction",
            "the mapping type did not run: its unidirectional marker names the other direction",
            "the mapping title did not run: its unidirectional marker names the other direction",
            "the mapping encounter.encounterProfile did not run: its unidirectional marker names the other direction",
            "the mapping encounter.finished did not run: its unidirectional marker names the other direction",
            "the mapping encounter.inpatient did not run: its unidirectional marker names the other direction",
            "the mapping diagnosticSummary.title did not run: its unidirectional marker names the other direction",
            "the mapping diagnosticSummary.code did not run: its unidirectional marker names the other direction",
            "the mapping metaURL did not run: its unidirectional marker names the other direction",
            "the engine filled /composer",
            "the engine filled /context/setting",
            "the engine filled /language",
            "the engine filled /territory",
        ],
        "every element the composition does not carry is declared, and nothing else"
    );
    let canonical: Value = serde_json::from_str(received.composition())?;
    assert_eq!(
        canonical
            .pointer("/feeder_audit/original_content/value")
            .and_then(Value::as_str),
        Some(text.as_str()),
        "the document is kept byte for byte"
    );
    assert_eq!(
        canonical.pointer("/context/start_time/value"),
        Some(&Value::from(ADMITTED))
    );
    assert_eq!(
        canonical.pointer("/context/end_time/value"),
        Some(&Value::from(DISCHARGED))
    );
    let written = canonical.to_string();
    for value in [DIAGNOSIS, "2026-10-11T22:00:00Z", "2026-10-12T10:15:00Z"] {
        assert!(written.contains(value), "{value} is mapped: {written}");
    }
    Ok(())
}

#[test]
fn the_profile_elements_of_the_discharge_report_survive_the_round_trip()
-> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    let first = sent(
        &mapping,
        &composition(&[diagnosis()]),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d104",
    )?;
    let received = mapping.to_openehr(&ReceivedDocument::read(&first)?, &settings())?;
    let mut stored: Value = serde_json::from_str(received.composition())?;
    let object = stored
        .as_object_mut()
        .ok_or("the composition is an object")?;
    object.insert(
        String::from("uid"),
        serde_json::json!({
            "_type": "OBJECT_VERSION_ID",
            "value": "5b0c3f7e-0000-4000-8000-00000000d105::ferrobridge.example::1"
        }),
    );
    let second = sent(
        &mapping,
        &stored.to_string(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d106",
    )?;
    let document = ReceivedDocument::read(&second)?;
    document.check(&hdr(HDR_BUNDLE)?, &hdr(HDR_COMPOSITION)?)?;
    let (before, after) = (
        only(document_bundle(&first)?.as_ref(), "Condition")?,
        only(document.bundle(), "Condition")?,
    );
    for pointer in [
        "/meta/profile",
        "/code",
        "/onsetDateTime",
        "/extension",
        "/bodySite",
        "/note",
    ] {
        assert_eq!(before.pointer(pointer), after.pointer(pointer), "{pointer}");
    }
    let (before, after) = (
        only(document_bundle(&first)?.as_ref(), "Encounter")?,
        only(document.bundle(), "Encounter")?,
    );
    for pointer in ["/meta/profile", "/status", "/class", "/period"] {
        assert_eq!(before.pointer(pointer), after.pointer(pointer), "{pointer}");
    }
    Ok(())
}

/// The decoded document `Bundle` of the text `text`.
fn document_bundle(text: &str) -> Result<Box<Bundle>, Box<dyn Error>> {
    Ok(Box::new(ReceivedDocument::read(text)?.bundle().clone()))
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
fn bundle_of(resources: &[Value]) -> Result<Bundle, Box<dyn Error>> {
    let entries: Vec<Value> = resources
        .iter()
        .map(|resource| serde_json::json!({ "resource": resource }))
        .collect();
    Ok(serde_json::from_value(serde_json::json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": entries
    }))?)
}

#[test]
fn a_run_without_a_composition_is_refused() -> Result<(), Box<dyn Error>> {
    let (mut document, _, _) = discharge_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d108")?;
    let run = bundle_of(&[serde_json::json!({
        "resourceType": "Condition",
        "id": "synthetic-condition",
        "code": { "text": "Synthetic diagnosis" },
        "subject": { "reference": "Patient/synthetic-patient" }
    })])?;
    assert!(matches!(
        document.with_mapped_composition(&run),
        Err(DocumentError::Uncomposed { found: 0 })
    ));
    Ok(())
}

#[test]
fn a_section_whose_code_names_no_hdr_slot_is_refused() -> Result<(), Box<dyn Error>> {
    let (mut document, _, _) = discharge_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d109")?;
    let run = bundle_of(&[serde_json::json!({
        "resourceType": "Composition",
        "status": "final",
        "type": { "coding": [{ "system": "http://loinc.org", "code": "34105-7" }] },
        "date": NOW,
        "author": [{ "display": "Synthetic author" }],
        "title": "Discharge report",
        "section": [{
            "title": "Medication summary",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "10160-0" }] }
        }]
    })])?;
    match document.with_mapped_composition(&run) {
        Err(DocumentError::Unslotted { codes }) => {
            assert_eq!(codes, ["10160-0"]);
            Ok(())
        }
        other => Err(format!("the foreign section was placed: {other:?}").into()),
    }
}

#[test]
fn an_entry_the_run_does_not_hold_is_refused() -> Result<(), Box<dyn Error>> {
    let (mut document, _, _) = discharge_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d110")?;
    let run = bundle_of(&[serde_json::json!({
        "resourceType": "Composition",
        "status": "final",
        "type": { "coding": [{ "system": "http://loinc.org", "code": "34105-7" }] },
        "date": NOW,
        "author": [{ "display": "Synthetic author" }],
        "title": "Discharge report",
        "section": [{
            "title": "Diagnostic summary",
            "code": { "coding": [{ "system": "http://loinc.org", "code": "11535-2" }] },
            "entry": [{ "reference": "Condition/absent" }]
        }]
    })])?;
    assert!(matches!(
        document.with_mapped_composition(&run),
        Err(DocumentError::Dangling {
            slice: "sectionDiagnosticSummary"
        })
    ));
    Ok(())
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
    assert!(
        of_type(bundle, "OperationOutcome")?.is_empty(),
        "the run declares no loss"
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

#[test]
fn every_section_of_the_report_survives_the_round_trip() -> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    let first = sent(
        &mapping,
        &full_report(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d111",
    )?;
    let received = ReceivedDocument::read(&first)?;
    received.check(&hdr(HDR_BUNDLE)?, &hdr(HDR_COMPOSITION)?)?;
    let stored = mapping.to_openehr(&received, &settings())?;
    let mut canonical: Value = serde_json::from_str(stored.composition())?;
    canonical
        .as_object_mut()
        .ok_or("the composition is an object")?
        .insert(
            String::from("uid"),
            serde_json::json!({
                "_type": "OBJECT_VERSION_ID",
                "value": "5b0c3f7e-0000-4000-8000-00000000d112::ferrobridge.example::1"
            }),
        );
    let second = sent(
        &mapping,
        &canonical.to_string(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d113",
    )?;
    let document = ReceivedDocument::read(&second)?;
    document.check(&hdr(HDR_BUNDLE)?, &hdr(HDR_COMPOSITION)?)?;
    for (kind, pointers) in [
        (
            "Procedure",
            &[
                "/meta/profile",
                "/status",
                "/code",
                "/performedDateTime",
                "/bodySite",
                "/note",
            ][..],
        ),
        (
            "MedicationRequest",
            &[
                "/meta/profile",
                "/status",
                "/intent",
                "/medicationCodeableConcept",
                "/dosageInstruction",
                "/note",
            ][..],
        ),
        (
            "Condition",
            &["/meta/profile", "/code", "/onsetDateTime"][..],
        ),
    ] {
        let (before, after) = (
            only(received.bundle(), kind)?,
            only(document.bundle(), kind)?,
        );
        for pointer in pointers {
            assert_eq!(
                before.pointer(pointer),
                after.pointer(pointer),
                "{kind}{pointer}"
            );
        }
    }
    let root: Value = serde_json::from_str(&second)?;
    let composition = &root["entry"][0]["resource"];
    for code in ["11535-2", "10185-7", "75311-1"] {
        assert_eq!(section_entries(composition, code).len(), 1, "{code}");
    }
    emit("eehrxf-discharge-full", &second)?;
    Ok(())
}

#[test]
fn every_entry_of_the_mapped_report_meets_the_profile_it_claims() -> Result<(), Box<dyn Error>> {
    let text = sent(
        &mapping()?,
        &full_report(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000d114",
    )?;
    let document = ReceivedDocument::read(&text)?;
    let mut checked = Vec::new();
    for resource in document
        .bundle()
        .entry
        .iter()
        .filter_map(|entry| entry.resource.as_ref())
    {
        let object = Json::to_json(resource)?;
        let (
            Some(fhir_types::codec::Value::String(kind)),
            Some(fhir_types::codec::Value::Object(meta)),
        ) = (object.get("resourceType"), object.get("meta"))
        else {
            continue;
        };
        let Some(fhir_types::codec::Value::Array(profiles)) = meta.get("profile") else {
            continue;
        };
        for profile in profiles {
            let fhir_types::codec::Value::String(url) = profile else {
                continue;
            };
            let read = if url.starts_with("http://hl7.eu/fhir/hdr/") {
                hdr(url)?
            } else {
                vendored_base(url)?
            };
            let expected: &'static str = match kind.as_str() {
                "Composition" => "Composition",
                "Encounter" => "Encounter",
                "Condition" => "Condition",
                "Procedure" => "Procedure",
                "MedicationRequest" => "MedicationRequest",
                "Patient" => "Patient",
                other => return Err(format!("an unexpected {other} claims {url}").into()),
            };
            check_resource(&read, expected, &object)
                .map_err(|error| format!("{kind} against {url}: {error:?}"))?;
            checked.push(kind.clone());
        }
    }
    checked.sort();
    assert_eq!(
        checked,
        [
            "Composition",
            "Condition",
            "Encounter",
            "MedicationRequest",
            "Patient",
            "Procedure"
        ],
        "every claimed profile of every entry is checked"
    );
    Ok(())
}
