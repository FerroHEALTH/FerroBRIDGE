// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The electronic prescription and dispensation mapped between openEHR and
//! the HL7 Europe MPD 1.0.0 resources through FerroBRIDGE's FHIRconnect
//! contexts, beside the native records of `medication`.
//!
//! The mapping is FerroBRIDGE's MPD project over its ADL 2 medication
//! template, whose operational template the suite builds from the vendored
//! CKM archetypes. Each case receives a synthetic `collection` Bundle into
//! one openEHR composition, maps the composition back out, and reads the
//! resource it answers as the native record, held to the vendored profiles.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::path::PathBuf;

use eehrxf::mapping::Mapping;
use eehrxf::mapping::identified;
use eehrxf::mappings::{MPD, UnknownContext};
use eehrxf::medication::Medicinal;
use eehrxf::medication::dispensation::Dispense;
use eehrxf::medication::prescription::Prescription;
use eehrxf::receive::ReceiveError;
use eehrxf::receive::ReceivedResources;
use eehrxf::receive::openehr::FHIR_JSON;
use eehrxf::receive::openehr::InstructionError;
use ferrobridge_testkit::fixtures::CKM_MEDICATION_ARCHETYPES;
use ferrobridge_testkit::fixtures::FERROBRIDGE_MEDICATION_TEMPLATE;
use ferrobridge_testkit::fixtures::FERROBRIDGE_MEDICATION_TEMPLATE_ID;
use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::traverse::defaults::Defaults;
use fhirconnect::operations::run::Settings;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::parse::Dialect;
use openehr_base::v1_3::base_types::identification::locatable_ref::LocatableRef;
use openehr_mapping_core::template::TemplateSource;

use super::fixtures::profiles;
use crate::support::emit;

/// The request context of FerroBRIDGE's MPD project.
const REQUEST: &str = "ferrobridge_eu_mpd_request.context";

/// The dispense context of FerroBRIDGE's MPD project.
const DISPENSE: &str = "ferrobridge_eu_mpd_dispense.context";

/// The base the written collections name their entries under, in the
/// `example.org` domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://ehr.example.org/fhir";

/// The remaining-dispensations extension of `eehrxf`.
const REMAINING: &str = "https://ferrobridge.eu/fhir/StructureDefinition/remaining-dispensations";

/// The paths of the shipped MPD project's files.
fn files() -> Vec<PathBuf> {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("mappings")
        .join(MPD.directory());
    MPD.files()
        .iter()
        .map(|file| project.join(file.name))
        .collect()
}

/// The ADL 2 operational template of the medication template.
fn template() -> Result<TemplateSource, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    for (file, source) in CKM_MEDICATION_ARCHETYPES {
        let archetype = openehr_adl::assemble::parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    let template =
        openehr_adl::assemble::parse_artefact(FERROBRIDGE_MEDICATION_TEMPLATE, Dialect::Adl2)
            .map_err(|errors| format!("the template: {errors:?}"))?;
    let raw = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
        resolved_id: String::from(FERROBRIDGE_MEDICATION_TEMPLATE_ID),
    })
}

/// The mapping of the one context `context`, compiled from the embedded
/// project.
fn mapping(context: &str) -> Result<Mapping, Box<dyn Error>> {
    Ok(Mapping::compile_project(
        &template()?,
        &MPD.with_context(context)?,
    )?)
}

/// The run settings: a synthetic engine device, a fixed instant and the
/// composition defaults a received collection does not state.
fn settings() -> Settings {
    let now = "2026-10-06T10:00:00Z";
    Settings::new("Device/ferrobridge-eehrxf-test", now)
        .with_defaults(Defaults::at(now).with_language("en").with_territory("NL"))
}

/// A synthetic `Medication-eu-mpd`: its code, its form and its batch.
fn medication() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Medication",
        "id": "synthetic-product",
        "meta": {"profile": ["http://hl7.eu/fhir/mpd/StructureDefinition/Medication-eu-mpd"]},
        "code": {"coding": [{
            "system": "http://example.org/fhir/CodeSystem/ferrobridge-products",
            "code": "SYN-PRODUCT-0001",
            "display": "Synthetic tablet 500 mg"
        }]},
        "form": {"coding": [{
            "system": "http://example.org/fhir/CodeSystem/ferrobridge-forms",
            "code": "SYN-TABLET",
            "display": "Synthetic tablet"
        }]}
    })
}

