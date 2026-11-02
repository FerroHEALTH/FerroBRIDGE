// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Building a canonical composition from node values and reading one value
//! back out of it, through the template index.

use openehr_rm::v1_2::paths::PathSegment;
use openehr_rm::v1_2::paths::RmPath;
use openehr_sdt::flat::build::BuildOptions;
use openehr_sdt::flat::build::build_composition_with;
use openehr_sdt::flat::path::FlatKey;
use openehr_sdt::flat::path::MAX_INSTANCE_INDEX;
use openehr_sdt::flat::path::Segment;
use serde_json::Value;

use crate::index::ResolvedNode;
use crate::index::WebTemplateIndex;
use crate::template::PathError;

use super::CanonicalComposition;
use super::CanonicalDocument;
use super::FlatIndex;
use super::NodeValue;
use super::Place;
use super::RmPosition;

impl WebTemplateIndex {
    /// Builds a canonical composition from `values`.
    ///
    /// Every value is keyed by its node's place in the template, the
    /// composition is built through the Simplified Formats builder of
    /// `openehr-sdt`, and the result is validated against the same template
    /// before it is returned, so a composition this function answers with is
    /// one the CDR's own validation admits. `now` is the caller's current
    /// timestamp, which the builder uses for the `ctx/time` default alone
    /// (Simplified Formats, §Context); this crate reads no clock.
    ///
    /// Every `ARCHETYPED.rm_version` names the RM release the composition
    /// conforms to: Release 1.1.0, unless it holds an element the development
    /// line changed (a `GENERIC_ENTRY` whose `data` is no `ITEM_TREE`).
    ///
    /// # Errors
    ///
    /// Returns [`PathError::UnknownNode`] when a value names a node of another
    /// template, [`PathError::OccurrenceCount`] when its occurrences do not
    /// match the repeating nodes above it, [`PathError::CompositionBuild`]
    /// when the builder refuses the values, and
    /// [`PathError::InvalidComposition`] when the result does not validate.
    pub fn build_composition(
        &self,
        values: &[NodeValue],
        now: &str,
    ) -> Result<CanonicalComposition, PathError> {
        let mut flat = serde_json::Map::new();
        for value in values {
            flat.insert(self.flat_key(value)?.to_string(), value.value.clone());
        }
        self.build_from_flat(&flat, now)
    }

    /// Builds a canonical composition from a FLAT document.
    ///
    /// FLAT (simSDT) is the single-level serialization whose keys are template
    /// paths (Simplified Formats, §Flat format), the second form a composition
    /// is exchanged in. `now` is the caller's current timestamp, used for the
    /// `ctx/time` default alone; this crate reads no clock.
    /// The `rm_version` follows [`Self::build_composition`].
    ///
    /// # Errors
    ///
    /// Returns [`PathError::CompositionBuild`] when the reader or the builder
    /// refuses the document and [`PathError::InvalidComposition`] when the
    /// result does not validate against the template.
    pub fn build_from_flat(
        &self,
        flat: &serde_json::Map<String, Value>,
        now: &str,
    ) -> Result<CanonicalComposition, PathError> {
        let document = openehr_sdt::flat::sim::flat::parse_flat(flat).map_err(|source| {
            PathError::CompositionBuild {
                template_id: self.template_id().to_owned(),
                source: Box::new(source),
            }
        })?;
        let built = crate::release::build(|release| {
            let options = BuildOptions::default().with_rm_version(release.spec_version());
            build_composition_with(&document, self.web_template(), now, &options).map_err(
                |source| PathError::CompositionBuild {
                    template_id: self.template_id().to_owned(),
                    source: Box::new(source),
                },
            )
        })?;
        let messages = openehr_sdt::rm_instance::validate_composition(&built, self.web_template());
        if !messages.is_empty() {
            return Err(PathError::InvalidComposition {
                template_id: self.template_id().to_owned(),
                messages: messages
                    .iter()
                    .map(|message| format!("{}: {}", message.path, message.message))
                    .collect(),
            });
        }
        Ok(CanonicalComposition {
            value: built,
            template_id: self.template_id().to_owned(),
            generation: self.generation(),
        })
    }

