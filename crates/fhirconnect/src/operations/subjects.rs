// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The subjects a `$toopenehr` Bundle names, compared as the entries they
//! resolve to.
//!
//! One Bundle maps to one composition and a composition belongs to one EHR, so
//! a Bundle whose resources concern more than one patient is refused.
//! FHIRconnect governs no part of that refusal: our own design. What counts as
//! one patient follows the R4 rules for resolving a reference inside a Bundle
//! (<https://hl7.org/fhir/R4/bundle.html#references>, §2.36.4.1), so a
//! document that addresses its Patient entry by a `urn:uuid` `fullUrl` and a
//! collection that names it as `Patient/<id>` both count one subject.

use fhir_types::codec::Value;

use crate::operations::error::OperationError;

/// The two elements a resource names its patient by.
const SUBJECT_ELEMENTS: [&str; 2] = ["subject", "patient"];

/// The path segment that marks a version-specific reference.
const HISTORY: &str = "/_history/";

/// One entry of the Bundle: its `fullUrl`, when it carries one, and its
/// resource.
type Entry = (Option<String>, Value);

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

/// Returns the position of every entry `reference`, written in the entry
/// whose `fullUrl` is `referrer`, resolves to.
///
/// A version-specific reference loses its version for the match and then
/// matches the entry's `meta.versionId`; an absolute reference matches a
/// `fullUrl`; a relative `<type>/<id>` is made absolute against the root of a
/// server `referrer` URL (bundle.html §2.36.4.1). More than one match leaves the
/// reference ambiguous, and every match is returned so the caller sees each.
fn resolve(entries: &[Entry], referrer: Option<&str>, reference: &str) -> Vec<usize> {
    let (unversioned, version) = match reference.split_once(HISTORY) {
        Some((head, version)) => (head, Some(version)),
        None => (reference, None),
    };
    let matching = |predicate: &dyn Fn(&Entry) -> bool| -> Vec<usize> {
        entries
            .iter()
            .enumerate()
            .filter(|&(_, entry)| predicate(entry) && versioned(&entry.1, version))
            .map(|(position, _)| position)
            .collect()
    };
    if is_absolute(unversioned) {
        return matching(&|entry| entry.0.as_deref() == Some(unversioned));
    }
    let Some((kind, id)) = relative_parts(unversioned) else {
        return Vec::new();
    };
    if let Some(root) = referrer.and_then(restful_root) {
        let absolute = format!("{root}/{kind}/{id}");
        return matching(&|entry| entry.0.as_deref() == Some(absolute.as_str()));
    }
    // NOTE: bundle.html §2.36.4.1 gives a relative reference in an entry with
    // no RESTful `fullUrl` no meaning; no specification governs this: our own
    // design reads it against each entry's `resourceType` and `id`.
    matching(&|entry| {
        entry.1.get("resourceType").and_then(Value::as_str) == Some(kind)
            && entry.1.get("id").and_then(Value::as_str) == Some(id)
    })
}

/// Returns whether `resource` is the version `version` names, any version
/// when it names none.
fn versioned(resource: &Value, version: Option<&str>) -> bool {
    version.is_none_or(|wanted| {
        resource
            .get("meta")
            .and_then(|meta| meta.get("versionId"))
            .and_then(Value::as_str)
            == Some(wanted)
    })
}

/// Returns whether `reference` is an absolute URI, which names its scheme
/// before any path separator (<https://hl7.org/fhir/R4/references.html#literal>).
fn is_absolute(reference: &str) -> bool {
    match (reference.find(':'), reference.find('/')) {
        (Some(colon), Some(slash)) => colon < slash,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// Returns the type and id of a relative reference of the form
/// `<type>/<id>`.
fn relative_parts(reference: &str) -> Option<(&str, &str)> {
    let (kind, id) = reference.split_once('/')?;
    (!kind.is_empty() && !id.is_empty() && !id.contains('/')).then_some((kind, id))
}

/// Returns the service base of a server `fullUrl`, the URL with its
/// `<type>/<id>` tail removed.
///
/// A server URL is an `http` or `https` URL that ends in `<type>/<id>`
/// (<https://hl7.org/fhir/R4/references.html#literal>); a `urn:uuid` or
/// `urn:oid` `fullUrl` has no base.
fn restful_root(full_url: &str) -> Option<&str> {
    if !(full_url.starts_with("http://") || full_url.starts_with("https://")) {
        return None;
    }
    let (rest, id) = full_url.rsplit_once('/')?;
    let (root, kind) = rest.rsplit_once('/')?;
    let typed = kind
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase());
    (typed && !id.is_empty() && root.contains("://")).then_some(root)
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

#[cfg(test)]
mod tests {
    use super::is_absolute;
    use super::relative_parts;
    use super::restful_root;

    #[test]
    fn a_uri_with_a_scheme_is_absolute() {
        assert!(is_absolute("urn:uuid:0fed5ebe-ca8f-4ad1-aba4-ddad45bd6cc8"));
        assert!(is_absolute("urn:oid:1.2.3.4"));
        assert!(is_absolute("http://example.org/fhir/Patient/1"));
        assert!(!is_absolute("Patient/1"));
        assert!(!is_absolute("Patient/a:b"));
    }

    #[test]
    fn a_relative_reference_has_a_type_and_an_id() {
        assert_eq!(relative_parts("Patient/1"), Some(("Patient", "1")));
        assert_eq!(relative_parts("Patient"), None);
        assert_eq!(relative_parts("Patient/1/extra"), None);
    }

    #[test]
    fn only_a_restful_full_url_has_a_root() {
        assert_eq!(
            restful_root("http://example.org/fhir/Condition/2"),
            Some("http://example.org/fhir")
        );
        assert_eq!(
            restful_root("urn:uuid:0fed5ebe-ca8f-4ad1-aba4-ddad45bd6cc8"),
            None
        );
        assert_eq!(restful_root("http://example.org/condition/2"), None);
    }
}