/// A synthetic `MedicationRequest-eu-mpd` that authorises two refills, 28
/// days apart, with one left, and 56 tablets for one fill.
fn request() -> serde_json::Value {
    // DV_DURATION.adoc gives a Duration's `unit` no openEHR row, so a duration carries none.
    serde_json::json!({
        "resourceType": "MedicationRequest",
        "meta": {"profile": ["http://hl7.eu/fhir/mpd/StructureDefinition/MedicationRequest-eu-mpd"]},
        "identifier": [{
            "system": "http://example.org/fhir/sid/ferrobridge-prescriptions",
            "value": "synthetic-prescription-0001"
        }],
        "status": "unknown",
        "intent": "order",
        "medicationReference": {"reference": "Medication/synthetic-product"},
        "subject": {"reference": "Patient/synthetic-patient"},
        "authoredOn": "2026-03-02T09:00:00+01:00",
        "requester": {"display": "Synthetic Prescriber"},
        "dosageInstruction": [{
            "sequence": 1,
            "text": "One tablet twice a day"
        }],
        "dispenseRequest": {
            "extension": [{"url": REMAINING, "valueUnsignedInt": 1}],
            "validityPeriod": {
                "start": "2026-03-02T00:00:00+01:00",
                "end": "2026-06-02T00:00:00+02:00"
            },
            "numberOfRepeatsAllowed": 2,
            "dispenseInterval": {
                "value": 28,
                "system": "http://unitsofmeasure.org",
                "code": "d"
            },
            "quantity": {
                "value": 56,
                "unit": "tablet",
                "system": "http://unitsofmeasure.org",
                "code": "{tbl}"
            }
        }
    })
}

/// A synthetic `MedicationDispense-eu-mpd` of 20 tablets.
fn dispense() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "MedicationDispense",
        "meta": {"profile": ["http://hl7.eu/fhir/mpd/StructureDefinition/MedicationDispense-eu-mpd"]},
        "extension": [{
            "url": "http://hl7.org/fhir/5.0/StructureDefinition/extension-MedicationDispense.recorded",
            "valueDateTime": "2026-03-03T10:00:00+01:00"
        }],
        "status": "completed",
        "medicationReference": {"reference": "Medication/synthetic-product"},
        "subject": {"reference": "Patient/synthetic-patient"},
        "performer": [{"actor": {"display": "Synthetic Pharmacist"}}],
        "quantity": {
            "value": 20,
            "unit": "tablet",
            "system": "http://unitsofmeasure.org",
            "code": "{tbl}"
        },
        "whenHandedOver": "2026-03-03T10:05:00+01:00"
    })
}

/// The `collection` Bundle of `resource` and the product it references.
fn collection(resource: &serde_json::Value) -> String {
    let name = resource
        .get("resourceType")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Resource");
    serde_json::json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [
            {"fullUrl": format!("http://example.org/fhir/{name}/synthetic"), "resource": resource},
            {"fullUrl": "http://example.org/fhir/Medication/synthetic-product", "resource": medication()}
        ]
    })
    .to_string()
}

/// Receives `resource` through `context` and maps the composition back out,
/// answering the composition and the resources of the Bundle out.
fn round_trip(
    context: &str,
    resource: &serde_json::Value,
) -> Result<(String, Vec<Object>), Box<dyn Error>> {
    round_trip_through(&mapping(context)?, resource)
}

/// Receives `resource` through `mapping` and maps the composition back out,
/// as [`round_trip`] does.
fn round_trip_through(
    mapping: &Mapping,
    resource: &serde_json::Value,
) -> Result<(String, Vec<Object>), Box<dyn Error>> {
    let received = ReceivedResources::read(&collection(resource))?;
    let composition = mapping.resources_to_openehr(&received, &settings())?;
    let committed = committed(composition.composition())?;
    // FHIRconnect 1.0.0 `$tofhir` context: the caller names the patient the record is of.
    let context = CallContext::new().with_patient(Reference {
        reference: Some("Patient/synthetic-patient".into()),
        ..Reference::default()
    });
    let answer = mapping.to_fhir_with(&committed, &settings(), context)?;
    let resources = answer
        .bundle()
        .entry
        .iter()
        .filter_map(|entry| entry.resource.as_ref())
        .map(Json::to_json)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((composition.composition().to_owned(), resources))
}