    /// Wraps canonical JSON a caller handed in, validated against this
    /// template.
    ///
    /// A composition that reaches the bridge from outside is validated before
    /// anything reads it, so a document of another template is a refusal
    /// rather than a read that answers nothing.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::InvalidComposition`] when the document does not
    /// validate against the template.
    pub fn accept(&self, value: Value) -> Result<CanonicalComposition, PathError> {
        let messages = openehr_sdt::rm_instance::validate_composition(&value, self.web_template());
        if !messages.is_empty() {
            return Err(PathError::InvalidComposition {
                template_id: self.template_id().to_owned(),
                messages: messages
                    .iter()
                    .map(|message| format!("{}: {}", message.path, message.message))
                    .collect(),
            });
        }
        Ok(CanonicalComposition {
            value,
            template_id: self.template_id().to_owned(),
            generation: self.generation(),
        })
    }

    /// Returns `composition` as a FLAT document.
    ///
    /// The template drives the walk, so the result is the same content in the
    /// other serialization (Simplified Formats,
    /// §Conversion Between Formats).
    ///
    /// # Errors
    ///
    /// Returns [`PathError::TemplateMismatch`] when the composition was built
    /// against another template and [`PathError::CompositionFlatten`] when the
    /// flattener refuses it.
    pub fn flatten(
        &self,
        composition: &CanonicalComposition,
    ) -> Result<serde_json::Map<String, Value>, PathError> {
        if composition.document_template_id() != self.template_id() {
            return Err(PathError::TemplateMismatch {
                expected: self.template_id().to_owned(),
                found: composition.document_template_id().to_owned(),
            });
        }
        let document = openehr_sdt::flat::flatten::flatten_composition(
            &composition.value,
            self.web_template(),
        )
        .map_err(|source| PathError::CompositionFlatten {
            template_id: self.template_id().to_owned(),
            source: Box::new(source),
        })?;
        Ok(openehr_sdt::flat::sim::flat::emit_flat(&document))
    }

    /// Reads the value of one node out of a composition.
    ///
    /// The occurrences name the instance of each repeating node on the way to
    /// the target, outermost first, and become the positional predicates of
    /// the path the canonical JSON is navigated with.
    ///
    /// A value node of a polymorphic `ELEMENT` reads only a value of its own
    /// class ([`WebTemplateIndex::alternatives`]).
    ///
    /// # Errors
    ///
    /// Returns [`PathError::TemplateMismatch`] when the composition was built
    /// against another template, [`PathError::UnknownNode`] when the node
    /// belongs to another template, [`PathError::OccurrenceCount`] when the
    /// occurrences do not match the repeating nodes above it, and
    /// [`PathError::UntypedAlternative`] when the value of a polymorphic
    /// `ELEMENT` names no `_type`.
    pub fn read<D: CanonicalDocument + ?Sized>(
        &self,
        composition: &D,
        node: &ResolvedNode,
        occurrences: &[RmPosition],
    ) -> Result<Option<Value>, PathError> {
        let Some(value) = self.read_any(composition, node, occurrences)? else {
            return Ok(None);
        };
        Ok(self.belongs(node, &value)?.then_some(value))
    }

    /// Returns whether a composition holds a value at one instance of a node's
    /// path, of the node's class or of another alternative of its `ELEMENT`.
    ///
    /// # Errors
    ///
    /// The errors of [`WebTemplateIndex::read`], less the untyped value.
    pub fn holds<D: CanonicalDocument + ?Sized>(
        &self,
        composition: &D,
        node: &ResolvedNode,
        occurrences: &[RmPosition],
    ) -> Result<bool, PathError> {
        Ok(self.read_any(composition, node, occurrences)?.is_some())
    }

    /// Reads what a composition holds at one instance of a node's path.
    fn read_any<D: CanonicalDocument + ?Sized>(
        &self,
        composition: &D,
        node: &ResolvedNode,
        occurrences: &[RmPosition],
    ) -> Result<Option<Value>, PathError> {
        if composition.document_template_id() != self.template_id() {
            return Err(PathError::TemplateMismatch {
                expected: self.template_id().to_owned(),
                found: composition.document_template_id().to_owned(),
            });
        }
        let path = self.instance_path(node, occurrences)?;
        Ok(Self::walk(composition.document(), &path.segments))
    }

