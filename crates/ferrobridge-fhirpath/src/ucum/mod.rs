// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Unit conversion by The Unified Code for Units of Measure
//! (<https://ucum.org/ucum>), version 2.2, over its vendored essence.
//!
//! [`Canonical::parse`] reads a case-sensitive unit expression by the syntax
//! of UCUM §7 to §10 and resolves it to its canonical form (§20): a
//! magnitude and the exponents of the seven base units. Annotations carry no
//! meaning (§6.2). Commensurable proper units convert by the ratio of their
//! magnitudes; the special units degree Celsius, degree Fahrenheit and
//! degree Réaumur convert by the functions of §30.4 and §43.4 and §43.5,
//! and every other special unit is refused with
//! [`UcumError::UnsupportedSpecial`]. An arbitrary unit converts to nothing
//! (§24, §25).

pub mod algebra;
mod essence;
mod magnitude;
mod parse;

use std::cmp::Ordering;
use std::sync::LazyLock;

use rust_decimal::Decimal;

use essence::Essence;
use magnitude::{Magnitude, Ratio};
use parse::{Component, MainTerm, Operator, Term};

/// The number of base units (UCUM §28).
const DIMENSIONS: usize = 7;

/// Why a unit expression does not resolve or two units do not convert.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UcumError {
    /// The expression breaks the syntax of UCUM §3 to §10.
    #[error("'{code}' is no UCUM expression: {reason} at byte {position}")]
    Syntax {
        /// The expression.
        code: String,
        /// The byte offset of the defect.
        position: usize,
        /// What is wrong there.
        reason: &'static str,
    },
    /// A symbol is no unit atom, or no prefix and metric atom (UCUM §4).
    #[error("'{symbol}' is no UCUM unit atom")]
    UnknownAtom {
        /// The symbol.
        symbol: String,
    },
    /// A special unit takes part in an operation or a power (UCUM §22.1).
    #[error("the special unit of function '{function}' takes part in an operation")]
    SpecialInTerm {
        /// The function name the essence gives the special unit.
        function: String,
    },
    /// A special unit's conversion function is one this crate does not
    /// carry.
    #[error("the special unit function '{function}' is not supported")]
    UnsupportedSpecial {
        /// The function name the essence gives.
        function: String,
    },
    /// An arbitrary unit converts to no other unit (UCUM §24.3, §25).
    #[error("an arbitrary unit converts to no other unit")]
    Arbitrary,
    /// The units are of different dimensions (UCUM §19).
    #[error("the units are not commensurable")]
    Incommensurable,
    /// The magnitude or the converted value leaves the decimal range.
    #[error("the conversion leaves the decimal range")]
    Overflow,
}

/// A proper unit (UCUM §20.2): its magnitude regarding the base units and
/// the exponent of each base unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Proper {
    ratio: Ratio,
    dimension: [i32; DIMENSIONS],
}

impl Proper {
    /// The unity, a pure number.
    const UNITY: Self = Self {
        ratio: Ratio::ONE,
        dimension: [0; DIMENSIONS],
    };

    fn combine(self, other: Self, operator: Operator) -> Result<Self, UcumError> {
        let mut dimension = self.dimension;
        for (slot, exponent) in dimension.iter_mut().zip(other.dimension) {
            *slot = match operator {
                Operator::Multiply => slot.checked_add(exponent),
                Operator::Divide => slot.checked_sub(exponent),
            }
            .ok_or(UcumError::Overflow)?;
        }
        let ratio = match operator {
            Operator::Multiply => self.ratio.mul(other.ratio),
            Operator::Divide => self.ratio.div(other.ratio),
        }
        .ok_or(UcumError::Overflow)?;
        Ok(Self { ratio, dimension })
    }

    fn pow(self, power: i32) -> Result<Self, UcumError> {
        let mut dimension = self.dimension;
        for slot in &mut dimension {
            *slot = slot.checked_mul(power).ok_or(UcumError::Overflow)?;
        }
        let ratio = self.ratio.pow(power).ok_or(UcumError::Overflow)?;
        Ok(Self { ratio, dimension })
    }
}

