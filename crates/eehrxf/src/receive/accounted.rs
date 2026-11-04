// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The received elements a mapping project accounts for, and the issue that
//! declares each one the run carried into no structured element (feature
//! `openehr`).
//!
//! FHIRconnect 1.0.0 is silent on an input element no mapping names, so a run
//! that leaves one out reports nothing; the engine lists instead every element
//! a mapping read (`fhirconnect::engine::outcome::Outcome::reads`). A project
//! names the elements whose loss a reader of the composition must be told of
//! ([`crate::mappings::Project::accounted`]), and each present instance of one
//! that no read reached, in the resource the read names, becomes a `warning`
//! issue of type `incomplete` whose `expression` is the element's FHIRPath,
//! the shape the engine's own losses take. The element stays in the
//! composition's `FEEDER_AUDIT.original_content`. No specification governs
//! this: our own design.

use fhir_types::codec::Value;
use fhir_types::r4::operation_outcome::OperationOutcomeIssue;
use fhir_types::r4::primitives;
use fhirconnect::engine::outcome::Location;
use fhirconnect::operations::issues::WARNING;
use fhirconnect::operations::issues::issue;

/// The `IssueType` code of an element the composition does not carry
/// (<https://hl7.org/fhir/R4/valueset-issue-type.html>), the code the engine
/// gives its own losses.
const INCOMPLETE: &str = "incomplete";

/// The sentence every issue carries.
const NOT_CARRIED: &str = "the element is carried into no structured element of the \
                           composition; the composition's original content keeps it";

/// Returns one issue per present instance of an element `accounted` names
/// that no element of `reads` reached, in the entries of the Bundle `document`.
///
/// Each `accounted` entry is a FHIRPath of element names rooted at a resource
/// type (`Observation.referenceRange`) with no choice element on it. An
/// instance is reported at the shallowest path below which no read lies, so
/// a range no mapping read is one issue and a range whose bounds were read
/// reports only what of it was not.
pub(super) fn not_carried(
    document: &Value,
    accounted: &[&str],
    reads: &[Location],
) -> Vec<OperationOutcomeIssue> {
    let mut issues = Vec::new();
    let Some(Value::Array(entries)) = document.get("entry") else {
        return issues;
    };
    for resource in entries.iter().filter_map(|entry| entry.get("resource")) {
        let Some(kind) = resource.get("resourceType").and_then(Value::as_str) else {
            continue;
        };
        let reference = resource
            .get("id")
            .and_then(Value::as_str)
            .map(|id| format!("{kind}/{id}"));
        let read: Vec<&str> = reads
            .iter()
            .filter(|location| location.resource() == reference.as_deref())
            .filter_map(Location::fhir)
            .collect();
        for path in accounted {
            let Some((root, steps)) = path.split_once('.') else {
                continue;
            };
            if root != kind {
                continue;
            }
            let steps: Vec<&str> = steps.split('.').collect();
            let mut found = Vec::new();
            instances(resource, String::from(kind), &steps, &mut found);
            for (at, value) in found {
                report(&at, value, &read, &mut issues);
            }
        }
    }
    issues
}

/// Appends every instance of the element `steps` names below `value`, with
/// its FHIRPath rendered from `path`, an index on every repeating step.
fn instances<'value>(
    value: &'value Value,
    path: String,
    steps: &[&str],
    found: &mut Vec<(String, &'value Value)>,
) {
    let Some((step, rest)) = steps.split_first() else {
        found.push((path, value));
        return;
    };
    match value.get(step) {
        Some(Value::Array(items)) => {
            for (index, item) in items.iter().enumerate() {
                instances(item, format!("{path}.{step}[{index}]"), rest, found);
            }
        }
        Some(member) => instances(member, format!("{path}.{step}"), rest, found),
        None => {}
    }
}

/// Appends the issues of the instance `value` at `path` that `read` leaves
/// unreached.
fn report(path: &str, value: &Value, read: &[&str], issues: &mut Vec<OperationOutcomeIssue>) {
    if read.iter().any(|reached| within(path, reached)) {
        return;
    }
    if !read.iter().any(|reached| within(reached, path)) {
        let mut declared = issue(WARNING, INCOMPLETE, NOT_CARRIED);
        declared.expression = vec![primitives::String::from(path)];
        issues.push(declared);
        return;
    }
    match value {
        Value::Object(members) => {
            for (key, member) in members {
                // NOTE: R4 json.html §Primitive Types: `_name` holds the id and
                // extensions of the primitive `name`, so it is reported under it.
                let name = key.strip_prefix('_').unwrap_or(key);
                report(&format!("{path}.{name}"), member, read, issues);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                report(&format!("{path}[{index}]"), item, read, issues);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

/// Returns whether the element at `path` lies at or below `ancestor`.
fn within(path: &str, ancestor: &str) -> bool {
    path.strip_prefix(ancestor)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.') || rest.starts_with('['))
}

#[cfg(test)]
mod tests {
    use super::within;

    #[test]
    fn an_element_lies_within_itself_and_its_ancestors_alone() {
        assert!(within("Observation.basedOn[0]", "Observation.basedOn[0]"));
        assert!(within(
            "Observation.basedOn[0].identifier",
            "Observation.basedOn[0]"
        ));
        assert!(within("Observation.basedOn[0]", "Observation.basedOn"));
        assert!(!within("Observation.basedOn[1]", "Observation.basedOn[0]"));
        assert!(!within("Observation.basedOnX", "Observation.basedOn"));
    }
}
