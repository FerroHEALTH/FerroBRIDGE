// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory report mapped between openEHR and the HL7 Europe
//! Laboratory Report 2.0.0 document through the shipped laboratory project,
//! over the published `KDS_Laborbericht` template.
//!
//! A synthetic `Bundle-eu-lab` is received, held to the vendored `Bundle`
//! and `Composition` profiles, and mapped into one openEHR composition; the
//! composition is mapped back out, and its report, results, specimens and
//! `Provenance` become a new document, received and held to the same
//! profiles.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::path::PathBuf;

use eehrxf::category::Category;
use eehrxf::document::laboratory::{LAB_BUNDLE, LAB_SLOTS, LABORATORY_REPORT_FORM};
use eehrxf::document::{Document, DocumentError, Section};
use eehrxf::mapping::Mapping;
use eehrxf::mappings::LABORATORY;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::category::Uncategorised;
use ferrobridge_testkit::fixtures::KDS_LABORBERICHT_OPT;
use fhir_types::codec::{Json, Path};
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::diagnostic_report::DiagnosticReport;
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::operations::run::Settings;
use serde_json::Value;

use super::support::{emit, lab};

/// The base the test documents name their entries under, in the
/// `example.org` domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The canonical URL of the laboratory `Composition` profile.
const LAB_COMPOSITION: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/Composition-eu-lab";

/// The canonical URL of the laboratory report profile.
const DIAGNOSTIC_REPORT_EU_LAB: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/DiagnosticReport-eu-lab";

/// The canonical URL of the laboratory result profile.
const OBSERVATION_RESULTSLAB_EU_LAB: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/Observation-resultslab-eu-lab";

/// The canonical URL of the laboratory specimen profile.
const SPECIMEN_EU_LAB: &str = "http://hl7.eu/fhir/laboratory/StructureDefinition/Specimen-eu-lab";

/// The instant every run is stamped with, so nothing reads a clock.
const NOW: &str = "2026-03-02T10:00:00Z";

/// The paths of the shipped laboratory project's files.
fn files() -> Vec<PathBuf> {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("mappings")
        .join(LABORATORY.directory());
    LABORATORY
        .files()
        .iter()
        .map(|file| project.join(file.name))
        .collect()
}

/// The shipped laboratory project compiled against `KDS_Laborbericht`.
fn mapping() -> Result<Mapping, Box<dyn Error>> {
    Ok(Mapping::compile(
        KDS_LABORBERICHT_OPT,
        &files(),
        LABORATORY.contexts(),
    )?)
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

/// A synthetic serum specimen in the shape `Specimen-eu-lab` constrains.
fn specimen() -> Value {
    serde_json::json!({
        "resourceType": "Specimen",
        "id": "synthetic-serum",
        "meta": {"profile": [SPECIMEN_EU_LAB]},
        "accessionIdentifier": {
            "system": "http://example.org/fhir/sid/ferrobridge-specimens",
            "value": "synthetic-specimen-0001"
        },
        "type": {"coding": [{
            "system": "http://snomed.info/sct",
            "code": "119364003",
            "display": "Serum specimen"
        }]},
        "subject": {"reference": "Patient/synthetic-patient"},
        "receivedTime": "2026-03-02T07:40:00+01:00",
        "collection": {"collectedDateTime": "2026-03-02T07:05:00+01:00"}
    })
}

/// A synthetic quantitative result in the shape
/// `Observation-resultslab-eu-lab` constrains.
fn result() -> Value {
    serde_json::json!({
        "resourceType": "Observation",
        "id": "synthetic-cholesterol",
        "meta": {"profile": [OBSERVATION_RESULTSLAB_EU_LAB]},
        "status": "final",
        "category": [{"coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/observation-category",
            "code": "laboratory"
        }]}],
        "code": {"coding": [{
            "system": "http://loinc.org",
            "code": "2093-3",
            "display": "Cholesterol [Mass/volume] in Serum or Plasma"
        }]},
        "subject": {"reference": "Patient/synthetic-patient"},
        "effectiveDateTime": "2026-03-02T08:15:00+01:00",
        "valueQuantity": {
            "value": 182,
            "unit": "mg/dL",
            "system": "http://unitsofmeasure.org",
            "code": "mg/dL"
        }
    })
}

/// A synthetic report in the shape `DiagnosticReport-eu-lab` constrains,
/// naming the result and the specimen.
fn report() -> Value {
    serde_json::json!({
        "resourceType": "DiagnosticReport",
        "id": "synthetic-report",
        "meta": {"profile": [DIAGNOSTIC_REPORT_EU_LAB]},
        "identifier": [{
            "system": "http://example.org/fhir/sid/ferrobridge-lab-reports",
            "value": "synthetic-report-0001"
        }],
        "status": "final",
        "code": {"coding": [{
            "system": "http://loinc.org",
            "code": "24331-1",
            "display": "Lipid panel - Serum or Plasma"
        }]},
        "subject": {"reference": "Patient/synthetic-patient"},
        "effectiveDateTime": "2026-03-02T08:15:00+01:00",
        "specimen": [{"reference": "Specimen/synthetic-serum"}],
        "result": [{"reference": "Observation/synthetic-cholesterol"}],
        "conclusion": "A synthetic conclusion."
    })
}

