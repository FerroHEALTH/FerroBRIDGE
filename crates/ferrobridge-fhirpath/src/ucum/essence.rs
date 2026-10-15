// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The prefixes and unit atoms of the vendored UCUM essence
//! (`ucum-essence.xml` of UCUM 2.2), read once and resolved to canonical
//! form.

use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::magnitude::{Magnitude, Ratio};
use super::{DIMENSIONS, Meaning, Proper, UcumError, evaluate, parse};

/// The essence, embedded whole and unmodified as the UCUM licence requires.
const TEXT: &str = include_str!("../../vendor/ucum/ucum-essence.xml");

/// The dimension letters of the essence's base units, in the order of a
/// [`Proper`] dimension vector (UCUM §28).
const LETTERS: [&str; DIMENSIONS] = ["L", "T", "M", "A", "C", "Q", "F"];

/// A unit atom: whether it takes a prefix (UCUM §11) and what it means,
/// `None` while the essence is still being resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Atom {
    pub(super) metric: bool,
    pub(super) meaning: Option<Meaning>,
}

/// The resolved tables.
#[derive(Debug)]
pub(super) struct Essence {
    pub(super) prefixes: BTreeMap<String, Ratio>,
    pub(super) atoms: BTreeMap<String, Atom>,
}

/// Why the embedded essence did not load.
#[derive(Debug, thiserror::Error)]
pub(super) enum EssenceError {
    #[error("the essence is not well-formed XML")]
    Xml(#[from] quick_xml::Error),
    #[error("an attribute of the essence is malformed")]
    Attribute(#[from] quick_xml::events::attributes::AttrError),
    #[error("a <{element}> carries no {attribute} attribute")]
    Missing {
        element: &'static str,
        attribute: &'static str,
    },
    #[error("'{code}' has the value '{text}', which is no decimal")]
    Value { code: String, text: String },
    #[error("the base unit '{code}' has the unknown dimension '{dim}'")]
    Dimension { code: String, dim: String },
    #[error("'{code}' is defined by a unit that does not resolve")]
    Unit {
        code: String,
        #[source]
        source: UcumError,
    },
    #[error("'{code}' depends on an atom that never resolves")]
    Unresolved { code: String },
}

/// A `<unit>` element as read, before its definition is resolved.
#[derive(Debug, Default)]
struct Definition {
    code: String,
    special: bool,
    arbitrary: bool,
    /// The `Unit` and `value` attributes of `<value>`.
    value: Option<(String, Option<String>)>,
    /// The `name`, `value` and `Unit` attributes of `<function>`.
    function: Option<(String, String, String)>,
}

impl Essence {
    /// Reads and resolves the embedded essence.
    pub(super) fn load() -> Result<Self, EssenceError> {
        Self::parse(TEXT)
    }

    /// Reads and resolves an essence document.
    fn parse(text: &str) -> Result<Self, EssenceError> {
        let mut reader = Reader::from_str(text);
        let mut prefixes = BTreeMap::new();
        let mut atoms = BTreeMap::new();
        let mut definitions = Vec::new();
        let mut prefix: Option<String> = None;
        let mut unit: Option<Definition> = None;
        loop {
            match reader.read_event()? {
                Event::Eof => break,
                Event::Start(start) | Event::Empty(start) => {
                    match start.local_name().as_ref() {
                        "prefix" => prefix = Some(required(&start, "prefix", "Code")?),
                        "base-unit" => {
                            let code = required(&start, "base-unit", "Code")?;
                            let dim = required(&start, "base-unit", "dim")?;
                            let index = LETTERS
                                .iter()
                                .position(|letter| *letter == dim)
                                .ok_or_else(|| EssenceError::Dimension {
                                    code: code.clone(),
                                    dim,
                                })?;
                            let mut dimension = [0; DIMENSIONS];
                            if let Some(slot) = dimension.get_mut(index) {
                                *slot = 1;
                            }
                            // NOTE: UCUM §28 marks every base unit metric.
                            let meaning = Meaning::Proper(Proper {
                                ratio: Ratio::ONE,
                                dimension,
                            });
                            atoms.insert(code, resolved(true, meaning));
                        }
                        "unit" => {
                            let code = required(&start, "unit", "Code")?;
                            atoms.insert(
                                code.clone(),
                                Atom {
                                    metric: flag(&start, "isMetric")?,
                                    meaning: None,
                                },
                            );
                            unit = Some(Definition {
                                code,
                                special: flag(&start, "isSpecial")?,
                                arbitrary: flag(&start, "isArbitrary")?,
                                ..Definition::default()
                            });
                        }
                        "value" => {
                            if let Some(code) = prefix.take() {
                                let text = required(&start, "value", "value")?;
                                let value = magnitude(&code, &text)?;
                                prefixes.insert(code, Ratio::new(value));
                            } else if let Some(definition) = unit.as_mut() {
                                definition.value = Some((
                                    required(&start, "value", "Unit")?,
                                    attribute(&start, "value")?,
                                ));
                            }
                        }
                        "function" => {
                            if let Some(definition) = unit.as_mut() {
                                definition.function = Some((
                                    required(&start, "function", "name")?,
                                    required(&start, "function", "value")?,
                                    required(&start, "function", "Unit")?,
                                ));
                            }
                        }
                        _ => {}
                    }
                }
                Event::End(end) if end.local_name().as_ref() == "unit" => {
                    definitions.extend(unit.take());
                }
                _ => {}
            }
        }
        let mut essence = Self { prefixes, atoms };
        essence.resolve(definitions)?;
        Ok(essence)
    }

    /// Resolves every unit definition, in as many passes as the chains of
    /// definitions need.
    fn resolve(&mut self, mut pending: Vec<Definition>) -> Result<(), EssenceError> {
        while !pending.is_empty() {
            let before = pending.len();
            let mut waiting = Vec::new();
            for definition in pending {
                match self.meaning(&definition)? {
                    Some(meaning) => {
                        if let Some(atom) = self.atoms.get_mut(&definition.code) {
                            atom.meaning = Some(meaning);
                        }
                    }
                    None => waiting.push(definition),
                }
            }
            if waiting.len() == before
                && let Some(stuck) = waiting.first()
            {
                return Err(EssenceError::Unresolved {
                    code: stuck.code.clone(),
                });
            }
            pending = waiting;
        }
        Ok(())
    }

    /// The meaning of a definition, `None` while it names an atom not yet
    /// resolved.
    fn meaning(&self, definition: &Definition) -> Result<Option<Meaning>, EssenceError> {
        let code = &definition.code;
        if definition.arbitrary {
            return Ok(Some(Meaning::Arbitrary));
        }
        let unit_error = |source| EssenceError::Unit {
            code: code.clone(),
            source,
        };
        let (scalar, term) = if definition.special {
            let (_, value, unit) = definition.function.as_ref().ok_or(EssenceError::Missing {
                element: "value",
                attribute: "function",
            })?;
            (value.as_str(), unit.as_str())
        } else {
            let (unit, value) = definition.value.as_ref().ok_or(EssenceError::Missing {
                element: "unit",
                attribute: "value",
            })?;
            let value = value.as_deref().ok_or(EssenceError::Missing {
                element: "value",
                attribute: "value",
            })?;
            (value, unit.as_str())
        };
        let scalar = Ratio::new(magnitude(code, scalar)?);
        let parsed = parse::main_term(term).map_err(unit_error)?;
        let Some(meaning) = evaluate(&parsed, self).map_err(unit_error)? else {
            return Ok(None);
        };
        let proper = match meaning {
            Meaning::Proper(proper) => proper,
            Meaning::Special { function, .. } => {
                return Err(unit_error(UcumError::SpecialInTerm { function }));
            }
            // UCUM §25: a term involving an arbitrary unit is arbitrary.
            Meaning::Arbitrary => return Ok(Some(Meaning::Arbitrary)),
        };
        let proper = Proper {
            ratio: proper
                .ratio
                .mul(scalar)
                .ok_or_else(|| unit_error(UcumError::Overflow))?,
            dimension: proper.dimension,
        };
        Ok(Some(match &definition.function {
            Some((name, _, _)) if definition.special => Meaning::Special {
                function: name.clone(),
                corresponding: proper,
                scale: Ratio::ONE,
            },
            _ => Meaning::Proper(proper),
        }))
    }
}

/// An atom already resolved.
const fn resolved(metric: bool, meaning: Meaning) -> Atom {
    Atom {
        metric,
        meaning: Some(meaning),
    }
}

/// The magnitude a definition's value attribute writes.
fn magnitude(code: &str, text: &str) -> Result<Magnitude, EssenceError> {
    Magnitude::parse(text).ok_or_else(|| EssenceError::Value {
        code: String::from(code),
        text: String::from(text),
    })
}

/// An attribute's value, `None` when the element does not carry it.
fn attribute(start: &BytesStart<'_>, name: &str) -> Result<Option<String>, EssenceError> {
    match start.try_get_attribute(name)? {
        Some(attribute) => Ok(Some(
            attribute
                .normalized_value(XmlVersion::Implicit1_0)?
                .into_owned(),
        )),
        None => Ok(None),
    }
}

/// An attribute the element must carry.
fn required(
    start: &BytesStart<'_>,
    element: &'static str,
    name: &'static str,
) -> Result<String, EssenceError> {
    attribute(start, name)?.ok_or(EssenceError::Missing {
        element,
        attribute: name,
    })
}

/// A `yes`/`no` flag attribute, `no` when absent.
fn flag(start: &BytesStart<'_>, name: &'static str) -> Result<bool, EssenceError> {
    Ok(attribute(start, name)?.as_deref() == Some("yes"))
}

#[cfg(test)]
mod tests {
    use super::{Essence, Meaning};

    #[test]
    fn the_embedded_essence_loads_and_every_atom_resolves() {
        let essence = Essence::load().expect("the essence loads");
        assert_eq!(essence.prefixes.len(), 24);
        assert_eq!(essence.atoms.len(), 312);
        assert!(essence.atoms.values().all(|atom| atom.meaning.is_some()));
        let special = essence
            .atoms
            .values()
            .filter(|atom| matches!(atom.meaning, Some(Meaning::Special { .. })))
            .count();
        assert_eq!(special, 21);
    }

    #[test]
    fn a_unit_naming_an_unknown_atom_is_refused() {
        let text = r#"<root><unit Code="x" isMetric="no"><value Unit="nope" value="1">1</value></unit></root>"#;
        assert!(Essence::parse(text).is_err());
    }
}
