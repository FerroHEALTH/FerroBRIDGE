// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The data-type cell of one mapping, run over its input values.

use fhir_types::codec::Value;
use fhir_types::r4::primitives::String as FhirString;
use openehr_mapping_core::composition::RmPosition;
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::model;

use crate::engine::cell;
use crate::engine::fhir::FhirKind;
use crate::engine::fhir::FhirValue;
use crate::engine::lens::LensError;
use crate::engine::lens::term_mapping::Withheld;
use crate::engine::outcome::Warning;
use crate::engine::rm::RmValue;
use crate::model::ast::keyword::Direction;
use crate::resolve::program::manual::Manual;
use crate::resolve::program::manual::ManualValue;
use crate::resolve::program::mapping::Derived;
use crate::resolve::program::mapping::Mapping;
use crate::resolve::program::target::FhirTarget;
use crate::resolve::program::target::OpenehrTarget;
use crate::tree::Occurrence;

use crate::tree::element::Table;

use crate::engine::traverse::Binding;
use crate::engine::traverse::Leaf;
use crate::engine::traverse::Run;
use crate::engine::traverse::Written;
use crate::engine::traverse::demographic::Shape;
use crate::engine::traverse::error::EngineError;
use crate::engine::traverse::place::rm_positions;
use crate::engine::traverse::translate::Output;

