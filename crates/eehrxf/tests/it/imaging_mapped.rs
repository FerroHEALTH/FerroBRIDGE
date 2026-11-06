// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging report mapped between openEHR and the HL7 Europe Imaging
//! Report 1.0.0-ballot document through the shipped imaging project, over
//! FerroBRIDGE's imaging report template, whose operational template the
//! suite builds from the vendored CKM archetypes.
//!
//! A synthetic `BundleReportEuImaging` is received, held to the vendored
//! `Bundle` and `Composition` profiles, and mapped into one openEHR
//! composition; the composition is mapped back out, and its report, its
//! study and its `Provenance` become a new document, received and held to
//! the same profiles.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::category::Category;
use eehrxf::document::Document;
use eehrxf::document::imaging::{IMAGING_BUNDLE, IMAGING_REPORT_FORM, IMAGING_SLOTS};
use eehrxf::document::section::Section;
use eehrxf::mapping::Mapping;
use eehrxf::mappings::IMAGING;
use eehrxf::receive::ReceivedDocument;
use ferrobridge_testkit::fixtures::{
    CKM_IMAGING_ARCHETYPES, CKM_LIBRARY_ARCHETYPES, FERROBRIDGE_IMAGING_TEMPLATE,
    FERROBRIDGE_IMAGING_TEMPLATE_ID,
};
use fhir_types::codec::{Json, Path};
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

use super::support::{emit, imaging};

/// The base the test documents name their entries under, in the
/// `example.org` domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The canonical URL of the imaging `Composition` profile.
const COMPOSITION_EU_IMAGING: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/CompositionEuImaging";

/// The canonical URL of the imaging report profile.
const DIAGNOSTIC_REPORT_EU_IMAGING: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/DiagnosticReportEuImaging";

/// The canonical URL of the imaging study profile.
const IMAGING_STUDY_EU_IMAGING: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/ImagingStudyEuImaging";

/// The report context of the imaging project.
const CONTEXT: &str = "ferrobridge_eu_imaging.context";

/// The instant every run is stamped with, so nothing reads a clock.
const NOW: &str = "2026-03-02T10:00:00Z";

/// The ADL 2 operational template of the imaging report template.
fn template() -> Result<TemplateSource, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    for (file, source) in CKM_LIBRARY_ARCHETYPES
        .iter()
        .chain(CKM_IMAGING_ARCHETYPES.iter())
    {
        let archetype = openehr_adl::assemble::parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    let template =
        openehr_adl::assemble::parse_artefact(FERROBRIDGE_IMAGING_TEMPLATE, Dialect::Adl2)
            .map_err(|errors| format!("the template: {errors:?}"))?;
    let opt = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(TemplateSource::Opt2 {
        template: Box::new(opt),
        resolved_id: String::from(FERROBRIDGE_IMAGING_TEMPLATE_ID),
    })
}

/// The report context of the shipped project, compiled from its embedded
/// files against the imaging report template.
fn mapping() -> Result<Mapping, Box<dyn Error>> {
    Ok(Mapping::compile_project(&template()?, &IMAGING)?)
}

/// The run settings: a synthetic engine device, a fixed instant and the
/// composition defaults a received document does not state.
fn settings() -> Settings {
    Settings::new("Device/ferrobridge-eehrxf-test", NOW)
        .with_defaults(Defaults::at(NOW).with_language("en").with_territory("NL"))
}

/// Decodes the FHIR JSON `value` as the R4 resource of `kind`.
fn resource(kind: &str, value: &Value) -> Result<Resource, Box<dyn Error>> {
    let fhir_types::codec::Value::Object(object) =
        serde_json::from_str::<fhir_types::codec::Value>(&value.to_string())?
    else {
        return Err(format!("the synthetic {kind} is no JSON object").into());
    };
    Ok(Resource::from_json(&object, &mut Path::root(kind))?)
}

