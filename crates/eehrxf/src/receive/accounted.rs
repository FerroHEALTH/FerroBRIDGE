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
//!
//! The engine records a read where a mapping takes a value, so the section
//! that leads to an entry records none, and neither does a reference a
//! mapping follows into its target. A project that accounts for [`SECTION`]
//! declares each section of the received `Composition` none of whose entries
//! resolves to a resource the run read, at the section, once, and an instance
//! of an element it follows (`crate::mappings::Project::followed`) that
//! resolves to a resource the run read is carried.

use std::collections::BTreeSet;

use fhir_types::codec::Value;
use fhir_types::r4::operation_outcome::OperationOutcomeIssue;
use fhir_types::r4::primitives;
use fhirconnect::engine::outcome::Location;
use fhirconnect::operations::issues::WARNING;
use fhirconnect::operations::issues::issue;

use crate::receive::reference::{self, Target};

/// The `IssueType` code of an element the composition does not carry
/// (<https://hl7.org/fhir/R4/valueset-issue-type.html>), the code the engine
/// gives its own losses.
const INCOMPLETE: &str = "incomplete";

/// The sentence every issue carries.
const NOT_CARRIED: &str = "the element is carried into no structured element of the \
                           composition; the composition's original content keeps it";

/// The accounted path of a section of the received `Composition`, carried
/// when one of its entries is.
const SECTION: &str = "Composition.section";

/// Returns one issue per present instance of an element `accounted` names
/// that no element of `reads` reached, in the entries of the Bundle `document`;
/// an instance of an element `followed` names that resolves to a resource
/// `reads` reached is carried.
///
/// Each `accounted` entry is a FHIRPath of element names rooted at a resource
/// type (`Observation.referenceRange`) with no choice element on it. An
/// instance is reported at the shallowest path below which no read lies, so
/// a range no mapping read is one issue and a range whose bounds were read
/// reports only what of it was not.
pub(super) fn not_carried(
    document: &Value,
    accounted: &[&str],
    followed: &[&str],
    reads: &[Location],
) -> Vec<OperationOutcomeIssue> {
    let mut issues = Vec::new();
    let Some(Value::Array(entries)) = document.get("entry") else {
        return issues;
    };
    let resolved = Resolved::new(document, reads);
    for (holder, resource) in entries
        .iter()
        .enumerate()
        .filter_map(|(holder, entry)| Some((holder, entry.get("resource")?)))
    {
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
        if kind == "Composition" && accounted.contains(&SECTION) {
            sections(&resolved, holder, &mut issues);
        }
        for path in accounted.iter().filter(|path| **path != SECTION) {
            let Some((root, steps)) = path.split_once('.') else {
                continue;
            };
            if root != kind {
                continue;
            }
            let steps: Vec<&str> = steps.split('.').collect();
            let mut found = Vec::new();
            instances(resource, String::from(kind), &steps, &mut found);
            let follows = followed.contains(path);
            for (at, value) in found {
                if !(follows && resolved.reaches_read(holder, value)) {
                    report(&at, value, &read, &mut issues);
                }
            }
        }
    }
    issues
}

/// The entries of a received document and the resources a run read, to tell
/// whether a reference names a resource the run carried.
struct Resolved<'d> {
    entries: Vec<reference::Entry<'d>>,
    read: BTreeSet<&'d str>,
}

impl<'d> Resolved<'d> {
    /// Reads the entries of the Bundle `document` and the resources `reads`
    /// reached.
    fn new(document: &'d Value, reads: &'d [Location]) -> Self {
        Self {
            entries: document
                .as_object()
                .map(reference::entries)
                .unwrap_or_default(),
            read: reads.iter().filter_map(Location::resource).collect(),
        }
    }

    /// Returns whether `value`, held by the resource of the entry at
    /// `holder`, is a reference that resolves to an entry whose resource the
    /// run read.
    fn reaches_read(&self, holder: usize, value: &Value) -> bool {
        let Some(reference) = value.as_object() else {
            return false;
        };
        // NOTE: no specification governs this: our own design; a reference that names no single entry
        // names no resource the run read, so it carries nothing.
        let Ok(Target::Entry(index)) = reference::resolve_in(&self.entries, holder, reference)
        else {
            return false;
        };
        self.entries
            .get(index)
            .and_then(|entry| entry.resource)
            .and_then(|resource| {
                let kind = resource.get("resourceType").and_then(Value::as_str)?;
                let id = resource.get("id").and_then(Value::as_str)?;
                Some(format!("{kind}/{id}"))
            })
            .is_some_and(|named| self.read.contains(named.as_str()))
    }
}

/// Appends one issue per section of the `Composition` of the entry at
/// `holder` none of whose entries resolves to a resource the run read.
fn sections(resolved: &Resolved<'_>, holder: usize, issues: &mut Vec<OperationOutcomeIssue>) {
    let Some(Value::Array(held)) = resolved
        .entries
        .get(holder)
        .and_then(|entry| entry.resource)
        .and_then(|composition| composition.get("section"))
    else {
        return;
    };
    for (index, section) in held.iter().enumerate() {
        let carried = section
            .get("entry")
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .any(|entry| resolved.reaches_read(holder, entry));
        if !carried {
            let mut declared = issue(WARNING, INCOMPLETE, NOT_CARRIED);
            declared.expression = vec![primitives::String::from(format!("{SECTION}[{index}]"))];
            issues.push(declared);
        }
    }
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
