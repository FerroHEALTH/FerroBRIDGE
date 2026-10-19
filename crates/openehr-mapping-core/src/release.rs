// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The RM release a composition this crate builds names in
//! `ARCHETYPED.rm_version`.
//!
//! RM Common IM Release 1.1.0 §3.2.3 defines `rm_version` as the "Version of
//! the openEHR reference model used to create this object", expressed as a
//! release version string. openEHR has published Releases 1.0.2 to 1.1.0 and
//! a development line, with no 1.2.0
//! (<https://specifications.openehr.org/releases/RM/>). The composition
//! builder of `openehr-sdt` writes the release its caller names, and a
//! composition that holds no element outside Release 1.1.0 was created to
//! that release, so it names 1.1.0.

use openehr_rm::Generation;
use serde_json::Value;

/// The member that names an object's RM class in canonical JSON.
const TYPE: &str = "_type";

/// Builds a composition through `build`, naming the RM release it conforms
/// to.
///
/// `build` is called with Release 1.1.0 first. The release is Release 1.1.0
/// unless the result holds an element the development line changed, and the
/// one such element a composition can hold is a `GENERIC_ENTRY` whose `data`
/// is no `ITEM_TREE`: Release 1.1.0 Integration IM §2.4.1 types
/// `GENERIC_ENTRY.data` as `ITEM_TREE`, and the development line widens it to
/// `ITEM`. Such a composition is built a second time naming the development
/// generation's version, since the element only shows once the template has
/// been walked (no specification governs this: our own design).
///
/// # Errors
///
/// Returns the error of `build`.
pub(crate) fn build<E>(mut build: impl FnMut(Generation) -> Result<Value, E>) -> Result<Value, E> {
    let composition = build(Generation::V1_1)?;
    if widens_generic_entry(&composition) {
        build(Generation::V1_2)
    } else {
        Ok(composition)
    }
}

/// Answers whether `value` holds a `GENERIC_ENTRY` whose `data` is no
/// `ITEM_TREE`.
fn widens_generic_entry(value: &Value) -> bool {
    match value {
        Value::Object(members) => {
            let widened = members.get(TYPE).and_then(Value::as_str) == Some("GENERIC_ENTRY")
                && members
                    .get("data")
                    .and_then(|data| data.get(TYPE))
                    .and_then(Value::as_str)
                    != Some("ITEM_TREE");
            widened || members.values().any(widens_generic_entry)
        }
        Value::Array(items) => items.iter().any(widens_generic_entry),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;

    use serde_json::json;

    use super::*;

    fn versions(value: &Value) -> Vec<String> {
        let mut found = Vec::new();
        collect(value, &mut found);
        found
    }

    fn collect(value: &Value, found: &mut Vec<String>) {
        match value {
            Value::Object(members) => {
                if let Some(version) = members
                    .get("archetype_details")
                    .and_then(|details| details.get("rm_version"))
                    .and_then(Value::as_str)
                {
                    found.push(version.to_owned());
                }
                members.values().for_each(|member| collect(member, found));
            }
            Value::Array(items) => items.iter().for_each(|item| collect(item, found)),
            _ => {}
        }
    }

    fn details(release: Generation) -> Value {
        json!({"archetype_id": {"value": "openEHR-EHR-X.y.v1"},
               "rm_version": release.spec_version()})
    }

    /// Builds `content` as a stand-in builder would, naming the release it is
    /// asked for, and returns the result with every release it was asked for.
    fn built(content: fn(Generation) -> Value) -> (Value, Vec<Generation>) {
        let mut asked = Vec::new();
        let result = build(|release| {
            asked.push(release);
            Ok::<Value, Infallible>(json!({
                "_type": "COMPOSITION",
                "archetype_details": details(release),
                "content": [content(release)]
            }))
        });
        let Ok(value) = result;
        (value, asked)
    }

    #[test]
    fn a_composition_of_release_elements_names_release_1_1_0() {
        let (composition, asked) =
            built(|release| json!({"_type": "EVALUATION", "archetype_details": details(release)}));
        assert_eq!(vec![Generation::V1_1], asked);
        assert_eq!(vec!["1.1.0", "1.1.0"], versions(&composition));
    }

    #[test]
    fn a_generic_entry_over_an_item_tree_is_a_release_element() {
        let (composition, asked) = built(|release| {
            json!({"_type": "GENERIC_ENTRY", "archetype_details": details(release),
                   "data": {"_type": "ITEM_TREE", "archetype_node_id": "at0001"}})
        });
        assert_eq!(vec![Generation::V1_1], asked);
        assert_eq!(vec!["1.1.0", "1.1.0"], versions(&composition));
    }

    #[test]
    fn a_generic_entry_over_another_item_names_the_development_version() {
        let (composition, asked) = built(|release| {
            json!({"_type": "GENERIC_ENTRY", "archetype_details": details(release),
                   "data": {"_type": "CLUSTER", "archetype_node_id": "at0001"}})
        });
        assert_eq!(vec![Generation::V1_1, Generation::V1_2], asked);
        assert_eq!(vec!["1.2.0", "1.2.0"], versions(&composition));
    }

    #[test]
    fn a_refused_build_is_not_retried() {
        let mut calls = 0_u8;
        let result = build(|_| {
            calls = calls.saturating_add(1);
            Err::<Value, &str>("refused")
        });
        assert_eq!(Err("refused"), result);
        assert_eq!(1, calls);
    }
}
