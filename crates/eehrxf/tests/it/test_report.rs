// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The non-laboratory medical test reports of draft `EEHRxF` Annex VIII
//! mapped from openEHR through the shipped medical test report project, over
//! FerroBRIDGE's template of published CKM archetypes: a spirometry result and
//! an electrocardiogram result each become an R4 `DiagnosticReport` that
//! claims no profile, written into a document with its Annex III metadata.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::category::{Category, PRIORITY_SYSTEM};
use eehrxf::crosswalk::test_report::{NON_LABORATORY_PROFILE, TEST_REPORT_ROWS};
use eehrxf::document::Document;
use eehrxf::document::metadata::DocumentMetadata;
use eehrxf::document::section::Section;
use eehrxf::document::test_report::{
    ELECTROCARDIOGRAM, SPIROMETRY, TEST_REPORT_BUNDLE, TEST_REPORT_FORM, TEST_REPORT_SLOTS,
};
use eehrxf::draft::{Annex, AnnexCrosswalk, SHIPPED};
use eehrxf::mapping::Mapping;
use eehrxf::mappings::TEST_REPORT;
use ferrobridge_testkit::fixtures::{
    CKM_LIBRARY_ARCHETYPES, CKM_TEST_REPORT_ARCHETYPES, FERROBRIDGE_TEST_REPORT_TEMPLATE,
    FERROBRIDGE_TEST_REPORT_TEMPLATE_ID,
};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::run::Settings;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::template::TemplateSource;
use serde_json::Value;

use super::support::emit;

/// The base the documents name their entries under, in the `example.org`
/// domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The instant every run is stamped with, so nothing reads a clock.
const NOW: &str = "2026-11-20T10:00:00Z";

/// The ADL 2 operational template of the medical test report template.
fn template() -> Result<TemplateSource, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    for (file, source) in CKM_LIBRARY_ARCHETYPES
        .iter()
        .chain(CKM_TEST_REPORT_ARCHETYPES.iter())
    {
        let archetype = openehr_adl::assemble::parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    let template =
        openehr_adl::assemble::parse_artefact(FERROBRIDGE_TEST_REPORT_TEMPLATE, Dialect::Adl2)
            .map_err(|errors| format!("the template: {errors:?}"))?;
    let raw = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
        resolved_id: String::from(FERROBRIDGE_TEST_REPORT_TEMPLATE_ID),
    })
}

/// The spirometry context of the shipped project.
const SPIROMETRY_CONTEXT: &str = "ferrobridge_spirometry_report.context";

/// The electrocardiogram context of the shipped project.
const ECG_CONTEXT: &str = "ferrobridge_ecg_report.context";

/// The shipped project narrowed to its context `context`, compiled from its
/// embedded files against the medical test report template; both contexts
/// map the one template, so a run selects one.
fn mapping(context: &str) -> Result<Mapping, Box<dyn Error>> {
    Ok(Mapping::compile_project(
        &template()?,
        &TEST_REPORT.with_context(context)?,
    )?)
}

/// The report composition holding `observation`.
fn composition(observation: &str) -> String {
    format!(
        r#"{{
  "_type": "COMPOSITION",
  "name": {{ "_type": "DV_TEXT", "value": "Medical test report" }},
  "archetype_node_id": "openEHR-EHR-COMPOSITION.report-result.v1",
  "archetype_details": {{
    "_type": "ARCHETYPED",
    "archetype_id": {{ "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.report-result.v1" }},
    "template_id": {{ "_type": "TEMPLATE_ID", "value": "{FERROBRIDGE_TEST_REPORT_TEMPLATE_ID}" }},
    "rm_version": "1.1.0"
  }},
  "uid": {{ "_type": "OBJECT_VERSION_ID", "value": "5b0c3f7e-0000-4000-8000-00000000f101::ferrobridge.example::1" }},
  "language": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }}, "code_string": "en" }},
  "territory": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "ISO_3166-1" }}, "code_string": "NL" }},
  "category": {{ "_type": "DV_CODED_TEXT", "value": "event", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "433" }} }},
  "composer": {{ "_type": "PARTY_IDENTIFIED", "name": "Synthetic composer" }},
  "context": {{
    "_type": "EVENT_CONTEXT",
    "start_time": {{ "_type": "DV_DATE_TIME", "value": "2026-11-20T08:00:00Z" }},
    "setting": {{ "_type": "DV_CODED_TEXT", "value": "other care", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "238" }} }}
  }},
  "content": [{observation}]
}}"#
    )
}

