// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging study manifest of draft `EEHRxF` Annex VII in the HL7 Europe
//! Imaging Manifest 1.0.0-ballot form (feature `imaging`).
//!
//! Draft `EEHRxF` Art 3 and Annex II(d) (initiative 17512, 2026-11-23)
//! represent an imaging study as DICOM retrieved over `DICOMweb` (PS3.18) with
//! a FHIR manifest beside the report. The manifest is an `EuMadoBundle`: an
//! `EuMadoComposition` whose event is the `EuMadoImagingStudy`, the patient,
//! the WADO-RS `EuMadoWadoEndpoint` of each series, and the creator
//! organization and device the composition's two author slices require.
//!
//! The study's identity, status, modality, start, interpreter, procedure and
//! description come from the `ImagingStudy` the imaging report mapping writes
//! from openEHR. Its series and instances and the services they are retrieved from
//! are DICOM facts openEHR does not hold, so the caller supplies them, read
//! from the image archive (QIDO-RS); the manifest never invents one, and a
//! manifest without them is refused with a [`ManifestError`].

use std::collections::BTreeMap;

use fhir_types::r4::bundle::{Bundle, BundleEntry};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::composition::{Composition, CompositionEvent};
use fhir_types::r4::device::Device;
use fhir_types::r4::endpoint::Endpoint;
use fhir_types::r4::extension::{Extension, ExtensionValue};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::imaging_study::{ImagingStudy, ImagingStudySeries, ImagingStudySeriesInstance};
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::resource::Resource;

use super::{URN_SYSTEM, claiming, loinc, narrative, reference};

/// The canonical URL of the HL7 Europe Imaging Manifest `Bundle` profile.
pub const MANIFEST_BUNDLE: &str =
    "http://hl7.eu/fhir/imaging-manifest/StructureDefinition/EuMadoBundle";

/// The canonical URL of the manifest's `Composition` profile.
pub const MANIFEST_COMPOSITION: &str =
    "http://hl7.eu/fhir/imaging-manifest/StructureDefinition/EuMadoComposition";

/// The canonical URL of the manifest's `ImagingStudy` profile.
pub const MANIFEST_IMAGING_STUDY: &str =
    "http://hl7.eu/fhir/imaging-manifest/StructureDefinition/EuMadoImagingStudy";

/// The canonical URL of the manifest's `Patient` profile.
pub const MANIFEST_PATIENT: &str =
    "http://hl7.eu/fhir/imaging-manifest/StructureDefinition/EuMadoPatient";

/// The canonical URL of the manifest's WADO-RS `Endpoint` profile.
pub const MANIFEST_WADO_ENDPOINT: &str =
    "http://hl7.eu/fhir/imaging-manifest/StructureDefinition/EuMadoWadoEndpoint";

/// The canonical URL of the manifest's creator `Device` profile.
pub const MANIFEST_CREATOR: &str =
    "http://hl7.eu/fhir/imaging-manifest/StructureDefinition/EuMadoCreator";

/// The canonical URL of the manifest's creator `Organization` profile.
pub const MANIFEST_CREATOR_ORGANIZATION: &str =
    "http://hl7.eu/fhir/imaging-manifest/StructureDefinition/EuMadoCreatorOrganization";

/// The `Composition.type` `EuMadoComposition` fixes: LOINC `18748-4`.
pub const MANIFEST_TYPE: &str = "18748-4";

/// The system of a DICOM UID as an identifier (`MadoStudyInstanceUidIdentifier`).
pub const DICOM_UID_SYSTEM: &str = "urn:dicom:uid";

/// The DICOM controlled terminology (PS3.16), the system of a modality.
pub const DCM: &str = "http://dicom.nema.org/resources/ontology/DCM";

/// The extension `EuMadoImagingStudy` requires for the anatomical region.
const ANATOMICAL_REGION: &str =
    "https://profiles.ihe.net/RAD/MADO/StructureDefinition/MadoAnatomicalRegionExtension";

/// The extension `EuMadoWadoEndpoint` requires for the retrieve location UID.
const RETRIEVE_LOCATION_UID: &str =
    "https://profiles.ihe.net/RAD/MADO/StructureDefinition/MadoRetrieveLocationUIDExtension";

