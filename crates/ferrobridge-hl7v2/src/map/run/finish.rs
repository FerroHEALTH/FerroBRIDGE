// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The end of a run: the translations asked, the writes applied, the
//! required-element pass, and the Bundle assembled.

use std::collections::{BTreeMap, BTreeSet};

use ferrobridge_term::outcome::Translation;
use fhir_types::codec::{Json, Value};
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::tree::Occurrence;
use fhirconnect::tree::element::{Move, resolve};
use fhirconnect::tree::path::FhirPath;
use fhirconnect::tree::write::write;

use crate::map::constraint;
use crate::map::run::dangling::{Unlinked, unlink};
use crate::map::run::resource::document_of;
use crate::map::run::write::repeating;
use crate::map::run::{Identity, Part, Pending, Resource, Run, Write};
use crate::map::{MapError, Outcome};

impl Run<'_> {
    /// Asks the terminology server for every recorded translation.
    ///
    /// # Errors
    ///
    /// Returns [`MapError::Terminology`] when the server refuses a
    /// translation or cannot be reached.
    pub(in crate::map) async fn translate(
        &mut self,
        terminology: Option<&ferrobridge_term::client::Client>,
    ) -> Result<usize, MapError> {
        let mut calls = 0usize;
        let mut outcomes = Vec::new();
        for request in &mut self.requests {
            let Some(client) = terminology else {
                outcomes.push(Outcome::NoTerminology {
                    at: request.at.clone(),
                    row: request.row.clone(),
                });
                continue;
            };
            for group in &request.groups {
                calls = calls.saturating_add(1);
                let answer = client
                    .translate(
                        &Translation::new(group.source.as_str(), request.code.as_str())
                            .with_concept_map(request.url.as_str())
                            .with_target_system(group.target.as_str()),
                    )
                    .await
                    .map_err(|source| MapError::Terminology {
                        concept_map: request.url.clone(),
                        source: Box::new(source),
                    })?;
                if let Some(found) = answer.accepted().find_map(|found| found.concept.clone()) {
                    request.answer = Some(found);
                    break;
                }
            }
            if request.answer.is_none() {
                outcomes.push(Outcome::Untranslated {
                    at: request.at.clone(),
                    row: request.row.clone(),
                    concept_map: request.url.clone(),
                });
            }
        }
        self.outcomes.extend(outcomes);
        Ok(calls)
    }

    /// Applies every write and assembles the Bundle.
    ///
    /// # Errors
    ///
    /// Returns [`MapError::NoMessageHeader`] when the Bundle is a `message`
    /// and no complete `MessageHeader` enters it.
    pub(in crate::map) fn finish(mut self) -> Result<(Value, Vec<Outcome>), MapError> {
        self.siblings();
        let mut documents = Vec::new();
        let resources = core::mem::take(&mut self.resources);
        for resource in resources.iter().filter(|resource| !resource.omitted) {
            let document = self.build(resource);
            documents.push((resource, document));
        }
        let mut entries: Vec<(&Resource, Value)> = Vec::new();
        let mut envelope = None;
        for (resource, mut document) in documents {
            let complete = self.complete(resource, &mut document);
            if resource.identity == Identity::Envelope {
                envelope = Some(document);
                continue;
            }
            if !complete {
                continue;
            }
            if let Value::Object(object) = &document {
                let decoded = fhir_types::r4::resource::Resource::from_json(
                    object,
                    &mut fhir_types::codec::Path::root(&resource.type_name),
                );
                if let Err(error) = decoded {
                    self.outcomes.push(Outcome::Undecodable {
                        resource: resource.type_name.clone(),
                        full_url: resource.full_url.clone(),
                        error,
                    });
                    continue;
                }
            }
            entries.push((resource, document));
        }
        self.unlink(&resources, &mut entries);
        entries.sort_by_key(|(resource, _)| resource.type_name != "MessageHeader");
        let mut bundle = envelope.unwrap_or_else(|| document_of("Bundle"));
        let message = bundle.get("type").and_then(Value::as_str) == Some("message");
        let headed = entries
            .first()
            .is_some_and(|(resource, _)| resource.type_name == "MessageHeader");
        // NOTE: HL7 R4 Bundle invariant bdl-12: a `message` Bundle's first resource is a
        // MessageHeader, so a run that could not complete one refuses the message.
        if message && !headed {
            let dropped = self
                .outcomes
                .iter()
                .rev()
                .find(|outcome| match outcome {
                    Outcome::MissingRequired {
                        resource, element, ..
                    } => resource == "MessageHeader" && element == "MessageHeader",
                    Outcome::Undecodable { resource, .. } => resource == "MessageHeader",
                    _ => false,
                })
                .cloned()
                .map(Box::new);
            return Err(MapError::NoMessageHeader { dropped });
        }
        let entries = entries
            .into_iter()
            .map(|(resource, document)| {
                (
                    resource.type_name.clone(),
                    resource.full_url.clone(),
                    document,
                )
            })
            .collect();
        enter(&mut bundle, entries, &mut self.outcomes);
        Ok((bundle, self.outcomes))
    }

    /// Removes every reference a kept entry holds to a resource left out of
    /// the Bundle, counting each, and leaves out an entry the removal makes
    /// incomplete, until no reference dangles.
    ///
    /// NOTE: no specification governs this: our own design; the reference goes and the
    /// referring resource stays, so the Bundle resolves (R4 Bundle §Resolving references).
    fn unlink(&mut self, resources: &[Resource], entries: &mut Vec<(&Resource, Value)>) {
        let mut dropped: BTreeMap<String, String> = resources
            .iter()
            .filter(|resource| resource.identity != Identity::Envelope)
            .filter(|resource| {
                !entries
                    .iter()
                    .any(|(kept, _)| kept.full_url == resource.full_url)
            })
            .map(|resource| (resource.full_url.clone(), resource.type_name.clone()))
            .collect();
        loop {
            let mut incomplete = Vec::new();
            for (position, (resource, document)) in entries.iter_mut().enumerate() {
                let mut found = Vec::new();
                unlink(document, &resource.type_name, &dropped, &mut found);
                if found.is_empty() {
                    continue;
                }
                for Unlinked {
                    element,
                    target,
                    target_resource,
                } in found
                {
                    self.outcomes.push(Outcome::DanglingReference {
                        resource: resource.type_name.clone(),
                        full_url: resource.full_url.clone(),
                        element,
                        target_resource,
                        target,
                    });
                }
                if !self.complete(resource, document) {
                    incomplete.push(position);
                }
            }
            if incomplete.is_empty() {
                return;
            }
            for position in incomplete.into_iter().rev() {
                if position < entries.len() {
                    let (resource, _) = entries.remove(position);
                    dropped.insert(resource.full_url.clone(), resource.type_name.clone());
                }
            }
        }
    }

    /// Drops from `document` every element that lacks one its definition
    /// requires, counting each, and answers whether the resource itself is
    /// complete.
    ///
    /// NOTE: no specification governs this: our own design; an element whose
    /// required sibling no row wrote is left out, so the resource decodes.
    fn complete(&mut self, resource: &Resource, document: &mut Value) -> bool {
        let (Value::Object(object), Some(schema)) =
            (document, SCHEMAS.type_named(&resource.type_name))
        else {
            return true;
        };
        let mut dropped = Vec::new();
        let incomplete = constraint::prune(object, schema, &resource.type_name, &mut dropped);
        let missing = |element: String, required: &str| Outcome::MissingRequired {
            resource: resource.type_name.clone(),
            full_url: resource.full_url.clone(),
            element,
            required: String::from(required),
        };
        for found in dropped {
            self.outcomes.push(missing(found.element, found.required));
        }
        let Some(required) = incomplete else {
            return true;
        };
        self.outcomes
            .push(missing(resource.type_name.clone(), required));
        false
    }

    /// Answers whether a write may go ahead, counting why when it may not: an
    /// element an earlier write filled, or a value outside the lexical form
    /// of the primitive `resolved` ends on.
    fn admits(
        &mut self,
        pending: &Write,
        resolved: Option<&fhirconnect::tree::element::Resolved>,
        value: &Value,
        superseded: bool,
    ) -> bool {
        if superseded {
            self.outcomes.push(Outcome::Superseded {
                at: pending.at.clone(),
                row: pending.row.clone(),
            });
            return false;
        }
        let Some((element, fhir_type, error)) =
            resolved.and_then(|resolved| refusal(resolved, value))
        else {
            return true;
        };
        self.outcomes.push(Outcome::InvalidValue {
            at: pending.at.clone(),
            row: pending.row.clone(),
            element,
            fhir_type: String::from(fhir_type),
            error,
        });
        false
    }

    /// Applies one resource's writes in order.
    fn build(&mut self, resource: &Resource) -> Value {
        let mut document = document_of(&resource.type_name);
        let mut indices: BTreeMap<(Vec<usize>, String, String), usize> = BTreeMap::new();
        let mut lengths: BTreeMap<(Vec<usize>, String), usize> = BTreeMap::new();
        let mut written: BTreeSet<(String, Vec<usize>)> = BTreeSet::new();
        let mut chosen: BTreeMap<(String, Vec<usize>), String> = BTreeMap::new();
        for pending in &resource.writes {
            let Some(value) = self.settle(&pending.value) else {
                continue;
            };
            let names: Vec<&str> = pending
                .slots
                .iter()
                .map(|slot| slot.name.as_str())
                .collect();
            let flags = match repeating(&resource.type_name, &names) {
                Ok(flags) => flags,
                Err(error) => {
                    self.outcomes.push(Outcome::UnknownElement {
                        at: pending.at.clone(),
                        row: pending.row.clone(),
                        error,
                    });
                    continue;
                }
            };
            let mut occurrence = Vec::new();
            let mut fresh = Vec::new();
            let mut prefix = String::new();
            for (slot, repeats) in pending.slots.iter().zip(flags) {
                prefix.push('.');
                prefix.push_str(&slot.name);
                if !repeats {
                    continue;
                }
                let key = (occurrence.clone(), prefix.clone(), slot.key.clone());
                let index = if let Some(index) = indices.get(&key) {
                    *index
                } else {
                    let length = (occurrence.clone(), prefix.clone());
                    let next = lengths.get(&length).copied().unwrap_or_default();
                    fresh.push((key, length, next));
                    next
                };
                occurrence.push(index);
            }
            let path = format!("$resource{prefix}");
            let Ok(path) = path.parse::<FhirPath>() else {
                continue;
            };
            // NOTE: no specification governs this: our own design; a path that does not
            // resolve is left to `write`, which counts its refusal as `unwritable`.
            let resolved = resolve(&SCHEMAS, &resource.type_name, &path).ok();
            let leaf = (prefix.clone(), occurrence.clone());
            let taken = resolved
                .as_ref()
                .map(|resolved| alternatives(resolved, &occurrence))
                .unwrap_or_default();
            let superseded = written.contains(&leaf)
                || taken
                    .iter()
                    .any(|(choice, key)| chosen.get(choice).is_some_and(|earlier| earlier != key));
            if !self.admits(pending, resolved.as_ref(), &value, superseded) {
                continue;
            }
            match write(
                &SCHEMAS,
                &mut document,
                &path,
                &Occurrence::new(occurrence),
                value,
            ) {
                Ok(()) => {
                    for (key, length, next) in fresh {
                        indices.insert(key, next);
                        lengths.insert(length, next.saturating_add(1));
                    }
                    written.insert(leaf);
                    chosen.extend(taken);
                }
                Err(error) => self.outcomes.push(Outcome::Unwritable {
                    at: pending.at.clone(),
                    row: pending.row.clone(),
                    error,
                }),
            }
        }
        document
    }

    /// The value a pending write carries once the translations are in.
    pub(super) fn settle(&self, pending: &Pending) -> Option<Value> {
        match pending {
            Pending::Ready(value) => Some(value.clone()),
            Pending::Translation { request, part } => {
                let concept = self.requests.get(*request)?.answer.as_ref()?;
                let text = match part {
                    Part::Code => concept.code.clone(),
                    Part::System => concept.system.clone(),
                    Part::Display => concept.display.clone(),
                }?;
                Some(match text.as_str() {
                    "true" if *part == Part::Code => Value::Bool(true),
                    "false" if *part == Part::Code => Value::Bool(false),
                    _ => Value::String(text),
                })
            }
            Pending::Sibling { .. } => None,
        }
    }
}

