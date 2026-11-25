// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The MyHealth@EU vaccination history (`myhealth.eu.fhir.euvac`
//! 10.0.0-preview), mapped from the immunisations of FerroBRIDGE's patient
//! summary template through the component's `euvac` project and written as a
//! `bundle-euvac-myhealtheu`, held to the vendored profiles and emitted for
//! the HL7 FHIR Validator lane.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::crosswalk::vaccination::{EUVAC_IMMUNIZATION, VACCINATION};
use eehrxf::dataset::ResourceProfile;
use eehrxf::document::metadata::Attestation;
use eehrxf::document::section::Section;
use eehrxf::document::vaccination::{
    EUVAC_BUNDLE, EUVAC_COMPOSITION, EUVAC_SLOTS, VACCINATION_HISTORY_FORM,
};
use eehrxf::document::{Document, DocumentError};
use eehrxf::mapping::Mapping;
use eehrxf::mappings::EUVAC;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::conform::check_resource;
use fhir_types::codec::Json;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::practitioner::Practitioner;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::outcome::Loss;
use serde_json::Value;

use super::eps_mapped::{
    BASE, ENTRY, call, composition, details, patient, run, section, settings, template,
};
use super::support::{annex_iii, emit, xtehr};

/// The root context of the `euvac` project.
const ROOT: &str = "ferrobridge_myhealtheu_euvac.context";

/// The vaccine's SNOMED CT code, which MVC 10.1.0 `eHDSIVaccine` lists.
const VACCINE: &str = "836398006";

/// The LOINC code of the immunisations section and of the document type.
const IMMUNISATIONS: &str = "11369-6";

/// The immunisations section of the template.
const IMMUNISATION_LIST: (&str, &str) =
    ("Immunisations", "openEHR-EHR-SECTION.immunisation_list.v0");

/// The vendored `myhealth.eu.fhir.euvac` 10.0.0-preview package, as the
/// registry serves it.
fn euvac_package() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/ehdsi-euvac/myhealth.eu.fhir.euvac-10.0.0-preview.tgz")
}

/// Reads the profile at `url` from the vendored Vaccination guide package.
fn euvac(url: &str) -> Result<ResourceProfile, Box<dyn Error>> {
    Ok(ResourceProfile::read(
        std::fs::File::open(euvac_package())?,
        url,
    )?)
}

/// The `euvac` project compiled over the patient summary template.
fn history() -> Result<Mapping, Box<dyn Error>> {
    Ok(Mapping::compile_project(&template()?, &EUVAC)?)
}

/// A medication action recording a dose of the vaccine administered, its
/// sequence number `sequence` when given.
fn administered(sequence: Option<u32>) -> String {
    let sequence = sequence.map_or_else(String::new, |number| {
        format!(
            r#", {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Sequence number" }}, "archetype_node_id": "at0025", "value": {{ "_type": "DV_COUNT", "magnitude": {number} }} }}"#
        )
    });
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
      {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Medication item" }}, "archetype_node_id": "at0020", "value": {{ "_type": "DV_CODED_TEXT", "value": "Streptococcus pneumoniae antigen-containing vaccine product", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "http://snomed.info/sct" }}, "code_string": "{VACCINE}" }} }} }}{sequence}
    ]
  }}
}}"#,
        details = details("openEHR-EHR-ACTION.medication.v1"),
    )
}

/// The composition holding one administered dose of the vaccine in its
/// immunisations section.
fn vaccinated(sequence: Option<u32>) -> String {
    composition(&[section(IMMUNISATION_LIST, &[administered(sequence)])])
}

/// The FHIR JSON of the resources of `resources` of the type `name`.
fn objects(resources: &[Resource], name: &str) -> Result<Vec<Value>, Box<dyn Error>> {
    let mut found = Vec::new();
    for resource in resources {
        let object = serde_json::to_value(resource)?;
        if object["resourceType"] == name {
            found.push(object);
        }
    }
    Ok(found)
}

/// The vaccination history of `composition`, written as a document.
fn written(composition: &str) -> Result<Bundle, Box<dyn Error>> {
    let mut document = Document::of(
        &VACCINATION_HISTORY_FORM,
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-000000002168"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Vaccination history"),
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
    let authenticator = document.with_author(&Resource::Practitioner(Box::new(Practitioner {
        id: Some(String::from("authenticator")),
        name: vec![HumanName {
            text: Some("Synthetic Authenticator".into()),
            ..HumanName::default()
        }],
        ..Practitioner::default()
    })))?;
    let answer = history()?.to_fhir_with(composition, &settings(), call(&subject, &device))?;
    for mapped in document.with_mapped_composition(answer.bundle())? {
        document.with_section(mapped.into_section(vec![String::from("One synthetic vaccine.")]))?;
    }
    document.with_metadata(
        annex_iii(IMMUNISATIONS).with_legal_authentication(Attestation::new(
            authenticator,
            String::from("2026-10-06T10:00:00Z"),
        )),
    );
    Ok(document.into_bundle()?)
}

#[test]
fn the_vaccination_crosswalk_holds_against_the_pinned_packages() -> Result<(), Box<dyn Error>> {
    let defects = VACCINATION.check(
        xtehr()?.dataset(VACCINATION.root)?,
        &euvac(VACCINATION.profile)?,
    );
    assert!(defects.is_empty(), "{defects:#?}");
    Ok(())
}

#[test]
fn the_project_compiles_its_one_root_context() -> Result<(), Box<dyn Error>> {
    assert_eq!(EUVAC.contexts(), [ROOT]);
    history()?;
    Ok(())
}

