// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `fixed[x]` and `pattern[x]`: the value an element definition sets, matched
//! against an occurrence.

use fhir_types::codec::Value;
use fhir_types::definition::ElementDefinitionRead;

use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::release::Release;
use crate::validate::ValidateError;
use crate::validate::walk::Walk;

/// Which value an element definition sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stem {
    /// `fixed[x]`.
    Fixed,
    /// `pattern[x]`.
    Pattern,
}

/// The JSON value of `element`'s `fixed[x]` or `pattern[x]`; a primitive
/// written only as its `_` sibling has none.
pub(crate) fn value_of<E: ElementDefinitionRead>(
    element: &E,
    stem: Stem,
) -> Result<Option<Value>, fhir_types::codec::EncodeError> {
    let choice = match stem {
        Stem::Fixed => element.fixed()?,
        Stem::Pattern => element.pattern()?,
    };
    Ok(choice.and_then(|choice| choice.value))
}

/// Answers whether `instance` is exactly `fixed`.
///
/// ElementDefinition.fixed\[x\]: "all values must be an exact match" and
/// "Missing elements/attributes must also be missing".
pub(crate) fn equals_fixed(fixed: &Value, instance: &Value) -> bool {
    match (fixed, instance) {
        (Value::Object(fixed), Value::Object(instance)) => {
            // NOTE: elementdefinition-definitions.html#ElementDefinition.fixed_x_ is silent on
            // id and extension; no specification governs this: our own design, an instance
            // id, extension or `_` sibling the fixed value lacks is not a difference.
            let extra = instance
                .keys()
                .any(|key| !fixed.contains_key(key) && !ignorable(key));
            !extra
                && fixed.iter().all(|(key, value)| {
                    instance
                        .get(key)
                        .is_some_and(|held| equals_fixed(value, held))
                })
        }
        (Value::Array(fixed), Value::Array(instance)) => {
            fixed.len() == instance.len()
                && fixed
                    .iter()
                    .zip(instance)
                    .all(|(fixed, held)| equals_fixed(fixed, held))
        }
        _ => fixed == instance,
    }
}

/// Answers whether an instance member the fixed value lacks is no difference:
/// an element id, extensions, or the `_` sibling of a primitive.
fn ignorable(key: &str) -> bool {
    key == "id" || key == "extension" || key.starts_with('_')
}

/// Answers whether `instance` contains `pattern`.
///
/// ElementDefinition.pattern\[x\]: a primitive matches exactly, each property
/// of a complex pattern is present and matches recursively, and each item of
/// a pattern array matches at least one item of the instance array.
pub(crate) fn contains_pattern(pattern: &Value, instance: &Value) -> bool {
    match (pattern, instance) {
        (Value::Object(pattern), Value::Object(instance)) => pattern.iter().all(|(key, value)| {
            instance
                .get(key)
                .is_some_and(|held| contains_pattern(value, held))
        }),
        (Value::Array(pattern), Value::Array(instance)) => pattern
            .iter()
            .all(|wanted| instance.iter().any(|held| contains_pattern(wanted, held))),
        _ => pattern == instance,
    }
}

impl<R: Release> Walk<'_, '_, '_, R> {
    /// Matches one occurrence against `element`'s `fixed[x]` and
    /// `pattern[x]`.
    ///
    /// A primitive occurrence is matched by its value alone; an occurrence
    /// with only a `_` sibling has no value and matches neither.
    pub(crate) fn fixed_and_pattern(
        &mut self,
        element: &R::ElementDefinition,
        structure: &str,
        id: &str,
        value: Option<&Value>,
        location: &str,
    ) -> Result<(), ValidateError> {
        let encode = |source| ValidateError::Encode {
            structure: structure.to_owned(),
            element: id.to_owned(),
            source,
        };
        if let Some(fixed) = value_of(element, Stem::Fixed).map_err(encode)?
            && !value.is_some_and(|held| equals_fixed(&fixed, held))
        {
            let shown = serde_json::to_string(&fixed).unwrap_or_default();
            self.out().issue(
                Severity::Error,
                IssueType::Value,
                MessageId::FixedMismatch,
                location,
                format!("the value is not the fixed value {shown} that {id} sets"),
            );
        }
        if let Some(pattern) = value_of(element, Stem::Pattern).map_err(encode)?
            && !value.is_some_and(|held| contains_pattern(&pattern, held))
        {
            let shown = serde_json::to_string(&pattern).unwrap_or_default();
            self.out().issue(
                Severity::Error,
                IssueType::Value,
                MessageId::PatternMismatch,
                location,
                format!("the value does not contain the pattern {shown} that {id} sets"),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use fhir_types::codec::Value;

    use super::contains_pattern;
    use super::equals_fixed;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).expect("test JSON")
    }

    #[test]
    fn a_pattern_array_item_matches_any_instance_item() {
        let pattern = json(r#"{"coding":[{"system":"http://loinc.org","code":"1-8"}]}"#);
        let held = json(
            r#"{"coding":[{"system":"urn:x","code":"a"},{"system":"http://loinc.org","code":"1-8","display":"d"}],"text":"t"}"#,
        );
        assert!(
            contains_pattern(&pattern, &held),
            "the second coding holds the pattern"
        );
        let missing = json(r#"{"coding":[{"system":"http://loinc.org","code":"2-6"}]}"#);
        assert!(
            !contains_pattern(&pattern, &missing),
            "no coding holds the pattern"
        );
    }

    #[test]
    fn a_fixed_value_refuses_an_extra_property_and_ignores_an_id() {
        let fixed = json(r#"{"system":"urn:x","code":"a"}"#);
        assert!(
            equals_fixed(&fixed, &json(r#"{"id":"c1","system":"urn:x","code":"a"}"#)),
            "an id is no difference"
        );
        assert!(
            !equals_fixed(
                &fixed,
                &json(r#"{"system":"urn:x","code":"a","display":"d"}"#)
            ),
            "a display the fixed value lacks is a difference"
        );
    }
}
