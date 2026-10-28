// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The entries of a composition one program maps, and the paths that name
//! them.
//!
//! An entry is one occurrence of the archetype the start model mapping of a
//! program declares, wherever the template places it: a `content` item, an
//! item of a `SECTION` at any depth, or an instance repeating below another
//! entry. A context whose start is a composition model maps the composition
//! itself (`types-of-mapping-files/composition-element.adoc`). The entries
//! and their paths are the ones the engine's `$tofhir` names
//! ([`fhirconnect::engine::traverse::occurrence::occurrences`]), so the
//! facade derives the deterministic id the operation answers for the same
//! occurrence (FHIRconnect v1.0.0 `engine/id-management.adoc`, §Deterministic
//! IDs). The identity map keys each entry by [`Entry::identity_path`], and
//! [`entry_named`] reads a recorded path back to its entry.

use fhirconnect::engine::identity::ROOT_PATH;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::occurrence::occurrences;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_rm::v1_2::paths::RmPath;
use openehr_rm::v1_2::paths::item_at_path;

/// Where in a composition one entry a program maps sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The path that names the entry, as the engine renders it.
    rendered: String,
    /// The parsed path; `None` for the composition itself.
    path: Option<RmPath>,
    /// The paths of every other entry of the same program in the composition.
    others: Vec<RmPath>,
    /// The 1-based place of the entry among the program's entries.
    ordinal: usize,
}

/// Why the entries of a composition cannot be named.
#[derive(Debug, thiserror::Error)]
pub enum EntryError {
    /// The engine could not name the occurrences of the start archetype.
    #[error("the occurrences of the program's start archetype cannot be named")]
    Engine(#[source] EngineError),
    /// The engine named an occurrence by a path the openEHR path grammar
    /// refuses.
    #[error("the engine named an occurrence {path}, which does not parse as an openEHR path")]
    Path {
        /// The path the engine rendered.
        path: String,
    },
}

impl Entry {
    /// Returns the 1-based place of the entry among the program's entries.
    #[must_use]
    pub const fn ordinal(&self) -> usize {
        self.ordinal
    }

    /// Returns the path the identity map keys the entry by, the entry path
    /// the deterministic id derives from.
    ///
    /// At a repeating node the first instance keeps the template's predicate
    /// and every later one takes its 1-based place in the container, as
    /// openEHR BASE Release 1.2.0, Paths and Locators, §Using Positional
    /// Parameters, defines it, so the path parses back and navigates to the
    /// entry; the composition itself is `/`.
    #[must_use]
    pub fn identity_path(&self) -> &str {
        &self.rendered
    }

    /// Returns `composition` with every other entry of the program removed,
    /// wherever it sits, so the program maps this entry alone.
    #[must_use]
    pub fn isolate(&self, composition: &CanonicalComposition) -> CanonicalComposition {
        if self.path.is_none() || self.others.is_empty() {
            return composition.clone();
        }
        let mut value = composition.value().clone();
        for other in &self.others {
            if let Some(slot) = locate_mut(&mut value, other) {
                *slot = serde_json::Value::String(String::from(REMOVED));
            }
        }
        sweep(&mut value);
        CanonicalComposition::new(value, composition.template_id(), composition.generation())
    }

