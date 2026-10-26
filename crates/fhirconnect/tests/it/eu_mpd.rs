// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 Europe Medication Prescription and Dispense 1.0.0 round trip over
//! FerroBRIDGE's medication template.
//!
//! The template specialises seven published CKM archetypes, and its overlays
//! fill the nested medication, direction, dosage and timing slots; the suite
//! builds its ADL 2 operational template at test time. A prescription is a
//! `MedicationRequest-eu-mpd` over the medication order, a dispense a
//! `MedicationDispense-eu-mpd` over the medication ACTION at its dispensed
//! careflow step, and each references a `Medication-eu-mpd` it creates from
//! the medication details; the dosage is a `Dosage-eu-mpd`.

use core::error::Error;
use std::sync::Arc;

use ferrobridge_testkit::fixtures::CKM_MEDICATION_ARCHETYPES;
use ferrobridge_testkit::fixtures::FERROBRIDGE_MEDICATION_TEMPLATE;
use ferrobridge_testkit::fixtures::FERROBRIDGE_MEDICATION_TEMPLATE_ID;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::BundleReferences;
use fhirconnect::engine::seam::NoReferences;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::laws;
use crate::support::FIXTURES;
use crate::support::render;

/// The project directory this crate authored for the profiles.
const PROJECT_DIR: &str = "projects/ferrobridge/eu_mpd";

/// The files of the project directory.
const PROJECT: &[&str] = &[
    "ferrobridge_eu_mpd_request.context.yml",
    "ferrobridge_medication_order.v3.yml",
    "ferrobridge_medication.v2.yml",
    "ferrobridge_dosage.v2.yml",
    "ferrobridge_eu_mpd_request.yml",
    "ferrobridge_eu_mpd_medication.yml",
    "ferrobridge_eu_mpd_dispense.context.yml",
    "ferrobridge_medication.v1.yml",
    "ferrobridge_eu_mpd_dispense.yml",
];

/// The request context this crate authored.
const REQUEST: &str = "ferrobridge_eu_mpd_request.context";

/// The dispense context this crate authored.
const DISPENSE: &str = "ferrobridge_eu_mpd_dispense.context";

/// The canonical of the dispense profile.
const MEDICATION_DISPENSE_EU_MPD: &str =
    "http://hl7.eu/fhir/mpd/StructureDefinition/MedicationDispense-eu-mpd";

/// The canonical of the request profile.
const MEDICATION_REQUEST_EU_MPD: &str =
    "http://hl7.eu/fhir/mpd/StructureDefinition/MedicationRequest-eu-mpd";

/// The canonical of the medication profile.
const MEDICATION_EU_MPD: &str = "http://hl7.eu/fhir/mpd/StructureDefinition/Medication-eu-mpd";

/// Builds the index over the ADL 2 operational template of FerroBRIDGE's
/// medication template, compiled from the vendored CKM archetypes.
fn medication_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::new();
    for (file, source) in CKM_MEDICATION_ARCHETYPES {
        let archetype = openehr_adl::assemble::parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    let template =
        openehr_adl::assemble::parse_artefact(FERROBRIDGE_MEDICATION_TEMPLATE, Dialect::Adl2)
            .map_err(|errors| format!("the template: {errors:?}"))?;
    let opt = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(opt),
        resolved_id: String::from(FERROBRIDGE_MEDICATION_TEMPLATE_ID),
    })?)
}

#[test]
fn a_node_under_an_overlay_carries_the_published_archetype() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    assert_eq!(index.template_id(), FERROBRIDGE_MEDICATION_TEMPLATE_ID);
    let paths: Vec<String> = index
        .nodes()
        .map(|node| node.aql_path().as_str().to_owned())
        .collect();
    assert!(paths.iter().all(|path| !path.contains("-ferrobridge.")));
    assert!(paths.iter().any(|path| path.ends_with(
        "/items[openEHR-EHR-CLUSTER.therapeutic_direction.v1]/items[openEHR-EHR-CLUSTER.dosage.v2]/items[openEHR-EHR-CLUSTER.timing_daily.v1]/items[at0003]/value"
    )));
    Ok(())
}

/// Compiles the context `context` against the medication template.
fn program(context: &str) -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = medication_template()?;
    let files: Vec<String> = PROJECT
        .iter()
        .map(|file| format!("{FIXTURES}/{PROJECT_DIR}/{file}"))
        .collect();
    let set = load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics))?;
    let program = compile(
        &set,
        &MappingName::new(context)?,
        &index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    Ok((program, index))
}

#[test]
fn the_two_contexts_compile_against_the_medication_template() -> Result<(), Box<dyn Error>> {
    for (context, resource) in [
        (REQUEST, "MedicationRequest"),
        (DISPENSE, "MedicationDispense"),
    ] {
        let (program, _) = program(context)?;
        assert_eq!(
            program.template().id().as_str(),
            FERROBRIDGE_MEDICATION_TEMPLATE_ID
        );
        assert_eq!(program.resource().as_str(), resource);
    }
    Ok(())
}

