// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Which entry of a Bundle a literal reference names.
//!
//! One resolver serves the `$toopenehr` subject check and the reference seam,
//! so the two can never disagree about which entry a reference addresses. It
//! follows the R4 rules for resolving a reference inside a Bundle
//! (<https://hl7.org/fhir/R4/bundle.html#references>, §2.36.4.1): an absolute
//! reference matches a `fullUrl`, a relative `<type>/<id>` is read against the
//! service base of a server `fullUrl` of the entry that holds it, and a
//! version-specific reference matches the entry's `meta.versionId`.

use fhir_types::codec::Value;

/// The path segment that marks a version-specific reference.
const HISTORY: &str = "/_history/";

/// One entry of a Bundle: its `fullUrl`, when it carries one, and its
/// resource.
pub(crate) type Entry = (Option<String>, Value);

/// Returns the position of every entry `reference`, written in the entry
/// whose `fullUrl` is `referrer`, resolves to.
///
/// A version-specific reference loses its version for the match and then
/// matches the entry's `meta.versionId`; an absolute reference matches a
/// `fullUrl`; a relative `<type>/<id>` is made absolute against the root of a
/// server `referrer` URL (bundle.html §2.36.4.1). More than one match leaves
/// the reference ambiguous, and every match is returned so the caller sees
/// each.
#[must_use]
pub(crate) fn resolve(entries: &[Entry], referrer: Option<&str>, reference: &str) -> Vec<usize> {
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
    // no server `fullUrl` no meaning; no specification governs this: our own
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

#[cfg(test)]
mod tests {
    use super::Entry;
    use super::is_absolute;
    use super::relative_parts;
    use super::resolve;
    use super::restful_root;
    use fhir_types::codec::Value;

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

    /// Returns a synthetic entry of `kind` and `id` at `full_url`, at
    /// `version` when one is given.
    fn entry(full_url: Option<&str>, kind: &str, id: &str, version: Option<&str>) -> Entry {
        let mut resource = serde_json::json!({ "resourceType": kind, "id": id });
        if let (Some(version), Some(object)) = (version, resource.as_object_mut()) {
            object.insert(
                String::from("meta"),
                serde_json::json!({ "versionId": version }),
            );
        }
        (full_url.map(String::from), Value::from_serde_json(resource))
    }

    #[test]
    fn a_urn_uuid_reference_names_the_entry_of_that_full_url() {
        let entries = [
            entry(Some("urn:uuid:7a1c"), "Patient", "synthetic-1", None),
            entry(Some("urn:uuid:7a1d"), "Patient", "synthetic-2", None),
        ];
        assert_eq!(resolve(&entries, None, "urn:uuid:7a1d"), vec![1]);
    }

    #[test]
    fn an_absolute_reference_names_the_entry_of_that_full_url() {
        let entries = [entry(
            Some("http://example.org/fhir/Patient/synthetic-1"),
            "Patient",
            "synthetic-1",
            None,
        )];
        assert_eq!(
            resolve(
                &entries,
                None,
                "http://example.org/fhir/Patient/synthetic-1"
            ),
            vec![0]
        );
        assert!(
            resolve(
                &entries,
                None,
                "http://example.net/fhir/Patient/synthetic-1"
            )
            .is_empty()
        );
    }

    #[test]
    fn a_relative_reference_is_read_against_the_server_base_of_its_referrer() {
        // bundle.html §2.36.4.1: the reference is relative to the server base
        // of a RESTful fullUrl, so the entry on another server is no match.
        let entries = [
            entry(
                Some("http://example.net/fhir/Patient/synthetic-1"),
                "Patient",
                "synthetic-1",
                None,
            ),
            entry(
                Some("http://example.org/fhir/Patient/synthetic-1"),
                "Patient",
                "synthetic-1",
                None,
            ),
        ];
        let referrer = Some("http://example.org/fhir/Condition/synthetic-9");
        assert_eq!(resolve(&entries, referrer, "Patient/synthetic-1"), vec![1]);
    }

    #[test]
    fn a_versioned_reference_names_only_the_entry_at_that_version() {
        let entries = [entry(
            Some("http://example.org/fhir/Patient/synthetic-1"),
            "Patient",
            "synthetic-1",
            Some("3"),
        )];
        let referrer = Some("http://example.org/fhir/Condition/synthetic-9");
        assert_eq!(
            resolve(&entries, referrer, "Patient/synthetic-1/_history/3"),
            vec![0]
        );
        assert!(resolve(&entries, referrer, "Patient/synthetic-1/_history/2").is_empty());
        assert_eq!(
            resolve(
                &entries,
                None,
                "http://example.org/fhir/Patient/synthetic-1/_history/3"
            ),
            vec![0]
        );
    }
}
