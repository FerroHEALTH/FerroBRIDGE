// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `DV_COUNT` node against the restricted R4 integers `unsignedInt` and
//! `positiveInt`.
//!
//! R4 defines `unsignedInt` as an integer from 0 and `positiveInt` as one
//! from 1 (<https://hl7.org/fhir/R4/datatypes.html#unsignedInt>); the
//! FHIRconnect `DV_COUNT` page names only `integer`, so carrying a count
//! through either is our own design. The template is the synthetic diagnosis
//! template with its problem name value a `DV_COUNT`.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::lens::LensError;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;

use crate::engine::defaults;
use crate::engine::scalar::PROBLEM;
use crate::engine::scalar::retyped_template;
use crate::support::compiled_against;

/// Returns the template with a `DV_COUNT` problem name value.
fn counted_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    retyped_template("DV_COUNT")
}

/// A synthetic `MedicationRequest` that allows `repeats` refills.
fn request(repeats: u32) -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "MedicationRequest",
        "status": "unknown",
        "intent": "order",
        "medicationCodeableConcept": {"text": "A synthetic product"},
        "subject": {"reference": "Patient/synthetic-patient"},
        "dispenseRequest": {"numberOfRepeatsAllowed": repeats}
    }))
}

/// A synthetic `Immunization` whose one protocol gives the dose `dose`.
fn immunization(dose: u32) -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Immunization",
        "status": "completed",
        "vaccineCode": {"text": "A synthetic vaccine"},
        "patient": {"reference": "Patient/synthetic-patient"},
        "occurrenceDateTime": "2026-03-02",
        "protocolApplied": [{"doseNumberPositiveInt": dose}]
    }))
}

/// Runs `document` into openEHR through `fixture`, returning the count the
/// run wrote and the composition.
fn into_openehr(
    fixture: &str,
    document: &Value,
) -> Result<(serde_json::Value, CanonicalComposition), Box<dyn Error>> {
    let index = counted_template()?;
    let program = compiled_against(fixture, &index)?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        document,
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?;
    let composition = inbound.into_value();
    let node = index
        .nodes()
        .find(|node| node.aql_path().as_str() == PROBLEM)
        .ok_or("the template holds the problem name value")?;
    let written = index
        .read(&composition, node, &[])?
        .ok_or("the run wrote the count")?;
    Ok((written, composition))
}

/// Runs `composition` out of openEHR through `fixture` as `root`.
fn out_of_openehr(
    fixture: &str,
    composition: &CanonicalComposition,
    root: &str,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let index = counted_template()?;
    let program = compiled_against(fixture, &index)?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    Ok(outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root(root))?)
}

/// Returns `composition` with the magnitude of every `DV_COUNT` it holds set
/// to `magnitude`.
fn recounted(composition: &CanonicalComposition, magnitude: i64) -> CanonicalComposition {
    fn walk(value: &mut serde_json::Value, magnitude: i64) {
        match value {
            serde_json::Value::Object(object) => {
                if object.get("_type") == Some(&serde_json::json!("DV_COUNT")) {
                    object.insert(String::from("magnitude"), serde_json::json!(magnitude));
                }
                for item in object.values_mut() {
                    walk(item, magnitude);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items.iter_mut() {
                    walk(item, magnitude);
                }
            }
            _ => {}
        }
    }
    let mut value = composition.value().clone();
    walk(&mut value, magnitude);
    CanonicalComposition::new(value, composition.template_id(), composition.generation())
}

#[test]
fn an_unsigned_int_travels_as_a_count_both_ways() -> Result<(), Box<dyn Error>> {
    let (written, composition) = into_openehr("ferrobridge_count_unsigned", &request(2))?;
    assert_eq!(written.get("_type"), Some(&serde_json::json!("DV_COUNT")));
    assert_eq!(written.get("magnitude"), Some(&serde_json::json!(2)));
    let resource = out_of_openehr(
        "ferrobridge_count_unsigned",
        &composition,
        "MedicationRequest",
    )?;
    assert_eq!(
        resource.pointer("/dispenseRequest/numberOfRepeatsAllowed"),
        Some(&serde_json::json!(2))
    );
    Ok(())
}

#[test]
fn a_zero_count_is_an_unsigned_int() -> Result<(), Box<dyn Error>> {
    let (written, composition) = into_openehr("ferrobridge_count_unsigned", &request(0))?;
    assert_eq!(written.get("magnitude"), Some(&serde_json::json!(0)));
    let resource = out_of_openehr(
        "ferrobridge_count_unsigned",
        &composition,
        "MedicationRequest",
    )?;
    assert_eq!(
        resource.pointer("/dispenseRequest/numberOfRepeatsAllowed"),
        Some(&serde_json::json!(0))
    );
    Ok(())
}

#[test]
fn a_positive_int_travels_as_a_count_both_ways() -> Result<(), Box<dyn Error>> {
    let (written, composition) = into_openehr("ferrobridge_count_positive", &immunization(3))?;
    assert_eq!(written.get("magnitude"), Some(&serde_json::json!(3)));
    let resource = out_of_openehr("ferrobridge_count_positive", &composition, "Immunization")?;
    assert_eq!(
        resource.pointer("/protocolApplied/0/doseNumberPositiveInt"),
        Some(&serde_json::json!(3))
    );
    Ok(())
}

/// Returns whether `error` is the cell's refusal of the magnitude.
fn refuses_the_magnitude(error: &(dyn Error + 'static)) -> bool {
    let mut cause = Some(error);
    while let Some(current) = cause {
        if let Some(LensError::Shape { attribute, .. }) = current.downcast_ref::<LensError>() {
            return *attribute == "DV_COUNT.magnitude";
        }
        if let Some(EngineError::Cell { source, .. }) = current.downcast_ref::<EngineError>() {
            return matches!(**source, LensError::Shape { attribute, .. } if attribute == "DV_COUNT.magnitude");
        }
        cause = current.source();
    }
    false
}

#[test]
fn a_negative_count_is_refused_as_an_unsigned_int() -> Result<(), Box<dyn Error>> {
    let (_, composition) = into_openehr("ferrobridge_count_unsigned", &request(2))?;
    let negative = recounted(&composition, -1);
    let error = out_of_openehr("ferrobridge_count_unsigned", &negative, "MedicationRequest")
        .expect_err("an unsignedInt is from 0, and the count is never clamped");
    assert!(refuses_the_magnitude(error.as_ref()), "{error}");
    Ok(())
}

#[test]
fn a_zero_count_is_refused_as_a_positive_int() -> Result<(), Box<dyn Error>> {
    let (_, composition) = into_openehr("ferrobridge_count_positive", &immunization(3))?;
    let zero = recounted(&composition, 0);
    let error = out_of_openehr("ferrobridge_count_positive", &zero, "Immunization")
        .expect_err("a positiveInt is from 1, and the count is never clamped");
    assert!(refuses_the_magnitude(error.as_ref()), "{error}");
    Ok(())
}

#[test]
fn a_zero_dose_is_refused_going_in() {
    // R4 datatypes.html#positiveInt: the document itself is no R4 positiveInt.
    assert!(into_openehr("ferrobridge_count_positive", &immunization(0)).is_err());
}
