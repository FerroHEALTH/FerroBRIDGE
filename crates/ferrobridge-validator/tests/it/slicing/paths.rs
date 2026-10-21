// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Discriminator paths in the simple FHIRPath subset
//! (<https://hl7.org/fhir/R4/fhirpath.html#simple>, §2.9.2): `$this`,
//! `resolve()`, `extension('url')`, `ofType()` and `as()`, and a path
//! outside the subset.

use ferrobridge_validator::issue::MessageId;
use ferrobridge_validator::issue::Severity;
use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use serde_json::json;

use super::against;
use super::identifier_child;
use super::identifier_profile;
use super::identifier_slice;
use super::patient_with;
use super::system_slice;
use super::unevaluated_discriminators;
use crate::support::BASE;
use crate::support::TestResult;
use crate::support::extension;
use crate::support::issues;
use crate::support::locations;
use crate::support::profile;

/// A Patient profile slicing `generalPractitioner` by a `type`
/// discriminator on `$this.resolve()`, with one closed slice for
/// organizations.
fn practitioner_profile() -> serde_json::Value {
    profile(
        "gp-patient",
        "Patient",
        &json!([
            {"id": "Patient", "path": "Patient", "min": 0, "max": "*"},
            {"id": "Patient.generalPractitioner", "path": "Patient.generalPractitioner",
             "min": 0, "max": "*", "type": [{"code": "Reference"}],
             "slicing": {"discriminator": [{"type": "type", "path": "$this.resolve()"}], "rules": "closed"}},
            {"id": "Patient.generalPractitioner:organization", "path": "Patient.generalPractitioner",
             "sliceName": "organization", "min": 0, "max": "*",
             "type": [{"code": "Reference",
                       "targetProfile": ["http://hl7.org/fhir/StructureDefinition/Organization"]}]}
        ]),
    )
}

/// A Patient holding a contained Organization `o1` and a contained
/// Practitioner `p1`, naming `reference` as its general practitioner.
fn patient_naming(reference: &str) -> serde_json::Value {
    json!({
        "resourceType": "Patient",
        "contained": [
            {"resourceType": "Organization", "id": "o1", "name": "Clinic"},
            {"resourceType": "Practitioner", "id": "p1"}
        ],
        "generalPractitioner": [{"reference": reference}],
        "managingOrganization": {"reference": "#o1"}
    })
}

#[test]
fn a_this_resolve_path_resolves_a_contained_resource() -> TestResult {
    let url = format!("{BASE}gp-patient");
    let report = against(&[practitioner_profile()], &patient_naming("#o1"), &url)?;
    assert!(
        locations(&report, MessageId::UnmatchedSliceItem).is_empty(),
        "{:#?}",
        report.issues()
    );
    assert_eq!(
        unevaluated_discriminators(&report),
        Vec::<(Constraint, Reason, String)>::new()
    );
    let report = against(&[practitioner_profile()], &patient_naming("#p1"), &url)?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Patient.generalPractitioner[0]"]
    );
    Ok(())
}

#[test]
fn an_unresolvable_reference_under_resolve_is_reported_and_never_a_silent_non_match() -> TestResult
{
    let url = format!("{BASE}gp-patient");
    let report = against(&[practitioner_profile()], &patient_naming("#absent"), &url)?;
    let found = issues(&report, MessageId::UnresolvedReference);
    assert_eq!(found.len(), 1, "{:#?}", report.issues());
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].expression, "Patient.generalPractitioner[0]");
    assert_eq!(
        unevaluated_discriminators(&report),
        [(
            Constraint::Discriminator {
                kind: "type".to_owned(),
                path: "$this.resolve()".to_owned()
            },
            Reason::UnresolvedReference,
            "Patient.generalPractitioner[0]".to_owned()
        )]
    );
    assert!(
        locations(&report, MessageId::UnmatchedSliceItem).is_empty(),
        "the item is undecided, not refused by the closed rule"
    );
    Ok(())
}

#[test]
fn a_reference_outside_the_resource_is_named_unevaluated() -> TestResult {
    let url = format!("{BASE}gp-patient");
    let report = against(
        &[practitioner_profile()],
        &patient_naming("Organization/1"),
        &url,
    )?;
    assert_eq!(
        unevaluated_discriminators(&report),
        [(
            Constraint::Discriminator {
                kind: "type".to_owned(),
                path: "$this.resolve()".to_owned()
            },
            Reason::ReferenceOutOfScope,
            "Patient.generalPractitioner[0]".to_owned()
        )]
    );
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        Vec::<String>::new()
    );
    Ok(())
}

#[test]
fn a_relative_path_through_resolve_reaches_the_contained_resource() -> TestResult {
    let definition = profile(
        "patient-list",
        "List",
        &json!([
            {"id": "List", "path": "List", "min": 0, "max": "*"},
            {"id": "List.entry", "path": "List.entry", "min": 0, "max": "*",
             "slicing": {"discriminator": [{"type": "type", "path": "item.resolve()"}], "rules": "closed"}},
            {"id": "List.entry:patients", "path": "List.entry", "sliceName": "patients", "min": 0, "max": "*"},
            {"id": "List.entry:patients.item", "path": "List.entry.item", "min": 1, "max": "1",
             "type": [{"code": "Reference",
                       "targetProfile": ["http://hl7.org/fhir/StructureDefinition/Patient"]}]}
        ]),
    );
    let list = |reference: &str| {
        json!({
            "resourceType": "List", "status": "current", "mode": "working",
            "contained": [{"resourceType": "Patient", "id": "a"}, {"resourceType": "Group", "id": "g",
                "type": "person", "actual": true}],
            "entry": [{"item": {"reference": reference}}]
        })
    };
    let url = format!("{BASE}patient-list");
    let report = against(std::slice::from_ref(&definition), &list("#a"), &url)?;
    assert!(
        locations(&report, MessageId::UnmatchedSliceItem).is_empty(),
        "{:#?}",
        report.issues()
    );
    let report = against(&[definition], &list("#g"), &url)?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["List.entry[0]"]
    );
    Ok(())
}

