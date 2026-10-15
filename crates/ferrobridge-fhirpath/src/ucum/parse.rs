// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The syntax of a UCUM unit expression, the Backus-Naur form of UCUM
//! Exhibit 1 (§7 to §10) with the lexical rules of §3, §5, §6 and §8.

use super::UcumError;

/// A parsed `<main-term>`: an optional leading solidus and a term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MainTerm {
    /// Whether a leading `/` inverts the term (UCUM §7.3).
    pub(super) inverted: bool,
    /// The term.
    pub(super) term: Term,
}

/// A `<term>`: components joined left to right by `.` and `/` (UCUM §7.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Term {
    /// The first component.
    pub(super) first: Component,
    /// Every further component with the operator before it.
    pub(super) rest: Vec<(Operator, Component)>,
}

/// The multiplication `.` or the division `/` of UCUM §7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Operator {
    /// `.`
    Multiply,
    /// `/`
    Divide,
}

/// A `<component>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Component {
    /// A simple unit symbol, prefix and atom not yet told apart, with its
    /// exponent (UCUM §9) and, when the symbol ends in digits, the digits
    /// read as the exponent.
    Unit {
        /// The symbol, the trailing exponent digits excluded.
        symbol: String,
        /// The whole symbol run when its trailing digits were read as the
        /// exponent, the reading UCUM §8.2 gives when the stem is no unit.
        whole: Option<String>,
        /// The exponent, 1 when none is written.
        exponent: i32,
    },
    /// A positive integer number in place of a unit (UCUM §8).
    Factor(String),
    /// An annotation with no symbol before it, the unity (UCUM §6.4).
    Annotation,
    /// A parenthesized term (UCUM §10).
    Group(Box<Term>),
}

/// Parses a unit expression.
pub(super) fn main_term(code: &str) -> Result<MainTerm, UcumError> {
    let mut parser = Parser { code, at: 0 };
    let inverted = parser.eat(b'/');
    let term = parser.term()?;
    match parser.peek() {
        None => Ok(MainTerm { inverted, term }),
        Some(b')') => Err(parser.error("a closing parenthesis has no opening one")),
        Some(_) => Err(parser.error("an operator is missing")),
    }
}

struct Parser<'a> {
    code: &'a str,
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.code.as_bytes().get(self.at).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        if found {
            self.at += 1;
        }
        found
    }

    fn error(&self, reason: &'static str) -> UcumError {
        UcumError::Syntax {
            code: String::from(self.code),
            position: self.at,
            reason,
        }
    }

    fn term(&mut self) -> Result<Term, UcumError> {
        let first = self.component()?;
        let mut rest = Vec::new();
        loop {
            let operator = match self.peek() {
                Some(b'.') => Operator::Multiply,
                Some(b'/') => Operator::Divide,
                _ => return Ok(Term { first, rest }),
            };
            self.at += 1;
            rest.push((operator, self.component()?));
        }
    }

    fn component(&mut self) -> Result<Component, UcumError> {
        match self.peek() {
            Some(b'(') => {
                self.at += 1;
                let term = self.term()?;
                if !self.eat(b')') {
                    return Err(self.error("a parenthesis is not closed"));
                }
                Ok(Component::Group(Box::new(term)))
            }
            Some(b'{') => {
                self.annotation()?;
                Ok(Component::Annotation)
            }
            Some(byte) if is_symbol_byte(byte) || byte == b'[' => self.simple_unit(),
            _ => Err(self.error("a unit is missing")),
        }
    }

    /// Reads `<annotatable>` with its optional annotation, or a factor.
    fn simple_unit(&mut self) -> Result<Component, UcumError> {
        let start = self.at;
        while let Some(byte) = self.peek() {
            if byte == b'[' {
                self.bracket()?;
            } else if byte == b']' {
                return Err(self.error("a square bracket is not opened"));
            } else if is_symbol_byte(byte) {
                self.at += 1;
            } else {
                break;
            }
        }
        let run = self
            .code
            .get(start..self.at)
            .ok_or_else(|| self.error("a symbol splits a character"))?;
        if run.bytes().all(|b| b.is_ascii_digit()) {
            // NOTE: UCUM Exhibit 1 gives a <factor> no exponent, so `2+10` is refused although
            // the commentary under §9 reads it as 2 to the power 10.
            return Ok(Component::Factor(String::from(run)));
        }
        let stem = run.trim_end_matches(|c: char| c.is_ascii_digit());
        let unit = if stem.len() < run.len() {
            let digits = run
                .get(stem.len()..)
                .ok_or_else(|| self.error("a symbol splits a character"))?;
            Component::Unit {
                symbol: String::from(stem),
                whole: Some(String::from(run)),
                exponent: exponent(digits)
                    .ok_or_else(|| self.error("the exponent is too large"))?,
            }
        } else {
            let exponent = self.signed_exponent()?;
            Component::Unit {
                symbol: String::from(run),
                whole: None,
                exponent,
            }
        };
        if self.peek() == Some(b'{') {
            self.annotation()?;
        }
        Ok(unit)
    }

    /// Reads `<sign><digits>` after a symbol, 1 when no sign follows.
    fn signed_exponent(&mut self) -> Result<i32, UcumError> {
        let negative = match self.peek() {
            Some(b'+') => false,
            Some(b'-') => true,
            _ => return Ok(1),
        };
        self.at += 1;
        let start = self.at;
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
        let digits = self
            .code
            .get(start..self.at)
            .ok_or_else(|| self.error("a symbol splits a character"))?;
        if digits.is_empty() {
            return Err(self.error("a sign is not followed by an exponent"));
        }
        let value = exponent(digits).ok_or_else(|| self.error("the exponent is too large"))?;
        Ok(if negative { -value } else { value })
    }

    /// Skips a square-bracketed part of a symbol (UCUM §5).
    fn bracket(&mut self) -> Result<(), UcumError> {
        self.at += 1;
        loop {
            match self.peek() {
                None => return Err(self.error("a square bracket is not closed")),
                Some(b']') => {
                    self.at += 1;
                    return Ok(());
                }
                Some(b'[') => return Err(self.error("square brackets are nested")),
                Some(byte) if (33..=126).contains(&byte) => self.at += 1,
                Some(_) => return Err(self.error("a character is outside the UCUM set")),
            }
        }
    }

    /// Skips an annotation (UCUM §6).
    fn annotation(&mut self) -> Result<(), UcumError> {
        self.at += 1;
        loop {
            match self.peek() {
                None => return Err(self.error("a curly brace is not closed")),
                Some(b'}') => {
                    self.at += 1;
                    return Ok(());
                }
                Some(b'{') => return Err(self.error("curly braces are nested")),
                Some(byte) if (33..=126).contains(&byte) => self.at += 1,
                Some(_) => return Err(self.error("a character is outside the UCUM set")),
            }
        }
    }
}

