// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The references a resource keeps to a resource left out of the Bundle.
//!
//! A reference in a Bundle resolves against the entries' `fullUrl` first
//! (<https://hl7.org/fhir/R4/bundle.html#references>), so one naming a
//! resource the run left out points at nothing. Each is removed from the
//! referring resource, which stays, and counted.

use std::collections::BTreeMap;

use fhir_types::codec::Value;

/// One reference removed: the instance path of the `Reference` element and
/// the resource it named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Unlinked {
    /// The instance path, for example `DiagnosticReport.result[1]`.
    pub(super) element: String,
    /// The full url of the resource left out.
    pub(super) target: String,
    /// The type of the resource left out.
    pub(super) target_resource: String,
}

/// Removes from `value`, the document at `path`, every `reference` naming a
/// key of `dropped` (a full url, mapped to its resource type), recording
/// each, and drops a `Reference` element or an array the removal leaves
/// empty.
///
/// Returns whether `value` itself is left empty by a removal inside it, so
/// its parent drops it.
pub(super) fn unlink(
    value: &mut Value,
    path: &str,
    dropped: &BTreeMap<String, String>,
    found: &mut Vec<Unlinked>,
) -> bool {
    match value {
        Value::Object(object) => {
            let mut removed = false;
            let named = object
                .get("reference")
                .and_then(Value::as_str)
                .and_then(|target| {
                    dropped
                        .get(target)
                        .map(|kind| (String::from(target), kind.clone()))
                });
            if let Some((target, target_resource)) = named {
                object.remove("reference");
                found.push(Unlinked {
                    element: String::from(path),
                    target,
                    target_resource,
                });
                removed = true;
            }
            let keys: Vec<String> = object.keys().cloned().collect();
            for key in keys {
                let emptied = object
                    .get_mut(&key)
                    .is_some_and(|child| unlink(child, &format!("{path}.{key}"), dropped, found));
                if emptied {
                    object.remove(&key);
                    removed = true;
                }
            }
            removed && object.is_empty()
        }
        Value::Array(items) => {
            let before = items.len();
            let mut position = 0usize;
            items.retain_mut(|item| {
                let emptied = unlink(item, &format!("{path}[{position}]"), dropped, found);
                position = position.saturating_add(1);
                !emptied
            });
            items.len() < before && items.is_empty()
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use fhir_types::codec::Value;

    use super::{Unlinked, unlink};

    fn document(text: &str) -> Value {
        serde_json::from_str(text).expect("a JSON document")
    }

    fn unlinked(element: &str) -> Unlinked {
        Unlinked {
            element: String::from(element),
            target: String::from("urn:uuid:b"),
            target_resource: String::from("Observation"),
        }
    }

    #[test]
    fn a_reference_to_a_dropped_resource_is_removed_and_its_element_with_it() {
        let mut report = document(
            r#"{"resourceType":"DiagnosticReport","subject":{"reference":"urn:uuid:p"},
               "result":[{"reference":"urn:uuid:a"},{"reference":"urn:uuid:b","display":"Hb"}],
               "basedOn":[{"reference":"urn:uuid:b"}]}"#,
        );
        let dropped = BTreeMap::from([(String::from("urn:uuid:b"), String::from("Observation"))]);
        let mut found = Vec::new();
        let emptied = unlink(&mut report, "DiagnosticReport", &dropped, &mut found);
        assert!(!emptied, "the resource itself is never emptied");
        assert_eq!(
            report,
            document(
                r#"{"resourceType":"DiagnosticReport","subject":{"reference":"urn:uuid:p"},
                   "result":[{"reference":"urn:uuid:a"},{"display":"Hb"}]}"#
            ),
            "the display keeps its Reference, and the empty basedOn array goes"
        );
        assert_eq!(
            found,
            [
                unlinked("DiagnosticReport.basedOn[0]"),
                unlinked("DiagnosticReport.result[1]"),
            ]
        );
    }

    #[test]
    fn a_document_with_no_dangling_reference_is_left_as_it_is() {
        let text =
            r#"{"resourceType":"Observation","subject":{"reference":"urn:uuid:p"},"note":[]}"#;
        let mut observation = document(text);
        let dropped = BTreeMap::from([(String::from("urn:uuid:x"), String::from("Patient"))]);
        let mut found = Vec::new();
        let emptied = unlink(&mut observation, "Observation", &dropped, &mut found);
        assert!(!emptied);
        assert_eq!(observation, document(text));
        assert_eq!(found, []);
    }
}
