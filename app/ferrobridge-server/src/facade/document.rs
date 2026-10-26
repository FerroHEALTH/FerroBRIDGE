// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document Bundle `Composition/$document` answers with.
//!
//! FHIR R4 defines the operation and leaves the assembly to the server: it
//! "takes the composition resource, locates all the referenced resources ...
//! and ... returns a full document bundle"
//! (<https://hl7.org/fhir/R4/composition-operation-document.html>), and a
//! document Bundle has the Composition as its first entry
//! (<https://hl7.org/fhir/R4/documents.html#bundle>). How this server
//! assembles it is its own design: the Bundle is `type` `document`, its
//! `identifier` a name-based UUID of the composition version the content came
//! from, so the same version always yields the same document identifier, its
//! `timestamp` the moment of assembly, entry 0 the Composition, and then every
//! resource the Composition context's outbound run created, in the order the
//! run created them, each with the absolute URL a read answers it at as its
//! `fullUrl`.

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use url::Url;

/// The `Bundle.identifier.system` of a URI value
/// (<https://hl7.org/fhir/R4/terminologies-systems.html>, `urn:ietf:rfc:3986`).
const URI_SYSTEM: &str = "urn:ietf:rfc:3986";

/// What one document is assembled from.
#[derive(Debug, Clone)]
pub(crate) struct Parts<'a> {
    /// The rendered Composition.
    pub(crate) composition: Object,
    /// Every resource the Composition context's outbound run created, in
    /// order, each rendered with its own id.
    pub(crate) created: Vec<Object>,
    /// The facade's base URL, which every `fullUrl` sits under.
    pub(crate) base: &'a Url,
    /// The URL of the composition version the content came from, which the
    /// document identifier is derived from.
    pub(crate) source: &'a Url,
    /// The instant the document is assembled, as an R4 `instant`.
    pub(crate) timestamp: &'a str,
}

/// Returns the document Bundle of `parts`.
// NOTE: R4 OperationDefinition Composition-document says the document is "wrapped inside the
// search bundle"; this server answers the document Bundle itself, by owner ruling on #116.
#[must_use]
pub(crate) fn assemble(parts: Parts<'_>) -> Object {
    let identifier =
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, parts.source.as_str().as_bytes());
    let entries = std::iter::once(parts.composition)
        .chain(parts.created)
        .map(|resource| entry(parts.base, resource))
        .collect();
    let mut bundle = Object::new();
    bundle.insert(
        String::from("resourceType"),
        Value::String(String::from("Bundle")),
    );
    let mut id = Object::new();
    id.insert(
        String::from("system"),
        Value::String(String::from(URI_SYSTEM)),
    );
    id.insert(
        String::from("value"),
        Value::String(format!("urn:uuid:{identifier}")),
    );
    bundle.insert(String::from("identifier"), Value::Object(id));
    bundle.insert(
        String::from("type"),
        Value::String(String::from("document")),
    );
    bundle.insert(
        String::from("timestamp"),
        Value::String(String::from(parts.timestamp)),
    );
    bundle.insert(String::from("entry"), Value::Array(entries));
    bundle
}

/// Returns the entry of one resource, with the absolute URL a read answers it
/// at as its `fullUrl`.
fn entry(base: &Url, resource: Object) -> Value {
    let kind = resource
        .get("resourceType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let id = resource
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let full_url = format!("{}/{kind}/{id}", base.as_str().trim_end_matches('/'));
    let mut entry = Object::new();
    entry.insert(String::from("fullUrl"), Value::String(full_url));
    entry.insert(String::from("resource"), Value::Object(resource));
    Value::Object(entry)
}

#[cfg(test)]
mod tests {
    use fhir_types::codec::Object;
    use fhir_types::codec::Value;
    use url::Url;

    use super::Parts;
    use super::assemble;

    fn resource(kind: &str, id: &str) -> Object {
        let mut object = Object::new();
        object.insert(
            String::from("resourceType"),
            Value::String(String::from(kind)),
        );
        object.insert(String::from("id"), Value::String(String::from(id)));
        object
    }

    fn document(source: &Url) -> serde_json::Value {
        let base = Url::parse("https://bridge.example/fhir/").expect("a URL");
        let bundle = assemble(Parts {
            composition: resource("Composition", "c-1"),
            created: vec![
                resource("DiagnosticReport", "d-1"),
                resource("Observation", "o-1"),
            ],
            base: &base,
            source,
            timestamp: "2026-03-02T09:30:00Z",
        });
        Value::Object(bundle)
            .to_serde_json(&mut fhir_types::codec::Path::root("Bundle"))
            .expect("the bundle serialises")
    }

    #[test]
    fn the_composition_is_entry_zero_and_every_created_resource_follows() {
        let source = Url::parse("https://cdr.example/ehr/e/composition/v::cdr::1").expect("a URL");
        let bundle = document(&source);
        assert_eq!(bundle["type"], "document");
        assert_eq!(bundle["timestamp"], "2026-03-02T09:30:00Z");
        let urls: Vec<&str> = bundle["entry"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|entry| entry["fullUrl"].as_str().expect("a fullUrl"))
            .collect();
        assert_eq!(
            urls,
            [
                "https://bridge.example/fhir/Composition/c-1",
                "https://bridge.example/fhir/DiagnosticReport/d-1",
                "https://bridge.example/fhir/Observation/o-1",
            ]
        );
        assert_eq!(
            bundle["entry"][0]["resource"]["resourceType"],
            "Composition"
        );
    }

    #[test]
    fn one_composition_version_always_names_the_same_document() {
        let one = Url::parse("https://cdr.example/ehr/e/composition/v::cdr::1").expect("a URL");
        let two = Url::parse("https://cdr.example/ehr/e/composition/v::cdr::2").expect("a URL");
        assert_eq!(document(&one)["identifier"], document(&one)["identifier"]);
        assert_ne!(document(&one)["identifier"], document(&two)["identifier"]);
        assert_eq!(document(&one)["identifier"]["system"], "urn:ietf:rfc:3986");
        assert!(
            document(&one)["identifier"]["value"]
                .as_str()
                .is_some_and(|value| value.starts_with("urn:uuid:"))
        );
    }
}