/// What an expression means.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Meaning {
    /// A proper unit.
    Proper(Proper),
    /// A special unit (UCUM §21, §22.4): its function, its corresponding
    /// proper unit and the scale factor a prefix gives it.
    Special {
        function: String,
        corresponding: Proper,
        scale: Ratio,
    },
    /// An arbitrary unit, or a term involving one (UCUM §25).
    Arbitrary,
}

/// The canonical form of a UCUM unit expression (UCUM §20).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canonical(Meaning);

/// The essence, resolved once.
static ESSENCE: LazyLock<Essence> = LazyLock::new(load);

#[expect(
    clippy::expect_used,
    reason = "the essence is embedded at build time and the essence test proves it loads"
)]
fn load() -> Essence {
    Essence::load().expect("the embedded UCUM essence should load")
}

impl Canonical {
    /// Resolves a case-sensitive UCUM unit expression.
    ///
    /// # Errors
    ///
    /// Returns [`UcumError::Syntax`] for an expression UCUM's syntax
    /// refuses, [`UcumError::UnknownAtom`] for a symbol that is no unit,
    /// [`UcumError::SpecialInTerm`] for an operation on a special unit and
    /// [`UcumError::Overflow`] for a magnitude out of range.
    pub fn parse(code: &str) -> Result<Self, UcumError> {
        let term = parse::main_term(code)?;
        evaluate(&term, &ESSENCE)?
            .map(Self)
            .ok_or_else(|| UcumError::UnknownAtom {
                symbol: String::from(code),
            })
    }

    /// Whether the unit is a special unit, one on a scale other than a ratio
    /// scale (UCUM §21).
    #[must_use]
    pub const fn is_special(&self) -> bool {
        matches!(self.0, Meaning::Special { .. })
    }

    /// Whether the two units are of one dimension and so compare (UCUM
    /// §17, §21.3).
    #[must_use]
    pub fn is_commensurable(&self, other: &Self) -> bool {
        matches!((self.step(), other.step()), (Ok(a), Ok(b)) if a.dimension == b.dimension)
    }

    /// The value, in `self`, expressed in `to`.
    ///
    /// # Errors
    ///
    /// Returns [`UcumError::Incommensurable`] for units of different
    /// dimensions, [`UcumError::Arbitrary`] for an arbitrary unit,
    /// [`UcumError::UnsupportedSpecial`] for a special unit without a
    /// supported function and [`UcumError::Overflow`] for a result out of
    /// the decimal range.
    pub fn convert(&self, value: Decimal, to: &Self) -> Result<Decimal, UcumError> {
        self.commensurable(to)?;
        if let (Meaning::Proper(from), Meaning::Proper(to)) = (&self.0, &to.0) {
            let ratio = from.ratio.div(to.ratio).ok_or(UcumError::Overflow)?;
            return ratio.apply(value).ok_or(UcumError::Overflow);
        }
        to.value_of_base(self.base_value(value)?)
    }

    /// The size of one `self` in `to`: for a special unit, the size of one
    /// degree.
    ///
    /// # Errors
    ///
    /// As [`Canonical::convert`].
    pub fn size_in(&self, to: &Self) -> Result<Decimal, UcumError> {
        self.commensurable(to)?;
        let ratio = self.step()?.ratio.div(to.step()?.ratio);
        ratio
            .and_then(|ratio| ratio.apply(Decimal::ONE))
            .ok_or(UcumError::Overflow)
    }

    /// Orders two commensurable units by size, the smaller being the more
    /// granular one N1 §6.1.1 compares in.
    ///
    /// # Errors
    ///
    /// As [`Canonical::convert`].
    pub fn compare_size(&self, other: &Self) -> Result<Ordering, UcumError> {
        self.commensurable(other)?;
        self.step()?
            .ratio
            .compare(other.step()?.ratio)
            .ok_or(UcumError::Overflow)
    }

