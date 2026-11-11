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
use eehrxf::document::section::{EmptyReason, Section};
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
use fhir_types::r4::practitioner::Practitioner;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::traverse::defaults::Defaults;
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
    let raw = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
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
/// section listing `study`, and each other one empty for
/// nothing known, which `eu-imaging-composition-1` asks of an empty section.
fn with_sections(document: &mut Document, study: &str) -> Result<(), Box<dyn Error>> {
    for slot in IMAGING_SLOTS.iter().take(4) {
        let mut section = Section::new(slot, vec![format!("{}.", slot.title)]);
        section = if slot.slice == "imagingstudy" {
            section.with_entry(study.to_owned())
        } else {
            section.empty_because(EmptyReason::NilKnown)
        };
        document.with_section(section)?;
    }
    Ok(())
}

/// The synthetic imaging report document a member sends.
fn received_text() -> Result<String, Box<dyn Error>> {
    received_text_of(&report())
}

/// The received document of [`received_text`], its report `report`.
fn received_text_of(report: &Value) -> Result<String, Box<dyn Error>> {
    received_text_with(report, None)
}

/// The received document of [`received_text_of`], holding `practitioner`
/// beside the performing organization when one is given.
fn received_text_with(
    report: &Value,
    practitioner: Option<Practitioner>,
) -> Result<String, Box<dyn Error>> {
    let (mut document, _, _) = imaging_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d001")?;
    let mut added = vec![
        Resource::Organization(Box::new(Organization {
            id: Some(String::from("synthetic-radiology")),
            name: Some("Synthetic radiology".into()),
            ..Organization::default()
        })),
        resource("ImagingStudy", &study())?,
    ];
    added.extend(practitioner.map(|held| Resource::Practitioner(Box::new(held))));
    let urls = document.add(&added)?;
    let study_url = urls.get(1).ok_or("the study's fullUrl")?.clone();
    with_sections(&mut document, &study_url)?;
    let Resource::DiagnosticReport(report) = resource("DiagnosticReport", report)? else {
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
        "Magnetic Resonance",
        "2026-03-02T08:00:00+01:00",
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
    let mut structured = canonical.clone();
    structured
        .as_object_mut()
        .ok_or("the composition is a JSON object")?
        .remove("feeder_audit");
    let structured = structured.to_string();
    for value in ["Synthetic radiology", "openEHR-EHR-CLUSTER.organisation.v1"] {
        assert!(
            structured.contains(value),
            "the performing organization is in the imaging service slot: {structured}"
        );
    }
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
        "/modality/0/system",
        "/modality/0/code",
        "/modality/0/display",
        "/started",
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
    // NOTE: HL7 Europe Imaging Report 1.0.0-ballot, CompositionEuImaging eu-imaging-composition-1:
    // a section with no entry carries an emptyReason.
    for section in composition["section"].as_array().ok_or("sections")? {
        if section["entry"].as_array().is_none_or(Vec::is_empty) {
            assert!(section["emptyReason"].is_object(), "{section}");
        }
    }
    assert_eq!(
        of_type(&output, "OperationOutcome").len(),
        0,
        "the run's outcome reports the run and stays out of the document"
    );
    Ok(())
}

// NOTE: HL7 Europe Imaging Report 1.0.0-ballot DiagnosticReportEuImaging.performer:organization
// is an organization-eu-core, whose name is 1..1; the imaging service slot holds the name.
#[test]
fn the_imaging_service_is_served_as_the_performing_organization() -> Result<(), Box<dyn Error>> {
    let (_, written) = round_trip()?;
    let output: Value = serde_json::from_str(&written)?;
    let performer = only(&output, "DiagnosticReport")?["performer"][0]["reference"]
        .as_str()
        .ok_or("the served report names a performer")?
        .to_owned();
    let organization = of_type(&output, "Organization")
        .into_iter()
        .find(|resource| {
            resource["id"]
                .as_str()
                .is_some_and(|id| performer.ends_with(&format!("Organization/{id}")))
        })
        .ok_or_else(|| format!("the organization {performer} names"))?;
    assert_eq!(
        organization["name"], "Synthetic radiology",
        "{organization}"
    );
    assert_eq!(
        organization["meta"]["profile"][0],
        "http://hl7.eu/fhir/base/StructureDefinition/organization-eu-core",
        "{organization}"
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
    let performer = only(&output, "DiagnosticReport")?["performer"][0]["reference"]
        .as_str()
        .ok_or("the mapped report names its performing organization")?;
    let organization = of_type(&output, "Organization")
        .into_iter()
        .find(|resource| {
            resource["id"]
                .as_str()
                .is_some_and(|id| performer.ends_with(&format!("Organization/{id}")))
        })
        .ok_or_else(|| format!("the organization {performer} the report names"))?;
    mapped.push(format!(
        "Organization/{}",
        organization["id"].as_str().ok_or("the organization's id")?
    ));
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
    expressions(&received_text()?, |issue| issue["severity"] == "warning")
}

/// The expression of each issue `keep` holds to, in the order the run that
/// mapped the received `text` into openEHR declared them.
fn expressions(text: &str, keep: impl Fn(&Value) -> bool) -> Result<Vec<String>, Box<dyn Error>> {
    expressions_by(&mapping()?, text, keep)
}

/// The expressions of [`expressions`], the run made by `mapping`.
fn expressions_by(
    mapping: &Mapping,
    text: &str,
    keep: impl Fn(&Value) -> bool,
) -> Result<Vec<String>, Box<dyn Error>> {
    let document = ReceivedDocument::read(text)?;
    let composition = mapping.to_openehr(&document, &settings())?;
    let outcome = composition.outcome().ok_or("the run declared its losses")?;
    let outcome = serde_json::to_value(outcome)?;
    Ok(outcome["issue"]
        .as_array()
        .ok_or("the outcome's issues")?
        .iter()
        .filter(|issue| keep(issue))
        .filter_map(|issue| issue["expression"][0].as_str())
        .map(str::to_owned)
        .collect())
}

// NOTE: basics/path_operators.adoc, the study's reference sits below the event that holds its
// modality and date, and reaches the protocol outside it, so neither is declared lost.
#[test]
fn the_study_modality_and_date_the_template_holds_in_the_event_are_mapped()
-> Result<(), Box<dyn Error>> {
    let declared = declared_losses()?;
    for expression in ["ImagingStudy.modality", "ImagingStudy.started"] {
        assert!(
            !declared.iter().any(|held| held.starts_with(expression)),
            "{expression} is mapped, not declared: {declared:?}"
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

/// The composition the received document of `report` maps into, as a CDR
/// answers it once committed.
fn composition_of(report: &Value) -> Result<String, Box<dyn Error>> {
    let document = ReceivedDocument::read(&received_text_of(report)?)?;
    let composition = mapping()?.to_openehr(&document, &settings())?;
    committed(composition.composition())
}

// NOTE: R4 DiagnosticReport.conclusionCode, "One or more codes that represent the summary
// conclusion": each imaging diagnosis (at0020) is one, in both directions.
#[test]
fn each_imaging_diagnosis_is_a_conclusion_code_both_ways() -> Result<(), Box<dyn Error>> {
    let mut coded = report();
    coded["conclusionCode"] = serde_json::json!([{
        "coding": [{"system": "http://example.org/fhir/CodeSystem/synthetic-findings", "code": "SYN-DX-1"}],
        "text": "A synthetic imaging diagnosis"
    }]);
    let composition = composition_of(&coded)?;
    let canonical: Value = serde_json::from_str(&composition)?;
    assert!(
        canonical
            .to_string()
            .contains("A synthetic imaging diagnosis"),
        "the diagnosis is in the composition"
    );
    let run = mapping()?.to_fhir_with(
        &composition,
        &Settings::new("Device/ferrobridge-eehrxf-test", NOW),
        CallContext::new().with_patient(reference("Patient/synthetic-patient")),
    )?;
    let bundle = serde_json::to_value(run.bundle())?;
    let served = only(&bundle, "DiagnosticReport")?;
    assert_eq!(
        served["conclusionCode"][0]["text"], "A synthetic imaging diagnosis",
        "{served}"
    );
    assert_eq!(
        served["conclusionCode"][0]["coding"][0]["code"], "SYN-DX-1",
        "{served}"
    );
    Ok(())
}

/// Adds `items` to the result data (`at0003`) of the first event `value`
/// holds, answering whether it found one.
fn add_findings(value: &mut Value, items: &[Value]) -> bool {
    match value {
        Value::Object(members) => {
            if members.get("archetype_node_id").and_then(Value::as_str) == Some("at0003")
                && let Some(Value::Array(held)) = members.get_mut("items")
            {
                held.extend(items.iter().cloned());
                return true;
            }
            members
                .values_mut()
                .any(|member| add_findings(member, items))
        }
        Value::Array(held) => held.iter_mut().any(|item| add_findings(item, items)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

// NOTE: no element of DiagnosticReportEuImaging or CompositionEuImaging the project writes holds
// a findings narrative, a differential, a comparison, a clinical summary, a recommendation or a
// comment, so a served report declares each the composition records.
#[test]
fn each_finding_element_the_report_has_no_place_for_is_declared() -> Result<(), Box<dyn Error>> {
    let mut canonical: Value = serde_json::from_str(&composition_of(&report())?)?;
    let element = |at: &str, name: &str, text: &str| {
        serde_json::json!({
            "_type": "ELEMENT",
            "name": {"_type": "DV_TEXT", "value": name},
            "archetype_node_id": at,
            "value": {"_type": "DV_TEXT", "value": text}
        })
    };
    let added = add_findings(
        &mut canonical,
        &[
            element(
                "at0014",
                "Clinical summary",
                "A synthetic clinical summary.",
            ),
            element("at0008", "Imaging findings", "Synthetic findings."),
            element("at0056", "Comparison findings", "A synthetic comparison."),
            element(
                "at0058",
                "Imaging differential diagnosis",
                "A synthetic differential",
            ),
            element("at0059", "Recommendation", "A synthetic recommendation"),
            element("at0023", "Comment", "A synthetic comment"),
        ],
    );
    assert!(added, "the composition holds the result data");
    let (run, warnings) = mapping()?.to_fhir_with_warnings(
        &canonical.to_string(),
        &Settings::new("Device/ferrobridge-eehrxf-test", NOW),
        CallContext::new().with_patient(reference("Patient/synthetic-patient")),
    )?;
    let mut unread: Vec<String> = warnings
        .iter()
        .filter_map(|warning| match *warning.loss() {
            Loss::Unread { ref node } => Some(node.clone()),
            _ => None,
        })
        .collect();
    unread.sort();
    assert_eq!(
        unread,
        ["at0008", "at0014", "at0023", "at0056", "at0058", "at0059"],
        "{warnings:?}"
    );
    let bundle = serde_json::to_value(run.bundle())?;
    let outcome = only(&bundle, "OperationOutcome")?;
    assert_eq!(
        outcome["issue"].as_array().map(Vec::len),
        Some(6),
        "the run's outcome names each: {outcome}"
    );
    Ok(())
}

/// The expressions of the issues the run over the received `text` declares
/// not carried, in the order the run declares them.
fn not_carried(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    not_carried_by(&mapping()?, text)
}

/// The expressions of [`not_carried`], the run made by `mapping`.
fn not_carried_by(mapping: &Mapping, text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    expressions_by(mapping, text, |issue| {
        issue["details"]["text"]
            .as_str()
            .is_some_and(|text| text.contains("no structured element"))
    })
}

// NOTE: HL7 Europe Imaging Report 1.0.0-ballot DiagnosticReportEuImaging.performer admits a
// Practitioner beside the organization slice; the imaging service slot holds an organisation alone.
#[test]
fn a_performer_that_is_no_organization_is_declared_and_the_organization_carried()
-> Result<(), Box<dyn Error>> {
    let mut performed = report();
    performed["performer"] = serde_json::json!([
        {"reference": "Organization/synthetic-radiology"},
        {"reference": "Practitioner/synthetic-radiologist"}
    ]);
    let text = received_text_with(
        &performed,
        Some(Practitioner {
            id: Some(String::from("synthetic-radiologist")),
            ..Practitioner::default()
        }),
    )?;
    let declared = not_carried(&text)?;
    assert!(
        declared.contains(&String::from("DiagnosticReport.performer[1]")),
        "the practitioner is declared not carried: {declared:?}"
    );
    assert!(
        !declared.contains(&String::from("DiagnosticReport.performer[0]")),
        "the organization is carried: {declared:?}"
    );
    Ok(())
}

// NOTE: no specification governs this: our own design; a gateway that compiles the shipped files
// from disk, as FerroFED's receive path does, declares what the compiled project declares.
#[test]
fn a_report_mapped_by_the_shipped_files_declares_what_the_project_declares()
-> Result<(), Box<dyn Error>> {
    let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("eehrxf-imaging-files");
    std::fs::create_dir_all(&directory)?;
    let files = IMAGING.write_to(&directory)?;
    let from_files = Mapping::compile_template(&template()?, &files, &[CONTEXT])?;
    let text = received_text()?;
    let declared = not_carried_by(&from_files, &text)?;
    assert!(
        !declared.is_empty(),
        "the files declare the report's losses"
    );
    assert_eq!(
        declared,
        not_carried(&text)?,
        "the files declare what the compiled project declares, and nothing the composition carries"
    );
    Ok(())
}

/// The path of the section of the received `document` coded `code`.
fn section_at(document: &Value, code: &str) -> Result<String, Box<dyn Error>> {
    let index = only(document, "Composition")?["section"]
        .as_array()
        .ok_or("the sections")?
        .iter()
        .position(|section| section["code"]["coding"][0]["code"] == code)
        .ok_or_else(|| format!("the section {code}"))?;
    Ok(format!("Composition.section[{index}]"))
}

/// The path of the element of the received report's `member` list whose
/// `field` is `value`.
fn report_member_at(
    document: &Value,
    member: &str,
    field: &str,
    value: &str,
) -> Result<String, Box<dyn Error>> {
    let index = only(document, "DiagnosticReport")?[member]
        .as_array()
        .ok_or_else(|| format!("the report's {member}"))?
        .iter()
        .position(|held| held.pointer(field).and_then(Value::as_str) == Some(value))
        .ok_or_else(|| format!("the report's {member} {value}"))?;
    Ok(format!("DiagnosticReport.{member}[{index}]"))
}

// NOTE: CompositionEuImaging requires the order, history and procedure sections, which the
// template holds no section for, and DiagnosticReportEuImaging the composition link and two categories.
#[test]
fn a_received_report_declares_each_section_and_element_it_does_not_carry()
-> Result<(), Box<dyn Error>> {
    let text = received_text()?;
    let document: Value = serde_json::from_str(&text)?;
    let declared = not_carried(&text)?;
    assert_eq!(
        declared,
        [
            section_at(&document, "55115-0")?,
            section_at(&document, "11329-0")?,
            section_at(&document, "55111-9")?,
            report_member_at(
                &document,
                "extension",
                "/url",
                "http://hl7.org/fhir/5.0/StructureDefinition/extension-DiagnosticReport.composition",
            )?,
            report_member_at(&document, "category", "/coding/0/code", "85430-7")?,
            report_member_at(&document, "category", "/coding/0/code", "Medical-Imaging")?,
        ],
        "each required section with no entry, the report's link and categories, and nothing the \
         template carries"
    );
    assert!(
        !declared.contains(&section_at(&document, "18726-0")?),
        "the imaging study section lists the carried study"
    );
    Ok(())
}

#[test]
fn each_report_and_study_element_and_narrative_section_is_declared() -> Result<(), Box<dyn Error>> {
    let mut document: Value = serde_json::from_str(&received_text()?)?;
    let entries = document["entry"].as_array_mut().ok_or("the entries")?;
    for entry in entries.iter_mut() {
        let resource = &mut entry["resource"];
        match resource["resourceType"].as_str() {
            Some("Composition") => resource["section"]
                .as_array_mut()
                .ok_or("the sections")?
                .push(serde_json::json!({
                    "title": "Findings",
                    "code": {"coding": [{"system": "http://loinc.org", "code": "59776-5"}]},
                    "text": {
                        "status": "generated",
                        "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">Synthetic findings.</div>"
                    }
                })),
            Some("DiagnosticReport") => {
                resource["issued"] = serde_json::json!("2026-03-02T09:00:00+01:00");
                resource["resultsInterpreter"] =
                    serde_json::json!([{"reference": "Organization/synthetic-radiology"}]);
            }
            Some("ImagingStudy") => {
                resource["numberOfSeries"] = serde_json::json!(1);
                resource["note"] = serde_json::json!([{"text": "A synthetic study note."}]);
            }
            _ => {}
        }
    }
    let findings = section_at(&document, "59776-5")?;
    let declared = not_carried(&serde_json::to_string(&document)?)?;
    for expression in [
        findings.as_str(),
        "DiagnosticReport.issued",
        "DiagnosticReport.resultsInterpreter[0]",
        "ImagingStudy.numberOfSeries",
        "ImagingStudy.note[0]",
    ] {
        assert!(
            declared.iter().any(|held| held == expression),
            "{expression} is declared: {declared:?}"
        );
    }
    for carried in [
        "DiagnosticReport.status",
        "DiagnosticReport.code",
        "DiagnosticReport.conclusion",
        "ImagingStudy.identifier",
        "ImagingStudy.status",
        "ImagingStudy.modality",
        "ImagingStudy.started",
        "ImagingStudy.description",
    ] {
        assert!(
            !declared.iter().any(|held| held.starts_with(carried)),
            "{carried} is carried, not declared: {declared:?}"
        );
    }
    Ok(())
}
