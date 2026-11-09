// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The RM release a composition this crate builds names in
//! `ARCHETYPED.rm_version`.
//!
//! RM Common IM Release 1.1.0 §3.2.3 defines `rm_version` as the "Version of
//! the openEHR reference model used to create this object". The composition
//! builder of `openehr-sdt` writes the release its caller names, and
//! `openehr_rm::Generation::earliest_for_composition` answers the oldest
//! release whose model admits everything a composition holds, which is the
//! release it was created to.

use openehr_rm::Generation;
use openehr_rm::release::NoGenerationDecodes;
use serde_json::Value;

/// Builds a composition through `build`, naming the RM release it conforms
/// to.
///
/// `build` is called with Release 1.1.0 first, and the result is decoded
/// oldest generation first. A composition only a later generation decodes is
/// built a second time naming that generation's version, since the element
/// that needs it only shows once the template has been walked (no
/// specification governs this: our own design).
///
/// # Errors
///
/// Returns the error of `build`, and the error `refused` makes when no
/// generation decodes the result.
pub(crate) fn build<E>(
    mut build: impl FnMut(Generation) -> Result<Value, E>,
    refused: impl FnOnce(NoGenerationDecodes) -> E,
) -> Result<Value, E> {
    let composition = build(Generation::V1_1)?;
    match Generation::earliest_for_composition(&composition) {
        Ok(Generation::V1_1) => Ok(composition),
        Ok(later) => build(later),
        Err(error) => Err(refused(error)),
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

    fn text(value: &str) -> Value {
        json!({"_type": "DV_TEXT", "value": value})
    }

    fn code(terminology: &str, code: &str, value: &str) -> Value {
        json!({"_type": "DV_CODED_TEXT", "value": value,
               "defining_code": {"_type": "CODE_PHRASE",
                                 "terminology_id": {"_type": "TERMINOLOGY_ID", "value": terminology},
                                 "code_string": code}})
    }

    fn details(archetype: &str, release: Generation) -> Value {
        json!({"_type": "ARCHETYPED",
               "archetype_id": {"_type": "ARCHETYPE_ID", "value": archetype},
               "template_id": {"_type": "TEMPLATE_ID", "value": "ferrobridge.release.v1"},
               "rm_version": release.spec_version()})
    }

    /// A composition holding `content`, built as a stand-in builder would,
    /// naming the release it is asked for.
    fn composition(release: Generation, content: &Value) -> Value {
        json!({
            "_type": "COMPOSITION",
            "name": text("Release"),
            "archetype_node_id": "openEHR-EHR-COMPOSITION.report.v1",
            "archetype_details": details("openEHR-EHR-COMPOSITION.report.v1", release),
            "language": {"_type": "CODE_PHRASE",
                         "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "ISO_639-1"},
                         "code_string": "en"},
            "territory": {"_type": "CODE_PHRASE",
                          "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "ISO_3166-1"},
                          "code_string": "NL"},
            "category": code("openehr", "433", "event"),
            "composer": {"_type": "PARTY_SELF"},
            "content": [content]
        })
    }

    fn generic_entry(release: Generation, data: &Value) -> Value {
        json!({"_type": "GENERIC_ENTRY",
               "name": text("Entry"),
               "archetype_node_id": "openEHR-EHR-GENERIC_ENTRY.x.v1",
               "archetype_details": details("openEHR-EHR-GENERIC_ENTRY.x.v1", release),
               "data": data})
    }

    fn element() -> Value {
        json!({"_type": "ELEMENT", "name": text("Value"), "archetype_node_id": "at0002",
               "value": text("v")})
    }

    /// Builds the composition `content` makes for each release asked for, and
    /// returns the result with every release asked for.
    fn built(content: fn(Generation) -> Value) -> (Result<Value, String>, Vec<Generation>) {
        let mut asked = Vec::new();
        let result = build(
            |release| {
                asked.push(release);
                Ok::<Value, String>(composition(release, &content(release)))
            },
            |refusal| refusal.to_string(),
        );
        (result, asked)
    }

    #[test]
    fn a_composition_of_release_elements_names_release_1_1_0() {
        let (composition, asked) = built(|release| {
            generic_entry(
                release,
                &json!({"_type": "ITEM_TREE", "name": text("Tree"), "archetype_node_id": "at0001",
                       "items": [element()]}),
            )
        });
        assert_eq!(vec![Generation::V1_1], asked);
        assert_eq!(
            Ok(vec![String::from("1.1.0"), String::from("1.1.0")]),
            composition.map(|value| versions(&value))
        );
    }

    #[test]
    fn a_generic_entry_over_an_element_names_the_development_version() {
        let (composition, asked) = built(|release| generic_entry(release, &element()));
        assert_eq!(vec![Generation::V1_1, Generation::V1_2], asked);
        assert_eq!(
            Ok(vec![String::from("1.2.0"), String::from("1.2.0")]),
            composition.map(|value| versions(&value))
        );
    }

    #[test]
    fn a_generic_entry_over_an_item_single_is_refused() {
        // NOTE: RM data_structures, `ITEM_SINGLE` is an `ITEM_STRUCTURE` and no
        // `ITEM`, so neither release admits it as `GENERIC_ENTRY.data`.
        let (composition, asked) = built(|release| {
            generic_entry(
                release,
                &json!({"_type": "ITEM_SINGLE", "name": text("Single"), "archetype_node_id": "at0001",
                       "item": element()}),
            )
        });
        assert_eq!(vec![Generation::V1_1], asked);
        assert!(composition.is_err(), "{composition:?}");
    }

    #[test]
    fn a_refused_build_is_not_retried() {
        let mut calls = 0_u8;
        let result = build(
            |_| {
                calls = calls.saturating_add(1);
                Err::<Value, &str>("refused")
            },
            |_| "undecodable",
        );
        assert_eq!(Err("refused"), result);
        assert_eq!(1, calls);
    }
}
