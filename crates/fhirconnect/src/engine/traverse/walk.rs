// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The walk over the mappings: the conditions, the input occurrences and the
//! children a mapping runs.

use openehr_mapping_core::composition::CanonicalDocument;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::paths::FlatId;
use openehr_mapping_core::template::PathError;

use crate::engine::condition;
use crate::engine::condition::ConditionError;
use crate::engine::condition::Verdict;
use crate::engine::outcome::Loss;
use crate::engine::outcome::SkipReason;
use crate::engine::outcome::Warning;
use crate::engine::rm;
use crate::engine::rm::RmValue;
use crate::engine::rm::carried::Carried;
use crate::model::ast::keyword::Direction;
use crate::resolve::derive;
use crate::resolve::program::SLOT_DEPTH_LIMIT;
use crate::resolve::program::hierarchy::Preprocessor;
use crate::resolve::program::mapping::Mapping;
use crate::resolve::program::mapping::Method;
use crate::resolve::program::target::Attachment;
use crate::resolve::program::target::OpenehrTarget;
use crate::resolve::program::target::Target;
use crate::tree::Occurrence;

use crate::tree::element::Table;
use crate::tree::read::Selected;
use crate::tree::read::read;

use crate::engine::traverse::Binding;
use crate::engine::traverse::INSTANCE_CEILING;
use crate::engine::traverse::LINK_TYPE;
use crate::engine::traverse::Leaf;
use crate::engine::traverse::Run;
use crate::engine::traverse::error::EngineError;
use crate::engine::traverse::place::fhir_axes;
use crate::engine::traverse::place::openehr_occurrence;
use crate::engine::traverse::place::rm_positions;
use crate::engine::traverse::tail::attribute_at;
use crate::engine::traverse::tail::list_length;
use crate::engine::traverse::tail::scalar;
use crate::engine::traverse::tail::tail_segments;
use crate::engine::traverse::tail::unsupported_tail;
use crate::engine::traverse::unread;

