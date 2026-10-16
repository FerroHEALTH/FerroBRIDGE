// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The unit of a product or a quotient of two quantities, as one UCUM unit.
//!
//! FHIRPath N1 §6.6.1 gives `12 'cm' * 3 'cm'` the unit `'cm2'` and §6.6.2
//! gives `12 'cm2' / 3 'cm'` the unit `'cm'`.
//!
//! Both units are read by the syntax of UCUM §7 to §10 and flattened into
//! their simple units with signed exponents (§7.4 reads `.` and `/` left to
//! right, §10 a parenthesized term as one component). Identical symbols
//! merge their exponents, a prefix staying part of its symbol, so `cm` and
//! `m` remain two units. The result writes the units of positive exponent
//! joined by `.`, then each unit of negative exponent after a `/`, and the
//! unity `1` when every exponent cancels.

use super::parse::{self, Component, Operator, Term};
use super::{ESSENCE, UcumError, simple_unit};

/// The UCUM unit of the product of a quantity in `left` and one in `right`.
///
/// `None` when the two do not flatten into simple units this module can
/// write back: a unit UCUM's syntax refuses, an annotation, or a number
/// factor raised to a power. The caller then composes the two expressions.
#[must_use]
pub fn product(left: &str, right: &str) -> Option<String> {
    combine(left, right, Operator::Multiply)
}

/// The UCUM unit of the quotient of a quantity in `left` by one in `right`.
///
/// `None` as for [`product`].
#[must_use]
pub fn quotient(left: &str, right: &str) -> Option<String> {
    combine(left, right, Operator::Divide)
}

/// A simple unit and its exponent, or a number factor (UCUM §8).
#[derive(Debug)]
struct Power {
    symbol: String,
    factor: bool,
    exponent: i32,
}

fn combine(left: &str, right: &str, operator: Operator) -> Option<String> {
    let mut powers = Vec::new();
    flatten(left, 1, &mut powers)?;
    let sign = match operator {
        Operator::Multiply => 1,
        Operator::Divide => -1,
    };
    flatten(right, sign, &mut powers)?;
    write(&powers)
}

/// Adds the simple units of `code` to `powers`, each exponent times `sign`.
// NOTE: an annotation's text is no part of the parsed term (UCUM §6.2 gives it no meaning), so a
// unit with one is not flattened and the caller keeps it whole.
fn flatten(code: &str, sign: i32, powers: &mut Vec<Power>) -> Option<()> {
    if code.contains('{') {
        return None;
    }
    let main = parse::main_term(code).ok()?;
    let sign = if main.inverted {
        sign.checked_neg()?
    } else {
        sign
    };
    term(&main.term, sign, powers)
}

fn term(term: &Term, sign: i32, powers: &mut Vec<Power>) -> Option<()> {
    component(&term.first, sign, powers)?;
    for (operator, next) in &term.rest {
        let next_sign = match operator {
            Operator::Multiply => sign,
            Operator::Divide => sign.checked_neg()?,
        };
        component(next, next_sign, powers)?;
    }
    Some(())
}

fn component(component: &Component, sign: i32, powers: &mut Vec<Power>) -> Option<()> {
    match component {
        Component::Unit {
            symbol,
            whole,
            exponent,
        } => {
            // UCUM §8.2: trailing digits are the exponent unless the stem is no unit.
            let (symbol, exponent) = match (simple_unit(symbol, &ESSENCE), whole) {
                (Err(UcumError::UnknownAtom { .. }), Some(whole)) => (whole, 1),
                _ => (symbol, *exponent),
            };
            merge(powers, symbol, false, exponent.checked_mul(sign)?)
        }
        Component::Factor(digits) if digits == "1" => Some(()),
        Component::Factor(digits) => merge(powers, digits, true, sign),
        Component::Group(group) => term(group, sign, powers),
        Component::Annotation => None,
    }
}

fn merge(powers: &mut Vec<Power>, symbol: &str, factor: bool, exponent: i32) -> Option<()> {
    match powers
        .iter_mut()
        .find(|power| power.symbol == symbol && power.factor == factor)
    {
        Some(power) => power.exponent = power.exponent.checked_add(exponent)?,
        None => powers.push(Power {
            symbol: String::from(symbol),
            factor,
            exponent,
        }),
    }
    Some(())
}

/// Writes the powers as one UCUM expression.
fn write(powers: &[Power]) -> Option<String> {
    let mut numerator = Vec::new();
    let mut denominator = Vec::new();
    for power in powers {
        match power.exponent {
            0 => {}
            exponent if exponent > 0 => numerator.push(written(power, exponent)?),
            exponent => denominator.push(written(power, exponent.checked_neg()?)?),
        }
    }
    if numerator.is_empty() && denominator.is_empty() {
        return Some(String::from("1"));
    }
    let mut unit = numerator.join(".");
    for part in denominator {
        unit.push('/');
        unit.push_str(&part);
    }
    Some(unit)
}

/// One power with a positive exponent: the symbol, then the exponent unless
/// it is 1; `None` for a factor or a symbol ending in a digit raised to a
/// power, which UCUM §8 and §9 give no written form.
fn written(power: &Power, exponent: i32) -> Option<String> {
    if exponent == 1 {
        return Some(power.symbol.clone());
    }
    let ends_in_digit = power
        .symbol
        .chars()
        .next_back()
        .is_some_and(|last| last.is_ascii_digit());
    (!power.factor && !ends_in_digit).then(|| format!("{}{exponent}", power.symbol))
}

#[cfg(test)]
mod tests {
    use super::{product, quotient};

    #[test]
    fn identical_symbols_merge_their_exponents() {
        assert_eq!(product("cm", "cm").as_deref(), Some("cm2"));
        assert_eq!(product("cm", "cm2").as_deref(), Some("cm3"));
        assert_eq!(quotient("cm2", "cm").as_deref(), Some("cm"));
        assert_eq!(product("10*3", "10*3").as_deref(), Some("10*6"));
    }

    #[test]
    fn a_prefix_keeps_its_symbol_apart() {
        assert_eq!(product("cm", "m").as_deref(), Some("cm.m"));
        assert_eq!(quotient("cm", "m").as_deref(), Some("cm/m"));
    }

    #[test]
    fn negative_exponents_follow_a_solidus() {
        assert_eq!(quotient("g", "m").as_deref(), Some("g/m"));
        assert_eq!(quotient("kg", "m.s2").as_deref(), Some("kg/m/s2"));
        assert_eq!(product("kg.m/s2", "s").as_deref(), Some("kg.m/s"));
        assert_eq!(quotient("1", "min").as_deref(), Some("/min"));
        assert_eq!(product("/s", "m").as_deref(), Some("m/s"));
    }

    #[test]
    fn units_that_cancel_are_the_unity() {
        assert_eq!(quotient("m", "m").as_deref(), Some("1"));
        assert_eq!(product("/s", "s").as_deref(), Some("1"));
        assert_eq!(quotient("mg/dL", "mg/dL").as_deref(), Some("1"));
    }

    #[test]
    fn what_has_no_written_form_is_left_to_the_caller() {
        assert_eq!(product("mg{total}", "mg"), None);
        assert_eq!(product("(m", "m"), None);
        assert_eq!(product("mmol/(24.h)", "/(24.h)"), None);
    }
}