#[test]
fn a_this_path_followed_by_a_step_reads_as_the_relative_path() -> TestResult {
    let definition = identifier_profile(
        &json!({"discriminator": [{"type": "value", "path": "$this.system"}], "rules": "closed"}),
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
fn a_path_outside_the_subset_is_named_unevaluated_never_a_non_match() -> TestResult {
    let definition = identifier_profile(
        &json!({"discriminator": [{"type": "value", "path": "value@$this.function"}], "rules": "closed"}),
        &system_slice("mrn", "urn:mrn", "*"),
    );
    let report = patient_with(&[definition], &json!([{"system": "urn:other"}]))?;
    assert_eq!(
        unevaluated_discriminators(&report),
        [(
            Constraint::Discriminator {
                kind: "value".to_owned(),
                path: "value@$this.function".to_owned()
            },
            Reason::DiscriminatorPath,
            "Patient.identifier[0]".to_owned()
        )]
    );
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        Vec::<String>::new()
    );
    Ok(())
}

/// An Observation profile slicing `component` by an `exists`
/// discriminator on `path`, with one closed slice whose value is a
/// required Quantity.
fn component_profile(path: &str) -> serde_json::Value {
    profile(
        "quantity-components",
        "Observation",
        &json!([
            {"id": "Observation", "path": "Observation", "min": 0, "max": "*"},
            {"id": "Observation.component", "path": "Observation.component", "min": 0, "max": "*",
             "slicing": {"discriminator": [{"type": "exists", "path": path}], "rules": "closed"}},
            {"id": "Observation.component:measured", "path": "Observation.component",
             "sliceName": "measured", "min": 0, "max": "*"},
            {"id": "Observation.component:measured.value[x]", "path": "Observation.component.value[x]",
             "min": 0, "max": "1", "type": [{"code": "Quantity"}, {"code": "string"}]},
            {"id": "Observation.component:measured.value[x]:valueQuantity",
             "path": "Observation.component.value[x]", "sliceName": "valueQuantity",
             "min": 1, "max": "1", "type": [{"code": "Quantity"}]}
        ]),
    )
}

/// An Observation with one component whose value is `value`.
fn observation_with(key: &str, value: &serde_json::Value) -> serde_json::Value {
    let mut component = json!({"code": {"text": "c"}});
    component[key] = value.clone();
    json!({"resourceType": "Observation", "status": "final", "code": {"text": "x"},
           "component": [component]})
}

#[test]
fn an_of_type_path_selects_the_typed_form_of_a_choice() -> TestResult {
    let url = format!("{BASE}quantity-components");
    let definition = component_profile("value.ofType(Quantity)");
    let report = against(
        std::slice::from_ref(&definition),
        &observation_with("valueQuantity", &json!({"value": 1})),
        &url,
    )?;
    assert!(
        locations(&report, MessageId::UnmatchedSliceItem).is_empty(),
        "{:#?}",
        report.issues()
    );
    let report = against(
        &[definition],
        &observation_with("valueString", &json!("x")),
        &url,
    )?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Observation.component[0]"]
    );
    Ok(())
}

#[test]
fn an_as_path_reads_the_same_as_of_type() -> TestResult {
    let url = format!("{BASE}quantity-components");
    let definition = component_profile("value.as(Quantity)");
    let report = against(
        std::slice::from_ref(&definition),
        &observation_with("valueQuantity", &json!({"value": 1})),
        &url,
    )?;
    assert!(
        locations(&report, MessageId::UnmatchedSliceItem).is_empty(),
        "{:#?}",
        report.issues()
    );
    let report = against(
        &[definition],
        &observation_with("valueString", &json!("x")),
        &url,
    )?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Observation.component[0]"]
    );
    Ok(())
}

#[test]
fn an_extension_path_selects_the_extension_by_url() -> TestResult {
    let flag = format!("{BASE}flag");
    let definition = identifier_profile(
        &json!({"discriminator": [{"type": "value", "path": format!("extension('{flag}').value")}],
                "rules": "closed"}),
        &[
            identifier_slice("flagged", 0, "*"),
            identifier_child(
                "flagged",
                "extension",
                &json!({"max": "*",
                "slicing": {"discriminator": [{"type": "value", "path": "url"}], "rules": "open"}}),
            ),
            json!({"id": "Patient.identifier:flagged.extension:flag", "path": "Patient.identifier.extension",
                   "sliceName": "flag", "min": 1, "max": "1",
                   "type": [{"code": "Extension", "profile": [flag]}]}),
            json!({"id": "Patient.identifier:flagged.extension:flag.value[x]",
                   "path": "Patient.identifier.extension.value[x]", "min": 1, "max": "1",
                   "type": [{"code": "boolean"}], "fixedBoolean": true}),
        ],
    );
    let set = [definition, extension("flag", "boolean", false)];
    let identifier =
        |value: bool| json!([{"value": "1", "extension": [{"url": flag, "valueBoolean": value}]}]);
    let report = patient_with(&set, &identifier(true))?;
    assert!(report.is_valid(), "{:#?}", report.issues());
    let report = patient_with(&set, &identifier(false))?;
    assert_eq!(
        locations(&report, MessageId::UnmatchedSliceItem),
        ["Patient.identifier[0]"]
    );
    Ok(())
}
