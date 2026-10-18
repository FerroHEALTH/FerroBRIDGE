// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Naming one element of a document as a FHIRPath in the simple subset.
//!
//! `OperationOutcome.issue.expression` admits "a simple subset of FHIRPath
//! limited to element names, repetition indicators and the default child
//! accessor"
//! (<https://hl7.org/fhir/R4/operationoutcome-definitions.html#OperationOutcome.issue.expression>),
//! so a located element is written with no function: a choice element by the
//! name without its type suffix, and an `extension(url)` step as the
//! `extension` element at the index the url matched in the document.

use fhir_types::codec::Value;

use crate::tree::Occurrence;
use crate::tree::element::Move;
use crate::tree::element::Resolved;

/// Returns the FHIRPath of the element `resolved` names at `occurrence`.
///
/// The path is rooted at the resource type the path was resolved against and
/// carries an index on every repeating element `occurrence` holds an index
/// for, so an occurrence shorter than the path's repeating elements leaves the
/// remaining ones unindexed (the default child accessor, which addresses all
/// of them). The walk stops at a `resolve()`, because the element holding the
/// reference is the one located. `document` is read only where the index of
/// an `extension(url)` entry in the whole `extension` list is needed; where
/// the document does not hold that entry the step is left unindexed. No value
/// of the document enters the path.
///
/// # Examples
///
/// ```
/// use core::str::FromStr;
///
/// use fhir_types::codec::Value;
/// use fhirconnect::tree::Occurrence;
/// use fhirconnect::tree::element::resolve;
/// use fhirconnect::tree::locate::locate;
/// use fhirconnect::tree::path::FhirPath;
///
/// let path = FhirPath::from_str("$resource.code.coding.code")?;
/// let resolved = resolve(&fhir_types::r4::schema::SCHEMAS, "Condition", &path)?;
/// let located = locate(&Value::Null, &resolved, &Occurrence::new([1]));
/// assert_eq!(located, "Condition.code.coding[1].code");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn locate(document: &Value, resolved: &Resolved, occurrence: &Occurrence) -> String {
    let mut path = String::from(resolved.resource());
    let mut indices = occurrence.indices().iter().copied();
    let mut cursor = (document.get("resourceType").and_then(Value::as_str)
        == Some(resolved.resource()))
    .then_some(document);
    for step in resolved.moves() {
        match *step {
            Move::Member(ref field) => {
                // NOTE: R4 fhirpath.html §Polymorphism in FHIR, a choice is named without its
                // `[x]`, so a type filter the resolver applied (`valueQuantity`) renders `value`.
                path.push('.');
                path.push_str(element_name(field.path()));
                let held = cursor.and_then(|value| value.get(field.key()));
                cursor = if field.repeats() {
                    index_into(&mut path, held, indices.next())
                } else {
                    held
                };
            }
            Move::Choice { ref field, schema } => {
                path.push('.');
                path.push_str(element_name(field.path()));
                let held = cursor.and_then(|value| {
                    schema
                        .choices()
                        .find_map(|form| value.get(&format!("{}{}", field.key(), form.suffix)))
                });
                cursor = if field.repeats() {
                    index_into(&mut path, held, indices.next())
                } else {
                    held
                };
            }
            Move::Extension { ref field, ref url } => {
                // NOTE: R4 OperationOutcome.issue.expression admits no function, so
                // `extension(url)` renders as the `extension` entry the url matched, by index.
                path.push('.');
                path.push_str(element_name(field.path()));
                let matched = indices.next().and_then(|position| {
                    cursor
                        .and_then(|value| value.get(field.key()))
                        .and_then(Value::as_array)?
                        .iter()
                        .enumerate()
                        .filter(|(_, entry)| {
                            entry.get("url").and_then(Value::as_str) == Some(url.as_str())
                        })
                        .nth(position)
                });
                cursor = matched.map(|(index, entry)| {
                    push_index(&mut path, index);
                    entry
                });
            }
            Move::Resolve { .. } => break,
            Move::Ordinal(_) | Move::Index(_) | Move::Predicate(_) => {}
        }
    }
    path
}

/// Appends the index of one repeating element, when the occurrence holds one,
/// and returns the entry the document holds there.
fn index_into<'tree>(
    path: &mut String,
    held: Option<&'tree Value>,
    index: Option<usize>,
) -> Option<&'tree Value> {
    let index = index?;
    push_index(path, index);
    held.and_then(Value::as_array)
        .and_then(|items| items.get(index))
}

