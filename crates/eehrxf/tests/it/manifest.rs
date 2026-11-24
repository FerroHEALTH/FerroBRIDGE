// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging study manifest of draft `EEHRxF` Annex VII: a synthetic
//! study with the series an image archive would report, written as the
//! `EuMadoBundle` of HL7 Europe Imaging Manifest 1.0.0-ballot and held to its
//! profiles, and every part the caller must supply refused when it is absent.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::document::manifest::{
    DCM, DICOM_UID_SYSTEM, DicomInstance, DicomSeries, MANIFEST_BUNDLE, MANIFEST_IMAGING_STUDY,
    MANIFEST_TYPE, Manifest, ManifestError, WadoService,
};
use eehrxf::receive::ReceivedDocument;
use ferrobridge_term::client::Client;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::imaging_study::ImagingStudy;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use serde_json::Value;

use super::support::profile_set;

/// The base the manifest names its entries under, in the `example.org`
/// domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The synthetic manifest identifier.
const IDENTIFIER: &str = "urn:uuid:5b0c3f7e-0000-4000-8000-00000000e001";

/// The synthetic study instance UID, under the DICOM example root `2.25`.
const STUDY_UID: &str = "2.25.1001";

/// The SOP class UID of an MR image storage instance (DICOM PS3.4 B.5).
const MR_IMAGE_STORAGE: &str = "1.2.840.10008.5.1.4.1.1.4";

/// The synthetic WADO-RS service.
fn service() -> WadoService {
    WadoService {
        address: String::from("https://pacs.example.org/dicomweb"),
        retrieve_location_uid: String::from("2.25.9001"),
    }
}

/// A synthetic MR series `uid` of `instances` images retrieved from `service`.
fn series(uid: &str, instances: usize, service: WadoService) -> DicomSeries {
    DicomSeries {
        uid: uid.to_owned(),
        number: Some(1),
        modality: String::from("MR"),
        description: Some(String::from("Synthetic axial T2")),
        body_site: None,
        started: Some(String::from("2026-11-20T09:05:00Z")),
        service,
        instances: (1..=instances)
            .map(|number| DicomInstance {
                uid: format!("{uid}.{number}"),
                sop_class: MR_IMAGE_STORAGE.to_owned(),
                number: u32::try_from(number).ok(),
                title: None,
            })
            .collect(),
    }
}

/// The study the imaging report mapping writes, for a synthetic MR of the head.
fn study() -> ImagingStudy {
    ImagingStudy {
        id: Some(String::from("study-1")),
        identifier: vec![Identifier {
            system: Some(DICOM_UID_SYSTEM.into()),
            value: Some(format!("urn:oid:{STUDY_UID}").into()),
            ..Identifier::default()
        }],
        status: "available".into(),
        modality: vec![Coding {
            system: Some(DCM.into()),
            code: Some("MR".into()),
            ..Coding::default()
        }],
        started: Some("2026-11-20T09:00:00Z".into()),
        procedure_code: vec![CodeableConcept {
            text: Some("Synthetic MR of the head".into()),
            ..CodeableConcept::default()
        }],
        description: Some("Synthetic MR of the head".into()),
        ..ImagingStudy::default()
    }
}

/// A manifest with every part given, its series `series`.
fn manifest(series: Vec<DicomSeries>) -> Manifest {
    let mut manifest = Manifest::new(
        BASE.to_owned(),
        IDENTIFIER.to_owned(),
        String::from("2026-11-20T10:00:00Z"),
    );
    manifest.with_patient(Patient {
        id: Some(String::from("patient-1")),
        name: vec![HumanName {
            family: Some("Synthetic".into()),
            ..HumanName::default()
        }],
        birth_date: Some("1970-01-01".into()),
        ..Patient::default()
    });
    manifest.with_creator(
        Organization {
            id: Some(String::from("radiology")),
            name: Some("Synthetic Radiology".into()),
            ..Organization::default()
        },
        Device {
            id: Some(String::from("gateway")),
            device_name: vec![DeviceDeviceName {
                name: "FerroBRIDGE".into(),
                r#type: "user-friendly-name".into(),
                ..DeviceDeviceName::default()
            }],
            ..Device::default()
        },
    );
    manifest.with_study(study());
    manifest.with_region(CodeableConcept {
        coding: vec![Coding {
            system: Some("http://snomed.info/sct".into()),
            code: Some("774007".into()),
            display: Some("Structure of head and/or neck".into()),
            ..Coding::default()
        }],
        ..CodeableConcept::default()
    });
    manifest.with_series(series);
    manifest
}

/// The one entry resource of type `kind` in `bundle`.
fn the<'a>(bundle: &'a Value, kind: &str) -> Result<&'a Value, Box<dyn Error>> {
    let found: Vec<&Value> = bundle["entry"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == kind)
        .collect();
    match found.as_slice() {
        [one] => Ok(one),
        other => Err(format!("{} {kind} entries", other.len()).into()),
    }
}