    /// Reads the `ELEMENT` a value node sits in out of a composition.
    ///
    /// The Web Template compacts an `ELEMENT` into the node of its value, whose
    /// `aqlPath` ends in `/value`, so the `LOCATABLE` attributes of the element
    /// (`links`, `null_flavour`, the openEHR RM `ELEMENT` and `LOCATABLE`
    /// classes) sit one step above what [`WebTemplateIndex::read`] returns.
    /// Simplified Formats writes both on one FLAT node ("there is no difference
    /// between an ELEMENT and its value", master05 §ELEMENT).
    ///
    /// # Errors
    ///
    /// The errors of [`WebTemplateIndex::read`], and
    /// [`PathError::NotAnElementValue`] when the node's `aqlPath` does not end
    /// in the `value` of an element.
    pub fn read_element<D: CanonicalDocument + ?Sized>(
        &self,
        composition: &D,
        node: &ResolvedNode,
        occurrences: &[RmPosition],
    ) -> Result<Option<Value>, PathError> {
        if composition.document_template_id() != self.template_id() {
            return Err(PathError::TemplateMismatch {
                expected: self.template_id().to_owned(),
                found: composition.document_template_id().to_owned(),
            });
        }
        let path = self.instance_path(node, occurrences)?;
        let element = match path.segments.split_last() {
            Some((last, element))
                if last.attribute == "value" && last.predicate.archetype_node_id.is_none() =>
            {
                element
            }
            _ => {
                return Err(PathError::NotAnElementValue {
                    aql_path: node.aql_path().as_str().to_owned(),
                });
            }
        };
        Ok(Self::walk(composition.document(), element))
    }

    /// Returns the path that names one instance of `node` in a composition,
    /// or `None` when the composition does not hold that instance.
    ///
    /// The path follows the node's template path. At a repeating node, the
    /// first instance keeps the template's predicate (the archetype node id
    /// and, where the template names the node, its name) and a later one
    /// takes the 1-based positional predicate of its place in the container,
    /// as openEHR BASE Release 1.2.0, Paths and Locators, §Using Positional
    /// Parameters, defines it, so the path parses and navigates back to the
    /// instance. Keeping the first instance's predicate means a composition
    /// that gains a second instance does not rename its first (no
    /// specification governs the rendering: our own design).
    ///
    /// # Errors
    ///
    /// The errors of [`WebTemplateIndex::read`], less the untyped value.
    pub fn entry_path<D: CanonicalDocument + ?Sized>(
        &self,
        composition: &D,
        node: &ResolvedNode,
        occurrences: &[RmPosition],
    ) -> Result<Option<RmPath>, PathError> {
        if composition.document_template_id() != self.template_id() {
            return Err(PathError::TemplateMismatch {
                expected: self.template_id().to_owned(),
                found: composition.document_template_id().to_owned(),
            });
        }
        let path = self.instance_path(node, occurrences)?;
        let mut current = composition.document();
        let mut named = RmPath {
            absolute: path.absolute,
            segments: Vec::with_capacity(path.segments.len()),
        };
        for segment in &path.segments {
            let mut identity = segment.clone();
            identity.predicate.position = None;
            let candidates = openehr_rm::v1_2::paths::select_children(current, &identity);
            let nth = segment
                .predicate
                .position
                .map_or(0, |position| position.saturating_sub(1));
            let Some(&chosen) = candidates.get(nth) else {
                return Ok(None);
            };
            if nth > 0 {
                let place = current
                    .get(&segment.attribute)
                    .and_then(Value::as_array)
                    .and_then(|items| items.iter().position(|item| core::ptr::eq(item, chosen)))
                    .and_then(|index| index.checked_add(1));
                identity.predicate = openehr_rm::v1_2::paths::Predicate {
                    position: place,
                    ..openehr_rm::v1_2::paths::Predicate::default()
                };
            }
            named.segments.push(identity);
            current = chosen;
        }
        Ok(Some(named))
    }