/// Receives `resource` through `context`, maps the composition back out and
/// writes the `collection` Bundle the mapping answers to the directory
/// `EEHRXF_EMIT_DIR` names as `name`, for a run of HL7's FHIR Validator.
fn emit_mapped(
    context: &str,
    resource: &serde_json::Value,
    name: &str,
) -> Result<(), Box<dyn Error>> {
    let mapping = mapping(context)?;
    let received = ReceivedResources::read(&collection(resource))?;
    let composition = mapping.resources_to_openehr(&received, &settings())?;
    let committed = committed(composition.composition())?;
    let context = CallContext::new().with_patient(Reference {
        reference: Some("Patient/synthetic-patient".into()),
        ..Reference::default()
    });
    let answer = mapping.to_fhir_with(&committed, &settings(), context)?;
    let collection = identified(answer.bundle(), BASE)?;
    assert!(
        collection
            .entry
            .iter()
            .all(|entry| entry.full_url.is_some()),
        "R4 Bundle.entry.fullUrl: every entry of an exchanged collection is named"
    );
    emit(name, &serde_json::to_string(&collection)?)
}

/// Returns `composition` as a CDR serves it once committed, with the version
/// id a mapped resource's id derives from.
fn committed(composition: &str) -> Result<String, Box<dyn Error>> {
    let mut canonical: serde_json::Value = serde_json::from_str(composition)?;
    let object = canonical
        .as_object_mut()
        .ok_or("the composition is an object")?;
    object.insert(
        String::from("uid"),
        serde_json::json!({
            "_type": "OBJECT_VERSION_ID",
            "value": "5f0c2a9e-0000-4000-8000-000000000001::synthetic.example.org::1"
        }),
    );
    Ok(canonical.to_string())
}

/// The one resource of type `name` among `resources`.
fn the_one<'a>(resources: &'a [Object], name: &str) -> Result<&'a Object, Box<dyn Error>> {
    let found: Vec<&Object> = resources
        .iter()
        .filter(|object| {
            object
                .get("resourceType")
                .and_then(fhir_types::codec::Value::as_str)
                == Some(name)
        })
        .collect();
    let [one] = found.as_slice() else {
        return Err(format!("{} {name} resources, not one", found.len()).into());
    };
    Ok(one)
}

/// The FHIR JSON text of `object`.
fn text(object: &Object) -> Result<String, Box<dyn Error>> {
    Ok(fhir_types::codec::Value::Object(object.clone())
        .to_serde_json(&mut fhir_types::codec::Path::root("Resource"))?
        .to_string())
}

#[test]
fn a_mapped_prescription_reads_as_the_native_record() -> Result<(), Box<dyn Error>> {
    let (_, resources) = round_trip(REQUEST, &request())?;
    let request = the_one(&resources, "MedicationRequest")?;
    let prescription = Prescription::read(&[&text(request)?])?;
    let [item] = prescription.items.as_slice() else {
        return Err("one prescription item".into());
    };
    assert_eq!(item.status, "unknown");
    assert_eq!(item.repeats, Some(2), "the maximum number of refills");
    assert_eq!(
        item.remaining_dispensations,
        Some(1),
        "the number of refills remaining"
    );
    assert_eq!(
        item.minimum_dispense_interval
            .as_ref()
            .and_then(|interval| interval.value.as_ref())
            .and_then(|value| value.value.as_deref()),
        Some("28"),
        "the minimum interval between refills"
    );
    assert_eq!(
        prescription
            .header
            .identifier
            .first()
            .and_then(|identifier| identifier.value.as_ref())
            .and_then(|value| value.value.as_deref()),
        Some("synthetic-prescription-0001")
    );
    assert!(
        matches!(item.medication, Medicinal::Referenced(_)),
        "the request references the product it created"
    );
    Ok(())
}

#[test]
fn a_mapped_prescription_carries_the_dispense_amount_of_one_fill() -> Result<(), Box<dyn Error>> {
    // R4 MedicationRequest.dispenseRequest.quantity, "the amount that is to be dispensed for one
    // fill", from the CKM dispense amount; the Xt-EHR item carries no per-fill amount.
    let (_, resources) = round_trip(REQUEST, &request())?;
    let request = the_one(&resources, "MedicationRequest")?;
    let quantity = request
        .get("dispenseRequest")
        .and_then(|dispense| dispense.get("quantity"))
        .cloned()
        .map(|quantity| quantity.to_serde_json(&mut fhir_types::codec::Path::root("Quantity")))
        .transpose()?;
    assert_eq!(
        quantity.as_ref().and_then(|quantity| quantity.get("value")),
        Some(&serde_json::json!(56))
    );
    Ok(())
}

