// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The entries of a composition one program maps, and the paths that name
//! them.
//!
//! An entry is a `content` item carrying the archetype the start model mapping
//! of a program declares, or the composition itself when that archetype is
//! the composition's own, as a context whose start is a composition model
//! maps it (`types-of-mapping-files/composition-element.adoc`). The identity
//! map keys each entry by the path [`Entry::identity_path`] writes, and
//! [`entry_named`] reads every path a build has written back to its entry.

use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_rm::v1_2::paths::PathSegment;
use openehr_rm::v1_2::paths::Predicate;
use openehr_rm::v1_2::paths::RmPath;

/// Where in a composition one entry a program maps sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The `LOCATABLE.uid` the entry carries, when it carries one.
    uid: Option<String>,
    /// The archetype id the entry carries.
    archetype: String,
    /// The 1-based place of the entry among the items of its archetype.
    ordinal: usize,
    /// The index of the entry in the composition's `content`.
    index: usize,
    /// Whether the composition holds more than one item of the archetype.
    several: bool,
    /// Whether the entry is the composition itself, the start of a context
    /// whose start model mapping is a composition model.
    root: bool,
}

impl Entry {
    /// Returns the `LOCATABLE.uid` the entry carries.
    #[must_use]
    pub fn uid(&self) -> Option<&str> {
        self.uid.as_deref()
    }

    /// Returns the 1-based place of the entry among the items of its
    /// archetype.
    #[must_use]
    pub const fn ordinal(&self) -> usize {
        self.ordinal
    }

    /// Returns the path the identity map keys the entry by.
    ///
    /// The first entry of an archetype keeps the archetype predicate
    /// `/content[<archetype>]` it has when it is the only one, so a
    /// composition that gains a second entry does not rename the resource of
    /// its first (no specification governs this: our own design). Every
    /// further entry takes the positional predicate `/content[<n>]`, `n` its
    /// 1-based place in `content`, as openEHR BASE Release 1.2.0, Paths and
    /// Locators, §Using Positional Parameters, defines it. The path is built
    /// with the `openehr-rm` path types, so it parses back.
    #[must_use]
    pub fn identity_path(&self) -> String {
        if self.root {
            return String::from(ROOT);
        }
        let predicate = if self.ordinal <= 1 {
            Predicate {
                archetype_node_id: Some(self.archetype.clone()),
                ..Predicate::default()
            }
        } else {
            Predicate {
                position: Some(self.index.saturating_add(1)),
                ..Predicate::default()
            }
        };
        RmPath {
            absolute: true,
            segments: vec![PathSegment {
                attribute: String::from(CONTENT),
                predicate,
                descendant: false,
            }],
        }
        .to_string()
    }

    /// Returns the path a build up to 0.0.6 keyed the entry by, when it
    /// differs from [`Entry::identity_path`].
    ///
    /// Those builds wrote `/content[<archetype>, <n>]` for an entry of a
    /// composition holding several: builds up to 0.0.5 for the first item
    /// only, `n` its place in `content`, and 0.0.6 for every later item, `n`
    /// its place among the items of the archetype. BASE Release 1.2.0, Paths
    /// and Locators, reads that comma as the name-based predicate shortcut,
    /// so the form is outside the path grammar; the identity map still holds
    /// ids under it, and the map wins once written.
    #[must_use]
    pub fn legacy_path(&self) -> Option<String> {
        let n = if self.ordinal <= 1 {
            self.index.saturating_add(1)
        } else {
            self.ordinal
        };
        (self.several && !self.root).then(|| format!("/content[{}, {n}]", self.archetype))
    }

    /// Returns `composition` with every other `content` item of this entry's
    /// archetype removed, so the program maps this entry alone.
    #[must_use]
    pub fn isolate(&self, composition: &CanonicalComposition) -> CanonicalComposition {
        if self.root {
            return composition.clone();
        }
        let mut value = composition.value().clone();
        if let Some(content) = value
            .get_mut("content")
            .and_then(serde_json::Value::as_array_mut)
        {
            let kept: Vec<serde_json::Value> = content
                .iter()
                .enumerate()
                .filter(|(index, item)| {
                    *index == self.index || archetype_node_id(item) != Some(self.archetype.as_str())
                })
                .map(|(_, item)| item.clone())
                .collect();
            *content = kept;
        }
        CanonicalComposition::new(value, composition.template_id(), composition.generation())
    }

