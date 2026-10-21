// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The five discriminator types and their combination
//! (<https://hl7.org/fhir/R4/profiling.html#discriminator>, §5.1.0.11).

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use serde_json::json;

use super::identifier_child;
use super::identifier_profile;
use super::identifier_slice;
use super::patient_with;
use super::system_slice;
use super::unevaluated_discriminators;
use crate::support::TestResult;
use crate::support::locations;

/// A closed slicing of `Patient.identifier` by `discriminators`.
fn closed(discriminators: &serde_json::Value) -> serde_json::Value {
    json!({"discriminator": discriminators, "rules": "closed"})
}

#[test]
fn a_value_discriminator_matches_the_fixed_value_of_the_slice() -> TestResult {
    let definition = identifier_profile(
        &closed(&json!([{"type": "value", "path": "system"}])),
        &system_slice("mrn", "urn:mrn", "1"),
    );
    let report = patient_with(&[definition], &json!([{"system": "urn:mrn", "value": "1"}]))?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    assert_eq!(
        unevaluated_discriminators(&report),
        Vec::<(Constraint, Reason, String)>::new()
    );
    Ok(())
}

#[test]
fn a_value_discriminator_does_not_match_another_value() -> TestResult {
    let definition = identifier_profile(
        &closed(&json!([{"type": "value", "path": "system"}])),
        &system_slice("mrn", "urn:mrn", "1"),
    );
    let report = patient_with(
        &[definition],
        &json!([{"system": "urn:other", "value": "1"}]),
    )?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Patient.identifier[0]"]
    );
    Ok(())
}

#[test]
fn a_pattern_discriminator_matches_the_pattern_of_the_slice() -> TestResult {
    let pattern = json!({"coding": [{"system": "http://terminology.hl7.org/CodeSystem/v2-0203", "code": "MR"}]});
    let definition = identifier_profile(
        &closed(&json!([{"type": "pattern", "path": "type"}])),
        &[
            identifier_slice("mrn", 0, "1"),
            identifier_child("mrn", "type", &json!({"patternCodeableConcept": pattern})),
        ],
    );
    let held = json!([{"type": {"coding": [
        {"system": "http://terminology.hl7.org/CodeSystem/v2-0203", "code": "MR", "display": "Medical record number"}
    ], "text": "MRN"}, "value": "1"}]);
    let report = patient_with(std::slice::from_ref(&definition), &held)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let other = json!([{"type": {"coding": [
        {"system": "http://terminology.hl7.org/CodeSystem/v2-0203", "code": "DL"}
    ]}, "value": "1"}]);
    let report = patient_with(&[definition], &other)?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Patient.identifier[0]"]
    );
    Ok(())
}

/// A slice `official` of `Patient.identifier` whose `use` is bound
/// (required) to the value set `value_set`.
fn bound_use(value_set: &str) -> Vec<serde_json::Value> {
    vec![
        identifier_slice("official", 0, "1"),
        identifier_child(
            "official",
            "use",
            &json!({"binding": {"strength": "required", "valueSet": value_set}}),
        ),
    ]
}

#[test]
fn a_value_discriminator_matches_a_code_of_an_enumerated_required_binding() -> TestResult {
    let value_set = json!({
        "resourceType": "ValueSet", "id": "official-use", "status": "active",
        "url": "http://example.org/fhir/ValueSet/official-use",
        "compose": {"include": [{"system": "http://hl7.org/fhir/identifier-use",
                                 "concept": [{"code": "official"}]}]}
    });
    let definition = identifier_profile(
        &closed(&json!([{"type": "value", "path": "use"}])),
        &bound_use("http://example.org/fhir/ValueSet/official-use"),
    );
    let set = [definition, value_set];
    let report = patient_with(&set, &json!([{"use": "official", "value": "1"}]))?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let report = patient_with(&set, &json!([{"use": "usual", "value": "1"}]))?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Patient.identifier[0]"]
    );
    Ok(())
}

