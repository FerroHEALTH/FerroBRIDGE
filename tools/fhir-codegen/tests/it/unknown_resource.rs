// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `UnknownResource` carries any resource's JSON body, a root-set type
//! included, beside typed entries in the same Bundle.
//!
//! A relay writes another server's resource as it arrived: every value as
//! given, a decimal in its lexical form
//! (<https://hl7.org/fhir/R4/datatypes.html#decimal>), and the members in the
//! sorted key order of `codec::Object`.

use fhir_types::codec::{Json, Value};
use fhir_types::r4::bundle::{Bundle, BundleEntry};
use fhir_types::r4::resource::{Resource, UnknownResource};

/// A synthetic `MedicationRequest` with a decimal carrying a trailing zero and
/// an extension the R4 model knows only as `Extension`, members out of order.
const MEDICATION_REQUEST: &str = r#"{"resourceType":"MedicationRequest","status":"active","intent":"order","id":"mr-1","extension":[{"url":"http://example.org/fhir/StructureDefinition/relay-origin","valueString":"member-a"}],"subject":{"reference":"Patient/p-1"},"dosageInstruction":[{"doseAndRate":[{"doseQuantity":{"value":1.50,"unit":"mg"}}]}],"medicationCodeableConcept":{"text":"synthetic"}}"#;

/// A searchset with one typed `OperationOutcome` entry.
const BUNDLE: &str = r#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"OperationOutcome","issue":[{"severity":"information","code":"informational"}]}}]}"#;

/// The bundle with the raw `MedicationRequest` appended as `Resource::Unknown`.
fn relayed() -> (Bundle, Value) {
    let raw: Value = serde_json::from_str(MEDICATION_REQUEST).expect("the member parses");
    let mut bundle: Bundle = serde_json::from_str(BUNDLE).expect("the bundle decodes");
    bundle.entry.push(BundleEntry {
        resource: Some(Resource::Unknown(UnknownResource {
            resource_type: "MedicationRequest".to_owned(),
            body: raw.clone(),
        })),
        ..BundleEntry::default()
    });
    (bundle, raw)
}

/// The `resource` of the entry at `index` in a written Bundle document.
fn entry_resource(document: &Value, index: usize) -> &Value {
    document
        .get("entry")
        .and_then(Value::as_array)
        .and_then(|entries| entries.get(index))
        .and_then(|entry| entry.get("resource"))
        .expect("the written bundle carries a resource at the entry")
}

#[test]
fn a_root_set_resource_carried_raw_is_written_as_given_beside_a_typed_entry() {
    let (bundle, raw) = relayed();
    let direct = serde_json::to_string(&bundle).expect("the bundle writes");
    let document = serde_json::to_string(&Value::Object(bundle.to_json().expect("encodes")))
        .expect("the document writes");
    assert_eq!(direct, document, "the two write paths agree");

    assert!(
        direct.contains(r#""value":1.50"#),
        "the decimal keeps its lexical form"
    );
    let sorted = serde_json::to_string(&raw).expect("the raw body writes");
    assert!(
        sorted.starts_with(r#"{"dosageInstruction":"#),
        "the raw body's members are in sorted key order: {sorted}"
    );
    assert!(
        direct.contains(&sorted),
        "the raw entry is written unchanged"
    );

    let written: Value = serde_json::from_str(&direct).expect("the written bundle parses");
    assert_eq!(entry_resource(&written, 1), &raw, "the raw entry's values");
    let Value::Object(typed) = entry_resource(&written, 0) else {
        panic!("the typed entry is an object");
    };
    assert_eq!(
        typed.get("resourceType"),
        Some(&Value::String("OperationOutcome".to_owned())),
        "the typed entry is written beside it"
    );
}

#[test]
fn decoding_the_written_bundle_yields_the_typed_variant_for_a_root_set_type() {
    let (bundle, _) = relayed();
    let direct = serde_json::to_string(&bundle).expect("the bundle writes");
    let reread: Bundle = serde_json::from_str(&direct).expect("the written bundle decodes");
    let resource = reread
        .entry
        .get(1)
        .and_then(|entry| entry.resource.as_ref())
        .expect("the second entry carries a resource");
    assert!(
        matches!(resource, Resource::MedicationRequest(_)),
        "the raw form is a write-side carrier: {resource:?}"
    );
    let rewritten = serde_json::to_string(&reread).expect("the reread bundle writes");
    assert_eq!(rewritten, direct, "the typed round trip keeps every value");
}