/// A laboratory report document, named `identifier`, of the synthetic
/// patient, by the synthetic gateway and operator.
fn lab_document(identifier: &str) -> Result<(Document, String, String), Box<dyn Error>> {
    let mut document = Document::of(
        &LABORATORY_REPORT_FORM,
        BASE,
        identifier.to_owned(),
        String::from(NOW),
        String::from("Laboratory report"),
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

/// The chemistry section listing `results`.
fn chemistry(results: Vec<String>) -> Result<Section, Box<dyn Error>> {
    let slot = LAB_SLOTS
        .iter()
        .find(|slot| slot.code == "18719-5")
        .ok_or("the chemistry slot")?;
    let mut section = Section::new(slot, vec![String::from("Lipid panel.")]);
    for result in results {
        section = section.with_entry(result);
    }
    Ok(section)
}

/// The synthetic laboratory report document a member sends.
fn received_text() -> Result<String, Box<dyn Error>> {
    let (mut document, _, _) = lab_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000b001")?;
    let urls = document.add(&[
        resource("Observation", &result())?,
        resource("Specimen", &specimen())?,
    ])?;
    let result_url = urls.first().ok_or("the result's fullUrl")?.clone();
    document.with_section(chemistry(vec![result_url])?)?;
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
            "value": "5b0c3f7e-0000-4000-8000-00000000c001::ferrobridge.example::1"
        }),
    );
    Ok(composition.to_string())
}

/// Maps the received document into openEHR, and the composition back out
/// into a new document, answering both documents' text.
fn round_trip() -> Result<(String, String), Box<dyn Error>> {
    let mapping = mapping()?;
    let received = received_text()?;
    let document = ReceivedDocument::read(&received)?;
    let composition = mapping.to_openehr(&document, &settings())?;
    assert_eq!(composition.template_id(), "KDS_Laborbericht");
    let (mut out, subject, device) = lab_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000b002")?;
    let context = CallContext::new()
        .with_patient(reference(&subject))
        .with_who(reference(&device));
    let run = mapping.to_fhir_with(
        &committed(composition.composition())?,
        &Settings::new(device.clone(), NOW),
        context,
    )?;
    let results = out.with_mapped_report(run.bundle())?;
    out.with_section(chemistry(results)?)?;
    Ok((received, serde_json::to_string(&out.into_bundle()?)?))
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

#[test]
fn the_shipped_project_compiles_against_the_published_template() -> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    let contexts: Vec<_> = mapping
        .contexts()
        .map(|context| (context.name, context.template, context.profile))
        .collect();
    assert_eq!(
        contexts,
        [(
            "ferrobridge_eu_laboratory.context",
            "KDS_Laborbericht",
            DIAGNOSTIC_REPORT_EU_LAB
        )]
    );
    Ok(())
}

#[test]
fn a_received_laboratory_report_is_of_its_category_and_conforms() -> Result<(), Box<dyn Error>> {
    let text = received_text()?;
    let document = ReceivedDocument::read(&text)?;
    let category = document.category()?;
    assert_eq!(category, Category::Laboratory);
    let profiles = category.profiles().ok_or("the category is received")?;
    assert_eq!(profiles.bundle, LAB_BUNDLE);
    assert_eq!(profiles.composition, LAB_COMPOSITION);
    document.check(&lab(profiles.bundle)?, &lab(profiles.composition)?)?;
    Ok(())
}