/// Appends a repetition indicator.
fn push_index(path: &mut String, index: usize) {
    path.push('[');
    path.push_str(&index.to_string());
    path.push(']');
}

/// Returns the name FHIRPath gives the element at `definition`.
///
/// The name is the last segment of the element path the table holds, with a
/// choice element's `[x]` dropped: "choice elements are labeled according to
/// the name without the '\[x\]' suffix" (<https://hl7.org/fhir/R4/fhirpath.html>,
/// §Polymorphism in FHIR). A primitive's extension sibling is named by its
/// element too, since FHIRPath reaches `id` and `extension` through it.
fn element_name(definition: &str) -> &str {
    let last = definition.rsplit('.').next().unwrap_or(definition);
    last.strip_suffix("[x]").unwrap_or(last)
}

#[cfg(test)]
mod tests {
    use core::str::FromStr;

    use fhir_types::codec::Value;

    use super::element_name;
    use super::locate;
    use crate::tree::Occurrence;
    use crate::tree::element::resolve;
    use crate::tree::path::FhirPath;

    /// Locates `expression` over `document` at `indices`.
    fn located(resource: &str, expression: &str, document: &Value, indices: &[usize]) -> String {
        let path = FhirPath::from_str(expression).expect("the expression parses");
        let resolved = resolve(&fhir_types::r4::schema::SCHEMAS, resource, &path)
            .expect("the expression resolves");
        locate(document, &resolved, &Occurrence::new(indices.to_vec()))
    }

    /// Parses a JSON document into the lexical tree.
    fn document(json: &str) -> Value {
        serde_json::from_str(json).expect("the fixture is JSON")
    }

    #[test]
    fn every_repeating_element_takes_its_index() {
        assert_eq!(
            located(
                "Condition",
                "$resource.code.coding.code",
                &Value::Null,
                &[2]
            ),
            "Condition.code.coding[2].code"
        );
    }

    #[test]
    fn an_element_with_no_index_is_left_to_the_default_child_accessor() {
        assert_eq!(
            located("Condition", "$resource.code.coding.code", &Value::Null, &[]),
            "Condition.code.coding.code"
        );
    }

    #[test]
    fn a_choice_is_named_without_its_type_suffix() {
        assert_eq!(
            located(
                "Observation",
                "$resource.value.ofType(Quantity).value",
                &Value::Null,
                &[]
            ),
            "Observation.value.value"
        );
        assert_eq!(
            located("Condition", "$resource.onset", &Value::Null, &[]),
            "Condition.onset"
        );
    }

    #[test]
    fn an_extension_step_takes_the_index_its_url_matched_in_the_document() {
        let patient = document(
            r#"{"resourceType":"Patient","extension":[
                {"url":"http://example.org/a","valueString":"x"},
                {"url":"http://example.org/b","valueString":"y"},
                {"url":"http://example.org/b","valueString":"z"}]}"#,
        );
        assert_eq!(
            located(
                "Patient",
                "$resource.extension('http://example.org/b').value",
                &patient,
                &[1]
            ),
            "Patient.extension[2].value"
        );
        assert_eq!(
            located(
                "Patient",
                "$resource.extension('http://example.org/b').value",
                &Value::Null,
                &[1]
            ),
            "Patient.extension.value",
            "an entry the document does not hold is left unindexed"
        );
    }

    #[test]
    fn a_primitive_extension_is_reached_through_its_element() {
        assert_eq!(
            located(
                "Patient",
                "$resource.birthDate.extension('http://example.org/a')",
                &Value::Null,
                &[]
            ),
            "Patient.birthDate.extension"
        );
    }

    #[test]
    fn a_resolved_reference_is_located_at_the_element_holding_it() {
        assert_eq!(
            located(
                "Condition",
                "$resource.subject.resolve().name.family",
                &Value::Null,
                &[]
            ),
            "Condition.subject"
        );
    }

    #[test]
    fn an_element_name_drops_the_choice_marker_only() {
        assert_eq!(element_name("Observation.value[x]"), "value");
        assert_eq!(element_name("Patient.birthDate"), "birthDate");
        assert_eq!(element_name("Patient"), "Patient");
    }
}
