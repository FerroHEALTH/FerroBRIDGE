// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The frames of one run: where a loss is located, which document and pin a
//! target reads and writes, and the scope a nested mapping binds in.

use super::{Binding, Frame, Run, openehr_path};
use crate::engine::outcome::Location;
use crate::model::ast::keyword::Direction;
use crate::resolve::program::mapping::Mapping;
use crate::resolve::program::target::FhirTarget;
use crate::resolve::program::target::OpenehrTarget;
use crate::tree::Occurrence;
use crate::tree::element::Table;
use crate::tree::locate::locate;
use fhir_types::codec::Value;
use std::collections::BTreeMap;

impl<T: Table + ?Sized> Run<'_, T> {
    /// Returns where a loss at `mapping` happened, its FHIR side at
    /// `occurrence`.
    ///
    /// The FHIR occurrence a binding carries is the input occurrence going
    /// into openEHR and the output occurrence coming out to FHIR, so the one
    /// rule locates the element the loss concerns in both directions.
    pub(super) fn locate(&self, mapping: &Mapping, occurrence: &Occurrence) -> Location {
        self.locate_targets(mapping.fhir(), occurrence, mapping.openehr())
    }

    /// Returns the location of a FHIR and an openEHR target, either absent.
    pub(super) fn locate_targets(
        &self,
        fhir: Option<&FhirTarget>,
        occurrence: &Occurrence,
        openehr: Option<&OpenehrTarget>,
    ) -> Location {
        let mut location = Location::default();
        if let Some(target) = fhir {
            let document = self.document_for(target);
            location = location.with_fhir(locate(document, target.resolved(), occurrence));
            if let Some(reference) = reference_of(document) {
                location = location.with_resource(reference);
            }
        }
        if let Some(target) = openehr {
            location = location.with_openehr(openehr_path(target));
        }
        location
    }

    /// Returns the FHIR document `target` is rooted in: the current one, or
    /// the one an enclosing `reference` mapping left.
    pub(super) fn document_for(&self, target: &FhirTarget) -> &Value {
        self.frames
            .get(target.level())
            .map_or(&self.fhir, |frame| &frame.document)
    }

    /// Returns the FHIR document a condition's `targetRoot` is rooted in.
    pub(super) fn condition_document(
        &self,
        gate: &crate::resolve::program::condition::Condition,
    ) -> &Value {
        match *gate.target() {
            crate::resolve::program::target::Target::Fhir(ref target) => self.document_for(target),
            crate::resolve::program::target::Target::Openehr(_) => &self.fhir,
        }
    }

    /// Returns the FHIR document `target` is rooted in, to write.
    pub(super) fn document_for_mut(&mut self, target: &FhirTarget) -> &mut Value {
        match self.frames.get_mut(target.level()) {
            Some(frame) => &mut frame.document,
            None => &mut self.fhir,
        }
    }

    /// Returns the split pin of the document `target` is rooted in.
    pub(super) fn pin_for(&self, target: &FhirTarget) -> Option<&(Vec<String>, Occurrence)> {
        self.frames
            .get(target.level())
            .map_or(self.fhir_pin.as_ref(), |frame| frame.pin.as_ref())
    }

    /// Returns the split pin of the document of `level`, to set.
    pub(super) fn pin_at(&mut self, level: usize) -> &mut Option<(Vec<String>, Occurrence)> {
        match self.frames.get_mut(level) {
            Some(frame) => &mut frame.pin,
            None => &mut self.fhir_pin,
        }
    }

    /// Returns the instance counters of the document of `level`.
    pub(super) fn counters_at(&mut self, level: usize) -> &mut BTreeMap<(String, String), usize> {
        match self.frames.get_mut(level) {
            Some(frame) => &mut frame.counters,
            None => &mut self.counters,
        }
    }

    /// Returns the binding a mapping whose FHIR side is `target` runs under.
    ///
    /// A parent at the target's level binds it. A target a `^` run bound in
    /// an enclosing document lies under the occurrence that document's
    /// reference was entered at, whatever the parent inside the reference
    /// bound.
    pub(super) fn scoped(&self, target: Option<&FhirTarget>, parent: &Binding) -> Binding {
        let Some(target) = target.filter(|target| target.level() != parent.level) else {
            return parent.clone();
        };
        self.frames.get(target.level()).map_or_else(
            || parent.clone(),
            |frame| Binding {
                fhir: frame.occurrence.clone(),
                openehr: parent.openehr.clone(),
                axes: parent.axes.clone(),
                level: target.level(),
            },
        )
    }

    /// Enters a document of its own for a `reference` mapping, keeping the
    /// current one as a frame, and returns the level of the entered one.
    ///
    /// Going into openEHR the FHIR side is the input, so the frame keeps the
    /// split pin and the counters stay with the openEHR output; coming out
    /// the FHIR side is the output, so the frame keeps the instance counters
    /// of the document left.
    pub(super) fn enter_frame(&mut self, document: Value, occurrence: Occurrence) -> usize {
        // NOTE: no specification governs this: our own design, the document a
        // `^` run climbs back into is kept whole beside the occurrence it left.
        let document = core::mem::replace(&mut self.fhir, document);
        let (pin, counters) = match self.direction {
            Direction::FhirToOpenehr => (self.fhir_pin.take(), BTreeMap::new()),
            Direction::OpenehrToFhir => (None, core::mem::take(&mut self.counters)),
        };
        self.frames.push(Frame {
            document,
            occurrence,
            pin,
            counters,
        });
        self.frames.len()
    }

    /// Leaves the document [`Self::enter_frame`] entered, and returns it.
    #[expect(
        clippy::expect_used,
        reason = "every caller pairs this with the enter_frame it made in the same function, \
                  so a frame is always there to leave"
    )]
    pub(super) fn leave_frame(&mut self) -> Value {
        let frame = self
            .frames
            .pop()
            .expect("a reference should leave the frame it entered");
        match self.direction {
            Direction::FhirToOpenehr => self.fhir_pin = frame.pin,
            Direction::OpenehrToFhir => self.counters = frame.counters,
        }
        core::mem::replace(&mut self.fhir, frame.document)
    }
}

/// Returns the relative literal reference of the resource `document` is, as
/// `Type/id` (<https://hl7.org/fhir/R4/references.html>), `None` when it
/// carries no `resourceType` or no `id`.
fn reference_of(document: &Value) -> Option<String> {
    let kind = document.get("resourceType").and_then(Value::as_str)?;
    let id = document.get("id").and_then(Value::as_str)?;
    Some(format!("{kind}/{id}"))
}