    /// Returns the entry's item in `composition`.
    #[must_use]
    pub fn item<'c>(&self, composition: &'c CanonicalComposition) -> Option<&'c serde_json::Value> {
        if self.root {
            return Some(composition.value());
        }
        composition
            .value()
            .get("content")
            .and_then(serde_json::Value::as_array)
            .and_then(|content| content.get(self.index))
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
        if self.root {
            return CanonicalComposition::new(
                item,
                composition.template_id(),
                composition.generation(),
            );
        }
        let mut value = composition.value().clone();
        if let Some(slot) = value
            .get_mut("content")
            .and_then(serde_json::Value::as_array_mut)
            .and_then(|content| content.get_mut(self.index))
        {
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

/// Returns the `archetype_node_id` of one `content` item.
fn archetype_node_id(item: &serde_json::Value) -> Option<&str> {
    item.get("archetype_node_id")
        .and_then(serde_json::Value::as_str)
}

/// Returns the first entry the start model mapping of `program` writes.
///
/// A context maps one profile onto one template through a start model mapping
/// whose `metadata.archetype` names the entry
/// (`types-of-mapping-files/context-mappings.adoc`), so the entry is the
/// `content` item carrying that `archetype_node_id`.
///
/// A composition with no such item answers `None`: the program mapped no entry
/// of its own archetype, which the caller reports rather than papering over.
#[must_use]
pub fn entry_of(program: &Program, composition: &CanonicalComposition) -> Option<Entry> {
    entries_of(program, composition).into_iter().next()
}

/// Returns every entry of the archetype the start model mapping of `program`
/// writes, in `content` order.
#[must_use]
pub fn entries_of(program: &Program, composition: &CanonicalComposition) -> Vec<Entry> {
    archetype_of(program)
        .map(|archetype| entries_at(composition, &archetype))
        .unwrap_or_default()
}

/// Why a stored entry path names no single entry of a composition.
///
/// Builds up to 0.0.5 recorded `/content[<archetype>, <n>]` for the first item
/// of the archetype, `n` its 1-based place in `content`, and 0.0.6 recorded
/// the same text for the `n`th item of the archetype. When both readings name
/// an item and the items differ, the binding cannot say which entry it meant.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the entry path {path} a build up to 0.0.6 recorded names the item at place {place} of content \
     as builds up to 0.0.5 wrote it and another item as 0.0.6 wrote it, so it names no single entry"
)]
pub struct AmbiguousEntry {
    /// The stored path.
    path: String,
    /// The `n` the path carries.
    place: usize,
}