#[test]
fn the_manifest_carries_the_annex_vii_study_with_its_series_and_endpoint()
-> Result<(), Box<dyn Error>> {
    let bundle = serde_json::to_value(
        manifest(vec![
            series("2.25.1001.1", 2, service()),
            series("2.25.1001.2", 3, service()),
        ])
        .into_bundle()?,
    )?;
    assert_eq!(bundle["type"], "document");
    assert_eq!(bundle["meta"]["profile"][0], MANIFEST_BUNDLE);
    assert_eq!(
        bundle["entry"][0]["resource"]["resourceType"],
        "Composition"
    );
    let composition = the(&bundle, "Composition")?;
    assert_eq!(composition["type"]["coding"][0]["code"], MANIFEST_TYPE);
    assert_eq!(
        composition["event"][0]["detail"][0]["reference"],
        "ImagingStudy/study-1"
    );
    let study = the(&bundle, "ImagingStudy")?;
    assert_eq!(study["meta"]["profile"][0], MANIFEST_IMAGING_STUDY);
    assert_eq!(
        study["identifier"][0]["value"],
        format!("urn:oid:{STUDY_UID}")
    );
    assert_eq!(
        study["identifier"][0]["type"]["coding"][0]["code"],
        "110180"
    );
    assert_eq!(study["numberOfSeries"], 2);
    assert_eq!(study["numberOfInstances"], 5);
    assert_eq!(study["series"][1]["numberOfInstances"], 3);
    assert_eq!(
        study["series"][0]["instance"][0]["sopClass"]["code"],
        format!("urn:oid:{MR_IMAGE_STORAGE}")
    );
    assert_eq!(
        study["series"][0]["endpoint"][0]["reference"],
        "Endpoint/wado-1"
    );
    let endpoint = the(&bundle, "Endpoint")?;
    assert_eq!(endpoint["connectionType"]["code"], "dicom-wado-rs");
    assert_eq!(endpoint["address"], "https://pacs.example.org/dicomweb");
    Ok(())
}

#[test]
fn each_distinct_service_is_one_endpoint() -> Result<(), Box<dyn Error>> {
    let other = WadoService {
        address: String::from("https://archive.example.org/dicomweb"),
        retrieve_location_uid: String::from("2.25.9002"),
    };
    let bundle = serde_json::to_value(
        manifest(vec![
            series("2.25.1001.1", 1, service()),
            series("2.25.1001.2", 1, other),
            series("2.25.1001.3", 1, service()),
        ])
        .into_bundle()?,
    )?;
    let endpoints = bundle["entry"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .filter(|entry| entry["resource"]["resourceType"] == "Endpoint")
        .count();
    assert_eq!(endpoints, 2);
    Ok(())
}

#[test]
fn the_manifest_validates_against_eu_mado_bundle() -> Result<(), Box<dyn Error>> {
    let Some(set) = profile_set()? else {
        return Ok(());
    };
    let text =
        serde_json::to_string(&manifest(vec![series("2.25.1001.1", 2, service())]).into_bundle()?)?;
    let received = ReceivedDocument::read(&text)?;
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    runtime
        .block_on(received.validate(set, MANIFEST_BUNDLE, None::<&Client>))
        .map_err(|error| format!("{error:?}"))?;
    Ok(())
}

#[test]
fn a_manifest_without_series_is_refused() {
    let refused = manifest(Vec::new()).into_bundle();
    assert!(
        matches!(refused, Err(ManifestError::NoSeries)),
        "{refused:?}"
    );
}

#[test]
fn a_series_without_instances_is_refused() {
    let refused = manifest(vec![series("2.25.1001.1", 0, service())]).into_bundle();
    assert!(
        matches!(refused, Err(ManifestError::NoInstances { ref series }) if series == "2.25.1001.1"),
        "{refused:?}"
    );
}

#[test]
fn a_study_without_its_uid_is_refused() {
    let mut manifest = manifest(vec![series("2.25.1001.1", 1, service())]);
    manifest.with_study(ImagingStudy {
        identifier: Vec::new(),
        ..study()
    });
    let refused = manifest.into_bundle();
    assert!(
        matches!(refused, Err(ManifestError::NoStudyUid)),
        "{refused:?}"
    );
}

#[test]
fn a_study_without_a_readable_procedure_is_refused() {
    let mut manifest = manifest(vec![series("2.25.1001.1", 1, service())]);
    manifest.with_study(ImagingStudy {
        procedure_code: Vec::new(),
        ..study()
    });
    let refused = manifest.into_bundle();
    assert!(
        matches!(refused, Err(ManifestError::NoProcedure)),
        "{refused:?}"
    );
}

#[test]
fn a_manifest_without_its_region_is_refused() {
    let mut manifest = Manifest::new(
        BASE.to_owned(),
        IDENTIFIER.to_owned(),
        String::from("2026-11-20T10:00:00Z"),
    );
    manifest.with_series(vec![series("2.25.1001.1", 1, service())]);
    manifest.with_study(study());
    manifest.with_patient(Patient {
        id: Some(String::from("patient-1")),
        ..Patient::default()
    });
    manifest.with_creator(
        Organization {
            id: Some(String::from("radiology")),
            ..Organization::default()
        },
        Device {
            id: Some(String::from("gateway")),
            ..Device::default()
        },
    );
    let refused = manifest.into_bundle();
    assert!(
        matches!(refused, Err(ManifestError::Missing { part: "region" })),
        "{refused:?}"
    );
}
