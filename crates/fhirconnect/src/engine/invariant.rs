// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The base invariants a produced FHIR document is held to before it leaves
//! the engine.
//!
//! An error-severity constraint is "a rule that all resources must conform
//! to" (R4 conformance-rules.html §2.1.0.6), and a mapping can break one the
//! decoder never reads: an `extensionUrl` manual that fires while its
//! `extensionValue` writes nothing leaves an extension with a url and no
//! value. Each check here is one such constraint, written as a structural
//! test over the lexical document and named by its key. A check is added
//! when a mapping can produce its breach; the full constraint set is the
//! profile validator's.

use core::fmt;

use fhir_types::codec::Object;
use fhir_types::codec::Value;

/// An error-severity constraint a produced document breaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breach {
    /// The constraint key, for example `ext-1`.
    key: &'static str,
    /// The instance path of the element that breaks it, for example
    /// `Condition.code.coding[0].extension[0]`.
    location: String,
}

impl Breach {
    /// Returns the constraint key.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        self.key
    }

    /// Returns the instance path of the element that breaks the constraint.
    #[must_use]
    pub fn location(&self) -> &str {
        &self.location
    }
}

impl fmt::Display for Breach {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} breaks {}", self.location, self.key)
    }
}

impl core::error::Error for Breach {}

/// Holds `document`, a resource the engine produced, to the checks this
/// module carries, and returns the first breach in document order.
///
/// `ext-1` (R4 `StructureDefinition-Extension`, "Must have either extensions
/// or value\[x\], not both", `extension.exists() != value.exists()`) holds
/// every object under an `extension` or `modifierExtension` member to exactly
/// one of a `value[x]` member and nested extensions, including the extensions
/// of a primitive's `_` sibling (R4 json.html §2.6.2).
///
/// # Errors
///
/// Returns the [`Breach`] of the first element that breaks a check.
pub fn check(document: &Value) -> Result<(), Breach> {
    let root = document
        .get("resourceType")
        .and_then(Value::as_str)
        .unwrap_or("Resource");
    walk(document, root)
}

/// Walks `value` at the instance path `at`.
fn walk(value: &Value, at: &str) -> Result<(), Breach> {
    match value {
        Value::Object(object) => {
            for (key, member) in object {
                let below = format!("{at}.{key}");
                if key == "extension" || key == "modifierExtension" {
                    extensions(member, &below)?;
                }
                walk(member, &below)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                walk(item, &format!("{at}[{index}]"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Holds each extension of the `extension` member `member` at `at` to `ext-1`.
fn extensions(member: &Value, at: &str) -> Result<(), Breach> {
    let Value::Array(items) = member else {
        return Ok(());
    };
    for (index, item) in items.iter().enumerate() {
        if let Value::Object(extension) = item
            && valued(extension) == extension.contains_key("extension")
        {
            return Err(Breach {
                key: "ext-1",
                location: format!("{at}[{index}]"),
            });
        }
    }
    Ok(())
}

/// Whether `extension` carries a `value[x]` member, `valueString` or any of
/// its kin (R4 json.html §2.6.2.5, choice elements).
fn valued(extension: &Object) -> bool {
    extension.keys().any(|key| {
        key.strip_prefix("value")
            .and_then(|suffix| suffix.chars().next())
            .is_some_and(char::is_uppercase)
    })
}

#[cfg(test)]
mod tests {
    use super::check;
    use fhir_types::codec::Value;

    fn document(json: &serde_json::Value) -> Value {
        Value::from_serde_json(json.clone())
    }

    #[test]
    fn an_extension_with_a_url_and_no_value_breaks_ext_1() {
        let condition = document(&serde_json::json!({
            "resourceType": "Condition",
            "code": {"coding": [{"system": "http://example.org/cs", "code": "c",
                "extension": [{"url": "http://example.org/certainty"}]}]}
        }));
        let breach = check(&condition).expect_err("a url alone breaks ext-1");
        assert_eq!(breach.key(), "ext-1");
        assert_eq!(breach.location(), "Condition.code.coding[0].extension[0]");
    }

    #[test]
    fn an_extension_with_a_value_and_extensions_breaks_ext_1() {
        let patient = document(&serde_json::json!({
            "resourceType": "Patient",
            "extension": [{"url": "http://example.org/e", "valueString": "x",
                "extension": [{"url": "part", "valueCode": "y"}]}]
        }));
        let breach = check(&patient).expect_err("both break ext-1");
        assert_eq!(breach.location(), "Patient.extension[0]");
    }

    #[test]
    fn a_primitive_sibling_extension_and_a_modifier_extension_are_held_too() {
        let patient = document(&serde_json::json!({
            "resourceType": "Patient",
            "_gender": {"extension": [{"url": "http://example.org/e"}]}
        }));
        assert_eq!(
            check(&patient).map_err(|breach| breach.location().to_owned()),
            Err(String::from("Patient._gender.extension[0]"))
        );
        let observation = document(&serde_json::json!({
            "resourceType": "Observation",
            "modifierExtension": [{"url": "http://example.org/m"}]
        }));
        assert!(check(&observation).is_err());
    }

    #[test]
    fn valued_and_nested_extensions_hold_ext_1() {
        let patient = document(&serde_json::json!({
            "resourceType": "Patient",
            "extension": [
                {"url": "http://example.org/a", "valueCodeableConcept": {"text": "t"}},
                {"url": "http://example.org/b", "extension": [{"url": "part", "valueString": "p"}]}
            ],
            "_gender": {"extension": [{"url": "http://example.org/e", "valueBoolean": true}]}
        }));
        assert_eq!(check(&patient), Ok(()));
    }
}
