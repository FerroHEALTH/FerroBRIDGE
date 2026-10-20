// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The subjects a `$toopenehr` Bundle names, compared as the entries they
//! resolve to.
//!
//! One Bundle maps to one composition and a composition belongs to one EHR, so
//! a Bundle whose resources concern more than one patient is refused.
//! FHIRconnect governs no part of that refusal: our own design. What counts as
//! one patient follows the R4 rules for resolving a reference inside a Bundle
//! (<https://hl7.org/fhir/R4/bundle.html#references>, §2.36.4.1), through the
//! resolver the reference seam shares, so a document that addresses its
//! Patient entry by a `urn:uuid` `fullUrl` and a collection that names it as
//! `Patient/<id>` both count one subject.

use fhir_types::codec::Value;

use crate::engine::bundle::Entry;
use crate::engine::bundle::resolve;
use crate::operations::error::OperationError;

/// The two elements a resource names its patient by.
const SUBJECT_ELEMENTS: [&str; 2] = ["subject", "patient"];

/// One subject the Bundle names.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Subject {
    /// The entry at this position of the Bundle.
    Entry(usize),
    /// A reference no entry of the Bundle resolves, as written.
    Outside(String),
}

/// Refuses a Bundle whose resources name more than one subject.
///
/// Every `Patient` entry is a subject, and so is whatever each top-level
/// `subject` or `patient` reference resolves to: the entry it addresses, or
/// the reference text when it addresses none.
///
/// # Errors
///
/// Returns [`OperationError::SeveralSubjects`] naming each distinct subject in
/// order: an entry by its `fullUrl` or else `<type>/<id>`, and a reference
/// outside the Bundle as written.
pub(crate) fn refuse_several(entries: &[Entry]) -> Result<(), OperationError> {
    let mut subjects: Vec<Subject> = Vec::new();
    for (position, (full_url, resource)) in entries.iter().enumerate() {
        if resource.get("resourceType").and_then(Value::as_str) == Some("Patient") {
            record(&mut subjects, Subject::Entry(position));
        }
        for element in SUBJECT_ELEMENTS {
            for reference in literals(resource.get(element)) {
                let found = resolve(entries, full_url.as_deref(), reference);
                if found.is_empty() {
                    record(&mut subjects, Subject::Outside(String::from(reference)));
                }
                for target in found {
                    record(&mut subjects, Subject::Entry(target));
                }
            }
        }
    }
    if subjects.len() > 1 {
        return Err(OperationError::SeveralSubjects {
            subjects: subjects
                .iter()
                .map(|subject| rendered(entries, subject))
                .collect(),
        });
    }
    Ok(())
}

/// Records `subject` when it is not recorded already.
fn record(subjects: &mut Vec<Subject>, subject: Subject) {
    if !subjects.contains(&subject) {
        subjects.push(subject);
    }
}

/// Returns the literal references an element carries, one for a single
/// `Reference` and one per item of a repeating one.
fn literals(element: Option<&Value>) -> Vec<&str> {
    match element {
        Some(Value::Array(items)) => items.iter().filter_map(literal).collect(),
        Some(single) => literal(single).into_iter().collect(),
        None => Vec::new(),
    }
}

/// Returns the literal reference of one `Reference`, when it carries one.
fn literal(reference: &Value) -> Option<&str> {
    reference.get("reference").and_then(Value::as_str)
}

/// Returns how a refusal names `subject`.
fn rendered(entries: &[Entry], subject: &Subject) -> String {
    match *subject {
        Subject::Outside(ref reference) => reference.clone(),
        Subject::Entry(position) => {
            let Some((full_url, resource)) = entries.get(position) else {
                return format!("entry {position}");
            };
            if let Some(url) = full_url {
                return url.clone();
            }
            let kind = resource.get("resourceType").and_then(Value::as_str);
            let id = resource.get("id").and_then(Value::as_str);
            match (kind, id) {
                (Some(kind), Some(id)) => format!("{kind}/{id}"),
                (Some(kind), None) => format!("{kind} at entry {position}"),
                _ => format!("entry {position}"),
            }
        }
    }
}