/// A synthetic study in the shape `ImagingStudyEuImaging` constrains: its
/// DICOM study instance UID under the `2.25` UUID arc, its modality, its
/// start and its description.
fn study() -> Value {
    serde_json::json!({
        "resourceType": "ImagingStudy",
        "id": "synthetic-study",
        "meta": {"profile": [IMAGING_STUDY_EU_IMAGING]},
        "identifier": [{
            "type": {"coding": [{
                "system": "http://dicom.nema.org/resources/ontology/DCM",
                "code": "110180",
                "display": "Study Instance UID"
            }]},
            "system": "urn:dicom:uid",
            "value": "urn:oid:2.25.110239581749205781493301578934620193847"
        }],
        "status": "available",
        "modality": [{
            "system": "http://dicom.nema.org/resources/ontology/DCM",
            "code": "MR",
            "display": "Magnetic Resonance"
        }],
        "subject": {"reference": "Patient/synthetic-patient"},
        "started": "2026-03-02T08:00:00+01:00",
        "description": "Synthetic MR study"
    })
}

/// A synthetic report in the shape `DiagnosticReportEuImaging` constrains,
/// performed by the synthetic radiology department and naming the study.
fn report() -> Value {
    serde_json::json!({
        "resourceType": "DiagnosticReport",
        "id": "synthetic-report",
        "meta": {"profile": [DIAGNOSTIC_REPORT_EU_IMAGING]},
        "identifier": [{
            "system": "http://example.org/fhir/sid/ferrobridge-imaging-reports",
            "value": "synthetic-imaging-report-0001"
        }],
        "status": "final",
        "code": {"coding": [{
            "system": "http://loinc.org",
            "code": "24802-1",
            "display": "MR Knee"
        }]},
        "effectiveDateTime": "2026-03-02T08:30:00+01:00",
        "performer": [{"reference": "Organization/synthetic-radiology"}],
        "imagingStudy": [{"reference": "ImagingStudy/synthetic-study"}],
        "conclusion": "A synthetic impression."
    })
}

/// An imaging report document, named `identifier`, of the synthetic
/// patient, by the synthetic gateway and operator, answering the patient's
/// and the gateway's `fullUrl`.
fn imaging_document(identifier: &str) -> Result<(Document, String, String), Box<dyn Error>> {
    let mut document = Document::of(
        &IMAGING_REPORT_FORM,
        BASE,
        identifier.to_owned(),
        String::from(NOW),
        String::from("Imaging report"),
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
    Ok((document, subject, device))
}

/// Adds the four sections `CompositionEuImaging` requires, the imaging study
/// section listing `study`.
fn with_sections(document: &mut Document, study: &str) -> Result<(), Box<dyn Error>> {
    for slot in IMAGING_SLOTS.iter().take(4) {
        let mut section = Section::new(slot, vec![format!("{}.", slot.title)]);
        if slot.slice == "imagingstudy" {
            section = section.with_entry(study.to_owned());
        }
        document.with_section(section)?;
    }
    Ok(())
}

/// The synthetic imaging report document a member sends.
fn received_text() -> Result<String, Box<dyn Error>> {
    let (mut document, _, _) = imaging_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d001")?;
    let urls = document.add(&[
        Resource::Organization(Box::new(Organization {
            id: Some(String::from("synthetic-radiology")),
            name: Some("Synthetic radiology".into()),
            ..Organization::default()
        })),
        resource("ImagingStudy", &study())?,
    ])?;
    let study_url = urls.get(1).ok_or("the study's fullUrl")?.clone();
    with_sections(&mut document, &study_url)?;
    let Resource::DiagnosticReport(report) = resource("DiagnosticReport", &report())? else {
        return Err("the synthetic report is no DiagnosticReport".into());
    };
    document.with_report(*report)?;
    Ok(serde_json::to_string(&document.into_bundle()?)?)
}

/// The literal reference `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}

/// The composition `canonical` as a CDR answers it once committed, with a
/// synthetic `OBJECT_VERSION_ID`.
fn committed(canonical: &str) -> Result<String, Box<dyn Error>> {
    let mut composition: Value = serde_json::from_str(canonical)?;
    let object = composition
        .as_object_mut()
        .ok_or("the composition is a JSON object")?;
    object.insert(
        String::from("uid"),
        serde_json::json!({
            "_type": "OBJECT_VERSION_ID",
            "value": "5b0c3f7e-0000-4000-8000-00000000d101::ferrobridge.example::1"
        }),
    );
    Ok(composition.to_string())
}

/// The resources of `bundle` of type `kind`.
fn of_type<'a>(bundle: &'a Value, kind: &str) -> Vec<&'a Value> {
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == kind)
        .collect()
}