    /// Returns the entry's item in `composition`.
    #[must_use]
    pub fn item<'c>(&self, composition: &'c CanonicalComposition) -> Option<&'c serde_json::Value> {
        match self.path {
            Some(ref path) => item_at_path(composition.value(), path),
            None => Some(composition.value()),
        }
    }

    /// Returns `composition` with the entry's item replaced by `item`, and
    /// every other item and field as it stands.
    ///
    /// An update of one entry's resource replaces that entry alone, because
    /// every other entry is a resource of its own (no specification governs
    /// how one entry is spliced into a composition: our own design).
    #[must_use]
    pub fn replaced(
        &self,
        composition: &CanonicalComposition,
        item: serde_json::Value,
    ) -> CanonicalComposition {
        let Some(ref path) = self.path else {
            return CanonicalComposition::new(
                item,
                composition.template_id(),
                composition.generation(),
            );
        };
        let mut value = composition.value().clone();
        if let Some(slot) = locate_mut(&mut value, path) {
            *slot = item;
        }
        CanonicalComposition::new(value, composition.template_id(), composition.generation())
    }

    /// Whether the entry's item in `composition` is `node` or holds it.
    #[must_use]
    pub fn holds(&self, composition: &CanonicalComposition, node: &serde_json::Value) -> bool {
        self.item(composition)
            .is_some_and(|item| contains(item, node))
    }
}

/// The marker an entry [`Entry::isolate`] removes stands as until the sweep.
///
/// A canonical composition holds no string inside a container of `LOCATABLE`
/// nodes, so the marker cannot collide with content (no specification
/// governs this: our own design).
const REMOVED: &str = "\u{0}ferrobridge-removed-entry";

/// Returns the node `path` names in `value`, for writing.
///
/// Each step reads as [`openehr_rm::v1_2::paths::select_children`] does: a
/// positional predicate is the 1-based place in the container and the other
/// conjuncts must still match, and without one the first matching element is
/// taken, which is how the engine renders the first instance.
fn locate_mut<'v>(
    value: &'v mut serde_json::Value,
    path: &RmPath,
) -> Option<&'v mut serde_json::Value> {
    let mut current = value;
    for segment in &path.segments {
        if segment.descendant {
            return None;
        }
        let predicate = &segment.predicate;
        let child = current.get_mut(&segment.attribute)?;
        current = match child {
            serde_json::Value::Array(items) => {
                let index = match predicate.position {
                    Some(position) => position.checked_sub(1).filter(|index| {
                        items
                            .get(*index)
                            .is_some_and(|item| predicate.matches(item))
                    })?,
                    None => items.iter().position(|item| predicate.matches(item))?,
                };
                items.get_mut(index)?
            }
            single => {
                if predicate.position.is_some_and(|position| position != 1)
                    || !predicate.matches(single)
                {
                    return None;
                }
                single
            }
        };
    }
    Some(current)
}

/// Removes every [`REMOVED`] marker from the containers of `value`.
fn sweep(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => {
            items.retain(|item| item.as_str() != Some(REMOVED));
            items.iter_mut().for_each(sweep);
        }
        serde_json::Value::Object(members) => members.values_mut().for_each(sweep),
        _ => {}
    }
}

/// Whether `value` is `node` or holds it at any depth.
fn contains(value: &serde_json::Value, node: &serde_json::Value) -> bool {
    if value == node {
        return true;
    }
    match value {
        serde_json::Value::Object(members) => members.values().any(|child| contains(child, node)),
        serde_json::Value::Array(items) => items.iter().any(|child| contains(child, node)),
        _ => false,
    }
}

