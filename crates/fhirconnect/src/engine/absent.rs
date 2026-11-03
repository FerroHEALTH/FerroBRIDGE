// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The elements a run wrote absent: an element whose one member is an
//! `extension` carrying the FHIR R4 data-absent-reason extension.
//!
//! A mapping writes such an element for one a profile requires and the
//! openEHR content does not hold, so the resource conforms and says the value
//! is missing (<https://hl7.org/fhir/R4/extension-data-absent-reason.html>).
//! The run declares each one as [`Loss::Absent`], so the gap stays as visible
//! as every other loss. No specification governs declaring it: our own design.

use fhir_types::codec::Object;
use fhir_types::codec::Value;

use crate::engine::outcome::Location;
use crate::engine::outcome::Loss;
use crate::engine::outcome::Warning;

/// The canonical URL of the FHIR R4 data-absent-reason extension.
pub const DATA_ABSENT_REASON: &str = "http://hl7.org/fhir/StructureDefinition/data-absent-reason";

/// Returns a warning for every element of `resource` written absent, in
/// document order, each located by its FHIRPath.
pub(crate) fn declared(resource: &Value) -> Vec<Warning> {
    let mut found = Vec::new();
    if let Value::Object(object) = resource {
        let root = object
            .get("resourceType")
            .and_then(Value::as_str)
            .unwrap_or("Resource")
            .to_owned();
        walk(object, &root, &mut found);
    }
    found
}

/// Collects the absent elements under `object`, at the FHIRPath `path`.
fn walk(object: &Object, path: &str, found: &mut Vec<Warning>) {
    for (name, member) in object {
        if name == "extension" {
            continue;
        }
        match member {
            Value::Object(child) => visit(child, &format!("{path}.{name}"), found),
            Value::Array(items) => {
                for (position, item) in items.iter().enumerate() {
                    if let Value::Object(child) = item {
                        visit(child, &format!("{path}.{name}[{position}]"), found);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Declares `object` at `path` when it is written absent, and walks into it
/// otherwise.
fn visit(object: &Object, path: &str, found: &mut Vec<Warning>) {
    match absent_reason(object) {
        Some(reason) => found.push(Warning::new(
            Loss::Absent { reason },
            Location::default().with_fhir(path),
        )),
        None => walk(object, path, found),
    }
}

/// Returns the data-absent-reason code of `object` when its one member is an
/// `extension` that carries the data-absent-reason extension.
fn absent_reason(object: &Object) -> Option<String> {
    let mut members = object.iter();
    let (Some((name, Value::Array(extensions))), None) = (members.next(), members.next()) else {
        return None;
    };
    if name != "extension" {
        return None;
    }
    extensions.iter().find_map(|extension| {
        let Value::Object(extension) = extension else {
            return None;
        };
        (extension.get("url").and_then(Value::as_str) == Some(DATA_ABSENT_REASON))
            .then(|| extension.get("valueCode").and_then(Value::as_str))
            .flatten()
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use fhir_types::codec::Value;

    use super::declared;
    use crate::engine::outcome::Loss;

    #[test]
    fn an_element_with_only_the_absent_reason_is_declared_where_it_is() {
        let observation = Value::from_serde_json(serde_json::json!({
            "resourceType": "Observation",
            "status": "final",
            "performer": [
                {"display": "Synthetic Laboratory"},
                {"extension": [{
                    "url": "http://hl7.org/fhir/StructureDefinition/data-absent-reason",
                    "valueCode": "unknown"
                }]}
            ]
        }));
        let found = declared(&observation);
        let [warning] = found.as_slice() else {
            panic!("one absent element: {found:?}");
        };
        assert_eq!(
            warning.loss(),
            &Loss::Absent {
                reason: String::from("unknown")
            }
        );
        assert_eq!(warning.location().fhir(), Some("Observation.performer[1]"));
    }

    #[test]
    fn an_element_with_a_value_beside_the_extension_is_not_absent() {
        let observation = Value::from_serde_json(serde_json::json!({
            "resourceType": "Observation",
            "performer": [{
                "display": "Synthetic Laboratory",
                "extension": [{
                    "url": "http://hl7.org/fhir/StructureDefinition/data-absent-reason",
                    "valueCode": "unknown"
                }]
            }]
        }));
        assert_eq!(declared(&observation), Vec::new());
    }
}
