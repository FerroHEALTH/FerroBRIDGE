// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Slicing (<https://hl7.org/fhir/R4/profiling.html#slicing>, §5.1.0.10 to
//! §5.1.0.14) over synthetic profiles: each discriminator type, each path
//! form, the rules, the order, the `@default` slice, re-slicing, extension
//! slices, slice cardinality, and the profiles a type names.

mod discriminators;
mod eps;
mod extensions;
mod paths;
mod profiles;
mod rules;

use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;

use crate::support::BASE;
use crate::support::TestResult;
use crate::support::profile;
use crate::support::set_with;
use crate::support::validate;

/// A Patient profile slicing `Patient.identifier` with `slicing` into the
/// snapshot elements `slices`.
pub(crate) fn identifier_profile(
    slicing: &serde_json::Value,
    slices: &[serde_json::Value],
) -> serde_json::Value {
    let mut elements = vec![
        json!({"id": "Patient", "path": "Patient", "min": 0, "max": "*"}),
        json!({"id": "Patient.identifier", "path": "Patient.identifier", "min": 0, "max": "*",
               "type": [{"code": "Identifier"}], "slicing": slicing}),
    ];
    elements.extend(slices.iter().cloned());
    profile("sliced-patient", "Patient", &json!(elements))
}

/// The slice `name` of `Patient.identifier`, with cardinality `min..max`.
pub(crate) fn identifier_slice(name: &str, min: u32, max: &str) -> serde_json::Value {
    json!({"id": format!("Patient.identifier:{name}"), "path": "Patient.identifier",
           "sliceName": name, "min": min, "max": max, "type": [{"code": "Identifier"}]})
}

/// The child `child` of the slice `slice` of `Patient.identifier`, with the
/// extra properties `extra`.
pub(crate) fn identifier_child(
    slice: &str,
    child: &str,
    extra: &serde_json::Value,
) -> serde_json::Value {
    let mut element = json!({
        "id": format!("Patient.identifier:{slice}.{child}"),
        "path": format!("Patient.identifier.{child}"),
        "min": 0, "max": "1"
    });
    if let (Some(element), Some(extra)) = (element.as_object_mut(), extra.as_object()) {
        element.extend(extra.clone());
    }
    element
}

/// A slice of `Patient.identifier` whose `system` is fixed to `system`.
pub(crate) fn system_slice(name: &str, system: &str, max: &str) -> Vec<serde_json::Value> {
    vec![
        identifier_slice(name, 0, max),
        identifier_child(name, "system", &json!({"fixedUri": system})),
    ]
}

/// The canonical of the profile [`identifier_profile`] builds.
pub(crate) fn sliced_url() -> String {
    format!("{BASE}sliced-patient")
}

/// Validates a Patient carrying `identifiers` against `definitions[0]`, with
/// every definition in the set.
pub(crate) fn patient_with(
    definitions: &[serde_json::Value],
    identifiers: &serde_json::Value,
) -> TestResult<Report> {
    let (_synthetic, set) = set_with(definitions)?;
    let resource = json!({"resourceType": "Patient", "identifier": identifiers});
    validate(&set, &resource, Some(&sliced_url()))
}

/// Validates `resource` against the profile `url`, with `definitions` in
/// the set.
pub(crate) fn against(
    definitions: &[serde_json::Value],
    resource: &serde_json::Value,
    url: &str,
) -> TestResult<Report> {
    let (_synthetic, set) = set_with(definitions)?;
    validate(&set, resource, Some(url))
}

/// The unevaluated entries of `report` for discriminators, as
/// `(constraint, reason, location)`.
pub(crate) fn unevaluated_discriminators(report: &Report) -> Vec<(Constraint, Reason, String)> {
    report
        .unevaluated()
        .iter()
        .filter(|entry| {
            matches!(
                entry.constraint,
                Constraint::Discriminator { .. } | Constraint::DiscriminatorValueSet { .. }
            )
        })
        .map(|entry| {
            (
                entry.constraint.clone(),
                entry.reason,
                entry.location.clone(),
            )
        })
        .collect()
}