    /// Walks the instance path `segments` down a canonical document.
    ///
    /// NOTE: no specification governs this: our own design, an absent optional
    /// node is a legitimate absence and never a defect, so it reads as `None`.
    fn walk(document: &Value, segments: &[PathSegment]) -> Option<Value> {
        let mut current = document;
        for segment in segments {
            // An RM positional predicate counts every element of the container
            // (BASE Release 1.2.0 §Paths and Locators, positional parameters),
            // and an instance counts the node's own occurrences only, so the
            // position picks among the elements the node's identity selects.
            let mut identity = segment.clone();
            identity.predicate.position = None;
            let candidates = openehr_rm::v1_2::paths::select_children(current, &identity);
            let index = segment
                .predicate
                .position
                .map_or(0, |position| position.saturating_sub(1));
            current = *candidates.get(index)?;
        }
        Some(current.clone())
    }

    /// Returns the FLAT key one value is written under.
    pub(crate) fn flat_key(&self, value: &NodeValue) -> Result<FlatKey, PathError> {
        let mut segments = Vec::new();
        let flat_id = match value.place {
            // NOTE: Simplified Formats master04 §Context, a context field is
            // spelled under the `ctx/` prefix.
            Place::Context => {
                segments.push(Segment {
                    name: String::from("ctx"),
                    index: None,
                });
                "ctx"
            }
            Place::Node {
                ref flat_id,
                ref occurrences,
            } => {
                let chain = self.chain(flat_id)?;
                let repeating = chain.iter().filter(|entry| entry.node().repeats()).count();
                if repeating != occurrences.len() {
                    return Err(PathError::OccurrenceCount {
                        flat_id: flat_id.as_str().to_owned(),
                        expected: repeating,
                        given: occurrences.len(),
                    });
                }
                let mut positions = occurrences.iter();
                for entry in chain {
                    // NOTE: the FLAT instance index is 0-based (Simplified
                    // Formats, §Instance Indexing), so the position converts.
                    let index = if entry.node().repeats() {
                        positions
                            .next()
                            .map(|&position| FlatIndex::from(position).get())
                    } else {
                        None
                    };
                    segments.push(Segment {
                        name: entry.id().to_owned(),
                        index,
                    });
                }
                flat_id.as_str()
            }
        };
        segments.extend(value.sub_path.iter().cloned());
        let key = FlatKey {
            segments,
            suffixes: value.suffixes.clone(),
        };
        let past = key
            .segments
            .iter()
            .map(|segment| segment.index)
            .chain(key.suffixes.iter().map(|suffix| suffix.index))
            .flatten()
            .find(|&index| index > MAX_INSTANCE_INDEX);
        if let Some(index) = past {
            return Err(PathError::InstanceIndex {
                flat_id: flat_id.to_owned(),
                index,
                max: MAX_INSTANCE_INDEX,
            });
        }
        Ok(key)
    }

    /// Returns the path of one instance of `node`, with its positions set.
    pub(crate) fn instance_path(
        &self,
        node: &ResolvedNode,
        occurrences: &[RmPosition],
    ) -> Result<RmPath, PathError> {
        let chain = self.chain(node.flat_id())?;
        let repeating: Vec<usize> = chain
            .iter()
            .filter(|entry| entry.node().repeats())
            .filter_map(|entry| position_segment(entry.path()))
            .collect();
        if repeating.len() != occurrences.len() {
            return Err(PathError::OccurrenceCount {
                flat_id: node.flat_id().as_str().to_owned(),
                expected: repeating.len(),
                given: occurrences.len(),
            });
        }
        let mut path = chain
            .last()
            .map(|entry| entry.path().clone())
            .ok_or_else(|| PathError::UnknownNode {
                template_id: self.template_id().to_owned(),
                flat_id: node.flat_id().as_str().to_owned(),
            })?;
        for (&segment, &position) in repeating.iter().zip(occurrences.iter()) {
            if let Some(target) = path.segments.get_mut(segment) {
                target.predicate.position = usize::try_from(position.get()).ok();
            }
        }
        Ok(path)
    }
}

/// Returns the segment of `path` a repeat of its node applies to.
///
/// The Web Template compacts the RM levels an archetype leaves unconstrained
/// (Simplified Formats, §Level Removal), so a node's own path may end below
/// the container that repeats. The repeat applies to the deepest segment
/// carrying an archetype node id, which is that container. No specification
/// governs this mapping: it is FerroBRIDGE's own rule.
fn position_segment(path: &RmPath) -> Option<usize> {
    path.segments
        .iter()
        .rposition(|segment| segment.predicate.archetype_node_id.is_some())
        .or_else(|| path.segments.len().checked_sub(1))
}