/// The one resource of type `kind` in `bundle`.
fn only<'a>(bundle: &'a Value, kind: &str) -> Result<&'a Value, Box<dyn Error>> {
    match of_type(bundle, kind).as_slice() {
        [one] => Ok(one),
        several => Err(format!("{} {kind} entries, not one", several.len()).into()),
    }
}

/// The `fullUrl` of the one entry of `bundle` whose resource is of `kind`.
fn full_url<'a>(bundle: &'a Value, kind: &str) -> Result<&'a str, Box<dyn Error>> {
    bundle["entry"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .find(|entry| entry["resource"]["resourceType"] == kind)
        .and_then(|entry| entry["fullUrl"].as_str())
        .ok_or_else(|| format!("the {kind}'s fullUrl").into())
}

/// Maps the received document into openEHR, and the composition back out
/// into a new document, answering both documents' text.
fn round_trip() -> Result<(String, String), Box<dyn Error>> {
    let mapping = mapping()?;
    let received = received_text()?;
    let document = ReceivedDocument::read(&received)?;
    let composition = mapping.to_openehr(&document, &settings())?;
    assert_eq!(composition.template_id(), FERROBRIDGE_IMAGING_TEMPLATE_ID);
    let (mut out, subject, device) =
        imaging_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d002")?;
    let context = CallContext::new()
        .with_patient(reference(&subject))
        .with_who(reference(&device));
    let run = mapping.to_fhir_with(
        &committed(composition.composition())?,
        &Settings::new(device.clone(), NOW),
        context,
    )?;
    out.with_mapped_report(run.bundle())?;
    let study_id = run
        .bundle()
        .entry
        .iter()
        .find_map(|entry| match entry.resource.as_ref() {
            Some(Resource::ImagingStudy(study)) => study.id.clone(),
            _ => None,
        })
        .ok_or("the mapped study")?;
    let study = out.full_url("ImagingStudy", &study_id);
    with_sections(&mut out, &study)?;
    Ok((received, serde_json::to_string(&out.into_bundle()?)?))
}

#[test]
fn the_shipped_project_compiles_against_the_imaging_template() -> Result<(), Box<dyn Error>> {
    assert_eq!(IMAGING.contexts(), &[CONTEXT]);
    let mapping = mapping()?;
    let contexts: Vec<_> = mapping
        .contexts()
        .map(|context| (context.name, context.template, context.profile))
        .collect();
    assert_eq!(
        contexts,
        [(
            CONTEXT,
            FERROBRIDGE_IMAGING_TEMPLATE_ID,
            DIAGNOSTIC_REPORT_EU_IMAGING
        )]
    );
    Ok(())
}

#[test]
fn the_template_holds_only_published_ckm_archetypes() -> Result<(), Box<dyn Error>> {
    for archetype in [
        "openEHR-EHR-COMPOSITION.report-result.v1",
        "openEHR-EHR-OBSERVATION.imaging_exam_result.v1",
    ] {
        assert!(
            FERROBRIDGE_IMAGING_TEMPLATE.contains(archetype),
            "the template names {archetype}"
        );
        let (file, source) = CKM_LIBRARY_ARCHETYPES
            .iter()
            .chain(CKM_IMAGING_ARCHETYPES.iter())
            .find(|(file, _)| file.starts_with(archetype))
            .ok_or_else(|| format!("{archetype} is vendored"))?;
        assert!(
            source.contains(r#"lifecycle_state = <"published">"#),
            "{file} is published on the CKM"
        );
    }
    Ok(())
}

#[test]
fn a_received_imaging_report_is_of_its_category_and_conforms() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(&received_text()?)?;
    assert_eq!(document.category()?, Category::Imaging);
    document.check(&imaging(IMAGING_BUNDLE)?, &imaging(COMPOSITION_EU_IMAGING)?)?;
    Ok(())
}