#[test]
fn a_required_binding_over_a_whole_code_system_enumerates_its_nested_codes() -> TestResult {
    let code_system = json!({
        "resourceType": "CodeSystem", "id": "kinds", "status": "active", "content": "complete",
        "url": "http://example.org/fhir/CodeSystem/kinds",
        "concept": [{"code": "a", "concept": [{"code": "a1"}]}, {"code": "b"}]
    });
    let value_set = json!({
        "resourceType": "ValueSet", "id": "kinds", "status": "active",
        "url": "http://example.org/fhir/ValueSet/kinds",
        "compose": {"include": [{"system": "http://example.org/fhir/CodeSystem/kinds"}]}
    });
    let definition = identifier_profile(
        &closed(&json!([{"type": "value", "path": "type"}])),
        &[
            identifier_slice("kind", 0, "*"),
            identifier_child(
                "kind",
                "type",
                &json!({"binding": {"strength": "required", "valueSet": "http://example.org/fhir/ValueSet/kinds"}}),
            ),
        ],
    );
    let held = json!([{"type": {"coding": [{"system": "http://example.org/fhir/CodeSystem/kinds", "code": "a1"}]}}]);
    let report = patient_with(&[definition, code_system, value_set], &held)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    Ok(())
}

#[test]
fn a_value_set_the_set_cannot_enumerate_is_named_and_decides_nothing() -> TestResult {
    let value_set = json!({
        "resourceType": "ValueSet", "id": "filtered", "status": "active",
        "url": "http://example.org/fhir/ValueSet/filtered",
        "compose": {"include": [{"system": "http://loinc.org",
                                 "filter": [{"property": "CLASS", "op": "=", "value": "X"}]}]}
    });
    let definition = identifier_profile(
        &closed(&json!([{"type": "value", "path": "use"}])),
        &bound_use("http://example.org/fhir/ValueSet/filtered"),
    );
    let report = patient_with(&[definition, value_set], &json!([{"use": "usual"}]))?;
    assert_eq!(
        unevaluated_discriminators(&report),
        [(
            Constraint::DiscriminatorValueSet {
                path: "use".to_owned(),
                value_set: "http://example.org/fhir/ValueSet/filtered".to_owned()
            },
            Reason::ValueSetNotEnumerated,
            "Patient.identifier[0]".to_owned()
        )]
    );
    assert!(
        locations(&report, MessageId::UnmatchedSliceItem).is_empty(),
        "an undecided item is not refused by the closed rule"
    );
    Ok(())
}

#[test]
fn an_exists_discriminator_matches_presence_and_absence() -> TestResult {
    let definition = identifier_profile(
        &closed(&json!([{"type": "exists", "path": "period"}])),
        &[
            identifier_slice("dated", 1, "*"),
            identifier_child("dated", "period", &json!({"min": 1})),
            identifier_slice("undated", 0, "*"),
            identifier_child("undated", "period", &json!({"max": "0"})),
        ],
    );
    let both = json!([{"value": "1", "period": {"start": "2020-01-01"}}, {"value": "2"}]);
    let report = patient_with(std::slice::from_ref(&definition), &both)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let report = patient_with(&[definition], &json!([{"value": "2"}]))?;
    let messages: Vec<_> = report
        .with_message(MessageId::MinCardinality)
        .map(|issue| issue.message.clone())
        .collect();
    assert_eq!(
        messages,
        ["Patient.identifier:dated occurs 0 times, below its minimum of 1"]
    );
    Ok(())
}

#[test]
fn every_discriminator_of_a_slice_must_match() -> TestResult {
    let definition = identifier_profile(
        &closed(&json!([{"type": "value", "path": "system"}, {"type": "value", "path": "use"}])),
        &[
            identifier_slice("mrn", 0, "*"),
            identifier_child("mrn", "system", &json!({"fixedUri": "urn:mrn"})),
            identifier_child("mrn", "use", &json!({"fixedCode": "official"})),
        ],
    );
    let both = json!([{"system": "urn:mrn", "use": "official"}]);
    let report = patient_with(std::slice::from_ref(&definition), &both)?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let one = json!([{"system": "urn:mrn", "use": "usual"}]);
    let report = patient_with(&[definition], &one)?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Patient.identifier[0]"]
    );
    Ok(())
}

#[test]
fn a_type_discriminator_matches_the_type_of_a_choice() -> TestResult {
    let definition = crate::support::profile(
        "typed-observation",
        "Observation",
        &json!([
            {"id": "Observation", "path": "Observation", "min": 0, "max": "*"},
            {"id": "Observation.value[x]", "path": "Observation.value[x]", "min": 0, "max": "1",
             "slicing": {"discriminator": [{"type": "type", "path": "$this"}], "rules": "closed"}},
            {"id": "Observation.value[x]:valueQuantity", "path": "Observation.value[x]",
             "sliceName": "valueQuantity", "min": 0, "max": "1", "type": [{"code": "Quantity"}]}
        ]),
    );
    let url = format!("{}typed-observation", crate::support::BASE);
    let observation = |value: (&str, serde_json::Value)| {
        let mut resource =
            json!({"resourceType": "Observation", "status": "final", "code": {"text": "x"}});
        resource[value.0] = value.1;
        resource
    };
    let report = super::against(
        std::slice::from_ref(&definition),
        &observation(("valueQuantity", json!({"value": 1}))),
        &url,
    )?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let report = super::against(
        &[definition],
        &observation(("valueString", json!("x"))),
        &url,
    )?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Observation.value.ofType(string)"]
    );
    Ok(())
}