    /// The proper unit one step of `self` is.
    fn step(&self) -> Result<Proper, UcumError> {
        match &self.0 {
            Meaning::Proper(proper) => Ok(*proper),
            Meaning::Special {
                corresponding,
                scale,
                ..
            } => Ok(Proper {
                ratio: corresponding.ratio.mul(*scale).ok_or(UcumError::Overflow)?,
                dimension: corresponding.dimension,
            }),
            Meaning::Arbitrary => Err(UcumError::Arbitrary),
        }
    }

    fn commensurable(&self, other: &Self) -> Result<(), UcumError> {
        if self.step()?.dimension == other.step()?.dimension {
            Ok(())
        } else {
            Err(UcumError::Incommensurable)
        }
    }

    /// The value in the base units of its dimension.
    fn base_value(&self, value: Decimal) -> Result<Decimal, UcumError> {
        match &self.0 {
            Meaning::Proper(proper) => proper.ratio.apply(value).ok_or(UcumError::Overflow),
            Meaning::Special {
                function,
                corresponding,
                scale,
            } => {
                let offset = offset(function)?;
                // UCUM §22.4: m = f⁻¹(α·r) × u.
                scale
                    .apply(value)
                    .and_then(|scaled| scaled.checked_add(offset))
                    .and_then(|degrees| corresponding.ratio.apply(degrees))
                    .ok_or(UcumError::Overflow)
            }
            Meaning::Arbitrary => Err(UcumError::Arbitrary),
        }
    }

    /// The value in `self` of a value in the base units of its dimension.
    fn value_of_base(&self, base: Decimal) -> Result<Decimal, UcumError> {
        match &self.0 {
            Meaning::Proper(proper) => proper
                .ratio
                .inverse()
                .apply(base)
                .ok_or(UcumError::Overflow),
            Meaning::Special {
                function,
                corresponding,
                scale,
            } => {
                let offset = offset(function)?;
                // UCUM §22.4: r = f(m/u) / α.
                corresponding
                    .ratio
                    .inverse()
                    .apply(base)
                    .and_then(|degrees| degrees.checked_sub(offset))
                    .and_then(|scaled| scale.inverse().apply(scaled))
                    .ok_or(UcumError::Overflow)
            }
            Meaning::Arbitrary => Err(UcumError::Arbitrary),
        }
    }
}

/// The offset of a temperature scale, in its own degrees, from absolute
/// zero: `f⁻¹(x) = u·(x + offset)` for the function pairs of UCUM §30.4
/// (`Cel`, 273.15), §43.4 (`degF`, 459.67) and §43.5 (`degRe`, 218.52).
fn offset(function: &str) -> Result<Decimal, UcumError> {
    match function {
        "Cel" => Ok(Decimal::new(27_315, 2)),
        "degF" => Ok(Decimal::new(45_967, 2)),
        "degRe" => Ok(Decimal::new(21_852, 2)),
        _ => Err(UcumError::UnsupportedSpecial {
            function: String::from(function),
        }),
    }
}

/// The meaning of a parsed expression against the essence's tables, `None`
/// while it names an atom the essence has not resolved yet.
fn evaluate(term: &MainTerm, essence: &Essence) -> Result<Option<Meaning>, UcumError> {
    let Some(meaning) = evaluate_term(&term.term, essence)? else {
        return Ok(None);
    };
    if !term.inverted {
        return Ok(Some(meaning));
    }
    combine(Meaning::Proper(Proper::UNITY), Operator::Divide, meaning).map(Some)
}

fn evaluate_term(term: &Term, essence: &Essence) -> Result<Option<Meaning>, UcumError> {
    let Some(mut meaning) = evaluate_component(&term.first, essence)? else {
        return Ok(None);
    };
    for (operator, component) in &term.rest {
        let Some(next) = evaluate_component(component, essence)? else {
            return Ok(None);
        };
        meaning = combine(meaning, *operator, next)?;
    }
    Ok(Some(meaning))
}