/// The code system of the creator device type `MadoCreator` fixes.
const MADO_DEVICE_TYPE: &str = "https://profiles.ihe.net/RAD/MADO/CodeSystem/MadoDeviceType";

/// The title every manifest carries.
const TITLE: &str = "Imaging study manifest";

/// A `DICOMweb` WADO-RS service series are retrieved from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WadoService {
    /// The WADO-RS base URL.
    pub address: String,
    /// The retrieve location UID of the source (DICOM PS3.3 Table
    /// A.35.4-1), which `EuMadoWadoEndpoint` requires.
    pub retrieve_location_uid: String,
}

/// One DICOM instance of a series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DicomInstance {
    /// The SOP instance UID.
    pub uid: String,
    /// The SOP class UID.
    pub sop_class: String,
    /// The instance number.
    pub number: Option<u32>,
    /// The instance title.
    pub title: Option<String>,
}

/// One DICOM series of the study, as the image archive describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DicomSeries {
    /// The series instance UID.
    pub uid: String,
    /// The series number.
    pub number: Option<u32>,
    /// The modality, a DICOM PS3.16 code.
    pub modality: String,
    /// The series description.
    pub description: Option<String>,
    /// The body site the series images.
    pub body_site: Option<Coding>,
    /// When the series started, an R4 `dateTime`.
    pub started: Option<String>,
    /// The service the series is retrieved from.
    pub service: WadoService,
    /// The instances of the series.
    pub instances: Vec<DicomInstance>,
}

/// Why a manifest cannot be written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// The caller supplied no series; the manifest is never written with
    /// series it was not given.
    #[error("no DICOM series was supplied for the study")]
    NoSeries,
    /// A series holds no instance, which `EuMadoImagingStudy` requires.
    #[error("the DICOM series {series} holds no instance")]
    NoInstances {
        /// The series instance UID.
        series: String,
    },
    /// A part of the manifest was not given.
    #[error("the manifest requires its {part}, which was not given")]
    Missing {
        /// The part: `patient`, `creator`, `study` or `region`.
        part: &'static str,
    },
    /// The study carries no DICOM study instance UID.
    #[error("the study carries no identifier in the {DICOM_UID_SYSTEM} system")]
    NoStudyUid,
    /// The study names its procedure neither by text nor by a display,
    /// which `EuMadoImagingStudy` `mado-reqproc-1` requires.
    #[error("the study names its procedure neither by text nor by display (mado-reqproc-1)")]
    NoProcedure,
    /// A resource carries no `id`, so it has no `fullUrl`.
    #[error("a {resource} resource carries no id")]
    NoId {
        /// The resource type.
        resource: &'static str,
    },
    /// A count is larger than an R4 `unsignedInt` holds.
    #[error("{count} {what} are more than an R4 unsignedInt holds")]
    TooMany {
        /// What was counted.
        what: &'static str,
        /// The count.
        count: usize,
        /// Why it does not fit.
        #[source]
        source: std::num::TryFromIntError,
    },
}

/// An imaging study manifest as it is assembled.
#[derive(Debug, Clone)]
pub struct Manifest {
    base: String,
    identifier: String,
    timestamp: String,
    patient: Option<Patient>,
    creator: Option<(Organization, Device)>,
    study: Option<ImagingStudy>,
    region: Option<CodeableConcept>,
    series: Vec<DicomSeries>,
}

impl Manifest {
    /// Starts a manifest whose entries are named under `base`, identified by
    /// the URI `identifier` and stamped `timestamp`, an R4 `instant`.
    #[must_use]
    pub const fn new(base: String, identifier: String, timestamp: String) -> Self {
        Self {
            base,
            identifier,
            timestamp,
            patient: None,
            creator: None,
            study: None,
            region: None,
            series: Vec::new(),
        }
    }

    /// Sets the patient the study images.
    pub fn with_patient(&mut self, patient: Patient) {
        self.patient = Some(patient);
    }

    /// Sets the organization and the device that create the manifest, its
    /// two composition authors.
    pub fn with_creator(&mut self, organization: Organization, device: Device) {
        self.creator = Some((organization, device));
    }