#[test]
fn a_received_imaging_report_maps_into_one_composition_that_keeps_it() -> Result<(), Box<dyn Error>>
{
    let text = received_text()?;
    let document = ReceivedDocument::read(&text)?;
    let composition = mapping()?.to_openehr(&document, &settings())?;
    assert_eq!(composition.template_id(), FERROBRIDGE_IMAGING_TEMPLATE_ID);
    let canonical: Value = serde_json::from_str(composition.composition())?;
    let written = canonical.to_string();
    for value in [
        "A synthetic impression.",
        "synthetic-imaging-report-0001",
        "24802-1",
        "urn:oid:2.25.110239581749205781493301578934620193847",
        "Synthetic MR study",
        "at0076",
        "at0101",
    ] {
        assert!(written.contains(value), "{value} is mapped: {written}");
    }
    assert_eq!(
        canonical
            .pointer("/feeder_audit/original_content/value")
            .and_then(Value::as_str),
        Some(text.as_str()),
        "the document is kept byte for byte"
    );
    Ok(())
}

#[test]
fn the_report_mapped_back_out_is_a_document_the_profiles_constrain() -> Result<(), Box<dyn Error>> {
    let (received, written) = round_trip()?;
    let document = ReceivedDocument::read(&written)?;
    assert_eq!(document.category()?, Category::Imaging);
    document.check(&imaging(IMAGING_BUNDLE)?, &imaging(COMPOSITION_EU_IMAGING)?)?;
    emit("eehrxf-imaging-mapped", &written)?;

    let input: Value = serde_json::from_str(&received)?;
    let output: Value = serde_json::from_str(&written)?;
    let (before, after) = (
        only(&input, "DiagnosticReport")?,
        only(&output, "DiagnosticReport")?,
    );
    assert_eq!(
        after["meta"]["profile"],
        serde_json::json!([DIAGNOSTIC_REPORT_EU_IMAGING]),
        "the report claims its profile once"
    );
    for pointer in [
        "/status",
        "/code/coding/0/system",
        "/code/coding/0/code",
        "/effectiveDateTime",
        "/conclusion",
    ] {
        assert_eq!(after.pointer(pointer), before.pointer(pointer), "{pointer}");
    }
    let (study_before, study_after) = (
        only(&input, "ImagingStudy")?,
        only(&output, "ImagingStudy")?,
    );
    assert_eq!(
        study_after["meta"]["profile"],
        serde_json::json!([IMAGING_STUDY_EU_IMAGING]),
        "the study claims its profile once"
    );
    for pointer in [
        "/identifier/0/system",
        "/identifier/0/value",
        "/status",
        "/description",
    ] {
        assert_eq!(
            study_after.pointer(pointer),
            study_before.pointer(pointer),
            "{pointer}: {study_after}"
        );
    }
    // NOTE: HL7 Europe Imaging Report 1.0.0-ballot, ImagingStudyEuImaging im-imagingstudy-01:
    // a DICOM study UID identifier carries a value in the urn:oid: form.
    assert!(
        study_after["identifier"][0]["value"]
            .as_str()
            .is_some_and(|value| value.starts_with("urn:oid:")),
        "{study_after}"
    );
    // NOTE: HL7 Europe Imaging Report 1.0.0-ballot, StudyInstanceUidIdentifierEuImaging requires
    // Identifier.type with the DICOM coding 110180 "Study Instance UID".
    assert_eq!(
        study_after.pointer("/identifier/0/type/coding/0"),
        Some(&serde_json::json!({
            "system": "http://dicom.nema.org/resources/ontology/DCM",
            "code": "110180",
            "display": "Study Instance UID"
        })),
        "{study_after}"
    );
    let study_url = full_url(&output, "ImagingStudy")?;
    let named = after["imagingStudy"][0]["reference"]
        .as_str()
        .ok_or("the report names its study")?;
    assert!(
        study_url.ends_with(named),
        "the report names the mapped study: {named} against {study_url}"
    );
    let composition = only(&output, "Composition")?;
    let listed = composition["section"]
        .as_array()
        .ok_or("sections")?
        .iter()
        .find(|section| section["code"]["coding"][0]["code"] == "18726-0")
        .and_then(|section| section["entry"][0]["reference"].as_str());
    assert_eq!(
        listed,
        Some(study_url),
        "the imaging study section lists it"
    );
    assert_eq!(
        of_type(&output, "OperationOutcome").len(),
        0,
        "the run's outcome reports the run and stays out of the document"
    );
    Ok(())
}

