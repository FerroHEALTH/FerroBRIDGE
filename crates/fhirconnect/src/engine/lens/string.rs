// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An openEHR `String` against FHIR `Coding`, `Identifier` and `Reference`.
//!
//! The rows are
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/String.adoc`:
//! a coded value travels as `system::code` and an identifier as
//! `system::value`, the double colon chosen "since it represents a unique
//! combination that rarely appears in text"; text with no double colon is the
//! `code` or the `value` alone. A `Reference` takes the identifier form "as a
//! default ... when transforming from openEHR to FHIR", and coming in a
//! reference with no identifier writes its `reference` string, the page's
//! "Reference contained" row, which runs one way.
//!
//! The page warns that text holding `::` "may cause problems" going back. A
//! `system` that holds `::`, or a code or value that holds it with no system,
//! would read back split differently, so the cell refuses it rather than
//! write text that names another code.

use fhir_types::r4::coding::Coding;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives;
use fhir_types::r4::primitives::Uri;
use fhir_types::r4::reference::Reference;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::model::ast::keyword::Direction;

/// The name the `Coding` cell carries in a diagnostic.
pub const CODING_CELL: &str = "String against Coding";

/// The name the `Identifier` cell carries in a diagnostic.
pub const IDENTIFIER_CELL: &str = "String against Identifier";

/// The name the `Reference` cell carries in a diagnostic.
pub const REFERENCE_CELL: &str = "String against Reference";

/// The separator between a system and a code or value (`String.adoc`).
pub const SEPARATOR: &str = "::";

/// An openEHR `String` against `Coding`, `Identifier` and `Reference`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StringLens;

impl Lens<String, Coding> for StringLens {
    fn cell(&self) -> &'static str {
        CODING_CELL
    }

    fn get(&self, source: &String) -> Result<Coding, LensError> {
        let (system, code) = split(source, CODING_CELL)?;
        Ok(Coding {
            system: system.map(Uri::from),
            code: Some(Code::from(code)),
            ..Coding::default()
        })
    }

    fn put(&self, view: &Coding, _existing: Option<&String>) -> Result<String, LensError> {
        let system = text_of(view.system.as_ref().map(|system| &system.value));
        let code =
            text_of(view.code.as_ref().map(|code| &code.value)).ok_or(LensError::Missing {
                cell: CODING_CELL,
                attribute: "Coding.code",
                direction: Direction::FhirToOpenehr,
            })?;
        joined(system, code, CODING_CELL, "Coding.code")
    }

    fn fallbacks(&self, _view: &Coding) -> Vec<Fallback> {
        Vec::new()
    }
}

impl Lens<String, Identifier> for StringLens {
    fn cell(&self) -> &'static str {
        IDENTIFIER_CELL
    }

    fn get(&self, source: &String) -> Result<Identifier, LensError> {
        identifier(source, IDENTIFIER_CELL)
    }

    fn put(&self, view: &Identifier, _existing: Option<&String>) -> Result<String, LensError> {
        identifier_text(view, IDENTIFIER_CELL)
    }

    fn fallbacks(&self, _view: &Identifier) -> Vec<Fallback> {
        Vec::new()
    }
}

impl Lens<String, Reference> for StringLens {
    fn cell(&self) -> &'static str {
        REFERENCE_CELL
    }

    fn get(&self, source: &String) -> Result<Reference, LensError> {
        Ok(Reference {
            identifier: Some(Box::new(identifier(source, REFERENCE_CELL)?)),
            ..Reference::default()
        })
    }

    fn put(&self, view: &Reference, _existing: Option<&String>) -> Result<String, LensError> {
        if let Some(identifier) = view.identifier.as_deref() {
            return identifier_text(identifier, REFERENCE_CELL);
        }
        text_of(view.reference.as_ref().map(|reference| &reference.value))
            .map(String::from)
            .ok_or(LensError::Missing {
                cell: REFERENCE_CELL,
                attribute: "Reference.identifier",
                direction: Direction::FhirToOpenehr,
            })
    }

    fn fallbacks(&self, view: &Reference) -> Vec<Fallback> {
        if view.identifier.is_none() {
            return vec![Fallback::new(REFERENCE_CELL, "String")];
        }
        Vec::new()
    }
}

/// Splits `text` at its first `::` into a system and a code or value.
fn split<'text>(
    text: &'text str,
    cell: &'static str,
) -> Result<(Option<&'text str>, &'text str), LensError> {
    let (system, rest) = match text.split_once(SEPARATOR) {
        Some((system, rest)) => (Some(system), rest),
        None => (None, text),
    };
    if rest.is_empty() || system.is_some_and(str::is_empty) {
        return Err(LensError::Shape {
            cell,
            attribute: "String",
            found: String::from(text),
            reason: "the text names no system::code pair and no code alone",
            direction: Direction::OpenehrToFhir,
        });
    }
    Ok((system, rest))
}

/// Writes a system and a code or value as the page's `system::code` text.
fn joined(
    system: Option<&str>,
    code: &str,
    cell: &'static str,
    attribute: &'static str,
) -> Result<String, LensError> {
    let ambiguous = match system {
        Some(system) => system.contains(SEPARATOR) || system.ends_with(':'),
        None => code.contains(SEPARATOR),
    };
    if ambiguous {
        return Err(LensError::Shape {
            cell,
            attribute,
            found: String::from(system.unwrap_or(code)),
            reason: "text holding :: reads back as another system and code",
            direction: Direction::FhirToOpenehr,
        });
    }
    Ok(match system {
        Some(system) => format!("{system}{SEPARATOR}{code}"),
        None => String::from(code),
    })
}

/// Reads `text` as an identifier.
fn identifier(text: &str, cell: &'static str) -> Result<Identifier, LensError> {
    let (system, value) = split(text, cell)?;
    Ok(Identifier {
        system: system.map(Uri::from),
        value: Some(primitives::String::from(value)),
        ..Identifier::default()
    })
}

/// Writes an identifier as text.
fn identifier_text(view: &Identifier, cell: &'static str) -> Result<String, LensError> {
    let system = text_of(view.system.as_ref().map(|system| &system.value));
    let value =
        text_of(view.value.as_ref().map(|value| &value.value)).ok_or(LensError::Missing {
            cell,
            attribute: "Identifier.value",
            direction: Direction::FhirToOpenehr,
        })?;
    joined(system, value, cell, "Identifier.value")
}

#[cfg(test)]
mod tests {
    use super::StringLens;
    use crate::engine::lens::Lens;
    use fhir_types::r4::coding::Coding;
    use fhir_types::r4::identifier::Identifier;
    use fhir_types::r4::primitives::Code;
    use fhir_types::r4::primitives;
    use fhir_types::r4::primitives::Uri;
    use fhir_types::r4::reference::Reference;
    use proptest::prop_compose;
    use proptest::proptest;

    prop_compose! {
        /// Text in the page's `system::code` form, or a code alone.
        fn text()(system in proptest::option::of("[a-z]{1,8}:/[a-z]{1,8}"), code in "[A-Za-z0-9-]{1,12}") -> String {
            match system {
                Some(system) => format!("{system}::{code}"),
                None => code,
            }
        }
    }

    proptest! {
        #[test]
        fn coding_get_put(source in text()) {
            let view: Coding = StringLens.get(&source).expect("the text travels");
            let back = StringLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source.clone(), "GetPut failed for {source:?}");
            let again: Coding = StringLens.get(&back).expect("the text travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn identifier_get_put(source in text()) {
            let view: Identifier = StringLens.get(&source).expect("the text travels");
            let back = StringLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source.clone(), "GetPut failed for {source:?}");
            let again: Identifier = StringLens.get(&back).expect("the text travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn reference_get_put(source in text()) {
            let view: Reference = StringLens.get(&source).expect("the text travels");
            let back = StringLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source.clone(), "GetPut failed for {source:?}");
            let again: Reference = StringLens.get(&back).expect("the text travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn a_coding_travels_as_system_and_code() {
        // String.adoc §Coding: `http://loinc.org::LP74849-8`.
        let source = String::from("http://loinc.org::LP74849-8");
        let view: Coding = StringLens.get(&source).expect("the text travels");
        assert_eq!(
            view.system.and_then(|system| system.value).as_deref(),
            Some("http://loinc.org")
        );
        assert_eq!(
            view.code.and_then(|code| code.value).as_deref(),
            Some("LP74849-8")
        );
    }

    #[test]
    fn text_holding_the_separator_ambiguously_is_refused_going_in() {
        let coding = Coding {
            code: Some(Code::from("a::b")),
            ..Coding::default()
        };
        assert!(Lens::<String, Coding>::put(&StringLens, &coding, None).is_err());
        let identifier = Identifier {
            system: Some(Uri::from("urn:x::y")),
            value: Some(primitives::String::from("1")),
            ..Identifier::default()
        };
        assert!(Lens::<String, Identifier>::put(&StringLens, &identifier, None).is_err());
    }

    #[test]
    fn a_reference_with_no_identifier_writes_its_reference_one_way() {
        // String.adoc §Reference contained.
        let view = Reference {
            reference: Some(primitives::String::from("Patient/synthetic-1")),
            ..Reference::default()
        };
        assert_eq!(
            StringLens
                .put(&view, None)
                .expect("the reference is copied"),
            "Patient/synthetic-1"
        );
        assert_eq!(StringLens.fallbacks(&view).len(), 1);
    }

    #[test]
    fn text_with_an_empty_side_is_refused_going_out() {
        for text in ["::code", "system::", ""] {
            assert!(
                Lens::<String, Coding>::get(&StringLens, &String::from(text)).is_err(),
                "{text}"
            );
        }
    }
}
