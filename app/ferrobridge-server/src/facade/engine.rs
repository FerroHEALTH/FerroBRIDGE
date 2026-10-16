// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Running one program, and reading the identity of the entry it maps.
//!
//! The facade is a client of the same in-process engine the `$tofhir` and
//! `$toopenehr` operations call, so the two surfaces cannot disagree (no
//! specification governs this: our own design). This module is the thin seam: it hands the
//! engine the element table and the template index, and it reads back out of
//! the produced composition the two facts identity needs, the entry's path and
//! its `LOCATABLE.uid`.

use fhir_types::codec::Value;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::origin::Origin;
use fhirconnect::engine::origin::SourceItem;
use fhirconnect::engine::outcome::Outcome;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_rm::v1_2::paths::PathSegment;
use openehr_rm::v1_2::paths::Predicate;
use openehr_rm::v1_2::paths::RmPath;

/// The composer the facade records when no mapping sets one.
///
/// "The composer … could be defaulted with a value such as `FHIRconnect`
/// party" (`engine/defaults-for-fields.adoc`); this deployment names itself so
/// a reader can tell which bridge wrote the composition.
pub const COMPOSER: &str = "FerroBRIDGE";

/// Maps one FHIR document into a composition.
///
/// `now` is the one instant this ingest defaults every composition field
/// from, so a single request cannot carry two clock readings. The composition
/// carries a `FEEDER_AUDIT` naming this bridge as the system, the document's
/// `resourceType`, `id` and `meta.versionId` as the source item, and every
/// field the engine defaulted, because `FEEDER_AUDIT` "describes the origin of
/// data that have been transformed into openEHR form and committed to the
/// system" (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html>).
///
/// # Errors
///
/// Returns [`EngineError`] for any element the program cannot map, and for a
/// set of values the template does not admit as a composition.
pub fn inbound(
    program: &Program,
    index: &WebTemplateIndex,
    document: &Value,
    now: &str,
    settings: &crate::facade::Settings,
) -> Result<Outcome<CanonicalComposition>, EngineError> {
    inbound_from(
        program,
        index,
        document,
        now,
        settings,
        SourceItem::of(document),
    )
}

/// Maps one FHIR document into a composition that names `source` as its
/// origin.
///
/// This is [`inbound`] with the originating item given by the caller, so a
/// face that received something other than the resource itself, such as a
/// message, names what it received. `None` records no source item.
///
/// # Errors
///
/// Returns [`EngineError`] for any element the program cannot map, and for a
/// set of values the template does not admit as a composition.
pub fn inbound_from(
    program: &Program,
    index: &WebTemplateIndex,
    document: &Value,
    now: &str,
    settings: &crate::facade::Settings,
    source: Option<SourceItem>,
) -> Result<Outcome<CanonicalComposition>, EngineError> {
    // TODO(#95): run the terminology calls the concept mappings need here,
    // before any value is built, once the PROGRAMMED registry lands.
    let mut origin = Origin::new(&settings.system_id);
    if let Some(source) = source {
        origin = origin.with_source(source);
    }
    let defaults = Defaults::at(now)
        .with_composer(COMPOSER)
        .with_language(&settings.language)
        .with_territory(&settings.territory)
        .with_origin(origin);
    to_openehr(
        program,
        &fhir_types::r4::schema::SCHEMAS,
        index,
        document,
        &Seams::default(),
        &defaults,
        &CallContext::new(),
    )
}

/// Maps one composition into a FHIR document.
///
/// # Errors
///
/// Returns [`EngineError`] for any element the program cannot map.
pub fn outbound(
    program: &Program,
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<Outcome<Value>, EngineError> {
    to_fhir(
        program,
        &fhir_types::r4::schema::SCHEMAS,
        index,
        composition,
        &Seams::default(),
        &CallContext::new(),
    )
}

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
    /// Those builds wrote `/content[<archetype>, <n>]`, `n` the place among
    /// the items of the archetype, for an entry of a composition holding
    /// several. BASE Release 1.2.0, Paths and Locators, reads that comma as
    /// the name-based predicate shortcut, so the form is outside the path
    /// grammar; the identity map still holds ids under it, and the map wins
    /// once written.
    #[must_use]
    pub fn legacy_path(&self) -> Option<String> {
        self.several
            .then(|| format!("/content[{}, {}]", self.archetype, self.ordinal))
    }

    /// Returns `composition` with every other `content` item of this entry's
    /// archetype removed, so the program maps this entry alone.
    #[must_use]
    pub fn isolate(&self, composition: &CanonicalComposition) -> CanonicalComposition {
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

/// Returns the entry `path` names in `composition`, as
/// [`Entry::identity_path`] or [`Entry::legacy_path`] wrote it.
///
/// `/content[<archetype>]` names the first item of the archetype and
/// `/content[<n>]` the `content` item at 1-based place `n`, which must carry
/// the archetype; the legacy `/content[<archetype>, <n>]` names the `n`th item
/// of the archetype (no specification governs the identity: our own design).
#[must_use]
pub fn entry_named(
    program: &Program,
    composition: &CanonicalComposition,
    path: &str,
) -> Option<Entry> {
    let archetype = archetype_of(program)?;
    let place = place_of(path, &archetype)?;
    entries_at(composition, &archetype)
        .into_iter()
        .find(|entry| match place {
            Place::Ordinal(ordinal) => entry.ordinal == ordinal,
            Place::Position(position) => entry.index.checked_add(1) == Some(position),
        })
}

/// The `content` attribute every entry path starts with.
const CONTENT: &str = "content";

/// How an entry path names its entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// The 1-based place among the items of the archetype.
    Ordinal(usize),
    /// The 1-based place in `content`.
    Position(usize),
}

/// Returns how `path` names an entry of `archetype`.
fn place_of(path: &str, archetype: &str) -> Option<Place> {
    let Ok(parsed) = path.parse::<RmPath>() else {
        // NOTE: no specification governs this: our own design, a path the BASE
        // grammar refuses is either the legacy comma form or names no entry.
        return legacy_ordinal(path, archetype).map(Place::Ordinal);
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

/// Returns the 1-based place the legacy `/content[<archetype>, <n>]` names
/// among the items of `archetype`.
fn legacy_ordinal(path: &str, archetype: &str) -> Option<usize> {
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
    use super::Entry;
    use super::Place;
    use super::entries_at;
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
    fn a_legacy_path_names_its_place_among_the_items() {
        assert_eq!(
            Some(Place::Ordinal(2)),
            place_of(&format!("/content[{DIAGNOSIS}, 2]"), DIAGNOSIS)
        );
        assert_eq!(
            None,
            place_of(&format!("/content[{DIAGNOSIS}, 0]"), DIAGNOSIS)
        );
        assert_eq!(None, place_of("/content[other, 2]", DIAGNOSIS));
    }
}