/// Two meanings joined by an operator: a special unit takes part in none
/// (UCUM §22.1), and a term with an arbitrary unit is arbitrary (§25).
fn combine(left: Meaning, operator: Operator, right: Meaning) -> Result<Meaning, UcumError> {
    match (left, right) {
        (Meaning::Special { function, .. }, _) | (_, Meaning::Special { function, .. }) => {
            Err(UcumError::SpecialInTerm { function })
        }
        (Meaning::Arbitrary, _) | (_, Meaning::Arbitrary) => Ok(Meaning::Arbitrary),
        (Meaning::Proper(left), Meaning::Proper(right)) => {
            left.combine(right, operator).map(Meaning::Proper)
        }
    }
}

fn evaluate_component(
    component: &Component,
    essence: &Essence,
) -> Result<Option<Meaning>, UcumError> {
    match component {
        Component::Annotation => Ok(Some(Meaning::Proper(Proper::UNITY))),
        Component::Factor(digits) => {
            let magnitude = Magnitude::parse(digits).ok_or(UcumError::Overflow)?;
            Ok(Some(Meaning::Proper(Proper {
                ratio: Ratio::new(magnitude),
                dimension: [0; DIMENSIONS],
            })))
        }
        Component::Group(term) => evaluate_term(term, essence),
        Component::Unit {
            symbol,
            whole,
            exponent,
        } => {
            let found = match (simple_unit(symbol, essence), whole) {
                (Err(UcumError::UnknownAtom { .. }), Some(whole)) => {
                    simple_unit(whole, essence).map(|found| found.map(|f| (f, 1)))
                }
                (found, _) => found.map(|found| found.map(|f| (f, *exponent))),
            }?;
            let Some(((prefix, meaning), exponent)) = found else {
                return Ok(None);
            };
            match meaning {
                Meaning::Proper(proper) => {
                    let prefixed = Proper {
                        ratio: proper.ratio.mul(prefix).ok_or(UcumError::Overflow)?,
                        dimension: proper.dimension,
                    };
                    // UCUM §9.2: the prefix is raised to the power with the atom.
                    prefixed
                        .pow(exponent)
                        .map(|proper| Some(Meaning::Proper(proper)))
                }
                Meaning::Special {
                    function,
                    corresponding,
                    scale,
                } if exponent == 1 => Ok(Some(Meaning::Special {
                    function,
                    corresponding,
                    scale: scale.mul(prefix).ok_or(UcumError::Overflow)?,
                })),
                Meaning::Special { function, .. } => Err(UcumError::SpecialInTerm { function }),
                Meaning::Arbitrary => Ok(Some(Meaning::Arbitrary)),
            }
        }
    }
}

/// The prefix and atom a symbol names: the longest prefix whose remainder is
/// a metric atom, else the symbol as an atom (UCUM §4.4); `None` while the
/// atom is not yet resolved.
fn simple_unit(symbol: &str, essence: &Essence) -> Result<Option<(Ratio, Meaning)>, UcumError> {
    let prefixed = essence
        .prefixes
        .iter()
        .filter_map(|(prefix, value)| {
            let atom = essence.atoms.get(symbol.strip_prefix(prefix.as_str())?)?;
            atom.metric.then_some((prefix.len(), *value, atom))
        })
        .max_by_key(|(length, _, _)| *length);
    let (prefix, atom) = match prefixed {
        Some((_, prefix, atom)) => (prefix, atom),
        None => match essence.atoms.get(symbol) {
            Some(atom) => (Ratio::ONE, atom),
            None => {
                return Err(UcumError::UnknownAtom {
                    symbol: String::from(symbol),
                });
            }
        },
    };
    Ok(atom.meaning.clone().map(|meaning| (prefix, meaning)))
}