/// A synthetic medicinal product in the shape `Medication-eu-mpd`
/// constrains: its code, its form and its batch.
fn medication() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Medication",
        "id": "synthetic-product",
        "meta": {"profile": [MEDICATION_EU_MPD]},
        "code": {"coding": [{
            "system": "http://example.org/fhir/CodeSystem/ferrobridge-products",
            "code": "SYN-PRODUCT-0001",
            "display": "Synthetic tablet 500 mg"
        }]},
        "form": {"coding": [{
            "system": "http://example.org/fhir/CodeSystem/ferrobridge-forms",
            "code": "SYN-TABLET",
            "display": "Synthetic tablet"
        }]},
        "batch": {
            "lotNumber": "SYN-LOT-0001",
            "expirationDate": "2027-12-31T00:00:00Z"
        }
    })
}

/// A synthetic prescription in the shape `MedicationRequest-eu-mpd`
/// constrains: the required authoring time and prescriber, the referenced
/// product, the reason, a note, one dosage and the validity period.
fn request() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "MedicationRequest",
        "meta": {"profile": [MEDICATION_REQUEST_EU_MPD]},
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
        "reasonCode": [{"text": "A synthetic indication"}],
        "note": [{"text": "A synthetic note."}],
        "dosageInstruction": [{
            "sequence": 1,
            "text": "One tablet twice a day",
            "route": {"coding": [{
                "system": "http://example.org/fhir/CodeSystem/ferrobridge-routes",
                "code": "SYN-ORAL",
                "display": "Synthetic oral route"
            }]},
            "doseAndRate": [{"doseQuantity": {
                "value": 1,
                "unit": "tablet",
                "system": "http://unitsofmeasure.org",
                "code": "{tbl}"
            }}]
        }],
        "dispenseRequest": {"validityPeriod": {
            "start": "2026-03-02T00:00:00+01:00",
            "end": "2026-06-02T00:00:00+02:00"
        }}
    })
}

/// The request and the resources a run out of openEHR writes.
struct Outbound {
    /// The resource the context names.
    resource: serde_json::Value,
    /// The resources it created.
    created: Vec<serde_json::Value>,
}

/// Runs `input` through `program` into openEHR and back, with the
/// synthetic product resolvable.
fn round_trip(
    program: &Program,
    index: &WebTemplateIndex,
    input: serde_json::Value,
    root: &str,
) -> Result<Outbound, Box<dyn Error>> {
    let bundle = BundleReferences::new(
        vec![(
            Some(String::from("Medication/synthetic-product")),
            Value::from_serde_json(medication()),
        )],
        &NoReferences,
    );
    let into = to_openehr(
        program,
        &SCHEMAS,
        index,
        &Value::from_serde_json(input),
        &Seams::default().with_references(&bundle),
        &laws::defaults(),
        &CallContext::new(),
    )?;
    let back = to_fhir(
        program,
        &SCHEMAS,
        index,
        into.value(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    let created = back
        .created()
        .iter()
        .map(|created| created.to_serde_json(&mut fhir_types::codec::Path::root("Resource")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Outbound {
        resource: back
            .value()
            .to_serde_json(&mut fhir_types::codec::Path::root(root))?,
        created,
    })
}

#[test]
fn the_profile_elements_of_the_prescription_survive_the_round_trip() -> Result<(), Box<dyn Error>> {
    let (program, index) = program(REQUEST)?;
    let input = request();
    let Outbound { resource, created } =
        round_trip(&program, &index, input.clone(), "MedicationRequest")?;
    assert_eq!(
        resource
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(MEDICATION_REQUEST_EU_MPD),
        "the request names its profile: {resource}"
    );
    for pointer in [
        "/identifier/0",
        "/status",
        "/intent",
        "/authoredOn",
        "/requester/display",
        "/reasonCode/0/text",
        "/note/0/text",
        "/dispenseRequest/validityPeriod/start",
        "/dispenseRequest/validityPeriod/end",
    ] {
        assert_eq!(
            resource.pointer(pointer),
            input.pointer(pointer),
            "{pointer}: {resource}"
        );
    }
    let product = the_one_product(&created)?;
    assert_eq!(
        resource
            .pointer("/medicationReference/reference")
            .and_then(serde_json::Value::as_str),
        product
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(|id| format!("Medication/{id}"))
            .as_deref(),
        "the request references the product it created: {resource}"
    );
    Ok(())
}

/// The `Dosage-eu-mpd` elements of the one dosage: its sequence, text, route
/// and dose quantity.
#[test]
fn the_dosage_of_the_prescription_survives_the_round_trip() -> Result<(), Box<dyn Error>> {
    let (program, index) = program(REQUEST)?;
    let input = request();
    let Outbound { resource, .. } =
        round_trip(&program, &index, input.clone(), "MedicationRequest")?;
    let dosages = resource
        .get("dosageInstruction")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    assert_eq!(dosages, 1, "one dosage: {resource}");
    for pointer in [
        "/dosageInstruction/0/sequence",
        "/dosageInstruction/0/text",
        "/dosageInstruction/0/route/coding/0",
        "/dosageInstruction/0/doseAndRate/0/doseQuantity",
    ] {
        assert_eq!(
            resource.pointer(pointer),
            input.pointer(pointer),
            "{pointer}: {resource}"
        );
    }
    Ok(())
}

/// The `Medication-eu-mpd` elements of the referenced product: its profile,
/// code, form and batch.
#[test]
fn the_product_of_the_prescription_survives_the_round_trip() -> Result<(), Box<dyn Error>> {
    let (program, index) = program(REQUEST)?;
    let Outbound { created, .. } = round_trip(&program, &index, request(), "MedicationRequest")?;
    let product = the_one_product(&created)?;
    let input = medication();
    assert_eq!(
        product
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(MEDICATION_EU_MPD),
        "the product names its profile: {product}"
    );
    for pointer in [
        "/code/coding/0",
        "/form/coding/0",
        "/batch/lotNumber",
        "/batch/expirationDate",
    ] {
        assert_eq!(
            product.pointer(pointer),
            input.pointer(pointer),
            "{pointer}: {product}"
        );
    }
    Ok(())
}

/// The one Medication a run out of openEHR created.
fn the_one_product(created: &[serde_json::Value]) -> Result<&serde_json::Value, Box<dyn Error>> {
    let products: Vec<&serde_json::Value> = created
        .iter()
        .filter(|resource| {
            resource
                .get("resourceType")
                .and_then(serde_json::Value::as_str)
                == Some("Medication")
        })
        .collect();
    let [product] = products.as_slice() else {
        return Err(format!("the run created {} products, not one", products.len()).into());
    };
    Ok(product)
}

/// A synthetic dispense in the shape `MedicationDispense-eu-mpd` constrains:
/// the required `recorded` extension and quantity, the referenced product,
/// the hand-over time and the dispenser.
fn dispense() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "MedicationDispense",
        "meta": {"profile": [MEDICATION_DISPENSE_EU_MPD]},
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
        "whenHandedOver": "2026-03-03T10:05:00+01:00",
        "note": [{"text": "A synthetic dispense note."}]
    })
}