/// The Bundle paths of an entry's members.
const ENTRY_PATHS: [&str; 2] = ["$resource.entry.fullUrl", "$resource.entry.resource"];

/// Writes each entry's `fullUrl` and resource into `bundle` at its index,
/// counting every member the writer refuses as [`Outcome::EntryUnwritable`].
fn enter(bundle: &mut Value, entries: Vec<(String, String, Value)>, outcomes: &mut Vec<Outcome>) {
    for (index, (resource, full_url, document)) in entries.into_iter().enumerate() {
        let occurrence = Occurrence::new([index]);
        let [url_path, resource_path] = ENTRY_PATHS;
        for (path, value) in [
            (url_path, Value::String(full_url.clone())),
            (resource_path, document),
        ] {
            #[expect(
                clippy::expect_used,
                reason = "ENTRY_PATHS are literals in the FHIRPath grammar, which the test below parses"
            )]
            let parsed = path
                .parse::<FhirPath>()
                .expect("an entry path should parse");
            if let Err(error) = write(&SCHEMAS, bundle, &parsed, &occurrence, value) {
                outcomes.push(Outcome::EntryUnwritable {
                    resource: resource.clone(),
                    full_url: full_url.clone(),
                    path,
                    error,
                });
            }
        }
    }
}

/// The choice elements a write steps through, each keyed by its instance
/// (the members before it, the stem and the occurrence indices spent so far)
/// with the alternative the write takes.
///
/// A choice element that does not repeat holds one alternative
/// (<https://hl7.org/fhir/R4/json.html>, choice elements;
/// <https://hl7.org/fhir/R4/elementdefinition.html>, `ElementDefinition.max`),
/// so a write taking another alternative of an instance already written is
/// refused.
fn alternatives(
    resolved: &fhirconnect::tree::element::Resolved,
    occurrence: &[usize],
) -> Vec<((String, Vec<usize>), String)> {
    let mut found = Vec::new();
    let mut members = String::new();
    let mut spent = 0usize;
    for step in resolved.moves() {
        let (Move::Member(field) | Move::Extension { field, .. }) = step else {
            continue;
        };
        let stem = field
            .path()
            .rsplit('.')
            .next()
            .and_then(|name| name.strip_suffix("[x]"));
        if let Some(stem) = stem.filter(|_| !field.repeats()) {
            let spent_indices = occurrence.get(..spent).unwrap_or_default().to_vec();
            found.push((
                (format!("{members}.{stem}"), spent_indices),
                String::from(field.key()),
            ));
        }
        if field.repeats() {
            spent = spent.saturating_add(1);
        }
        members.push('.');
        members.push_str(field.key());
    }
    found
}