/// The value of a digit string, `None` when it exceeds `i32`.
fn exponent(digits: &str) -> Option<i32> {
    digits.bytes().try_fold(0_i32, |value, digit| {
        value
            .checked_mul(10)?
            .checked_add(i32::from(digit.checked_sub(b'0')?))
    })
}

/// Whether a byte may be part of a unit symbol outside square brackets: the
/// ASCII range 33 to 126 without the characters UCUM §3.2 reserves.
fn is_symbol_byte(byte: u8) -> bool {
    (33..=126).contains(&byte) && !b"\"()+-./=[]{}".contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::{Component, MainTerm, Operator, Term, main_term};
    use crate::ucum::UcumError;

    fn unit(symbol: &str, exponent: i32) -> Component {
        Component::Unit {
            symbol: String::from(symbol),
            whole: None,
            exponent,
        }
    }

    fn single(component: Component) -> MainTerm {
        MainTerm {
            inverted: false,
            term: Term {
                first: component,
                rest: Vec::new(),
            },
        }
    }

    #[test]
    fn a_symbol_with_trailing_digits_reads_them_as_its_exponent() {
        assert_eq!(
            main_term("cm2"),
            Ok(single(Component::Unit {
                symbol: String::from("cm"),
                whole: Some(String::from("cm2")),
                exponent: 2,
            }))
        );
        assert_eq!(main_term("s-1"), Ok(single(unit("s", -1))));
        assert_eq!(main_term("m+3"), Ok(single(unit("m", 3))));
        assert_eq!(main_term("10*-3"), Ok(single(unit("10*", -3))));
    }

    #[test]
    fn digits_alone_are_a_factor_and_digits_with_a_letter_a_symbol() {
        assert_eq!(
            main_term("1000"),
            Ok(single(Component::Factor(String::from("1000"))))
        );
        assert_eq!(main_term("12a"), Ok(single(unit("12a", 1))));
    }

    #[test]
    fn operators_chain_left_to_right_and_a_leading_solidus_inverts() {
        let parsed = main_term("/kg.m/s2").expect("parses");
        assert!(parsed.inverted);
        assert_eq!(parsed.term.first, unit("kg", 1));
        assert_eq!(parsed.term.rest.len(), 2);
        assert_eq!(
            parsed.term.rest.first().map(|(op, _)| *op),
            Some(Operator::Multiply)
        );
        assert_eq!(
            parsed.term.rest.get(1).map(|(op, _)| *op),
            Some(Operator::Divide)
        );
    }

    #[test]
    fn brackets_parentheses_and_annotations_read() {
        assert_eq!(main_term("[in_i]"), Ok(single(unit("[in_i]", 1))));
        assert_eq!(main_term("m[H2O]"), Ok(single(unit("m[H2O]", 1))));
        assert_eq!(
            main_term("[m/s2/Hz^(1/2)]"),
            Ok(single(unit("[m/s2/Hz^(1/2)]", 1)))
        );
        assert_eq!(main_term("{rbc}"), Ok(single(Component::Annotation)));
        assert_eq!(main_term("mg{total}"), Ok(single(unit("mg", 1))));
        let group = main_term("mmol/(24.h)").expect("parses");
        assert!(matches!(
            group.term.rest.first(),
            Some((Operator::Divide, Component::Group(_)))
        ));
    }

    #[test]
    fn malformed_expressions_are_refused() {
        for code in [
            "", "m.", ".m", "m..s", "(m", "m)", "[in_i", "in_i]", "[[a]]", "{a", "{a{b}}", "m s",
            "m-", "kg/", "m2{a}{b}", "(m)2", "2+10", "m\u{e9}",
        ] {
            assert!(
                matches!(main_term(code), Err(UcumError::Syntax { .. })),
                "{code:?} should be refused"
            );
        }
    }
}
