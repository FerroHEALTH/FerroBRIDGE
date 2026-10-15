// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Type tests over the base types the R4 element table names: `is`, `as`
//! and `ofType` test a type and the types it specializes (N1 §6.3), and a
//! type name may name a supertype (FHIR R4 §2.9.1.1).

use super::support::{eval, json, truth};

fn condition_with_an_age() -> fhir_types::codec::Value {
    json(
        r#"{
            "resourceType": "Condition",
            "subject": {"reference": "Patient/p1"},
            "onsetAge": {"value": 42, "unit": "a", "system": "http://unitsofmeasure.org", "code": "a"}
        }"#,
    )
}

fn patient() -> fhir_types::codec::Value {
    json(r#"{"resourceType": "Patient", "id": "p1", "gender": "female"}"#)
}

fn holds(expression: &str, resource: &fhir_types::codec::Value) -> Option<bool> {
    truth(&eval(expression, resource).expect("evaluates"))
}

#[test]
fn an_age_is_a_quantity() {
    // FHIR R4 Age: baseDefinition Quantity (https://hl7.org/fhir/R4/datatypes.html#Age).
    let condition = condition_with_an_age();
    assert_eq!(holds("onset is Age", &condition), Some(true));
    assert_eq!(holds("onset is Quantity", &condition), Some(true));
    assert_eq!(holds("onset is FHIR.Quantity", &condition), Some(true));
    assert_eq!(holds("onset is Duration", &condition), Some(false));
}

#[test]
fn of_type_quantity_selects_an_age() {
    // N1 §5.2.4: ofType() keeps the items of the type or a subtype of it.
    let condition = condition_with_an_age();
    assert_eq!(
        holds("onset.ofType(Quantity).count() = 1", &condition),
        Some(true)
    );
    assert_eq!(
        holds("(onset as Quantity).value = 42", &condition),
        Some(true)
    );
}

#[test]
fn a_quantity_is_no_age() {
    // N1 §6.3.1: is tests the type and its supertypes, never a subtype.
    let observation = json(
        r#"{"resourceType": "Observation", "status": "final", "code": {"text": "x"},
            "valueQuantity": {"value": 1}}"#,
    );
    assert_eq!(holds("value is Quantity", &observation), Some(true));
    assert_eq!(holds("value is Age", &observation), Some(false));
}

#[test]
fn a_patient_is_a_domain_resource_and_a_resource() {
    // FHIR R4 Patient: baseDefinition DomainResource, whose base is Resource.
    let patient = patient();
    assert_eq!(holds("Patient.is(DomainResource)", &patient), Some(true));
    assert_eq!(holds("Patient.is(Resource)", &patient), Some(true));
    assert_eq!(holds("Patient.is(FHIR.Resource)", &patient), Some(true));
}

#[test]
fn a_type_name_may_name_a_supertype_of_the_context() {
    // FHIR R4 §2.9.1.1: the name can also include super types such as DomainResource.
    let patient = patient();
    assert_eq!(holds("DomainResource.id = 'p1'", &patient), Some(true));
    assert_eq!(holds("Resource.id = 'p1'", &patient), Some(true));
}

#[test]
fn a_bundle_is_a_resource_and_no_domain_resource() {
    // FHIR R4 Bundle: baseDefinition Resource (https://hl7.org/fhir/R4/bundle.html).
    let bundle = json(r#"{"resourceType": "Bundle", "type": "collection"}"#);
    assert_eq!(holds("Bundle.is(Resource)", &bundle), Some(true));
    assert_eq!(holds("Bundle.is(DomainResource)", &bundle), Some(false));
}

#[test]
fn a_primitive_is_its_own_type_only() {
    // FHIR R4 §2.9.1.5: all primitives are independent types, so code is no string.
    let patient = patient();
    assert_eq!(holds("gender is code", &patient), Some(true));
    assert_eq!(holds("gender is string", &patient), Some(false));
}
