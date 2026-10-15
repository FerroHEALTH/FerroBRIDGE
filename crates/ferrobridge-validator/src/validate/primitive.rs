// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The primitive checks: the JSON type, the lexical form, the integer ranges
//! and the lengths.

use fhir_types::codec::Value;
use fhir_types::schema::ValueKind;
use regex::bytes::Regex;
use regex::bytes::RegexBuilder;

use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::validate::ValidateError;
use crate::validate::structure::Structure;
use crate::validate::walk::Walk;

/// The extension that carries a primitive's lexical form.
const REGEX: &str = "http://hl7.org/fhir/StructureDefinition/regex";

/// What the core definition of a primitive type says about its value.
#[derive(Debug)]
pub(crate) struct PrimitiveRule {
    lexical: Option<Regex>,
    max_length: Option<usize>,
}

impl PrimitiveRule {
    /// Reads the rule from the `<code>.value` element of the primitive's
    /// core definition: the `regex` extension on its type, and its
    /// `maxLength`.
    ///
    /// The regexes are informative (datatypes.html §2.24.0.1, "Issues with
    /// the specified regexes"), and the package form is the one the version's
    /// own tooling reads, so the validator reads it rather than a copy.
    pub(crate) fn read(structure: &Structure, code: &str) -> Result<Self, ValidateError> {
        let Some(element) = structure.element(&format!("{code}.value")) else {
            return Ok(Self {
                lexical: None,
                max_length: None,
            });
        };
        let pattern = element
            .r#type
            .iter()
            .flat_map(|kind| kind.extension.iter())
            .find(|extension| extension.url == REGEX)
            .and_then(|extension| match &extension.value {
                Some(fhir_types::r4::extension::ExtensionValue::String(text)) => text.value.clone(),
                _ => None,
            });
        // NOTE: the forms are XML Schema patterns (https://www.w3.org/TR/xmlschema-2/#regexs),
        // so they match anchored, over bytes, with Unicode mode off, as fhir-types compiles them.
        let lexical = pattern
            .map(|pattern| {
                RegexBuilder::new(&format!("^(?:{pattern})$"))
                    .unicode(false)
                    .build()
                    .map_err(|source| ValidateError::Regex {
                        type_code: code.to_owned(),
                        source,
                    })
            })
            .transpose()?;
        let max_length = element
            .max_length
            .as_ref()
            .and_then(|length| length.value)
            .and_then(|length| usize::try_from(length).ok());
        Ok(Self {
            lexical,
            max_length,
        })
    }
}

/// The range of an integer type (datatypes.html §2.24.0.1).
fn integer_range(code: &str) -> Option<(i64, i64)> {
    let max = i64::from(i32::MAX);
    match code {
        "integer" => Some((i64::from(i32::MIN), max)),
        "unsignedInt" => Some((0, max)),
        "positiveInt" => Some((1, max)),
        _ => None,
    }
}

impl Walk<'_, '_> {
    /// Checks one primitive value of the type `code`, which JSON carries as
    /// `kind`.
    pub(crate) fn primitive_value(
        &mut self,
        value: &Value,
        code: &str,
        kind: ValueKind,
        location: &str,
    ) -> Result<(), ValidateError> {
        let rule = self.validator().primitive(code)?;
        let text = match (kind, value) {
            (ValueKind::Boolean, Value::Bool(_)) => return Ok(()),
            (ValueKind::Integer | ValueKind::Decimal, Value::Number(number)) => number.as_str(),
            (ValueKind::Text, Value::String(text)) => text.as_str(),
            _ => {
                let expected = match kind {
                    ValueKind::Boolean => "a JSON boolean",
                    ValueKind::Integer | ValueKind::Decimal => "a JSON number",
                    ValueKind::Text => "a JSON string",
                };
                self.out().issue(
                    Severity::Error,
                    IssueType::Structure,
                    MessageId::WrongJsonType,
                    location,
                    format!("a {code} is written as {expected}"),
                );
                return Ok(());
            }
        };
        if text.is_empty() {
            // NOTE: json.html §2.6.2: "String property values can never be empty".
            self.out().issue(
                Severity::Error,
                IssueType::Value,
                MessageId::EmptyString,
                location,
                "the value is an empty string".to_owned(),
            );
            return Ok(());
        }
        if let Some(lexical) = &rule.lexical
            && !lexical.is_match(text.as_bytes())
        {
            self.out().issue(
                Severity::Error,
                IssueType::Value,
                MessageId::LexicalForm,
                location,
                format!("the value is not a valid {code}"),
            );
            return Ok(());
        }
        if kind == ValueKind::Integer
            && let Some((min, max)) = integer_range(code)
            && !text
                .parse::<i64>()
                .is_ok_and(|number| (min..=max).contains(&number))
        {
            self.out().issue(
                Severity::Error,
                IssueType::Value,
                MessageId::IntegerRange,
                location,
                format!("the value is outside the range of {code}, {min} to {max}"),
            );
        }
        if let Some(max_length) = rule.max_length
            && text.chars().count() > max_length
        {
            self.out().issue(
                Severity::Error,
                IssueType::TooLong,
                MessageId::TooLong,
                location,
                format!("the value is longer than the {max_length} characters a {code} allows"),
            );
        }
        Ok(())
    }

    /// Checks a text value against the profile's `maxLength`.
    pub(crate) fn max_length(
        &mut self,
        value: Option<&Value>,
        max_length: Option<i32>,
        element: &str,
        location: &str,
    ) {
        let (Some(Value::String(text)), Some(max_length)) = (value, max_length) else {
            return;
        };
        let Ok(limit) = usize::try_from(max_length) else {
            return;
        };
        if text.chars().count() > limit {
            self.out().issue(
                Severity::Error,
                IssueType::TooLong,
                MessageId::MaxLength,
                location,
                format!("the value is longer than the maxLength of {limit} that {element} sets"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::integer_range;

    #[test]
    fn the_integer_types_carry_the_r4_ranges() {
        assert_eq!(
            integer_range("integer"),
            Some((-2_147_483_648, 2_147_483_647))
        );
        assert_eq!(integer_range("unsignedInt"), Some((0, 2_147_483_647)));
        assert_eq!(integer_range("positiveInt"), Some((1, 2_147_483_647)));
        assert_eq!(integer_range("decimal"), None);
    }
}