#[test]
fn a_received_laboratory_report_maps_into_one_composition_that_keeps_it()
-> Result<(), Box<dyn Error>> {
    let text = received_text()?;
    let document = ReceivedDocument::read(&text)?;
    let composition = mapping()?.to_openehr(&document, &settings())?;
    assert_eq!(composition.template_id(), "KDS_Laborbericht");
    let canonical: Value = serde_json::from_str(composition.composition())?;
    let written = canonical.to_string();
    for value in ["A synthetic conclusion.", "synthetic-report-0001", "2093-3"] {
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
    assert_eq!(document.category()?, Category::Laboratory);
    document.check(&lab(LAB_BUNDLE)?, &lab(LAB_COMPOSITION)?)?;
    emit("eehrxf-laboratory-mapped", &written)?;

    let input: Value = serde_json::from_str(&received)?;
    let output: Value = serde_json::from_str(&written)?;
    let (before, after) = (
        only(&input, "DiagnosticReport")?,
        only(&output, "DiagnosticReport")?,
    );
    assert_eq!(
        after["meta"]["profile"],
        serde_json::json!([DIAGNOSTIC_REPORT_EU_LAB]),
        "the report claims its profile once"
    );
    for pointer in [
        "/status",
        "/code/coding/0",
        "/effectiveDateTime",
        "/conclusion",
    ] {
        assert_eq!(after.pointer(pointer), before.pointer(pointer), "{pointer}");
    }
    let result = only(&output, "Observation")?;
    assert_eq!(
        result["meta"]["profile"][0], OBSERVATION_RESULTSLAB_EU_LAB,
        "{result}"
    );
    // NOTE: HL7 Europe Laboratory Report 2.0.0 Observation-resultslab-eu-lab requires
    // effective[x]; an analyte takes the time of its laboratory test result's event.
    assert_eq!(
        result.pointer("/effectiveDateTime"),
        after.pointer("/effectiveDateTime"),
        "{result}"
    );
    assert_eq!(
        result.pointer("/valueQuantity/value"),
        only(&input, "Observation")?.pointer("/valueQuantity/value")
    );
    let specimen = only(&output, "Specimen")?;
    assert_eq!(specimen["meta"]["profile"][0], SPECIMEN_EU_LAB);
    assert_eq!(
        specimen.pointer("/type/coding/0/code"),
        only(&input, "Specimen")?.pointer("/type/coding/0/code")
    );
    let composition = only(&output, "Composition")?;
    let section = &composition["section"][0];
    let result_url = output["entry"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .find(|entry| entry["resource"]["resourceType"] == "Observation")
        .and_then(|entry| entry["fullUrl"].as_str())
        .ok_or("the result's fullUrl")?;
    assert_eq!(
        section["entry"][0]["reference"].as_str(),
        Some(result_url),
        "the section lists the mapped result"
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
    let mut mapped: Vec<String> = ["DiagnosticReport", "Observation", "Specimen"]
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

#[test]
fn a_run_with_no_report_is_refused() -> Result<(), Box<dyn Error>> {
    let (mut document, _, _) = lab_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000b003")?;
    let refused = document.with_mapped_report(&Bundle::default());
    assert!(
        matches!(refused, Err(DocumentError::Unmapped { found: 0 })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_second_mapped_report_is_refused() -> Result<(), Box<dyn Error>> {
    let (mut document, _, _) = lab_document("urn:uuid:5b0c3f7e-0000-4000-8000-00000000b004")?;
    document.with_report(DiagnosticReport::default())?;
    let refused = document.with_mapped_report(&Bundle::default());
    assert!(
        matches!(
            refused,
            Err(DocumentError::Report {
                expected: "takes one"
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_document_typed_otherwise_is_no_laboratory_report() -> Result<(), Box<dyn Error>> {
    let text = received_text()?.replace("11502-2", "11502-9");
    let document = ReceivedDocument::read(&text)?;
    assert_eq!(document.category(), Err(Uncategorised::Unknown));
    Ok(())
}

// NOTE: KDS_Laborbericht holds one report identifier (other_context at0002, 1..1), and the
// received report carries its own beside the document's (Bundle-eu-lab dr-comp-identifier).
#[test]
fn the_report_identifier_the_template_holds_one_of_is_declared_lost() -> Result<(), Box<dyn Error>>
{
    let document = ReceivedDocument::read(&received_text()?)?;
    let composition = mapping()?.to_openehr(&document, &settings())?;
    let outcome = composition.outcome().ok_or("the run declared its losses")?;
    let outcome = serde_json::to_value(outcome)?;
    let declared = outcome["issue"]
        .as_array()
        .ok_or("the outcome's issues")?
        .iter()
        .any(|issue| {
            issue["severity"] == "warning"
                && issue["expression"][0] == "DiagnosticReport.identifier.value"
        });
    assert!(declared, "the dropped identifier is declared: {outcome}");
    Ok(())
}

// NOTE: FHIR R4 extension-data-absent-reason.html: KDS_Laborbericht holds no performing
// laboratory, so the result's required performer is written absent and the run declares it.
#[test]
fn the_performer_the_template_holds_none_of_is_written_absent_and_declared()
-> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    let document = ReceivedDocument::read(&received_text()?)?;
    let composition = mapping.to_openehr(&document, &settings())?;
    let (run, warnings) = mapping.to_fhir_with_warnings(
        &committed(composition.composition())?,
        &Settings::new("Device/ferrobridge-eehrxf-test", NOW),
        CallContext::new(),
    )?;
    let bundle = serde_json::to_value(run.bundle())?;
    let result = only(&bundle, "Observation")?;
    assert_eq!(
        result.pointer("/performer/0"),
        Some(&serde_json::json!({"extension": [{
            "url": "http://hl7.org/fhir/StructureDefinition/data-absent-reason",
            "valueCode": "unknown"
        }]})),
        "{result}"
    );
    let declared = warnings.iter().any(|warning| {
        warning.loss()
            == &Loss::Absent {
                reason: String::from("unknown"),
            }
            && warning.location().fhir() == Some("Observation.performer[0]")
    });
    assert!(
        declared,
        "the absent performer is a declared loss: {warnings:?}"
    );
    let outcome = only(&bundle, "OperationOutcome")?;
    assert!(
        outcome["issue"]
            .as_array()
            .ok_or("the outcome's issues")?
            .iter()
            .any(|issue| issue["expression"][0] == "Observation.performer[0]"),
        "the Bundle's outcome names it: {outcome}"
    );
    Ok(())
}