    /// Sets the study the imaging report mapping wrote.
    pub fn with_study(&mut self, study: ImagingStudy) {
        self.study = Some(study);
    }

    /// Sets the anatomical region of the study, which `EuMadoImagingStudy`
    /// requires, coded from the MADO `ValueSetAnatomicalRegion`.
    pub fn with_region(&mut self, region: CodeableConcept) {
        self.region = Some(region);
    }

    /// Sets the DICOM series of the study, read from the image archive.
    pub fn with_series(&mut self, series: Vec<DicomSeries>) {
        self.series = series;
    }

    /// Writes the manifest as an `EuMadoBundle`.
    ///
    /// # Errors
    ///
    /// [`ManifestError::NoSeries`] when no series was supplied,
    /// [`ManifestError::NoInstances`] for a series with no instance,
    /// [`ManifestError::Missing`] for a part not given,
    /// [`ManifestError::NoStudyUid`] and [`ManifestError::NoProcedure`] for
    /// a study that lacks its UID or a readable procedure,
    /// [`ManifestError::NoId`] for a resource with no `id`, and
    /// [`ManifestError::TooMany`] for a count an R4 `unsignedInt` cannot hold.
    pub fn into_bundle(self) -> Result<Bundle, ManifestError> {
        if self.series.is_empty() {
            return Err(ManifestError::NoSeries);
        }
        if let Some(empty) = self
            .series
            .iter()
            .find(|series| series.instances.is_empty())
        {
            return Err(ManifestError::NoInstances {
                series: empty.uid.clone(),
            });
        }
        let mut patient = self
            .patient
            .ok_or(ManifestError::Missing { part: "patient" })?;
        let (mut organization, mut device) = self
            .creator
            .ok_or(ManifestError::Missing { part: "creator" })?;
        let study = self.study.ok_or(ManifestError::Missing { part: "study" })?;
        let region = self
            .region
            .ok_or(ManifestError::Missing { part: "region" })?;
        let base = self.base;
        let url = |kind: &str, id: &str| format!("{base}/{kind}/{id}");
        let patient_id = id_of("Patient", patient.id.as_ref())?;
        let organization_id = id_of("Organization", organization.id.as_ref())?;
        let device_id = id_of("Device", device.id.as_ref())?;
        let study_id = id_of("ImagingStudy", study.id.as_ref())?;
        let services = services(&self.series);
        let manifest_study = manifest_study(study, region, &self.series, &services, &patient_id)?;
        patient.meta = Some(claiming(MANIFEST_PATIENT));
        organization.meta = Some(claiming(MANIFEST_CREATOR_ORGANIZATION));
        device.meta = Some(claiming(MANIFEST_CREATOR));
        device.r#type = Some(creator_type());
        let description = manifest_study
            .description
            .as_ref()
            .and_then(|text| text.value.clone())
            .unwrap_or_else(|| TITLE.to_owned());
        let composition_id = self
            .identifier
            .rsplit(':')
            .next()
            .unwrap_or("manifest")
            .to_owned();
        let ids = Ids {
            composition: &composition_id,
            patient: &patient_id,
            organization: &organization_id,
            device: &device_id,
            study: &study_id,
        };
        let composition = composition(&ids, &self.identifier, &self.timestamp, description);
        let mut entries = vec![
            entry(
                url("Composition", &composition_id),
                Resource::Composition(Box::new(composition)),
            ),
            entry(
                url("ImagingStudy", &study_id),
                Resource::ImagingStudy(Box::new(manifest_study)),
            ),
            entry(
                url("Patient", &patient_id),
                Resource::Patient(Box::new(patient)),
            ),
        ];
        for (service, id) in &services {
            entries.push(entry_of_service(url("Endpoint", id), service, id));
        }
        entries.push(entry(
            url("Device", &device_id),
            Resource::Device(Box::new(device)),
        ));
        entries.push(entry(
            url("Organization", &organization_id),
            Resource::Organization(Box::new(organization)),
        ));
        Ok(Bundle {
            meta: Some(claiming(MANIFEST_BUNDLE)),
            identifier: Some(Identifier {
                system: Some(URN_SYSTEM.into()),
                value: Some(self.identifier.into()),
                ..Identifier::default()
            }),
            r#type: "document".into(),
            timestamp: Some(self.timestamp.into()),
            entry: entries,
            ..Bundle::default()
        })
    }
}