/// The element, primitive type and refusal of a value outside the lexical
/// form of the primitive `resolved` ends on, `None` when the value keeps it
/// or the path ends on no primitive.
fn refusal(
    resolved: &fhirconnect::tree::element::Resolved,
    value: &Value,
) -> Option<(String, &'static str, fhir_types::codec::DecodeErrorKind)> {
    if !matches!(
        resolved.location(),
        fhirconnect::tree::element::Location::Primitive(_)
            | fhirconnect::tree::element::Location::Attribute
    ) {
        return None;
    }
    let code = resolved.type_code()?;
    let error = constraint::lexical(code, value).err()?;
    Some((String::from(resolved.leaf()), code, error))
}

#[cfg(test)]
mod tests {
    use fhir_types::codec::Value;
    use fhirconnect::tree::path::FhirPath;

    use super::{ENTRY_PATHS, alternatives, enter};
    use crate::map::Outcome;
    use crate::map::run::write::resolve_path;

    #[test]
    fn the_entry_paths_parse() {
        for path in ENTRY_PATHS {
            assert!(path.parse::<FhirPath>().is_ok(), "{path} parses");
        }
    }

    #[test]
    fn an_entry_the_writer_refuses_is_counted_naming_the_resource() {
        let mut bundle: Value = serde_json::from_str(
            r#"{"resourceType":"Bundle","type":"message","entry":"not an array"}"#,
        )
        .expect("a JSON document");
        let patient: Value =
            serde_json::from_str(r#"{"resourceType":"Patient"}"#).expect("a JSON document");
        let mut outcomes = Vec::new();
        enter(
            &mut bundle,
            vec![(String::from("Patient"), String::from("urn:uuid:p"), patient)],
            &mut outcomes,
        );
        let refused: Vec<(&str, &str, &str)> = outcomes
            .iter()
            .filter_map(|outcome| match outcome {
                Outcome::EntryUnwritable {
                    resource,
                    full_url,
                    path,
                    ..
                } => Some((resource.as_str(), full_url.as_str(), *path)),
                _ => None,
            })
            .collect();
        assert_eq!(
            refused,
            [
                ("Patient", "urn:uuid:p", "$resource.entry.fullUrl"),
                ("Patient", "urn:uuid:p", "$resource.entry.resource"),
            ]
        );
        assert_eq!(outcomes.len(), 2, "each refused member is one outcome");
    }

    #[test]
    fn two_alternatives_of_one_extension_value_share_the_choice_instance() {
        let string = resolve_path("Observation", &["extension", "valueString"]).expect("a path");
        let concept = resolve_path(
            "Observation",
            &["extension", "valueCodeableConcept", "coding", "code"],
        )
        .expect("a path");
        let choice = (String::from(".extension.value"), vec![0]);
        assert_eq!(
            alternatives(&string, &[0]),
            vec![(choice.clone(), String::from("valueString"))]
        );
        assert_eq!(
            alternatives(&concept, &[0, 0]),
            vec![(choice, String::from("valueCodeableConcept"))]
        );
        assert_eq!(
            alternatives(&concept, &[1, 0]),
            vec![(
                (String::from(".extension.value"), vec![1]),
                String::from("valueCodeableConcept")
            )]
        );
    }
}