#[test]
fn one_provenance_covers_the_mapped_composition() -> Result<(), Box<dyn Error>> {
    let (_, written) = round_trip()?;
    let output: Value = serde_json::from_str(&written)?;
    let provenance = only(&output, "Provenance")?;
    let mut targets: Vec<&str> = provenance["target"]
        .as_array()
        .ok_or("the Provenance's targets")?
        .iter()
        .filter_map(|target| target["reference"].as_str())
        .collect();
    targets.sort_unstable();
    let mut mapped: Vec<String> = ["DiagnosticReport", "ImagingStudy"]
        .iter()
        .flat_map(|kind| {
            of_type(&output, kind)
                .into_iter()
                .filter_map(move |resource| {
                    resource["id"].as_str().map(|id| format!("{kind}/{id}"))
                })
        })
        .collect();
    mapped.sort_unstable();
    assert_eq!(
        targets, mapped,
        "the Provenance names every mapped resource"
    );
    Ok(())
}

/// The issues of severity `warning` the run that mapped the received
/// document into openEHR declared, by the expression each names.
fn declared_losses() -> Result<Vec<String>, Box<dyn Error>> {
    let document = ReceivedDocument::read(&received_text()?)?;
    let composition = mapping()?.to_openehr(&document, &settings())?;
    let outcome = composition.outcome().ok_or("the run declared its losses")?;
    let outcome = serde_json::to_value(outcome)?;
    Ok(outcome["issue"]
        .as_array()
        .ok_or("the outcome's issues")?
        .iter()
        .filter(|issue| issue["severity"] == "warning")
        .filter_map(|issue| issue["expression"][0].as_str())
        .map(str::to_owned)
        .collect())
}

// NOTE: no specification governs this: our own design; OBSERVATION.imaging_exam_result.v1 holds
// the modality and the study date in its event, which the study's reference does not reach.
#[test]
fn the_study_modality_and_date_the_template_holds_in_the_event_are_declared_lost()
-> Result<(), Box<dyn Error>> {
    let declared = declared_losses()?;
    for expression in ["ImagingStudy.modality", "ImagingStudy.started"] {
        assert!(
            declared.iter().any(|held| held.starts_with(expression)),
            "{expression} is declared: {declared:?}"
        );
    }
    Ok(())
}

// NOTE: OBSERVATION.imaging_exam_result.v1 holds one report identifier (at0104, 0..1), and
// the received report carries the document's beside its own, as the assembly writes it.
#[test]
fn the_report_identifier_the_archetype_holds_one_of_is_declared_lost() -> Result<(), Box<dyn Error>>
{
    let declared = declared_losses()?;
    assert!(
        declared
            .iter()
            .any(|held| held.starts_with("DiagnosticReport.identifier")),
        "the dropped identifier is declared: {declared:?}"
    );
    Ok(())
}