/// The `id` of a `resource` of the manifest.
fn id_of(resource: &'static str, id: Option<&String>) -> Result<String, ManifestError> {
    id.cloned().ok_or(ManifestError::NoId { resource })
}

/// The `Endpoint` id of each distinct service of `series`, numbered in the
/// order the series first name them.
fn services(series: &[DicomSeries]) -> BTreeMap<&WadoService, String> {
    let mut seen = BTreeMap::new();
    for one in series {
        let next = format!("wado-{}", seen.len() + 1);
        seen.entry(&one.service).or_insert(next);
    }
    seen
}

/// The device type `MadoCreator` fixes.
fn creator_type() -> CodeableConcept {
    CodeableConcept {
        coding: vec![Coding {
            system: Some(MADO_DEVICE_TYPE.into()),
            code: Some("mado-creator".into()),
            ..Coding::default()
        }],
        ..CodeableConcept::default()
    }
}

/// The ids of the resources the manifest's composition names.
struct Ids<'a> {
    composition: &'a str,
    patient: &'a str,
    organization: &'a str,
    device: &'a str,
    study: &'a str,
}

/// The `EuMadoComposition` of the manifest identified by `identifier` and
/// dated `timestamp`, its narrative the study's `description`.
fn composition(
    ids: &Ids<'_>,
    identifier: &str,
    timestamp: &str,
    description: String,
) -> Composition {
    Composition {
        id: Some(ids.composition.to_owned()),
        meta: Some(claiming(MANIFEST_COMPOSITION)),
        text: Some(narrative(TITLE, &[description])),
        identifier: Some(Identifier {
            system: Some(URN_SYSTEM.into()),
            value: Some(identifier.into()),
            ..Identifier::default()
        }),
        status: "final".into(),
        r#type: CodeableConcept {
            coding: vec![loinc(MANIFEST_TYPE)],
            ..CodeableConcept::default()
        },
        subject: Some(reference(&format!("Patient/{}", ids.patient))),
        date: timestamp.into(),
        author: vec![
            reference(&format!("Organization/{}", ids.organization)),
            reference(&format!("Device/{}", ids.device)),
        ],
        title: TITLE.into(),
        custodian: Some(reference(&format!("Organization/{}", ids.organization))),
        event: vec![CompositionEvent {
            detail: vec![reference(&format!("ImagingStudy/{}", ids.study))],
            ..CompositionEvent::default()
        }],
        ..Composition::default()
    }
}

