// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A PROGRAMMED mapping, run through the registered function its
//! `mappingCode` names.
//!
//! The mapping "will not auto-transform between the paths, since the mapping
//! logic is done in the program code"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/concept-type/concept-mappings.adoc`,
//! §Programmed mappings), so no data-type cell runs: the function reads the
//! whole FHIR element going in and the values at its ports coming out, and
//! the engine writes what it returns where the binding places it.

use fhir_types::codec::Value;

use crate::engine::traverse::functions::Datum;
use crate::engine::traverse::functions::MappingCodeError;
use crate::engine::traverse::functions::Ports;
use crate::model::ast::keyword::Direction;
use crate::resolve::program::mapping::Mapping;
use crate::resolve::program::port::Port;
use crate::resolve::program::target::FhirTarget;
use crate::tree::Occurrence;
use crate::tree::element::Table;
use crate::tree::read::Match;
use crate::tree::read::read;
use crate::tree::write::write;

use crate::engine::traverse::Binding;
use crate::engine::traverse::Held;
use crate::engine::traverse::Run;
use crate::engine::traverse::Written;
use crate::engine::traverse::error::EngineError;

impl<T: Table + ?Sized> Run<'_, T> {
    /// Runs the registered function a `mappingCode` names over every
    /// admitted input occurrence.
    pub(super) fn programmed(
        &mut self,
        mapping: &Mapping,
        parent: &Binding,
        inputs: &[Occurrence],
        code: &str,
        ports: &[Port],
    ) -> Result<Vec<Binding>, EngineError> {
        let bindings = self.bindings(mapping, parent, inputs)?;
        for (input, binding) in inputs.iter().zip(bindings.iter()) {
            match self.direction {
                Direction::FhirToOpenehr => {
                    self.programmed_in(mapping, code, ports, input, binding)?;
                }
                Direction::OpenehrToFhir => {
                    self.programmed_out(mapping, code, ports, input, binding)?;
                }
            }
        }
        Ok(bindings)
    }

    /// Runs the function over one FHIR element and writes each port it
    /// returns.
    fn programmed_in(
        &mut self,
        mapping: &Mapping,
        code: &str,
        ports: &[Port],
        input: &Occurrence,
        binding: &Binding,
    ) -> Result<(), EngineError> {
        let Some(target) = mapping.fhir() else {
            return Ok(());
        };
        let Some(element) = self.fhir_element(mapping, target, input)? else {
            return Ok(());
        };
        let produced = self
            .seams
            .functions()
            .to_openehr(code, &element)
            .map_err(|source| refusal(mapping, code, source))?;
        for (name, datum) in produced.iter() {
            let port = ports
                .iter()
                .find(|port| port.name() == name)
                .ok_or_else(|| {
                    refusal(
                        mapping,
                        code,
                        MappingCodeError::UndeclaredPort {
                            code: String::from(code),
                            port: String::from(name),
                        },
                    )
                })?;
            let node = port.value_of(datum.class()).ok_or_else(|| {
                refusal(
                    mapping,
                    code,
                    MappingCodeError::UnadmittedClass {
                        code: String::from(code),
                        port: String::from(name),
                        class: String::from(datum.class()),
                    },
                )
            })?;
            self.store(Written {
                flat_id: node.node().flat_id().clone(),
                positions: binding.openehr.clone(),
                value: Held::Canonical(datum.value().clone()),
            });
        }
        Ok(())
    }

    /// Reads the values at the function's ports for one input instance and
    /// merges the FHIR element the function returns into the output.
    fn programmed_out(
        &mut self,
        mapping: &Mapping,
        code: &str,
        ports: &[Port],
        input: &Occurrence,
        binding: &Binding,
    ) -> Result<(), EngineError> {
        let mut values = Ports::new();
        if let Some(anchor) = mapping.openehr() {
            let positions = Self::positions_of(mapping, input, anchor)?;
            for port in ports {
                for (class, node) in port.values() {
                    if let Some(value) = self.node_value(mapping.name(), node, &positions)? {
                        values = values.with(port.name(), Datum::new(class, value));
                        break;
                    }
                }
            }
        }
        let produced = self
            .seams
            .functions()
            .to_fhir(code, &values)
            .map_err(|source| refusal(mapping, code, source))?;
        let (Some(element), Some(target)) = (produced, mapping.fhir()) else {
            return Ok(());
        };
        self.merge_fhir(mapping, target, binding, element)
    }

    /// Returns the FHIR element one input occurrence names, `None` when the
    /// document holds none there.
    fn fhir_element(
        &self,
        mapping: &Mapping,
        target: &FhirTarget,
        occurrence: &Occurrence,
    ) -> Result<Option<Value>, EngineError> {
        let matches =
            read(self.table, self.document_for(target), target.expression()).map_err(|source| {
                EngineError::Read {
                    mapping: String::from(mapping.name()),
                    expression: String::from(target.expression().as_str()),
                    source: Box::new(source),
                }
            })?;
        Ok(matches
            .iter()
            .find(|matched| matched.occurrence() == occurrence)
            .and_then(Match::value)
            .cloned())
    }

    /// Writes a FHIR element a function built at the binding's occurrence,
    /// its members over the members the element already holds.
    ///
    /// Several functions write members of one element (`Timing.repeat` takes
    /// the frequency from one and the duration from another), so a function's
    /// element is merged rather than written over. No specification governs
    /// this: our own design.
    fn merge_fhir(
        &mut self,
        mapping: &Mapping,
        target: &FhirTarget,
        binding: &Binding,
        element: Value,
    ) -> Result<(), EngineError> {
        let held = self.fhir_element(mapping, target, &binding.fhir)?;
        let merged = match (held, element) {
            (Some(Value::Object(mut held)), Value::Object(produced)) => {
                held.extend(produced);
                Value::Object(held)
            }
            (_, element) => element,
        };
        let table = self.table;
        write(
            table,
            self.document_for_mut(target),
            target.expression(),
            &binding.fhir,
            merged,
        )
        .map_err(|source| EngineError::Write {
            mapping: String::from(mapping.name()),
            element: String::from(target.resolved().leaf()),
            source: Box::new(source),
        })
    }
}

/// Wraps a function's refusal with the mapping that called it.
fn refusal(mapping: &Mapping, code: &str, source: MappingCodeError) -> EngineError {
    EngineError::MappingCode {
        mapping: String::from(mapping.name()),
        code: String::from(code),
        source: Box::new(source),
    }
}