impl<T: Table + ?Sized> Run<'_, T> {
    /// Refuses an input the program's own condition does not admit.
    ///
    /// The context's condition is the one the input side carries, the same
    /// rule a mapping's condition follows (`basics/Conditions.adoc`).
    pub(super) fn admits_context(&self) -> Result<(), EngineError> {
        let gate = match self.direction {
            Direction::FhirToOpenehr => self.program.fhir_condition(),
            Direction::OpenehrToFhir => self.program.openehr_condition(),
        };
        let Some(gate) = gate.filter(|gate| condition::runs(gate, self.direction)) else {
            return Ok(());
        };
        let admits = match self.direction {
            Direction::FhirToOpenehr => condition::evaluate(self.table, &self.fhir, gate, false)
                .map(|verdict| verdict.admits_any())
                .map_err(|source| EngineError::Condition {
                    mapping: String::from(self.program.context().as_str()),
                    source: Box::new(source),
                })?,
            Direction::OpenehrToFhir => self.context_holds(gate)?,
        };
        if admits {
            return Ok(());
        }
        Err(EngineError::NotApplicable {
            context: String::from(self.program.context().as_str()),
        })
    }

    /// Returns whether the context's `openehrCondition` holds over the input.
    fn context_holds(
        &self,
        gate: &crate::resolve::program::condition::Condition,
    ) -> Result<bool, EngineError> {
        // NOTE: no specification governs this: our own design, a run over one occurrence of
        // the start archetype evaluates the context's condition at that occurrence.
        self.openehr_holds(self.program.context().as_str(), gate, &Binding::default())
    }

    /// Runs a list of mappings under one binding, in program order.
    pub(super) fn mappings(
        &mut self,
        mappings: &[Mapping],
        parent: &Binding,
    ) -> Result<(), EngineError> {
        for mapping in mappings {
            self.mapping(mapping, parent)?;
        }
        Ok(())
    }

    /// Runs one mapping under one binding.
    ///
    /// Answers [`Ran::Gated`] when the unidirectional marker or the input
    /// side's condition kept the mapping from running.
    fn mapping(&mut self, mapping: &Mapping, parent: &Binding) -> Result<Ran, EngineError> {
        if let Some(only) = mapping.direction()
            && only != self.direction
        {
            let location = self.locate(mapping, &parent.fhir);
            self.warnings.push(Warning::new(
                Loss::Skipped {
                    mapping: String::from(mapping.name()),
                    reason: SkipReason::Unidirectional,
                },
                location,
            ));
            return Ok(Ran::Gated);
        }
        let Some(admitted) = self.admits(mapping, parent)? else {
            return Ok(Ran::Gated);
        };
        let bindings = self.apply(mapping, parent, &admitted)?;
        for binding in &bindings {
            self.children(mapping, binding)?;
        }
        Ok(Ran::Ran)
    }

    /// Returns the input occurrences the conditions admit, `None` for a
    /// mapping a gate closed.
    ///
    /// A condition that filters out every occurrence the input carries closes
    /// the gate as a false one does: the mapping's input is there, and the
    /// condition says it is none this mapping maps.
    fn admits(
        &self,
        mapping: &Mapping,
        parent: &Binding,
    ) -> Result<Option<Vec<Occurrence>>, EngineError> {
        let inputs = self.inputs(mapping, parent)?;
        let Some(gate) = self.input_condition(mapping) else {
            return Ok(Some(inputs));
        };
        match self.direction {
            Direction::FhirToOpenehr => {
                // NOTE: Conditions.adoc §targetRoot, a condition whose target is the
                // `with` element filters that element's occurrences; the compiler
                // decided the attachment over the anchored paths.
                let attached = gate.attachment() == Attachment::Element;
                let verdict =
                    condition::evaluate(self.table, self.condition_document(gate), gate, attached)
                        .map_err(|source| EngineError::Condition {
                            mapping: String::from(mapping.name()),
                            source: Box::new(source),
                        })?;
                match verdict {
                    Verdict::Gate(false) => Ok(None),
                    Verdict::Gate(true) => Ok(Some(inputs)),
                    Verdict::Filter(admitted) => {
                        let present = !inputs.is_empty();
                        let kept: Vec<Occurrence> = inputs
                            .into_iter()
                            .filter(|occurrence| admitted.contains(occurrence))
                            .collect();
                        Ok((!kept.is_empty() || !present).then_some(kept))
                    }
                }
            }
            Direction::OpenehrToFhir => {
                let present = !inputs.is_empty();
                let mut admitted = Vec::with_capacity(inputs.len());
                for input in inputs {
                    let openehr = Self::positions_at(mapping, &input)?;
                    let held =
                        parent.of_openehr(mapping.openehr(), Occurrence::default(), openehr, 0);
                    if self.openehr_holds(mapping.name(), gate, &held)? {
                        admitted.push(input);
                    }
                }
                Ok((!admitted.is_empty() || !present).then_some(admitted))
            }
        }
    }

    /// Returns whether an `openehrCondition` holds at one instance.
    ///
    /// The condition is evaluated once per instance of the input, so a
    /// `targetRoot` that is the `with` path filters the instances and one
    /// pointing elsewhere answers the same for all of them, which is the plain
    /// true or false `Conditions.adoc` §targetRoot asks for.
    pub(super) fn openehr_holds(
        &self,
        name: &str,
        gate: &crate::resolve::program::condition::Condition,
        instance: &Binding,
    ) -> Result<bool, EngineError> {
        self.openehr_condition(name, gate, instance)
            .map(|(holds, _)| holds)
    }

    /// Evaluates an `openehrCondition` at one instance, answering the
    /// verdict and the node instances it read.
    pub(super) fn openehr_condition(
        &self,
        name: &str,
        gate: &crate::resolve::program::condition::Condition,
        instance: &Binding,
    ) -> Result<(bool, Vec<unread::Instance>), EngineError> {
        let Some(composition) = self.composition else {
            return Ok((true, Vec::new()));
        };
        let mut read_at = Vec::new();
        let mut found = condition::Attributes::default();
        for attribute in gate.attributes() {
            let Target::Openehr(ref target) = *attribute else {
                return Err(EngineError::Condition {
                    mapping: String::from(name),
                    source: Box::new(ConditionError::WrongSide),
                });
            };
            let positions = self.bound_positions(target, instance);
            let read = self
                .read_target(composition, target, &positions)
                .map_err(|source| EngineError::Template {
                    mapping: String::from(name),
                    node: String::from(target.node().aql_path().as_str()),
                    source: Box::new(source),
                })?;
            let Some(value) = read else {
                continue;
            };
            let elements = Self::bound_elements(target, instance);
            let Some(at_tail) = attribute_at(&value, target, &elements) else {
                continue;
            };
            let at_tail = if target.tail().segments.is_empty() {
                text_of(at_tail)
            } else {
                at_tail
            };
            read_at.push(unread::instance_of(target.node().flat_id(), &positions));
            found.present = found.present.saturating_add(1);
            found
                .types
                .push(held_class(at_tail, target.node().rm_type()));
            if let Some(text) = scalar(at_tail) {
                found.values.push(text);
            }
        }
        Ok((condition::decide(gate, &found), read_at))
    }

    /// Returns the positions one input occurrence names, as it was read.
    fn positions_at(mapping: &Mapping, input: &Occurrence) -> Result<Vec<RmPosition>, EngineError> {
        rm_positions(mapping.name(), input.indices())
    }

    /// Returns the condition the direction evaluates, if the mapping carries
    /// one.
    fn input_condition<'mapping>(
        &self,
        mapping: &'mapping Mapping,
    ) -> Option<&'mapping crate::resolve::program::condition::Condition> {
        match self.direction {
            Direction::FhirToOpenehr => mapping.fhir_condition(),
            Direction::OpenehrToFhir => mapping.openehr_condition(),
        }
        .filter(|gate| condition::runs(gate, self.direction))
    }

    /// Returns the occurrences of the input side, under the parent binding.
    fn inputs(&self, mapping: &Mapping, parent: &Binding) -> Result<Vec<Occurrence>, EngineError> {
        match self.direction {
            Direction::FhirToOpenehr => {
                let Some(input) = mapping.fhir() else {
                    return Ok(vec![parent.fhir.clone()]);
                };
                let parent = &self.scoped(Some(input), parent);
                let matches = read(self.table, self.document_for(input), input.expression())
                    .map_err(|source| EngineError::Read {
                        mapping: String::from(mapping.name()),
                        expression: String::from(input.expression().as_str()),
                        source: Box::new(source),
                    })?;
                let pinned = self
                    .pin_for(input)
                    .filter(|&(axes, _)| fhir_axes(input).starts_with(axes))
                    .map(|(_, occurrence)| occurrence);
                let mut occurrences = Vec::new();
                for matched in matches {
                    if !matched
                        .occurrence()
                        .indices()
                        .starts_with(parent.fhir.indices())
                    {
                        continue;
                    }
                    if pinned.is_some_and(|pin| {
                        !matched.occurrence().indices().starts_with(pin.indices())
                    }) {
                        continue;
                    }
                    if let Selected::Deferred(_) = *matched.selected() {
                        return Err(EngineError::DeferredReference {
                            mapping: String::from(mapping.name()),
                            expression: String::from(input.expression().as_str()),
                        });
                    }
                    occurrences.push(matched.occurrence().clone());
                }
                Ok(occurrences)
            }
            Direction::OpenehrToFhir => {
                // NOTE: no specification governs this: our own design, a
                // mapping with no openEHR side reads the instance its parent
                // bound, so its one input is the parent's openEHR occurrence.
                let Some(input) = mapping.openehr() else {
                    return Ok(vec![openehr_occurrence(mapping.name(), &parent.openehr)?]);
                };
                let instances = self.instances(mapping.name(), input, parent)?;
                let mut occurrences = Vec::with_capacity(instances.len());
                for positions in instances {
                    occurrences.push(openehr_occurrence(mapping.name(), &positions)?);
                }
                Ok(occurrences)
            }
        }
    }

    /// Returns the instances of the input node under the parent binding.
    ///
    /// An axis the parent bound or a split pinned keeps its instance. The
    /// composition carries no instance count, so the probe of the first free
    /// axis stops at the first instance the read does not find.
    pub(super) fn instances(
        &self,
        name: &str,
        input: &OpenehrTarget,
        parent: &Binding,
    ) -> Result<Vec<Vec<RmPosition>>, EngineError> {
        let nodes = self.node_instances(name, input, parent)?;
        if input.lists().is_empty() {
            return Ok(nodes);
        }
        let mut found = Vec::new();
        for positions in nodes {
            found.extend(self.element_instances(name, input, parent, positions)?);
        }
        Ok(found)
    }

    /// Returns the instance of the input node followed by every element of
    /// each `List` axis of the input inside it, the element a parent bound
    /// keeping its place.
    ///
    /// The list holds its elements in order, so element `n` is the
    /// positional predicate `[n]` of openEHR BASE Release 1.2.0 §Paths and
    /// Locators.
    fn element_instances(
        &self,
        name: &str,
        input: &OpenehrTarget,
        parent: &Binding,
        node: Vec<RmPosition>,
    ) -> Result<Vec<Vec<RmPosition>>, EngineError> {
        let Some(value) = self.node_value(name, input, &node)? else {
            return Ok(Vec::new());
        };
        let depth = node.len();
        let bound = parent.shared_lists(input);
        let mut found = vec![node];
        for (offset, axis) in input.lists().iter().enumerate() {
            let mut deeper = Vec::new();
            for partial in found {
                if let Some(&held) = parent.elements.get(offset).filter(|_| offset < bound) {
                    let mut positions = partial;
                    positions.push(held);
                    deeper.push(positions);
                    continue;
                }
                let outer = partial.get(depth..).unwrap_or_default();
                let length = list_length(&value, input, axis, outer);
                let ceiling = u32::try_from(length).unwrap_or(INSTANCE_CEILING);
                for element in 1..=ceiling.min(INSTANCE_CEILING) {
                    let mut positions = partial.clone();
                    positions.push(RmPosition::new(element).map_err(|source| {
                        EngineError::Position {
                            mapping: String::from(name),
                            source,
                        }
                    })?);
                    deeper.push(positions);
                }
            }
            found = deeper;
        }
        Ok(found)
    }

    /// Returns the instances of the input node under the parent binding.
    fn node_instances(
        &self,
        name: &str,
        input: &OpenehrTarget,
        parent: &Binding,
    ) -> Result<Vec<Vec<RmPosition>>, EngineError> {
        let axes = input.occurrences();
        let depth = axes.len();
        let bound = parent.shared(axes);
        let mut prefix: Vec<RmPosition> = parent.openehr.iter().take(bound).copied().collect();
        while let Some(pinned) = axes
            .get(prefix.len())
            .and_then(|axis| self.pin(axis.as_str()))
        {
            prefix.push(pinned);
        }
        if prefix.len() >= depth {
            prefix.truncate(depth);
            // NOTE: no specification governs this: our own design, a node the
            // composition does not hold is no input occurrence, so nothing below it runs.
            if !self.node_held(name, input, &prefix)? {
                return Ok(Vec::new());
            }
            return Ok(vec![prefix]);
        }
        let start = prefix.len();
        let mut found = vec![prefix];
        for (offset, axis) in axes.iter().enumerate().skip(start) {
            let last = offset.saturating_add(1) == depth;
            let mut deeper = Vec::new();
            for partial in found {
                let mut instance = 1u32;
                while instance <= INSTANCE_CEILING {
                    let mut positions = partial.clone();
                    let pinned = self.pin(axis.as_str());
                    positions.push(match pinned {
                        Some(pinned) => pinned,
                        None => {
                            RmPosition::new(instance).map_err(|source| EngineError::Position {
                                mapping: String::from(name),
                                source,
                            })?
                        }
                    });
                    let held = if last {
                        self.node_held(name, input, &positions)?
                    } else {
                        self.axis_held(name, axis, &positions)?
                    };
                    if !held {
                        break;
                    }
                    deeper.push(positions);
                    if pinned.is_some() {
                        break;
                    }
                    instance = instance.saturating_add(1);
                }
            }
            found = deeper;
        }
        Ok(found)
    }

    /// Returns whether the composition holds one instance of the repeating
    /// node `axis`, a free axis above an input node.
    ///
    /// Each free axis is probed on its own node, outermost first, so an input
    /// below two repeating nodes the parent did not bind reads every instance
    /// of both, as the start archetype's instances are found.
    fn axis_held(
        &self,
        name: &str,
        axis: &FlatId,
        positions: &[RmPosition],
    ) -> Result<bool, EngineError> {
        let Some(composition) = self.composition else {
            return Ok(false);
        };
        let refuse = |source| EngineError::Template {
            mapping: String::from(name),
            node: String::from(axis.as_str()),
            source: Box::new(source),
        };
        let node = self.index.node_by_flat_id(axis).map_err(refuse)?;
        self.index
            .holds(composition, node, positions)
            .map_err(refuse)
    }

    /// Reads one instance of a target's node, or of the `ELEMENT` the node is
    /// the value of when the target's tail names the element's attributes.
    fn read_target(
        &self,
        composition: &dyn CanonicalDocument,
        target: &OpenehrTarget,
        positions: &[RmPosition],
    ) -> Result<Option<serde_json::Value>, PathError> {
        let positions = at_node(target, positions);
        if target.on_element() {
            self.index
                .read_element(composition, target.node(), positions)
        } else {
            self.index.read(composition, target.node(), positions)
        }
    }

    /// Returns whether the composition holds one instance of the target's
    /// node, counting a value of another alternative of a polymorphic
    /// `ELEMENT` ([`openehr_mapping_core::index::WebTemplateIndex::holds`]).
    pub(super) fn node_held(
        &self,
        name: &str,
        input: &OpenehrTarget,
        positions: &[RmPosition],
    ) -> Result<bool, EngineError> {
        let Some(composition) = self.composition else {
            return Ok(false);
        };
        let node = input.node();
        let positions = at_node(input, positions);
        let held = if input.on_element() {
            self.index
                .read_element(composition, node, positions)
                .map(|element| element.is_some())
        } else {
            self.index.holds(composition, node, positions)
        };
        held.map_err(|source| EngineError::Template {
            mapping: String::from(name),
            node: String::from(node.aql_path().as_str()),
            source: Box::new(source),
        })
    }

    /// Returns the canonical JSON one instance of the target's node holds.
    pub(super) fn node_value(
        &self,
        name: &str,
        input: &OpenehrTarget,
        positions: &[RmPosition],
    ) -> Result<Option<serde_json::Value>, EngineError> {
        let Some(composition) = self.composition else {
            return Ok(None);
        };
        let node = input.node();
        let value = self
            .read_target(composition, input, positions)
            .map_err(|source| EngineError::Template {
                mapping: String::from(name),
                node: String::from(node.aql_path().as_str()),
                source: Box::new(source),
            })?;
        if value.is_some() {
            self.carried
                .borrow_mut()
                .insert(unread::instance_of(node.flat_id(), positions));
        }
        Ok(value)
    }

    /// Returns the value one instance of the input holds at the end of its
    /// path, below the node when the path names a tail.
    ///
    /// The tail is walked by attribute name over the node's canonical JSON,
    /// and the leaf is read as the class the resolver recorded for it, so the
    /// tail's class selects the cell. A tail no FLAT part of the node's class
    /// carries is refused here as it is on the write side.
    pub(super) fn value_at(
        &self,
        mapping: &Mapping,
        input: &OpenehrTarget,
        positions: &[RmPosition],
    ) -> Result<Option<Leaf>, EngineError> {
        let Some(value) = self.node_value(mapping.name(), input, positions)? else {
            return Ok(None);
        };
        let node = input.node();
        if !input.tail().segments.is_empty() {
            let segments = tail_segments(input);
            let (_class, carried) = rm::carried::carried(input.tail_class(), &segments)
                .ok_or_else(|| unsupported_tail(mapping, input))?;
            if input.on_element() && segments == [rm::null_flavour::NULL_FLAVOUR] {
                return rm::null_flavour::null_flavour_of(&value, node.aql_path().as_str())
                    .map(|found| found.map(|flavour| Leaf::NullFlavour(Box::new(flavour))))
                    .map_err(|source| EngineError::Rm {
                        mapping: String::from(mapping.name()),
                        node: String::from(node.aql_path().as_str()),
                        source: Box::new(source),
                    });
            }
            let elements = positions
                .get(input.occurrences().len()..)
                .unwrap_or_default();
            let Some(found) = attribute_at(&value, input, elements) else {
                return Ok(None);
            };
            if !matches!(
                carried,
                Carried::Value | Carried::Family | Carried::EntryTime
            ) {
                return Ok(scalar(found).map(Leaf::Scalar));
            }
            let leaf = input
                .leaf_class()
                .ok_or_else(|| unsupported_tail(mapping, input))?;
            return RmValue::from_canonical(leaf, node.aql_path().as_str(), found)
                .map(|value| Some(Leaf::Value(Box::new(value))))
                .map_err(|source| EngineError::Rm {
                    mapping: String::from(mapping.name()),
                    node: String::from(node.aql_path().as_str()),
                    source: Box::new(source),
                });
        }
        RmValue::from_canonical(node.rm_type(), node.aql_path().as_str(), &value)
            .map(|value| Some(Leaf::Value(Box::new(value))))
            .map_err(|source| EngineError::Rm {
                mapping: String::from(mapping.name()),
                node: String::from(node.aql_path().as_str()),
                source: Box::new(source),
            })
    }

    /// Applies one mapping's method to every admitted input occurrence.
    ///
    /// Returns the binding each occurrence produced, which is what a
    /// `followedBy` child runs under.
    fn apply(
        &mut self,
        mapping: &Mapping,
        parent: &Binding,
        inputs: &[Occurrence],
    ) -> Result<Vec<Binding>, EngineError> {
        match *mapping.method() {
            Method::Value => self.values(mapping, parent, inputs),
            Method::Slot {
                ref model,
                ref preprocessors,
                ref mappings,
            }
            | Method::SlotContext {
                ref model,
                ref preprocessors,
                ref mappings,
                ..
            } => {
                let name = String::from(model.as_str());
                if self
                    .chain
                    .iter()
                    .filter(|entered| **entered == name)
                    .count()
                    >= SLOT_DEPTH_LIMIT
                {
                    return Err(EngineError::SlotDepth {
                        chain: self.chain.join(" -> "),
                        model: name,
                        limit: SLOT_DEPTH_LIMIT,
                    });
                }
                let bindings = self.bindings(mapping, parent, inputs)?;
                let mut admitted = Vec::with_capacity(bindings.len());
                for binding in &bindings {
                    if self.slot_admits(mapping, preprocessors, binding)? {
                        admitted.push(binding);
                    } else {
                        let location = self.locate(mapping, &binding.fhir);
                        self.warnings.push(Warning::new(
                            Loss::Skipped {
                                mapping: String::from(mapping.name()),
                                reason: SkipReason::PreprocessorGate {
                                    model: name.clone(),
                                },
                            },
                            location,
                        ));
                    }
                }
                // NOTE: HierarchyMappings.adoc, the hierarchy lives in the
                // preprocessor of the file it belongs to, so a slotted file
                // splits the slotted mappings under each slot binding.
                let hierarchy = preprocessors
                    .iter()
                    .find(|preprocessor| preprocessor.model() == model)
                    .and_then(Preprocessor::hierarchy);
                self.chain.push(name);
                for binding in admitted {
                    self.split_or_run(mappings, hierarchy, model.as_str(), binding)?;
                }
                self.chain.pop();
                Ok(bindings)
            }
            Method::Programmed {
                ref code,
                ref ports,
            } => self.programmed(mapping, parent, inputs, code, ports),
            Method::Reference {
                ref resource,
                ref mappings,
            } => self.reference(mapping, parent, inputs, resource, mappings),
            Method::Link {
                ref meaning,
                ref link_type,
            } => {
                let meaning = meaning.as_deref().unwrap_or(mapping.name());
                let link_type = link_type.as_deref().unwrap_or(LINK_TYPE);
                self.link(mapping, parent, inputs, meaning, link_type)
            }
            Method::Participation { ref function } => {
                self.participation(mapping, parent, inputs, function)
            }
        }
    }

    /// Returns whether every preprocessor gate of a slotted file admits the
    /// input at `binding`.
    ///
    /// A file's preprocessor condition "defines that the mapping file is only
    /// executed if the given condition is met" (`basics/Conditions.adoc`,
    /// §Conditions in the preprocessor), read on the input side like every
    /// condition, so a closed gate skips the slotted mappings for that
    /// occurrence and the skip is a recorded outcome.
    fn slot_admits(
        &self,
        mapping: &Mapping,
        preprocessors: &[Preprocessor],
        binding: &Binding,
    ) -> Result<bool, EngineError> {
        for preprocessor in preprocessors {
            let gate = match self.direction {
                Direction::FhirToOpenehr => preprocessor.fhir_condition(),
                Direction::OpenehrToFhir => preprocessor.openehr_condition(),
            };
            let Some(gate) = gate.filter(|gate| condition::runs(gate, self.direction)) else {
                continue;
            };
            let holds = match self.direction {
                Direction::FhirToOpenehr => {
                    condition::evaluate(self.table, self.condition_document(gate), gate, false)
                        .map(|verdict| verdict.admits_any())
                        .map_err(|source| EngineError::Condition {
                            mapping: String::from(mapping.name()),
                            source: Box::new(source),
                        })?
                }
                Direction::OpenehrToFhir => {
                    self.openehr_holds(preprocessor.model().as_str(), gate, binding)?
                }
            };
            if !holds {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Runs the `followedBy` children of one mapping under its binding.
    ///
    /// "If we have a parent node with a `1..1` cardinality and a child node
    /// with a `1..1` cardinality, the mapping should fail if the child is not
    /// provided" (`engine/Fail.adoc`), so a child the template requires and
    /// the input does not carry refuses the unit. A child its own condition
    /// closed was provided and is none the mapping maps, so it is skipped,
    /// and a structural node counts as provided once any value below it is
    /// written.
    fn children(&mut self, mapping: &Mapping, binding: &Binding) -> Result<(), EngineError> {
        for child in mapping.followed_by() {
            let ran = self.mapping(child, binding)?;
            if self.direction != Direction::FhirToOpenehr || ran == Ran::Gated {
                continue;
            }
            let Some(target) = child.openehr() else {
                continue;
            };
            if target.node().min().is_none_or(|min| min < 1) {
                continue;
            }
            if !self.provided(target) {
                return Err(EngineError::MissingRequired {
                    mapping: String::from(child.name()),
                    node: String::from(target.node().aql_path().as_str()),
                });
            }
        }
        Ok(())
    }

    /// Returns whether the run wrote the node a required child names.
    ///
    /// A structural node holds no value of its own, so it is provided once
    /// the run wrote any value at or below it.
    fn provided(&self, target: &OpenehrTarget) -> bool {
        let node = target.node().flat_id();
        if !openehr_rm::v1_2::model::is_a(target.node().rm_type(), derive::STRUCTURAL) {
            return self.written.iter().any(|written| written.flat_id == *node);
        }
        let below = format!("{}/", node.as_str());
        self.written
            .iter()
            .any(|written| written.flat_id == *node || written.flat_id.as_str().starts_with(&below))
            || self.families.iter().any(|family| {
                family
                    .flat_id()
                    .is_some_and(|flat_id| flat_id == node || flat_id.as_str().starts_with(&below))
            })
    }
}

/// Whether a mapping ran, or its input side kept it from running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ran {
    /// The mapping ran over the occurrences its input admitted.
    Ran,
    /// The unidirectional marker or the input side's condition kept it from
    /// running.
    Gated,
}

/// Returns the positions of a target's node among `positions`, which may go
/// on with the element of each `List` axis below it.
fn at_node<'positions>(
    target: &OpenehrTarget,
    positions: &'positions [RmPosition],
) -> &'positions [RmPosition] {
    positions
        .get(..target.occurrences().len())
        .unwrap_or(positions)
}

