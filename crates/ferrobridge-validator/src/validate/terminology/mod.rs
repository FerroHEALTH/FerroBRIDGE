// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Value set membership read from the package set
//! (<https://hl7.org/fhir/R4/valueset.html>), and the terminology bindings
//! an element definition declares checked against it
//! (<https://hl7.org/fhir/R4/terminologies.html#strength>).
//!
//! Membership is three-valued: a code is in the value set, out of it, or the
//! package set cannot tell, and the third answer carries the reason. A
//! binding the package set cannot decide is handed to a terminology server
//! through [`crate::terminology`], or listed as unevaluated.

pub(crate) mod binding;
mod compose;
pub(crate) mod index;

use fhir_types::codec::Object;
use fhir_types::codec::Value;

use crate::report::Reason;

/// Whether a code is in a value set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Membership {
    /// The code is in the value set.
    Member,
    /// The code is not in the value set.
    NotMember,
    /// The package set cannot tell, for the reason given.
    Unknown(Reason),
}

impl Membership {
    /// Returns the membership of a union: in when either side is, out when
    /// both are.
    fn or(self, other: Self) -> Self {
        match (self, other) {
            (Self::Member, _) | (_, Self::Member) => Self::Member,
            (Self::NotMember, Self::NotMember) => Self::NotMember,
            (Self::Unknown(reason), _) | (_, Self::Unknown(reason)) => Self::Unknown(reason),
        }
    }

    /// Returns the membership of an intersection: out when either side is,
    /// in when both are.
    fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::NotMember, _) | (_, Self::NotMember) => Self::NotMember,
            (Self::Member, Self::Member) => Self::Member,
            (Self::Unknown(reason), _) | (_, Self::Unknown(reason)) => Self::Unknown(reason),
        }
    }

    /// Returns the complement.
    fn not(self) -> Self {
        match self {
            Self::Member => Self::NotMember,
            Self::NotMember => Self::Member,
            unknown @ Self::Unknown(_) => unknown,
        }
    }
}

/// One code of an instance: a bare code, or the parts of a `Coding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Coding<'a> {
    /// `system`; `None` for a bare code, which any system of the value set
    /// may define.
    pub(crate) system: Option<&'a str>,
    /// `version`.
    pub(crate) version: Option<&'a str>,
    /// `code`.
    pub(crate) code: Option<&'a str>,
    /// `display`.
    pub(crate) display: Option<&'a str>,
    /// The index of the coding in `CodeableConcept.coding`, 0 otherwise.
    pub(crate) position: usize,
}

/// How a coded value is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Form {
    /// A `code`, `string` or `uri`, compared by its code alone
    /// (terminologies.html §4.1.6.1).
    Code,
    /// A `Coding`, or the unit of a `Quantity`.
    Coding,
    /// A `CodeableConcept`: one of its codings SHALL be from the set
    /// (terminologies.html §4.1.5.1).
    CodeableConcept,
}

/// A coded value of an instance, as a binding sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Coded<'a> {
    /// How it is held.
    pub(crate) form: Form,
    /// Its codes: one for a code or a `Coding`, every coding of a
    /// `CodeableConcept`.
    pub(crate) codes: Vec<Coding<'a>>,
    /// `CodeableConcept.text`.
    pub(crate) text: Option<&'a str>,
}

impl<'a> Coded<'a> {
    /// Reads a value by its JSON shape: a string as a bare code, an object
    /// with `coding` or `text` as a `CodeableConcept`, any other object as a
    /// `Coding`.
    pub(crate) fn infer(value: &'a Value) -> Option<Self> {
        match value {
            Value::String(code) => Some(Self::code(code)),
            Value::Object(object) => {
                if object.contains_key("coding") || object.contains_key("text") {
                    Some(Self::concept(object))
                } else {
                    Some(Self::coding(object))
                }
            }
            _ => None,
        }
    }

    /// Returns a bare code.
    pub(crate) fn code(code: &'a str) -> Self {
        Self {
            form: Form::Code,
            codes: vec![Coding {
                system: None,
                version: None,
                code: Some(code),
                display: None,
                position: 0,
            }],
            text: None,
        }
    }

    /// Returns a `Coding` or the unit of a `Quantity`.
    pub(crate) fn coding(object: &'a Object) -> Self {
        Self {
            form: Form::Coding,
            codes: vec![code_of(0, object)],
            text: None,
        }
    }

    /// Returns a `CodeableConcept`.
    pub(crate) fn concept(object: &'a Object) -> Self {
        Self {
            form: Form::CodeableConcept,
            codes: object
                .get("coding")
                .and_then(Value::as_array)
                .unwrap_or_default()
                .iter()
                .enumerate()
                .filter_map(|(position, coding)| Some((position, coding.as_object()?)))
                .map(|(position, coding)| code_of(position, coding))
                .collect(),
            text: object.get("text").and_then(Value::as_str),
        }
    }
}

/// The parts of one `Coding` object.
fn code_of(position: usize, object: &Object) -> Coding<'_> {
    let text = |key: &str| object.get(key).and_then(Value::as_str);
    Coding {
        system: text("system"),
        version: text("version"),
        code: text("code"),
        display: text("display"),
        position,
    }
}