#[test]
fn the_profile_elements_of_the_dispense_survive_the_round_trip() -> Result<(), Box<dyn Error>> {
    let (program, index) = program(DISPENSE)?;
    let input = dispense();
    let Outbound { resource, created } =
        round_trip(&program, &index, input.clone(), "MedicationDispense")?;
    assert_eq!(
        resource
            .pointer("/meta/profile/0")
            .and_then(serde_json::Value::as_str),
        Some(MEDICATION_DISPENSE_EU_MPD),
        "the dispense names its profile: {resource}"
    );
    for pointer in [
        "/extension/0",
        "/status",
        "/quantity",
        "/whenHandedOver",
        "/performer/0/actor/display",
        "/note/0/text",
    ] {
        assert_eq!(
            resource.pointer(pointer),
            input.pointer(pointer),
            "{pointer}: {resource}"
        );
    }
    let product = the_one_product(&created)?;
    assert_eq!(
        resource
            .pointer("/medicationReference/reference")
            .and_then(serde_json::Value::as_str),
        product
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(|id| format!("Medication/{id}"))
            .as_deref(),
        "the dispense references the product it created: {resource}"
    );
    Ok(())
}

/// The dispense is the ACTION at its careflow step "Prescription dispensed"
/// in the active state, the step the template records a dispense at. One
/// `manual` entry writes both leaves of the `ism_transition`: its paths reach
/// two data elements, and each is created (`manual.adoc`).
#[test]
fn the_dispense_is_recorded_at_the_dispensed_careflow_step() -> Result<(), Box<dyn Error>> {
    let (program, index) = program(DISPENSE)?;
    let into = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(dispense()),
        &Seams::default().with_references(&BundleReferences::new(
            vec![(
                Some(String::from("Medication/synthetic-product")),
                Value::from_serde_json(medication()),
            )],
            &NoReferences,
        )),
        &laws::defaults(),
        &CallContext::new(),
    )?;
    let document = openehr_mapping_core::composition::CanonicalDocument::document(into.value());
    let transition = document
        .pointer("/content/0/ism_transition")
        .ok_or("the ACTION carries no ism_transition")?;
    assert_eq!(
        transition.pointer("/careflow_step/defining_code/code_string"),
        Some(&serde_json::json!("at0003"))
    );
    assert_eq!(
        transition.pointer("/current_state/defining_code/code_string"),
        Some(&serde_json::json!("245"))
    );
    Ok(())
}