impl<T: Table + ?Sized> Run<'_, T> {
    /// Returns the text a `manual` path writes.
    ///
    /// A `$context` member is a per-call value the caller supplies, so a
    /// member this run does not carry is a refusal rather than an invented
    /// value (`basics/Variables.adoc`, §`$context`).
    pub(super) fn manual_text<'run>(
        &'run self,
        mapping: &Mapping,
        entry: &Manual,
        value: &'run ManualValue,
    ) -> Result<&'run str, EngineError> {
        match *value {
            ManualValue::Literal(ref text) => Ok(text.as_str()),
            ManualValue::Context(ref member) => {
                self.context
                    .member(member)
                    .ok_or_else(|| EngineError::ContextMember {
                        mapping: format!("{}.{}", mapping.name(), entry.name()),
                        member: member.clone(),
                    })
            }
        }
    }

    /// Runs the data-type cell of a plain `with` mapping, or only binds.
    pub(super) fn values(
        &mut self,
        mapping: &Mapping,
        parent: &Binding,
        inputs: &[Occurrence],
    ) -> Result<Vec<Binding>, EngineError> {
        let bindings = self.bindings(mapping, parent, inputs)?;
        if !mapping.manual().is_empty() {
            for entry in mapping.manual() {
                for binding in &bindings {
                    self.manual(mapping, entry, binding)?;
                }
            }
            return Ok(bindings);
        }
        // NOTE: PopulatingAnEntry.adoc writes `type: NONE` on the parent whose
        // `followedBy` children carry every value, so the parent anchors them
        // and transforms nothing of its own (recorded as a silence in #185).
        if mapping.data_type() == Some(crate::model::ast::keyword::DataType::None)
            || matches!(mapping.derived(), Some(Derived::Anchor))
        {
            return Ok(bindings);
        }
        let (Some(_fhir), Some(_openehr)) = (mapping.fhir(), mapping.openehr()) else {
            return Ok(bindings);
        };
        for (input, binding) in inputs.iter().zip(bindings.iter()) {
            self.convert(mapping, input, binding)?;
        }
        Ok(bindings)
    }

    /// Returns the FHIR value one openEHR value reads as, with what the cell
    /// withheld, a party's minted identity read back as its reference.
    fn read_value(
        &self,
        value: &RmValue,
        kind: FhirKind,
        terminology: Option<&str>,
    ) -> Result<(FhirValue, Dropped), LensError> {
        let (value, reference) = self.minted(value);
        let dropped = Dropped {
            code: cell::uncarried_code(&value, kind).cloned(),
            mappings: cell::withheld(&value, kind, terminology),
        };
        let mut view = cell::get(&value, kind, terminology)?;
        if let (Some(reference), FhirValue::Reference(held)) = (reference, &mut view) {
            held.reference = Some(FhirString::from(reference));
        }
        Ok((view, dropped))
    }

    /// Runs one occurrence of a mapping through its data-type cell.
    fn convert(
        &mut self,
        mapping: &Mapping,
        input: &Occurrence,
        binding: &Binding,
    ) -> Result<(), EngineError> {
        if let Some(Derived::Event(event)) = mapping.derived() {
            return match self.direction {
                Direction::FhirToOpenehr => self.event_in(mapping, event, input, binding),
                Direction::OpenehrToFhir => self.event_out(mapping, event, input, binding),
            };
        }
        match self.direction {
            Direction::FhirToOpenehr => self.convert_in(mapping, input, binding),
            Direction::OpenehrToFhir => self.convert_out(mapping, input, binding),
        }
    }

    /// Runs one occurrence of a mapping from FHIR into openEHR.
    fn convert_in(
        &mut self,
        mapping: &Mapping,
        input: &Occurrence,
        binding: &Binding,
    ) -> Result<(), EngineError> {
        let Some(target) = mapping.fhir() else {
            return Ok(());
        };
        let found = match mapping.derived() {
            Some(Derived::Polymorphic { read, pairings }) => {
                self.polymorphic_in(mapping, read, pairings, input)?
            }
            _ => match converted(mapping) {
                Some(openehr) => self
                    .input_value(mapping, target, openehr, input)?
                    .map(|view| (openehr, view)),
                None => None,
            },
        };
        let Some((openehr, view)) = found else {
            return Ok(());
        };
        let node = openehr.node();
        let Some(view) = self.translated(mapping, view, Output::openehr(openehr, input))? else {
            return Ok(());
        };
        if !openehr.tail().segments.is_empty() {
            return self.convert_tail(mapping, target, openehr, &view, binding);
        }
        let held = self.held(&binding.openehr, node.flat_id());
        let mut produced = cell::put(
            &view,
            node.rm_type(),
            held.as_ref(),
            self.binding_of(node).as_deref(),
        )
        .map_err(|source| EngineError::Cell {
            mapping: String::from(mapping.name()),
            element: String::from(target.expression().as_str()),
            source: Box::new(source),
        })?;
        self.resolve_display(mapping, &view, &mut produced)?;
        for taken in &produced.fallbacks {
            self.warnings
                .push(Warning::fallback(taken, self.locate(mapping, input)));
        }
        let value = self.identified(mapping, &view, produced.value, Shape::at(node))?;
        self.put_openehr_at(openehr, binding, value);
        Ok(())
    }

    /// Runs one occurrence of a mapping from openEHR out to FHIR.
    fn convert_out(
        &mut self,
        mapping: &Mapping,
        input: &Occurrence,
        binding: &Binding,
    ) -> Result<(), EngineError> {
        let Some(target) = mapping.fhir() else {
            return Ok(());
        };
        let Some((openehr, source, target, kind)) = self.output_source(mapping, target, input)?
        else {
            return Ok(());
        };
        let refuse = |source: LensError| EngineError::Cell {
            mapping: String::from(mapping.name()),
            element: String::from(target.expression().as_str()),
            source: Box::new(source),
        };
        let (view, dropped) = match source {
            Leaf::Value(ref value) if let RmValue::PartyRef(ref party) = **value => (
                self.reference_of_party(mapping, target, party)?,
                Dropped::default(),
            ),
            Leaf::Value(ref value) => {
                let terminology = self.binding_of(openehr.node());
                self.read_value(value, kind, terminology.as_deref())
                    .map_err(refuse)?
            }
            Leaf::Scalar(ref text) => (
                Self::read_scalar(mapping, target, openehr, kind, text)?,
                Dropped::default(),
            ),
        };
        let output = Output::fhir(target, &binding.fhir);
        let Some(view) = self.translated(mapping, view, output)? else {
            return Ok(());
        };
        self.put_fhir_at(mapping, target, binding, &view)?;
        self.record_dropped(mapping, binding, &dropped);
        Ok(())
    }

    /// Returns the openEHR node one occurrence reads out of, what it holds
    /// there, and the FHIR target and kind it writes, `None` when the node
    /// holds nothing.
    ///
    /// A [`Derived::Polymorphic`] mapping reads the first value node, in the
    /// order of its pairs, that holds a value, and writes the alternative that
    /// node's first pair names.
    fn output_source<'mapping>(
        &self,
        mapping: &'mapping Mapping,
        target: &'mapping FhirTarget,
        input: &Occurrence,
    ) -> Result<Option<OutSource<'mapping>>, EngineError> {
        if let Some(Derived::Polymorphic { pairings, .. }) = mapping.derived() {
            // NOTE: openEHR RM data_structures §ELEMENT: the value alternatives share
            // the `value` attribute, so the class the instance carries picks the pair.
            let Some(first) = pairings.first() else {
                return Ok(None);
            };
            let positions = Self::positions_of(mapping, input, first.target())?;
            let Some(held) = self.node_value(mapping.name(), first.target(), &positions)? else {
                return Ok(None);
            };
            let class = held
                .get("_type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let chosen = pairings.iter().find(|pairing| {
                let paired = pairing.target().node().rm_type();
                paired == class || model::is_a(class, paired)
            });
            if let Some(pairing) = chosen {
                let openehr = pairing.target();
                let Some(source) = self.value_at(mapping, openehr, &positions)? else {
                    return Ok(None);
                };
                let written = pairing.alternative();
                let kind = FhirKind::of_code(written.code()).ok_or_else(|| {
                    EngineError::UnknownElement {
                        mapping: String::from(mapping.name()),
                        element: String::from(written.target().resolved().leaf()),
                    }
                })?;
                return Ok(Some((openehr, source, written.target(), kind)));
            }
            return Err(EngineError::UnknownElement {
                mapping: String::from(mapping.name()),
                element: format!("{} from a {class}", target.resolved().leaf()),
            });
        }
        let Some(openehr) = converted(mapping) else {
            return Ok(None);
        };
        let positions = Self::positions_of(mapping, input, openehr)?;
        let Some(source) = self.value_at(mapping, openehr, &positions)? else {
            return Ok(None);
        };
        let (target, kind) = Self::output_target(mapping, target)?;
        Ok(Some((openehr, source, target, kind)))
    }

    /// Reads the text a scalar leaf holds as the FHIR element `kind` names.
    fn read_scalar(
        mapping: &Mapping,
        target: &FhirTarget,
        openehr: &OpenehrTarget,
        kind: FhirKind,
        text: &str,
    ) -> Result<FhirValue, EngineError> {
        let refuse = |source: LensError| EngineError::Cell {
            mapping: String::from(mapping.name()),
            element: String::from(target.expression().as_str()),
            source: Box::new(source),
        };
        // NOTE: a scalar leaf carries text, so only an element that travels as
        // text, or one `String.adoc` writes as text, takes it; no specification
        // governs the rest: our own design.
        match kind {
            FhirKind::Coding | FhirKind::Identifier | FhirKind::Reference
                if openehr.leaf_class() == Some(cell::STRING) =>
            {
                cell::get_string(text, kind).map_err(refuse)
            }
            FhirKind::String
            | FhirKind::Date
            | FhirKind::DateTime
            | FhirKind::Time
            | FhirKind::Url => {
                let element = target.resolved().leaf();
                FhirValue::read(kind, element, &Value::String(String::from(text))).map_err(
                    |source| EngineError::Fhir {
                        mapping: String::from(mapping.name()),
                        element: String::from(element),
                        source: Box::new(source),
                    },
                )
            }
            FhirKind::Boolean
            | FhirKind::Coding
            | FhirKind::CodeableConcept
            | FhirKind::Period
            | FhirKind::Reference
            | FhirKind::Identifier
            | FhirKind::Quantity
            | FhirKind::Integer
            | FhirKind::Decimal
            | FhirKind::Count
            | FhirKind::Duration
            | FhirKind::Attachment
            | FhirKind::Range => Err(refuse(LensError::NoCell {
                rm_type: String::from(openehr.leaf_class().unwrap_or(cell::STRING)),
                kind: kind.as_str(),
                direction: Direction::OpenehrToFhir,
            })),
        }
    }

    /// Records the defining code and each term mapping a cell of `mapping`
    /// left out of the FHIR element it wrote under `binding`.
    fn record_dropped(&mut self, mapping: &Mapping, binding: &Binding, dropped: &Dropped) {
        if dropped.code.is_none() && dropped.mappings.is_empty() {
            return;
        }
        let location = self.locate(mapping, &binding.fhir);
        // NOTE: no specification governs this: our own design, each loss
        // concerns one attribute of the value, so the openEHR side names that
        // attribute below the node's path.
        let below = |attribute: &str| match location.openehr() {
            Some(path) => location.clone().with_openehr(format!("{path}/{attribute}")),
            None => location.clone(),
        };
        if let Some(ref code) = dropped.code {
            self.warnings.push(Warning::uncarried_code(
                mapping.name(),
                code,
                below("defining_code"),
            ));
        }
        let mappings = below("mappings");
        for taken in &dropped.mappings {
            self.warnings
                .push(Warning::withheld(mapping.name(), taken, mappings.clone()));
        }
    }

    /// Reads the FHIR element one input occurrence names, as the conversion
    /// the compiler derived or the `type` key names.
    fn input_value(
        &self,
        mapping: &Mapping,
        target: &FhirTarget,
        openehr: &OpenehrTarget,
        input: &Occurrence,
    ) -> Result<Option<FhirValue>, EngineError> {
        match mapping.derived() {
            Some(Derived::Choice { read, .. } | Derived::Polymorphic { read, .. }) => {
                self.choice_at(mapping, read, openehr, input)
            }
            Some(Derived::Declared(declared)) => {
                let kind = Self::kind(mapping, declared.target())?;
                self.element_at(mapping, declared.target(), input, kind)
            }
            Some(
                Derived::Element(_) | Derived::Anchor | Derived::Value { .. } | Derived::Event(_),
            )
            | None => {
                let kind = Self::kind(mapping, target)?;
                self.element_at(mapping, target, input, kind)
            }
        }
    }

    /// Returns the target a mapping writes FHIR through, with the element
    /// kind it writes.
    fn output_target<'mapping>(
        mapping: &'mapping Mapping,
        target: &'mapping FhirTarget,
    ) -> Result<(&'mapping FhirTarget, FhirKind), EngineError> {
        match mapping.derived() {
            Some(Derived::Choice {
                write: Some(written),
                ..
            }) => {
                let kind = FhirKind::of_code(written.code()).ok_or_else(|| {
                    EngineError::UnknownElement {
                        mapping: String::from(mapping.name()),
                        element: String::from(written.target().resolved().leaf()),
                    }
                })?;
                Ok((written.target(), kind))
            }
            Some(Derived::Declared(declared)) => {
                Ok((declared.target(), Self::kind(mapping, declared.target())?))
            }
            Some(Derived::Choice { write: None, .. } | Derived::Polymorphic { .. }) => {
                Err(EngineError::UnknownElement {
                    mapping: String::from(mapping.name()),
                    element: String::from(target.resolved().leaf()),
                })
            }
            Some(
                Derived::Element(_) | Derived::Anchor | Derived::Value { .. } | Derived::Event(_),
            )
            | None => Ok((target, Self::kind(mapping, target)?)),
        }
    }

    /// Stores one written value, overwriting what the same place holds.
    pub(super) fn store(&mut self, written: Written) {
        if let Some(slot) = self
            .written
            .iter_mut()
            .find(|held| held.flat_id == written.flat_id && held.positions == written.positions)
        {
            *slot = written;
        } else {
            self.written.push(written);
        }
    }

    /// Returns the positions one input occurrence names for an openEHR node.
    pub(super) fn positions_of(
        mapping: &Mapping,
        input: &Occurrence,
        target: &OpenehrTarget,
    ) -> Result<Vec<RmPosition>, EngineError> {
        let depth = target.occurrences().len();
        let taken = input.indices().get(..depth).unwrap_or(input.indices());
        let mut positions = rm_positions(mapping.name(), taken)?;
        positions.resize(depth, RmPosition::first());
        Ok(positions)
    }
}

/// Returns the openEHR node a mapping's data-type cell reads and writes.
///
/// A polymorphic `ELEMENT` converts into the value alternative the compiler
/// chose (`Derived::Value`); every other mapping converts at its own node.
fn converted(mapping: &Mapping) -> Option<&OpenehrTarget> {
    match mapping.derived() {
        Some(Derived::Value { target, .. }) => Some(target),
        _ => mapping.openehr(),
    }
}

/// What a cell read out of openEHR left out of the FHIR element it wrote.
#[derive(Debug, Default)]
struct Dropped {
    /// The `defining_code` a `string` cannot carry.
    code: Option<CodePhrase>,
    /// The term mappings the element cannot carry.
    mappings: Vec<Withheld>,
}

/// The node an occurrence reads out of, what it holds, and the FHIR target
/// and kind it writes.
type OutSource<'mapping> = (
    &'mapping OpenehrTarget,
    Leaf,
    &'mapping FhirTarget,
    FhirKind,
);