/// Returns the first entry of `program` in `composition`.
///
/// A composition with no such entry answers `None`: the program mapped no entry
/// of its own archetype, which the caller reports rather than papering over.
///
/// # Errors
///
/// Returns [`EntryError`] when the entries cannot be named.
pub fn entry_of(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<Option<Entry>, EntryError> {
    Ok(entries_of(program, index, composition)?.into_iter().next())
}

/// Returns every entry of `program` in `composition`, in document order,
/// named as the engine's `$tofhir` names each occurrence.
///
/// # Errors
///
/// Returns [`EntryError`] when the template cannot answer for the start
/// archetype, or the engine renders a path the openEHR grammar refuses.
pub fn entries_of(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<Vec<Entry>, EntryError> {
    let found = occurrences(program, index, composition).map_err(EntryError::Engine)?;
    let mut named = Vec::with_capacity(found.len());
    for occurrence in &found {
        let rendered = occurrence.path();
        let path = if rendered == ROOT_PATH {
            None
        } else {
            Some(
                rendered
                    .parse::<RmPath>()
                    .map_err(|_refused| EntryError::Path {
                        path: rendered.to_owned(),
                    })?,
            )
        };
        named.push((rendered.to_owned(), path));
    }
    let paths: Vec<Option<RmPath>> = named.iter().map(|(_, path)| path.clone()).collect();
    Ok(named
        .into_iter()
        .zip(1_usize..)
        .map(|((rendered, path), ordinal)| Entry {
            others: paths
                .iter()
                .zip(1_usize..)
                .filter(|(_, place)| *place != ordinal)
                .filter_map(|(other, _)| other.clone())
                .collect(),
            rendered,
            path,
            ordinal,
        })
        .collect())
}

/// Returns the entry `path` names in `composition`, as
/// [`Entry::identity_path`] wrote it.
///
/// # Errors
///
/// Returns [`EntryError`] when the entries cannot be named.
pub fn entry_named(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
    path: &str,
) -> Result<Option<Entry>, EntryError> {
    Ok(entries_of(program, index, composition)?
        .into_iter()
        .find(|entry| entry.rendered == path))
}

/// Returns the archetype the start model mapping of `program` declares, the
/// archetype of every entry [`entries_of`] reads.
#[must_use]
pub fn archetype_of(program: &Program) -> Option<String> {
    program
        .models()
        .iter()
        .find(|model| model.name() == program.start())
        .and_then(|model| model.archetype())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::Entry;
    use super::REMOVED;
    use super::locate_mut;
    use super::sweep;
    use openehr_mapping_core::composition::CanonicalComposition;
    use openehr_mapping_core::template::Generation;
    use openehr_rm::v1_2::paths::RmPath;

    /// The archetype every case below looks for.
    const DIAGNOSIS: &str = "openEHR-EHR-EVALUATION.problem_diagnosis.v1";

    /// The section the nested cases hold their entries in.
    const SECTION: &str = "openEHR-EHR-SECTION.adhoc.v1";

    /// Returns a composition carrying `items` under `content`.
    fn composition(items: &serde_json::Value) -> CanonicalComposition {
        CanonicalComposition::new(
            serde_json::json!({ "_type": "COMPOSITION", "content": items }),
            "ferrobridge.diagnose.v1",
            Generation::Adl14,
        )
    }

    /// Returns the entry at `rendered` among `all`, the paths of every entry.
    fn entry(rendered: &str, all: &[&str], ordinal: usize) -> Entry {
        let parse = |text: &str| text.parse::<RmPath>().expect("the path parses");
        Entry {
            rendered: rendered.to_owned(),
            path: Some(parse(rendered)),
            others: all
                .iter()
                .filter(|other| **other != rendered)
                .map(|other| parse(other))
                .collect(),
            ordinal,
        }
    }

    /// Two diagnoses inside one section, beside a third outside it.
    fn nested() -> CanonicalComposition {
        composition(&serde_json::json!([
            {
                "archetype_node_id": SECTION,
                "items": [
                    { "archetype_node_id": DIAGNOSIS, "n": 1 },
                    { "archetype_node_id": "openEHR-EHR-OBSERVATION.other.v1" },
                    { "archetype_node_id": DIAGNOSIS, "n": 2 }
                ]
            },
            { "archetype_node_id": DIAGNOSIS, "n": 3 }
        ]))
    }

    /// The three entry paths of [`nested`], as the engine renders them.
    fn nested_paths() -> [String; 3] {
        [
            format!("/content[{SECTION}]/items[{DIAGNOSIS}]"),
            format!("/content[{SECTION}]/items[3]"),
            format!("/content[{DIAGNOSIS}]"),
        ]
    }

    #[test]
    fn an_entry_inside_a_section_is_read_at_its_path() {
        let held = nested();
        let paths = nested_paths();
        let all: Vec<&str> = paths.iter().map(String::as_str).collect();
        for (place, expected) in [1, 2, 3].into_iter().zip([1, 2, 3]) {
            let one = entry(&paths[place - 1], &all, place);
            assert_eq!(
                Some(&serde_json::json!(expected)),
                one.item(&held).and_then(|item| item.get("n")),
                "{}",
                one.identity_path()
            );
        }
    }

    #[test]
    fn an_isolated_entry_is_the_only_entry_left_at_any_depth() {
        let held = nested();
        let paths = nested_paths();
        let all: Vec<&str> = paths.iter().map(String::as_str).collect();
        let second = entry(&paths[1], &all, 2);
        let isolated = second.isolate(&held);
        assert_eq!(
            &serde_json::json!([
                {
                    "archetype_node_id": SECTION,
                    "items": [
                        { "archetype_node_id": "openEHR-EHR-OBSERVATION.other.v1" },
                        { "archetype_node_id": DIAGNOSIS, "n": 2 }
                    ]
                }
            ]),
            isolated.value().get("content").expect("content")
        );
    }

    #[test]
    fn a_replaced_entry_leaves_every_other_item_as_it_stood() {
        let held = nested();
        let paths = nested_paths();
        let all: Vec<&str> = paths.iter().map(String::as_str).collect();
        let second = entry(&paths[1], &all, 2);
        let replaced = second.replaced(
            &held,
            serde_json::json!({ "archetype_node_id": DIAGNOSIS, "n": 9 }),
        );
        let mut expected = held.value().clone();
        expected["content"][0]["items"][2]["n"] = serde_json::json!(9);
        assert_eq!(&expected, replaced.value());
    }

    #[test]
    fn a_node_held_by_an_entry_is_found_in_that_entry_alone() {
        let held = nested();
        let paths = nested_paths();
        let all: Vec<&str> = paths.iter().map(String::as_str).collect();
        let node = serde_json::json!({ "archetype_node_id": DIAGNOSIS, "n": 3 });
        assert!(entry(&paths[2], &all, 3).holds(&held, &node));
        assert!(!entry(&paths[0], &all, 1).holds(&held, &node));
    }

    #[test]
    fn a_positional_step_whose_element_does_not_match_names_nothing() {
        let mut value = nested().value().clone();
        let path = format!("/content[{SECTION}]/items[{DIAGNOSIS}]")
            .replace(&format!("items[{DIAGNOSIS}]"), "items[2]")
            .parse::<RmPath>()
            .expect("the path parses");
        assert!(
            locate_mut(&mut value, &path).is_some(),
            "a bare position matches any element"
        );
        let mut narrowed = path.clone();
        if let Some(last) = narrowed.segments.last_mut() {
            last.predicate.archetype_node_id = Some(String::from(DIAGNOSIS));
        }
        assert!(locate_mut(&mut value, &narrowed).is_none());
    }

    #[test]
    fn the_sweep_removes_only_the_marker() {
        let mut value = serde_json::json!({"a": [REMOVED, {"b": [REMOVED, 1]}, 2]});
        sweep(&mut value);
        assert_eq!(serde_json::json!({"a": [{"b": [1]}, 2]}), value);
    }

    /// A context whose start is a composition model maps the composition
    /// itself, so its one entry is the root, named `/`.
    #[test]
    fn a_composition_rooted_entry_is_the_composition_itself() {
        let held = nested();
        let root = Entry {
            rendered: String::from("/"),
            path: None,
            others: Vec::new(),
            ordinal: 1,
        };
        assert_eq!("/", root.identity_path());
        assert_eq!(Some(held.value()), root.item(&held));
        assert_eq!(held, root.isolate(&held));
        let replaced = root.replaced(&held, serde_json::json!({"_type": "COMPOSITION"}));
        assert_eq!(
            &serde_json::json!({"_type": "COMPOSITION"}),
            replaced.value()
        );
    }
}