impl AmbiguousEntry {
    /// Returns the stored path that names two entries.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// Returns the entry `path` names in `composition`, as
/// [`Entry::identity_path`] or [`Entry::legacy_path`] wrote it.
///
/// `/content[<archetype>]` names the first item of the archetype and
/// `/content[<n>]` the `content` item at 1-based place `n`, which must carry
/// the archetype. The legacy `/content[<archetype>, <n>]` has two readings:
/// the first item of the archetype when it sits at place `n` of `content`,
/// as builds up to 0.0.5 wrote it, and the `n`th item of the archetype, as
/// 0.0.6 wrote it. One reading that names an item, or two that name the same
/// one, resolves (no specification governs the identity: our own design).
///
/// # Errors
///
/// Returns [`AmbiguousEntry`] when the two readings of a legacy path name two
/// different items.
pub fn entry_named(
    program: &Program,
    composition: &CanonicalComposition,
    path: &str,
) -> Result<Option<Entry>, AmbiguousEntry> {
    match archetype_of(program) {
        Some(archetype) => named_in(composition, &archetype, path),
        None => Ok(None),
    }
}

/// Returns the entry of `archetype` that `path` names in `composition`, as
/// [`entry_named`] reads it.
fn named_in(
    composition: &CanonicalComposition,
    archetype: &str,
    path: &str,
) -> Result<Option<Entry>, AmbiguousEntry> {
    if path == ROOT {
        return Ok(root_entry(composition, archetype));
    }
    let Some(place) = place_of(path, archetype) else {
        return Ok(None);
    };
    let entries = entries_at(composition, archetype);
    let at = |position: usize| {
        entries
            .iter()
            .find(|entry| entry.index.checked_add(1) == Some(position))
    };
    let nth = |ordinal: usize| entries.iter().find(|entry| entry.ordinal == ordinal);
    let found = match place {
        Place::Ordinal(ordinal) => nth(ordinal),
        Place::Position(position) => at(position),
        Place::Legacy(n) => {
            let early = entries
                .first()
                .filter(|first| first.index.checked_add(1) == Some(n));
            match (early, nth(n)) {
                (Some(early), Some(later)) if early != later => {
                    return Err(AmbiguousEntry {
                        path: path.to_owned(),
                        place: n,
                    });
                }
                (Some(one), _) | (None, Some(one)) => Some(one),
                (None, None) => None,
            }
        }
    };
    Ok(found.cloned())
}

/// The `content` attribute every entry path starts with.
const CONTENT: &str = "content";

/// The path of the composition itself (openEHR BASE Release 1.2.0, Paths and
/// Locators: `/` names the root of the archetyped structure).
const ROOT: &str = "/";

/// Returns the composition itself as the one entry of `archetype`, when the
/// composition carries it.
fn root_entry(composition: &CanonicalComposition, archetype: &str) -> Option<Entry> {
    (archetype_node_id(composition.value()) == Some(archetype)).then(|| Entry {
        uid: composition
            .value()
            .get("uid")
            .and_then(|uid| uid.get("value"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        archetype: archetype.to_owned(),
        ordinal: 1,
        index: 0,
        several: false,
        root: true,
    })
}

/// How an entry path names its entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// The 1-based place among the items of the archetype.
    Ordinal(usize),
    /// The 1-based place in `content`.
    Position(usize),
    /// The `n` of the legacy `/content[<archetype>, <n>]`.
    Legacy(usize),
}

/// Returns how `path` names an entry of `archetype`.
fn place_of(path: &str, archetype: &str) -> Option<Place> {
    let Ok(parsed) = path.parse::<RmPath>() else {
        // NOTE: no specification governs this: our own design, a path the BASE
        // grammar refuses is either the legacy comma form or names no entry.
        return legacy_place(path, archetype).map(Place::Legacy);
    };
    let [segment] = parsed.segments.as_slice() else {
        return None;
    };
    let predicate = &segment.predicate;
    if !parsed.absolute
        || segment.descendant
        || segment.attribute != CONTENT
        || predicate.name_value.is_some()
        || predicate.uid.is_some()
        || !predicate.comparisons.is_empty()
    {
        return None;
    }
    match (predicate.archetype_node_id.as_deref(), predicate.position) {
        (Some(named), None) if named == archetype => Some(Place::Ordinal(1)),
        (None, Some(position)) => Some(Place::Position(position)),
        _ => None,
    }
}

/// Returns the `n` of a legacy `/content[<archetype>, <n>]` naming
/// `archetype`.
fn legacy_place(path: &str, archetype: &str) -> Option<usize> {
    let inner = path.strip_prefix("/content[")?.strip_suffix(']')?;
    let (named, place) = inner.split_once(',')?;
    if named.trim() != archetype {
        return None;
    }
    place
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|place| *place >= 1)
}

/// Returns every `content` item of `composition` carrying `archetype`.
fn entries_at(composition: &CanonicalComposition, archetype: &str) -> Vec<Entry> {
    if let Some(root) = root_entry(composition, archetype) {
        return vec![root];
    }
    let Some(content) = composition
        .value()
        .get("content")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let matching: Vec<(usize, &serde_json::Value)> = content
        .iter()
        .enumerate()
        .filter(|(_, item)| archetype_node_id(item) == Some(archetype))
        .collect();
    let several = matching.len() > 1;
    matching
        .into_iter()
        .zip(1_usize..)
        .map(|((index, item), ordinal)| Entry {
            uid: item
                .get("uid")
                .and_then(|uid| uid.get("value"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            archetype: archetype.to_owned(),
            ordinal,
            index,
            several,
            root: false,
        })
        .collect()
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
    use super::AmbiguousEntry;
    use super::Entry;
    use super::Place;
    use super::entries_at;
    use super::named_in;
    use super::place_of;
    use openehr_mapping_core::composition::CanonicalComposition;
    use openehr_mapping_core::template::Generation;
    use openehr_rm::v1_2::paths::RmPath;
    use openehr_rm::v1_2::paths::item_at_path;

    /// The archetype every case below looks for.
    const DIAGNOSIS: &str = "openEHR-EHR-EVALUATION.problem_diagnosis.v1";

    /// Returns a composition carrying `items` under `content`.
    fn composition(items: &serde_json::Value) -> CanonicalComposition {
        CanonicalComposition::new(
            serde_json::json!({ "_type": "COMPOSITION", "content": items }),
            "ferrobridge.diagnose.v1",
            Generation::Adl14,
        )
    }

    /// Returns the first entry of `archetype` in `composition`.
    fn entry_at(composition: &CanonicalComposition, archetype: &str) -> Option<Entry> {
        entries_at(composition, archetype).into_iter().next()
    }

    #[test]
    fn one_entry_of_an_archetype_has_an_unindexed_path() {
        let built = composition(&serde_json::json!([{ "archetype_node_id": DIAGNOSIS }]));
        let entry = entry_at(&built, DIAGNOSIS).expect("the entry is found");
        assert_eq!(format!("/content[{DIAGNOSIS}]"), entry.identity_path());
        assert_eq!(None, entry.legacy_path());
        assert_eq!(None, entry.uid());
    }

    #[test]
    fn a_later_entry_takes_the_one_based_positional_predicate_of_its_place_in_content() {
        let built = composition(&serde_json::json!([
            { "archetype_node_id": DIAGNOSIS },
            { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" },
            { "archetype_node_id": DIAGNOSIS }
        ]));
        let entries = entries_at(&built, DIAGNOSIS);
        let identities: Vec<String> = entries.iter().map(Entry::identity_path).collect();
        assert_eq!(
            vec![
                format!("/content[{DIAGNOSIS}]"),
                String::from("/content[3]")
            ],
            identities
        );
        let legacy: Vec<Option<String>> = entries.iter().map(Entry::legacy_path).collect();
        assert_eq!(
            vec![
                Some(format!("/content[{DIAGNOSIS}, 1]")),
                Some(format!("/content[{DIAGNOSIS}, 2]"))
            ],
            legacy
        );
    }

    #[test]
    fn every_identity_path_parses_back_and_navigates_to_its_entry() {
        // NOTE: openEHR BASE Release 1.2.0, Paths and Locators, §Using Positional
        // Parameters: `[n]` selects the nth item of the container, 1-based.
        let built = composition(&serde_json::json!([
            { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" },
            { "archetype_node_id": DIAGNOSIS, "name": "one" },
            { "archetype_node_id": DIAGNOSIS, "name": "two" }
        ]));
        for entry in entries_at(&built, DIAGNOSIS) {
            let path = entry.identity_path();
            let parsed = path.parse::<RmPath>().expect("the identity path parses");
            assert_eq!(path, parsed.to_string(), "the path renders back unchanged");
            assert_eq!(
                entry.item(&built),
                item_at_path(built.value(), &parsed),
                "{path} navigates to its entry"
            );
        }
    }

    #[test]
    fn the_legacy_comma_form_is_outside_the_path_grammar() {
        // NOTE: openEHR BASE Release 1.2.0, Paths and Locators, §Name-based
        // Predicate: the comma shortcut takes a quoted name, so `, 2` is refused.
        assert!(
            format!("/content[{DIAGNOSIS}, 2]")
                .parse::<RmPath>()
                .is_err()
        );
    }

    #[test]
    fn an_entry_uid_is_read_when_the_composition_carries_one() {
        let built = composition(&serde_json::json!([{
            "archetype_node_id": DIAGNOSIS,
            "uid": { "_type": "HIER_OBJECT_ID", "value": "3a2f1c4e-0000-4000-8000-0000000000ab" }
        }]));
        let entry = entry_at(&built, DIAGNOSIS).expect("the entry is found");
        assert_eq!(Some("3a2f1c4e-0000-4000-8000-0000000000ab"), entry.uid());
    }

    #[test]
    fn a_composition_holding_no_such_entry_answers_none() {
        let built = composition(&serde_json::json!([
            { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" }
        ]));
        assert!(entry_at(&built, DIAGNOSIS).is_none());
        assert!(entry_at(&composition(&serde_json::json!([])), DIAGNOSIS).is_none());
    }

    #[test]
    fn the_place_of_an_entry_counts_only_items_of_its_archetype() {
        let built = composition(&serde_json::json!([
            { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" },
            { "archetype_node_id": DIAGNOSIS, "name": "one" },
            { "archetype_node_id": DIAGNOSIS, "name": "two" }
        ]));
        let entries = entries_at(&built, DIAGNOSIS);
        let ordinals: Vec<usize> = entries.iter().map(Entry::ordinal).collect();
        assert_eq!(vec![1, 2], ordinals);
        let identities: Vec<String> = entries.iter().map(Entry::identity_path).collect();
        assert_eq!(
            vec![
                format!("/content[{DIAGNOSIS}]"),
                String::from("/content[3]")
            ],
            identities,
            "the first entry keeps the identity it had alone"
        );
        let legacy: Vec<Option<String>> = entries.iter().map(Entry::legacy_path).collect();
        assert_eq!(
            vec![
                Some(format!("/content[{DIAGNOSIS}, 2]")),
                Some(format!("/content[{DIAGNOSIS}, 2]"))
            ],
            legacy,
            "the first entry's early key, its place in content, is the second entry's 0.0.6 key"
        );
    }

    #[test]
    fn an_isolated_entry_is_the_only_item_of_its_archetype_left() {
        let built = composition(&serde_json::json!([
            { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" },
            { "archetype_node_id": DIAGNOSIS, "name": "one" },
            { "archetype_node_id": DIAGNOSIS, "name": "two" }
        ]));
        let second = entries_at(&built, DIAGNOSIS)
            .into_iter()
            .nth(1)
            .expect("two entries");
        let isolated = second.isolate(&built);
        assert_eq!(
            serde_json::json!([
                { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" },
                { "archetype_node_id": DIAGNOSIS, "name": "two" }
            ]),
            isolated.value()["content"]
        );
        assert!(second.holds(&built, &serde_json::json!("two")));
        assert!(!second.holds(&built, &serde_json::json!("one")));
    }

    #[test]
    fn a_replaced_entry_leaves_every_other_item_as_it_stood() {
        let built = composition(&serde_json::json!([
            { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" },
            { "archetype_node_id": DIAGNOSIS, "name": "one" },
            { "archetype_node_id": DIAGNOSIS, "name": "two" }
        ]));
        let second = entries_at(&built, DIAGNOSIS)
            .into_iter()
            .nth(1)
            .expect("two entries");
        assert_eq!(
            Some(&serde_json::json!({ "archetype_node_id": DIAGNOSIS, "name": "two" })),
            second.item(&built)
        );
        let replaced = second.replaced(
            &built,
            serde_json::json!({ "archetype_node_id": DIAGNOSIS, "name": "three" }),
        );
        assert_eq!(
            serde_json::json!([
                { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" },
                { "archetype_node_id": DIAGNOSIS, "name": "one" },
                { "archetype_node_id": DIAGNOSIS, "name": "three" }
            ]),
            replaced.value()["content"]
        );
        assert_eq!(built.template_id(), replaced.template_id());
    }

    #[test]
    fn a_recorded_path_names_its_entry_by_archetype_or_by_its_place_in_content() {
        assert_eq!(
            Some(Place::Ordinal(1)),
            place_of(&format!("/content[{DIAGNOSIS}]"), DIAGNOSIS)
        );
        assert_eq!(Some(Place::Position(3)), place_of("/content[3]", DIAGNOSIS));
        assert_eq!(None, place_of("/content[other]", DIAGNOSIS));
        assert_eq!(None, place_of("/content[0]", DIAGNOSIS));
        assert_eq!(None, place_of("/context", DIAGNOSIS));
        assert_eq!(None, place_of("/content[3]/items[1]", DIAGNOSIS));
        assert_eq!(
            None,
            place_of(&format!("/content[{DIAGNOSIS}, 'x']"), DIAGNOSIS)
        );
    }

    #[test]
    fn a_legacy_path_is_read_as_the_comma_form() {
        assert_eq!(
            Some(Place::Legacy(2)),
            place_of(&format!("/content[{DIAGNOSIS}, 2]"), DIAGNOSIS)
        );
        assert_eq!(
            None,
            place_of(&format!("/content[{DIAGNOSIS}, 0]"), DIAGNOSIS)
        );
        assert_eq!(None, place_of("/content[other, 2]", DIAGNOSIS));
    }

    /// Returns the `name` of the entry `path` names in `built`.
    fn named(built: &CanonicalComposition, path: &str) -> Result<Option<String>, AmbiguousEntry> {
        Ok(named_in(built, DIAGNOSIS, path)?.and_then(|entry| {
            entry
                .item(built)
                .and_then(|item| item["name"].as_str())
                .map(str::to_owned)
        }))
    }

    #[test]
    fn a_legacy_path_an_early_build_wrote_names_the_first_entry_at_its_place_in_content() {
        // Builds up to 0.0.5 bound the first diagnosis at content place 3.
        let built = composition(&serde_json::json!([
            { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" },
            { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" },
            { "archetype_node_id": DIAGNOSIS, "name": "one" },
            { "archetype_node_id": DIAGNOSIS, "name": "two" }
        ]));
        assert_eq!(
            Ok(Some(String::from("one"))),
            named(&built, &format!("/content[{DIAGNOSIS}, 3]")),
            "place 3 of content is the first diagnosis, and there is no third diagnosis"
        );
    }

    #[test]
    fn a_legacy_path_0_0_6_wrote_names_the_nth_entry_of_the_archetype() {
        let built = composition(&serde_json::json!([
            { "archetype_node_id": DIAGNOSIS, "name": "one" },
            { "archetype_node_id": DIAGNOSIS, "name": "two" },
            { "archetype_node_id": DIAGNOSIS, "name": "three" }
        ]));
        assert_eq!(
            Ok(Some(String::from("three"))),
            named(&built, &format!("/content[{DIAGNOSIS}, 3]")),
            "the first diagnosis is not at place 3, so only the 0.0.6 reading names an entry"
        );
        assert_eq!(
            Ok(Some(String::from("one"))),
            named(&built, &format!("/content[{DIAGNOSIS}, 1]")),
            "both readings of place 1 name the first diagnosis"
        );
        assert_eq!(
            Ok(None),
            named(&built, &format!("/content[{DIAGNOSIS}, 4]"))
        );
    }

    #[test]
    fn a_legacy_path_whose_two_readings_name_two_entries_is_reported() {
        let built = composition(&serde_json::json!([
            { "archetype_node_id": "openEHR-EHR-OBSERVATION.body_weight.v2" },
            { "archetype_node_id": DIAGNOSIS, "name": "one" },
            { "archetype_node_id": DIAGNOSIS, "name": "two" }
        ]));
        let path = format!("/content[{DIAGNOSIS}, 2]");
        let refused = named(&built, &path).expect_err("two entries answer the path");
        assert_eq!(path, refused.path());
    }

    /// A context whose start is a composition model maps the composition
    /// itself, so its one entry is the root, named `/`.
    #[test]
    fn a_composition_rooted_program_maps_the_composition_itself() {
        let report = "openEHR-EHR-COMPOSITION.ferrobridge_report.v1";
        let held = CanonicalComposition::new(
            serde_json::json!({
                "_type": "COMPOSITION",
                "archetype_node_id": report,
                "content": [{"archetype_node_id": DIAGNOSIS}]
            }),
            "ferrobridge.diagnose.v1",
            Generation::Adl14,
        );
        let entries = entries_at(&held, report);
        let [ref root] = *entries else {
            panic!("one root entry, got {entries:?}");
        };
        assert_eq!("/", root.identity_path());
        assert_eq!(None, root.legacy_path());
        assert_eq!(Some(held.value()), root.item(&held));
        assert_eq!(held, root.isolate(&held));
        assert_eq!(Ok(Some(root.clone())), named_in(&held, report, "/"));
        assert_eq!(Ok(None), named_in(&held, DIAGNOSIS, "/"));
        let replaced = root.replaced(&held, serde_json::json!({"_type": "COMPOSITION"}));
        assert_eq!(
            &serde_json::json!({"_type": "COMPOSITION"}),
            replaced.value()
        );
    }
}
