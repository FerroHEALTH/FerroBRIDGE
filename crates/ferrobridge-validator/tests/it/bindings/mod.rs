// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Terminology bindings (<https://hl7.org/fhir/R4/terminologies.html#strength>):
//! each strength on each bindable type, the value set composition the
//! package set evaluates (valueset.html §4.9.5), coding displays, and the
//! checks a terminology server answers.

mod compose;
mod eu;
mod server;
mod strengths;

use ferrobridge_validator::report::Constraint;
use ferrobridge_validator::report::Reason;
use ferrobridge_validator::report::Report;
use serde_json::json;

use crate::support::BASE;
use crate::support::Synthetic;
use crate::support::TestResult;
use crate::support::profile;
use crate::support::set_with;
use crate::support::validate;

/// The canonical base of the synthetic value sets.
pub(crate) const VS: &str = "http://example.org/fhir/ValueSet/";

/// The synthetic code system every local case draws from.
pub(crate) const COLORS: &str = "http://example.org/fhir/CodeSystem/colors";

/// The canonical of the profile [`bound`] writes.
pub(crate) fn bound_url() -> String {
    format!("{BASE}bound")
}

/// A complete code system with an is-a hierarchy: `red`, `blue` with the
/// child `navy`, and `green`; each with the property `hue`.
pub(crate) fn colors() -> serde_json::Value {
    json!({
        "resourceType": "CodeSystem", "id": "colors", "url": COLORS, "version": "1.0.0",
        "status": "active", "content": "complete", "hierarchyMeaning": "is-a",
        "property": [{"code": "hue", "type": "code"}],
        "concept": [
            {"code": "red", "display": "Red", "designation": [{"value": "Crimson"}],
             "property": [{"code": "hue", "valueCode": "warm"}]},
            {"code": "blue", "display": "Blue", "property": [{"code": "hue", "valueCode": "cold"}],
             "concept": [{"code": "navy", "display": "Navy", "property": [{"code": "hue", "valueCode": "cold"}]}]},
            {"code": "green", "display": "Green", "property": [{"code": "hue", "valueCode": "cold"}]}
        ]
    })
}

/// A value set `id` with the compose `compose`.
pub(crate) fn value_set(id: &str, compose: &serde_json::Value) -> serde_json::Value {
    json!({
        "resourceType": "ValueSet", "id": id, "url": format!("{VS}{id}"),
        "status": "active", "compose": compose
    })
}

/// The value set of the listed codes `red` and `blue`.
pub(crate) fn listed() -> serde_json::Value {
    value_set(
        "listed",
        &json!({"include": [{"system": COLORS, "concept": [{"code": "red"}, {"code": "blue"}]}]}),
    )
}

/// The value set of every code of [`colors`].
pub(crate) fn every_color() -> serde_json::Value {
    value_set("every-color", &json!({"include": [{"system": COLORS}]}))
}

/// A Patient profile whose element `path`, of the type `kind` and at most
/// `max` occurrences, carries `binding`.
pub(crate) fn bound(
    path: &str,
    kind: &str,
    max: &str,
    binding: &serde_json::Value,
) -> serde_json::Value {
    let mut elements = vec![json!({"id": "Patient", "path": "Patient", "min": 0, "max": "*"})];
    let mut prefix = String::from("Patient");
    let segments: Vec<&str> = path.split('.').skip(1).collect();
    for (index, segment) in segments.iter().enumerate() {
        prefix = format!("{prefix}.{segment}");
        if index + 1 < segments.len() {
            elements.push(json!({"id": prefix, "path": prefix, "min": 0, "max": "*"}));
        }
    }
    elements.push(json!({
        "id": path, "path": path, "min": 0, "max": max,
        "type": [{"code": kind}], "binding": binding
    }));
    profile("bound", "Patient", &json!(elements))
}

/// A binding of `strength` to the synthetic value set `id`.
pub(crate) fn binding(strength: &str, id: &str) -> serde_json::Value {
    json!({"strength": strength, "valueSet": format!("{VS}{id}")})
}

/// The package set of `resources`, [`colors`], [`listed`] and
/// [`every_color`].
pub(crate) fn terminology_set(
    resources: &[serde_json::Value],
) -> TestResult<(Synthetic, ferrobridge_validator::set::PackageSet)> {
    let mut all = resources.to_vec();
    all.extend([colors(), listed(), every_color()]);
    set_with(&all)
}

/// Validates `resource` against the profile `definition`, with the
/// terminology set and `extra` resources beside it.
pub(crate) fn check(
    definition: &serde_json::Value,
    extra: &[serde_json::Value],
    resource: &serde_json::Value,
) -> TestResult<Report> {
    let mut resources = vec![definition.clone()];
    resources.extend(extra.iter().cloned());
    let (_synthetic, set) = terminology_set(&resources)?;
    validate(&set, resource, Some(&bound_url()))
}

/// The binding listings of `report`, as constraint and reason.
pub(crate) fn binding_listings(report: &Report) -> Vec<(Constraint, Reason)> {
    report
        .unevaluated()
        .iter()
        .filter(|entry| {
            matches!(
                entry.constraint,
                Constraint::Binding { .. } | Constraint::MaxValueSet { .. }
            )
        })
        .map(|entry| (entry.constraint.clone(), entry.reason.clone()))
        .collect()
}
