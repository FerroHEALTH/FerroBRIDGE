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
//! builder of `openehr-sdt` stamps the `v1_2` development generation's 1.2.0
//! on every archetype root. A composition that holds no element outside
//! Release 1.1.0 was created to that release, so it names 1.1.0.

use openehr_rm::Generation;
use serde_json::Value;

/// The member that names an object's RM class in canonical JSON.
const TYPE: &str = "_type";

/// Names the RM release `composition` conforms to in the `rm_version` of
/// every `archetype_details` it holds, and returns that release.
///
/// The release is Release 1.1.0 unless the composition holds an element the
/// development line changed. The one such element a composition can hold is
/// a `GENERIC_ENTRY` whose `data` is no `ITEM_TREE`: Release 1.1.0
/// Integration IM §2.4.1 types `GENERIC_ENTRY.data` as `ITEM_TREE`, and the
/// development line widens it to `ITEM`. Such a composition keeps the
/// development generation's version.
pub(crate) fn stamp(composition: &mut Value) -> Generation {
    let generation = if widens_generic_entry(composition) {
        Generation::V1_2
    } else {
        Generation::V1_1
    };
    name(composition, generation.spec_version());
    generation
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

/// Writes `version` into the `rm_version` of every `archetype_details` under
/// `value`.
fn name(value: &mut Value, version: &str) {
    match value {
        Value::Object(members) => {
            if let Some(Value::Object(details)) = members.get_mut("archetype_details")
                && details.contains_key("rm_version")
            {
                details.insert(String::from("rm_version"), Value::from(version));
            }
            for member in members.values_mut() {
                name(member, version);
            }
        }
        Value::Array(items) => {
            for item in items {
                name(item, version);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
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

    fn details() -> Value {
        json!({"archetype_id": {"value": "openEHR-EHR-X.y.v1"}, "rm_version": "1.2.0"})
    }

    #[test]
    fn a_composition_of_release_elements_names_release_1_1_0() {
        let mut composition = json!({
            "_type": "COMPOSITION",
            "archetype_details": details(),
            "content": [{"_type": "EVALUATION", "archetype_details": details()}]
        });
        assert_eq!(Generation::V1_1, stamp(&mut composition));
        assert_eq!(vec!["1.1.0", "1.1.0"], versions(&composition));
    }

    #[test]
    fn a_generic_entry_over_an_item_tree_is_a_release_element() {
        let mut composition = json!({
            "_type": "COMPOSITION",
            "archetype_details": details(),
            "content": [{"_type": "GENERIC_ENTRY", "archetype_details": details(),
                         "data": {"_type": "ITEM_TREE", "archetype_node_id": "at0001"}}]
        });
        assert_eq!(Generation::V1_1, stamp(&mut composition));
        assert_eq!(vec!["1.1.0", "1.1.0"], versions(&composition));
    }

    #[test]
    fn a_generic_entry_over_another_item_keeps_the_development_version() {
        let mut composition = json!({
            "_type": "COMPOSITION",
            "archetype_details": details(),
            "content": [{"_type": "GENERIC_ENTRY", "archetype_details": details(),
                         "data": {"_type": "CLUSTER", "archetype_node_id": "at0001"}}]
        });
        assert_eq!(Generation::V1_2, stamp(&mut composition));
        assert_eq!(vec!["1.2.0", "1.2.0"], versions(&composition));
    }

    #[test]
    fn details_without_a_version_gain_none() {
        let mut composition = json!({
            "_type": "COMPOSITION",
            "archetype_details": {"archetype_id": {"value": "openEHR-EHR-X.y.v1"}}
        });
        stamp(&mut composition);
        assert!(versions(&composition).is_empty());
    }
}