/// The `EuMadoImagingStudy` of the mapped `study` with the region and the
/// series given, naming the patient `patient_id`.
///
/// The study keeps its identity, status, modality, start, procedure and
/// description; its links to the order, the readers and the encounter stay
/// with the report, whose entries the manifest does not carry.
fn manifest_study(
    study: ImagingStudy,
    region: CodeableConcept,
    series: &[DicomSeries],
    services: &BTreeMap<&WadoService, String>,
    patient_id: &str,
) -> Result<ImagingStudy, ManifestError> {
    let uid = study
        .identifier
        .iter()
        .find(|identifier| {
            identifier
                .system
                .as_ref()
                .and_then(|system| system.value.as_deref())
                == Some(DICOM_UID_SYSTEM)
        })
        .and_then(|identifier| identifier.value.as_ref())
        .and_then(|value| value.value.clone())
        .ok_or(ManifestError::NoStudyUid)?;
    let readable = study.procedure_code.iter().any(|code| {
        code.text.as_ref().is_some_and(|text| text.value.is_some())
            || code.coding.iter().any(|coding| coding.display.is_some())
    });
    if !readable {
        return Err(ManifestError::NoProcedure);
    }
    let mut written = Vec::with_capacity(series.len());
    let mut instances = 0_usize;
    for one in series {
        instances = instances.saturating_add(one.instances.len());
        let endpoint = services
            .get(&one.service)
            .map(|id| reference(&format!("Endpoint/{id}")))
            .into_iter()
            .collect();
        written.push(ImagingStudySeries {
            uid: one.uid.as_str().into(),
            number: one.number.map(Into::into),
            modality: Coding {
                system: Some(DCM.into()),
                code: Some(one.modality.as_str().into()),
                ..Coding::default()
            },
            description: one.description.as_deref().map(Into::into),
            number_of_instances: Some(count("instances of a series", one.instances.len())?),
            endpoint,
            body_site: one.body_site.clone(),
            started: one.started.as_deref().map(Into::into),
            instance: one
                .instances
                .iter()
                .map(|instance| ImagingStudySeriesInstance {
                    uid: instance.uid.as_str().into(),
                    sop_class: Coding {
                        system: Some(URN_SYSTEM.into()),
                        code: Some(format!("urn:oid:{}", instance.sop_class).into()),
                        ..Coding::default()
                    },
                    number: instance.number.map(Into::into),
                    title: instance.title.as_deref().map(Into::into),
                    ..ImagingStudySeriesInstance::default()
                })
                .collect(),
            ..ImagingStudySeries::default()
        });
    }
    Ok(ImagingStudy {
        id: study.id,
        meta: Some(claiming(MANIFEST_IMAGING_STUDY)),
        extension: vec![Extension {
            url: ANATOMICAL_REGION.into(),
            value: Some(ExtensionValue::CodeableConcept(Box::new(region))),
            ..Extension::default()
        }],
        identifier: vec![Identifier {
            r#type: Some(CodeableConcept {
                coding: vec![Coding {
                    system: Some(DCM.into()),
                    code: Some("110180".into()),
                    display: Some("Study Instance UID".into()),
                    ..Coding::default()
                }],
                ..CodeableConcept::default()
            }),
            system: Some(DICOM_UID_SYSTEM.into()),
            value: Some(uid.into()),
            ..Identifier::default()
        }],
        status: study.status,
        modality: study.modality,
        subject: reference(&format!("Patient/{patient_id}")),
        started: study.started,
        interpreter: study.interpreter,
        number_of_series: Some(count("series", series.len())?),
        number_of_instances: Some(count("instances", instances)?),
        procedure_code: study.procedure_code,
        description: study.description,
        series: written,
        ..ImagingStudy::default()
    })
}

/// The `unsignedInt` of `value`, a count of `what`.
fn count(
    what: &'static str,
    value: usize,
) -> Result<fhir_types::r4::primitives::UnsignedInt, ManifestError> {
    u32::try_from(value)
        .map(Into::into)
        .map_err(|source| ManifestError::TooMany {
            what,
            count: value,
            source,
        })
}

/// The bundle entry of `resource` under `full_url`.
fn entry(full_url: String, resource: Resource) -> BundleEntry {
    BundleEntry {
        full_url: Some(full_url.into()),
        resource: Some(resource),
        ..BundleEntry::default()
    }
}

/// The bundle entry of the `EuMadoWadoEndpoint` `id` of `service`.
fn entry_of_service(full_url: String, service: &WadoService, id: &str) -> BundleEntry {
    let endpoint = Endpoint {
        id: Some(id.to_owned()),
        meta: Some(claiming(MANIFEST_WADO_ENDPOINT)),
        extension: vec![Extension {
            url: RETRIEVE_LOCATION_UID.into(),
            value: Some(ExtensionValue::String(
                service.retrieve_location_uid.as_str().into(),
            )),
            ..Extension::default()
        }],
        status: "active".into(),
        connection_type: Coding {
            system: Some("http://terminology.hl7.org/CodeSystem/endpoint-connection-type".into()),
            code: Some("dicom-wado-rs".into()),
            ..Coding::default()
        },
        payload_type: vec![CodeableConcept {
            coding: vec![Coding {
                system: Some("http://terminology.hl7.org/CodeSystem/endpoint-payload-type".into()),
                code: Some("none".into()),
                ..Coding::default()
            }],
            text: Some("DICOM WADO-RS".into()),
            ..CodeableConcept::default()
        }],
        address: service.address.as_str().into(),
        ..Endpoint::default()
    };
    entry(full_url, Resource::Endpoint(Box::new(endpoint)))
}