#[test]
fn a_mapped_prescription_conforms_to_mpd() -> Result<(), Box<dyn Error>> {
    let mpd = profiles()?;
    let (_, resources) = round_trip(REQUEST, &request())?;
    let prescription = Prescription::read(&[&text(the_one(&resources, "MedicationRequest")?)?])?;
    for request in prescription.to_json()? {
        mpd.check_request(&request)?;
    }
    Ok(())
}

#[test]
fn a_mapped_prescription_has_one_provenance() -> Result<(), Box<dyn Error>> {
    let (_, resources) = round_trip(REQUEST, &request())?;
    the_one(&resources, "Provenance")?;
    the_one(&resources, "Medication")?;
    Ok(())
}

#[test]
fn a_received_dispense_is_one_composition_that_keeps_the_collection() -> Result<(), Box<dyn Error>>
{
    let mapping = mapping(DISPENSE)?;
    let text = collection(&dispense());
    let received = ReceivedResources::read(&text)?;
    let composition = mapping.resources_to_openehr(&received, &settings())?;
    assert_eq!(
        composition.template_id(),
        FERROBRIDGE_MEDICATION_TEMPLATE_ID
    );
    let canonical: serde_json::Value = serde_json::from_str(composition.composition())?;
    assert_eq!(
        canonical
            .pointer("/feeder_audit/original_content/formalism")
            .and_then(serde_json::Value::as_str),
        Some(FHIR_JSON)
    );
    assert_eq!(
        canonical
            .pointer("/feeder_audit/original_content/value")
            .and_then(serde_json::Value::as_str),
        Some(text.as_str()),
        "the collection is kept as received"
    );
    assert_eq!(
        canonical
            .pointer("/content/0/ism_transition/careflow_step/defining_code/code_string")
            .and_then(serde_json::Value::as_str),
        Some("at0003"),
        "the dispense is the ACTION at its dispensed careflow step"
    );
    Ok(())
}

#[test]
fn a_mapped_dispense_reads_as_the_native_record() -> Result<(), Box<dyn Error>> {
    let mpd = profiles()?;
    let (_, resources) = round_trip(DISPENSE, &dispense())?;
    let dispense = Dispense::read(&text(the_one(&resources, "MedicationDispense")?)?)?;
    mpd.check_dispense(&dispense.to_json()?)?;
    the_one(&resources, "Provenance")?;
    Ok(())
}

/// A synthetic reference to the medication order of a versioned composition.
fn order() -> Result<LocatableRef, Box<dyn Error>> {
    Ok(serde_json::from_value(serde_json::json!({
        "_type": "LOCATABLE_REF",
        "namespace": "local",
        "type": "INSTRUCTION",
        "id": {
            "_type": "OBJECT_VERSION_ID",
            "value": "8849182c-82ad-4088-a07f-48ead4180515::synthetic.example.org::1"
        },
        "path": "/content[openEHR-EHR-INSTRUCTION.medication_order.v3]"
    }))?)
}

#[test]
fn a_received_dispense_names_the_order_it_carries_out() -> Result<(), Box<dyn Error>> {
    let mapping = mapping(DISPENSE)?;
    let received = ReceivedResources::read(&collection(&dispense()))?;
    let composition = mapping
        .resources_to_openehr(&received, &settings())?
        .instructed_by(&order()?, "activities[at0001]")?;
    let canonical: serde_json::Value = serde_json::from_str(composition.composition())?;
    assert_eq!(
        canonical
            .pointer("/content/0/instruction_details/instruction_id/id/value")
            .and_then(serde_json::Value::as_str),
        Some("8849182c-82ad-4088-a07f-48ead4180515::synthetic.example.org::1")
    );
    assert_eq!(
        canonical
            .pointer("/content/0/instruction_details/activity_id")
            .and_then(serde_json::Value::as_str),
        Some("activities[at0001]")
    );
    Ok(())
}

#[test]
fn an_order_named_twice_is_refused() -> Result<(), Box<dyn Error>> {
    let mapping = mapping(DISPENSE)?;
    let received = ReceivedResources::read(&collection(&dispense()))?;
    let once = mapping
        .resources_to_openehr(&received, &settings())?
        .instructed_by(&order()?, "activities[at0001]")?;
    assert!(matches!(
        once.instructed_by(&order()?, "activities[at0001]"),
        Err(InstructionError::Taken)
    ));
    Ok(())
}

#[test]
fn a_composition_with_no_action_names_no_order() -> Result<(), Box<dyn Error>> {
    let mapping = mapping(REQUEST)?;
    let received = ReceivedResources::read(&collection(&request()))?;
    let composition = mapping.resources_to_openehr(&received, &settings())?;
    assert!(matches!(
        composition.instructed_by(&order()?, "activities[at0001]"),
        Err(InstructionError::NoAction)
    ));
    Ok(())
}