/// The observation `archetype` whose one event at `time` holds `items` in
/// its tree `tree`.
fn observation(archetype: &str, name: &str, tree: &str, items: &str) -> String {
    format!(
        r#"{{
  "_type": "OBSERVATION",
  "name": {{ "_type": "DV_TEXT", "value": "{name}" }},
  "archetype_node_id": "{archetype}",
  "archetype_details": {{ "_type": "ARCHETYPED", "archetype_id": {{ "_type": "ARCHETYPE_ID", "value": "{archetype}" }}, "rm_version": "1.1.0" }},
  "language": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }}, "code_string": "en" }},
  "encoding": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "IANA_character-sets" }}, "code_string": "UTF-8" }},
  "subject": {{ "_type": "PARTY_SELF" }},
  "data": {{
    "_type": "HISTORY",
    "name": {{ "_type": "DV_TEXT", "value": "Event Series" }},
    "archetype_node_id": "at0001",
    "origin": {{ "_type": "DV_DATE_TIME", "value": "2026-11-20T08:30:00Z" }},
    "events": [{{
      "_type": "POINT_EVENT",
      "name": {{ "_type": "DV_TEXT", "value": "Any event" }},
      "archetype_node_id": "at0002",
      "time": {{ "_type": "DV_DATE_TIME", "value": "2026-11-20T08:30:00Z" }},
      "data": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "Tree" }}, "archetype_node_id": "{tree}", "items": [{items}] }}
    }}]
  }}
}}"#
    )
}

/// A text element `node` named `name`.
fn text(node: &str, name: &str, value: &str) -> String {
    format!(
        r#"{{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "{name}" }}, "archetype_node_id": "{node}", "value": {{ "_type": "DV_TEXT", "value": "{value}" }} }}"#
    )
}

/// A spirometry result with its clinical interpretation.
fn spirometry() -> String {
    composition(&observation(
        "openEHR-EHR-OBSERVATION.spirometry_result.v2",
        "Spirometry result",
        "at0003",
        &text(
            "at0130",
            "Clinical interpretation",
            "Synthetic normal spirometry.",
        ),
    ))
}

/// An electrocardiogram result with its conclusion and one coded diagnosis.
fn electrocardiogram() -> String {
    let diagnosis = r#"{ "_type": "ELEMENT", "name": { "_type": "DV_TEXT", "value": "ECG diagnosis" }, "archetype_node_id": "at0081", "value": { "_type": "DV_CODED_TEXT", "value": "Sinus rhythm", "defining_code": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "http://snomed.info/sct" }, "code_string": "64730000" } } }"#;
    composition(&observation(
        "openEHR-EHR-OBSERVATION.ecg_result.v1",
        "ECG result",
        "at0005",
        &[
            diagnosis.to_owned(),
            text("at0089", "Conclusion", "Synthetic normal ECG."),
        ]
        .join(","),
    ))
}

/// The literal reference `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}

/// Maps `composition` and writes the report into a document with its
/// Annex III metadata, answering the document as JSON.
fn document_of(
    context: &str,
    composition: &str,
    identifier: &str,
) -> Result<Value, Box<dyn Error>> {
    let mapping = mapping(context)?;
    let mut document = Document::of(
        &TEST_REPORT_FORM,
        BASE,
        identifier.to_owned(),
        String::from(NOW),
        String::from("Medical test report"),
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
    let context = CallContext::new()
        .with_patient(reference(&subject))
        .with_who(reference(&device));
    let run = mapping.to_fhir_with(composition, &Settings::new(device.clone(), NOW), context)?;
    document.with_mapped_report(run.bundle())?;
    let slot = TEST_REPORT_SLOTS.first().ok_or("the results slot")?;
    document.with_section(Section::new(slot, vec![String::from("The test result.")]))?;
    let source: Value = serde_json::from_str(composition)?;
    let category = CodeableConcept {
        coding: vec![Coding {
            system: Some(PRIORITY_SYSTEM.into()),
            code: Some(Category::Laboratory.code().into()),
            ..Coding::default()
        }],
        ..CodeableConcept::default()
    };
    document.with_metadata(DocumentMetadata::from_composition(&source, category)?);
    let bundle = document.into_bundle()?;
    Ok(serde_json::to_value(bundle)?)
}

/// The one resource of type `kind` in `bundle`.
fn only<'a>(bundle: &'a Value, kind: &str) -> Result<&'a Value, Box<dyn Error>> {
    let found: Vec<&Value> = bundle["entry"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == kind)
        .collect();
    match found.as_slice() {
        [one] => Ok(one),
        other => Err(format!("{} {kind} entries, not one", other.len()).into()),
    }
}

