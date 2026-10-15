// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! References inside a Bundle (<https://hl7.org/fhir/R4/bundle.html#references>,
//! §2.36.4.1), the target profiles and aggregation a reference is held to
//! (elementdefinition.html §2.30.0.4.4), every entry's profiles, and the
//! document rules (<https://hl7.org/fhir/R4/documents.html#content>).

mod documents;
mod entries;
mod resolution;
mod targets;

use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;

/// A Bundle of type `kind` holding `entries`, each a fullUrl and a resource.
pub(crate) fn bundle(kind: &str, entries: &[(&str, serde_json::Value)]) -> serde_json::Value {
    let entry: Vec<serde_json::Value> = entries
        .iter()
        .map(|(full_url, resource)| json!({"fullUrl": full_url, "resource": resource}))
        .collect();
    json!({"resourceType": "Bundle", "type": kind, "entry": entry})
}

/// A synthetic Observation whose subject is `reference`.
pub(crate) fn observation(reference: &str) -> serde_json::Value {
    json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"text": "synthetic"},
        "subject": {"reference": reference}
    })
}

/// A synthetic Patient.
pub(crate) fn patient() -> serde_json::Value {
    json!({"resourceType": "Patient", "name": [{"family": "Doe"}]})
}

/// A synthetic Organization.
pub(crate) fn organization() -> serde_json::Value {
    json!({"resourceType": "Organization", "name": "Clinic"})
}

/// The reasons the target profiles of the element `element` were listed
/// unevaluated for.
pub(crate) fn target_reasons(report: &Report, element: &str) -> Vec<Reason> {
    report
        .unevaluated()
        .iter()
        .filter(|entry| {
            entry.element == element && matches!(entry.constraint, Constraint::TargetProfile { .. })
        })
        .map(|entry| entry.reason.clone())
        .collect()
}