#[test]
fn an_administered_dose_maps_to_a_euvac_vaccination() -> Result<(), Box<dyn Error>> {
    let resources = run(
        &history()?,
        &vaccinated(Some(2)),
        call("Patient/synthetic-subject", "Device/gateway"),
    )?;
    let immunizations = objects(&resources, "Immunization")?;
    let [immunization] = immunizations.as_slice() else {
        return Err(format!("{} immunizations", immunizations.len()).into());
    };
    assert_eq!(immunization["meta"]["profile"][0], EUVAC_IMMUNIZATION);
    assert_eq!(immunization["status"], "completed");
    assert_eq!(immunization["vaccineCode"]["coding"][0]["code"], VACCINE);
    assert_eq!(immunization["occurrenceDateTime"], "2026-09-15T09:30:00Z");
    assert_eq!(
        immunization["protocolApplied"][0]["doseNumberPositiveInt"],
        2
    );
    let locations = objects(&resources, "Location")?;
    let [location] = locations.as_slice() else {
        return Err(format!("{} locations", locations.len()).into());
    };
    assert_eq!(
        location["address"]["country"], "NL",
        "the composition's territory"
    );
    assert_eq!(
        immunization["location"]["reference"],
        format!("Location/{}", location["id"].as_str().unwrap_or_default())
    );
    let object = Json::to_json(&serde_json::from_value::<Resource>(immunization.clone())?)?;
    check_resource(&euvac(EUVAC_IMMUNIZATION)?, "Immunization", &object)?;
    Ok(())
}

#[test]
fn a_dose_with_no_sequence_number_states_its_number_absent() -> Result<(), Box<dyn Error>> {
    let (answer, warnings) = history()?.to_fhir_with_warnings(
        &vaccinated(None),
        &settings(),
        call("Patient/synthetic-subject", "Device/gateway"),
    )?;
    assert!(
        warnings.iter().any(
            |warning| matches!(warning.loss(), Loss::Absent { reason } if reason == "unknown")
        ),
        "the run declares the absent dose number: {warnings:?}"
    );
    let resources: Vec<Resource> = answer
        .bundle()
        .entry
        .iter()
        .filter_map(|entry| entry.resource.clone())
        .collect();
    let immunizations = objects(&resources, "Immunization")?;
    let [immunization] = immunizations.as_slice() else {
        return Err(format!("{} immunizations", immunizations.len()).into());
    };
    let applied = &immunization["protocolApplied"][0];
    assert!(applied.get("doseNumberPositiveInt").is_none(), "{applied}");
    assert_eq!(
        applied["_doseNumberString"]["extension"][0]["url"],
        "http://hl7.org/fhir/StructureDefinition/data-absent-reason"
    );
    assert_eq!(
        applied["_doseNumberString"]["extension"][0]["valueCode"],
        "unknown"
    );
    Ok(())
}

#[test]
fn the_vaccination_history_validates_against_the_euvac_profiles() -> Result<(), Box<dyn Error>> {
    let bundle = written(&vaccinated(Some(1)))?;
    let text = serde_json::to_string(&bundle)?;
    let received = ReceivedDocument::read(&text)?;
    received.check(&euvac(EUVAC_BUNDLE)?, &euvac(EUVAC_COMPOSITION)?)?;
    let root: Value = serde_json::from_str(&text)?;
    let composition = &root["entry"][0]["resource"];
    assert_eq!(composition["type"]["coding"][0]["code"], IMMUNISATIONS);
    assert_eq!(composition["attester"][0]["mode"], "legal");
    let sections = composition["section"].as_array().ok_or("the sections")?;
    let [immunisations] = sections.as_slice() else {
        return Err(format!("{} sections", sections.len()).into());
    };
    assert_eq!(immunisations["code"]["coding"][0]["code"], IMMUNISATIONS);
    assert_eq!(immunisations["entry"].as_array().map(Vec::len), Some(1));
    emit("eehrxf-euvac-vaccination-history", &text)?;
    Ok(())
}

#[test]
fn a_history_with_no_immunisation_is_refused() -> Result<(), Box<dyn Error>> {
    let mut document = Document::of(
        &VACCINATION_HISTORY_FORM,
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-000000002169"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Vaccination history"),
    );
    document.with_patient(patient())?;
    let slot = EUVAC_SLOTS.first().ok_or("the immunisations slot")?;
    document.with_section(Section::new(
        slot,
        vec![String::from("No vaccine recorded.")],
    ))?;
    assert!(matches!(
        document.into_bundle(),
        Err(DocumentError::Unentered {
            entry: "immunisation",
            slice: "immunisations"
        })
    ));
    Ok(())
}

#[test]
fn a_history_with_no_legal_authentication_is_refused() -> Result<(), Box<dyn Error>> {
    let mut document = Document::of(
        &VACCINATION_HISTORY_FORM,
        BASE,
        String::from("urn:uuid:4f5c1d1e-0000-4000-8000-000000002170"),
        String::from("2026-10-06T10:00:00Z"),
        String::from("Vaccination history"),
    );
    let subject = document.with_patient(patient())?;
    let answer = history()?.to_fhir_with(
        &vaccinated(Some(1)),
        &settings(),
        call(&subject, "Device/gateway"),
    )?;
    for mapped in document.with_mapped_composition(answer.bundle())? {
        document.with_section(mapped.into_section(vec![String::from("One synthetic vaccine.")]))?;
    }
    assert!(matches!(
        document.into_bundle(),
        Err(DocumentError::MissingMetadata {
            element: "legalAuthentication"
        })
    ));
    Ok(())
}