#[test]
fn a_document_is_no_collection_of_resources() {
    let text = serde_json::json!({
        "resourceType": "Bundle",
        "type": "document",
        "entry": [{"resource": medication()}]
    })
    .to_string();
    assert!(matches!(
        ReceivedResources::read(&text),
        Err(ReceiveError::NotACollection { found }) if found.as_deref() == Some("document")
    ));
}

#[test]
fn a_collection_that_repeats_a_name_is_refused() {
    let text = r#"{"resourceType": "Bundle", "type": "collection", "type": "collection"}"#;
    assert!(matches!(
        ReceivedResources::read(text),
        Err(ReceiveError::RepeatedName { .. })
    ));
}

#[test]
fn a_resource_the_collection_cannot_hold_is_refused() {
    let mut product = medication();
    if let Some(object) = product.as_object_mut() {
        object.insert(String::from("strength"), serde_json::json!("500 mg"));
    }
    let text = serde_json::json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [{"resource": product}]
    })
    .to_string();
    assert!(ReceivedResources::read(&text).is_err());
}

#[test]
fn every_mapped_resource_is_an_r4_resource() -> Result<(), Box<dyn Error>> {
    let (_, resources) = round_trip(REQUEST, &request())?;
    for object in &resources {
        Resource::from_json(object, &mut fhir_types::codec::Path::root("Resource"))?;
    }
    Ok(())
}

#[test]
fn the_embedded_project_compiles_as_its_files_do() -> Result<(), Box<dyn Error>> {
    let embedded = Mapping::compile_project(&template()?, &MPD)?;
    let from_disk = Mapping::compile_template(&template()?, &files(), MPD.contexts())?;
    assert_eq!(embedded.len(), MPD.contexts().len());
    assert_eq!(
        embedded.contexts().collect::<Vec<_>>(),
        from_disk.contexts().collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn one_named_context_of_the_project_compiles_alone() -> Result<(), Box<dyn Error>> {
    for context in [REQUEST, DISPENSE] {
        let narrowed = MPD.with_context(context)?;
        assert_eq!(narrowed.contexts(), [context]);
        assert_eq!(narrowed.files(), MPD.files(), "the same files load");
        let mapping = Mapping::compile_project(&template()?, &narrowed)?;
        let names: Vec<&str> = mapping.contexts().map(|compiled| compiled.name).collect();
        assert_eq!(names, [context]);
    }
    Ok(())
}

#[test]
fn a_context_the_project_does_not_compile_is_refused() {
    let refused = MPD.with_context("ferrobridge_eu_mpd_unknown.context");
    assert_eq!(
        refused,
        Err(UnknownContext {
            project: "mpd",
            context: String::from("ferrobridge_eu_mpd_unknown.context"),
        })
    );
}

// FHIR R4 Resource.meta.profile: every mapped resource claims its MPD profile, and the
// validator lane holds each claim.
#[test]
fn the_mapped_prescription_is_written_for_the_validator() -> Result<(), Box<dyn Error>> {
    emit_mapped(REQUEST, &request(), "eehrxf-mpd-prescription")
}

#[test]
fn the_mapped_dispense_is_written_for_the_validator() -> Result<(), Box<dyn Error>> {
    emit_mapped(DISPENSE, &dispense(), "eehrxf-mpd-dispense")
}

// NOTE: no specification governs this: our own design; a spawned thread gets the 2 MiB
// default stack (std::thread "Stack size"); the template build, compile and run fit in it.
#[test]
fn both_contexts_round_trip_on_a_default_stack_thread() -> Result<(), Box<dyn Error>> {
    let spawned = std::thread::Builder::new().spawn(|| -> Result<usize, String> {
        let source = template().map_err(|error| error.to_string())?;
        let mut mapped = 0;
        for (context, resource) in [(REQUEST, request()), (DISPENSE, dispense())] {
            let project = MPD
                .with_context(context)
                .map_err(|error| error.to_string())?;
            let mapping =
                Mapping::compile_project(&source, &project).map_err(|error| error.to_string())?;
            let (_, resources) =
                round_trip_through(&mapping, &resource).map_err(|error| error.to_string())?;
            mapped += resources.len();
        }
        Ok(mapped)
    })?;
    let mapped = spawned
        .join()
        .map_err(|_panicked| "the mapping thread panicked")??;
    assert!(mapped > 0, "both round trips mapped");
    Ok(())
}