#[test]
fn the_shipped_project_compiles_against_the_test_report_template() -> Result<(), Box<dyn Error>> {
    assert_eq!(TEST_REPORT.contexts(), &[SPIROMETRY_CONTEXT, ECG_CONTEXT]);
    for context in [SPIROMETRY_CONTEXT, ECG_CONTEXT] {
        let mapping = mapping(context)?;
        let contexts: Vec<_> = mapping
            .contexts()
            .map(|compiled| (compiled.name, compiled.template, compiled.profile))
            .collect();
        assert_eq!(
            contexts,
            [(
                context,
                FERROBRIDGE_TEST_REPORT_TEMPLATE_ID,
                "http://hl7.org/fhir/StructureDefinition/DiagnosticReport"
            )]
        );
    }
    Ok(())
}

#[test]
fn a_spirometry_result_becomes_a_spirometry_report() -> Result<(), Box<dyn Error>> {
    let bundle = document_of(
        SPIROMETRY_CONTEXT,
        &spirometry(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000f001",
    )?;
    assert_eq!(bundle["meta"]["profile"][0], TEST_REPORT_BUNDLE);
    let report = only(&bundle, "DiagnosticReport")?;
    assert_eq!(report["status"], "final");
    assert_eq!(report["code"]["coding"][0]["code"], SPIROMETRY);
    assert_eq!(report["effectiveDateTime"], "2026-11-20T08:30:00Z");
    assert_eq!(report["conclusion"], "Synthetic normal spirometry.");
    let composition = only(&bundle, "Composition")?;
    assert_eq!(composition["type"]["coding"][0]["code"], SPIROMETRY);
    emit("eehrxf-test-report-spirometry", &bundle.to_string())?;
    Ok(())
}

#[test]
fn an_electrocardiogram_result_becomes_an_ecg_report() -> Result<(), Box<dyn Error>> {
    let bundle = document_of(
        ECG_CONTEXT,
        &electrocardiogram(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000f002",
    )?;
    let report = only(&bundle, "DiagnosticReport")?;
    assert_eq!(report["code"]["coding"][0]["code"], ELECTROCARDIOGRAM);
    assert_eq!(report["conclusion"], "Synthetic normal ECG.");
    assert_eq!(report["conclusionCode"][0]["coding"][0]["code"], "64730000");
    let composition = only(&bundle, "Composition")?;
    assert_eq!(composition["type"]["coding"][0]["code"], ELECTROCARDIOGRAM);
    emit("eehrxf-test-report-ecg", &bundle.to_string())?;
    Ok(())
}

#[test]
fn the_report_carries_the_annex_iii_document_metadata() -> Result<(), Box<dyn Error>> {
    let bundle = document_of(
        SPIROMETRY_CONTEXT,
        &spirometry(),
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000f003",
    )?;
    let composition = only(&bundle, "Composition")?;
    assert_eq!(composition["status"], "final");
    assert_eq!(composition["language"], "en");
    assert!(composition["identifier"]["value"].is_string());
    assert!(composition["date"].is_string());
    assert!(
        composition["author"]
            .as_array()
            .is_some_and(|authors| !authors.is_empty())
    );
    Ok(())
}

#[test]
fn every_annex_viii_element_has_one_non_laboratory_row() -> Result<(), Box<dyn Error>> {
    assert_eq!(NON_LABORATORY_PROFILE, None, "no HL7 Europe profile");
    let crosswalk: AnnexCrosswalk = SHIPPED.parse()?;
    let annex: Vec<&str> = crosswalk
        .annex(Annex::Viii)
        .filter_map(|row| row.draft_element())
        .collect();
    let rows: Vec<&str> = TEST_REPORT_ROWS.iter().map(|row| row.element).collect();
    assert_eq!(
        rows, annex,
        "one row per Annex VIII element, in the annex's order"
    );
    let specimen = TEST_REPORT_ROWS
        .iter()
        .find(|row| row.element == "body.specimen")
        .ok_or("the specimen row")?;
    assert!(!specimen.applies, "a specimen is laboratory alone");
    Ok(())
}