/// Returns the `value` of a text value a condition reads with an empty tail,
/// and anything else as it stands.
///
/// A `manual` write treats an empty tail as the node's `value`
/// (`types-of-mappings/concept-type/manual.adoc`), so a condition on the same
/// path compares that text. No specification governs reading the tail: our
/// own design.
fn text_of(value: &serde_json::Value) -> &serde_json::Value {
    let class = value.get("_type").and_then(serde_json::Value::as_str);
    match (class, value.get("value")) {
        (Some(class), Some(text))
            if rm::carried::carried(class, &["value"])
                .is_some_and(|(_, held)| held == Carried::Text) =>
        {
            text
        }
        _ => value,
    }
}

/// Returns the class the `type` operator tests a condition attribute by: the
/// class of the data value the attribute holds.
///
/// `basics/Conditions.adoc` §openehrCondition names `items[at0001]`, an
/// `ELEMENT`, and tests `DV_CODED_TEXT`, the class of its value; a value
/// states its class as `_type` (openEHR ITS-JSON). The node's declared class
/// answers for a value that states none.
fn held_class(value: &serde_json::Value, declared: &str) -> String {
    let stated = value.get("_type").and_then(serde_json::Value::as_str);
    let held = match stated {
        Some("ELEMENT") => value
            .get("value")
            .and_then(|inner| inner.get("_type"))
            .and_then(serde_json::Value::as_str),
        other => other,
    };
    String::from(held.unwrap_or(declared))
}