#[test]
fn slicing_without_a_discriminator_matches_the_slice_the_item_validates_against() -> TestResult {
    let definition = identifier_profile(
        &json!({"rules": "closed"}),
        &system_slice("mrn", "urn:mrn", "*"),
    );
    let report = patient_with(
        std::slice::from_ref(&definition),
        &json!([{"system": "urn:mrn"}]),
    )?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let report = patient_with(&[definition], &json!([{"system": "urn:other"}]))?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Patient.identifier[0]"]
    );
    Ok(())
}

#[test]
fn an_item_matching_two_slices_is_an_overlap() -> TestResult {
    let mut slices = system_slice("first", "urn:same", "*");
    slices.extend(system_slice("second", "urn:same", "*"));
    let definition = identifier_profile(
        &closed(&json!([{"type": "value", "path": "system"}])),
        &slices,
    );
    let report = patient_with(&[definition], &json!([{"system": "urn:same"}]))?;
    assert_eq!(
        locations(&report, MessageId::OverlappingSlices),
        ["Patient.identifier[0]"]
    );
    Ok(())
}

#[test]
fn a_discriminator_type_r4_does_not_define_is_named() -> TestResult {
    let definition = identifier_profile(
        &closed(&json!([{"type": "position", "path": "system"}])),
        &system_slice("mrn", "urn:mrn", "*"),
    );
    let report = patient_with(&[definition], &json!([{"system": "urn:mrn"}]))?;
    assert_eq!(
        unevaluated_discriminators(&report),
        [(
            Constraint::Discriminator {
                kind: "position".to_owned(),
                path: "system".to_owned()
            },
            Reason::DiscriminatorType,
            "Patient.identifier[0]".to_owned()
        )]
    );
    Ok(())
}

/// A closed slicing of `Bundle.entry` by the type of its resource, with one
/// slice whose resource is typed `DomainResource`.
fn domain_resource_entries() -> serde_json::Value {
    crate::support::profile(
        "domain-resource-entries",
        "Bundle",
        &json!([
            {"id": "Bundle", "path": "Bundle", "min": 0, "max": "*"},
            {"id": "Bundle.entry", "path": "Bundle.entry", "min": 0, "max": "*",
             "slicing": {"discriminator": [{"type": "type", "path": "resource"}], "rules": "closed"}},
            {"id": "Bundle.entry:domain", "path": "Bundle.entry", "sliceName": "domain",
             "min": 0, "max": "*"},
            {"id": "Bundle.entry:domain.resource", "path": "Bundle.entry.resource",
             "min": 1, "max": "1", "type": [{"code": "DomainResource"}]}
        ]),
    )
}

/// A collection Bundle whose one entry holds `resource`.
fn bundle_of(resource: &serde_json::Value) -> serde_json::Value {
    json!({"resourceType": "Bundle", "type": "collection",
           "entry": [{"fullUrl": "urn:uuid:00000000-0000-0000-0000-000000000001", "resource": resource}]})
}

#[test]
fn a_type_discriminator_matches_a_resource_that_specializes_the_slice_type() -> TestResult {
    // NOTE: Patient specializes DomainResource (https://hl7.org/fhir/R4/patient.html).
    let url = format!("{}domain-resource-entries", crate::support::BASE);
    let report = super::against(
        &[domain_resource_entries()],
        &bundle_of(&json!({"resourceType": "Patient"})),
        &url,
    )?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_type_discriminator_does_not_match_a_resource_outside_the_slice_type() -> TestResult {
    // NOTE: Binary specializes Resource and not DomainResource (https://hl7.org/fhir/R4/binary.html).
    let url = format!("{}domain-resource-entries", crate::support::BASE);
    let binary = json!({"resourceType": "Binary", "contentType": "text/plain"});
    let report = super::against(&[domain_resource_entries()], &bundle_of(&binary), &url)?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Bundle.entry[0]"]
    );
    Ok(())
}
