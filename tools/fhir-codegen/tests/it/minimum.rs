// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The minimum cardinality the generated decoder holds a repeating element to.
//!
//! `ElementDefinition.min` is the least number of times an element appears
//! (<https://hl7.org/fhir/R4/elementdefinition-definitions.html#ElementDefinition.min>),
//! and a JSON array is never empty (<https://hl7.org/fhir/R4/json.html#2.6.2>), so
//! a `1..*` element is refused when it is absent and when its array is empty.

use fhir_types::codec::{DecodeError, DecodeErrorKind, Json, Path, Value, expect_object};

/// Decodes `text` as `T` from the path `path`.
fn decode<T: Json>(text: &str, mut path: Path) -> Result<T, DecodeError> {
    let value: Value = serde_json::from_str(text).expect("the fixture is JSON");
    let object = expect_object(&value, &path).expect("the fixture is an object");
    T::from_json(object, &mut path)
}

/// A synthetic R4 `Provenance` with every required element but the ones
/// `drop` names, which `extra` may carry in another shape.
fn provenance(drop: &str, extra: &str) -> String {
    let mut members = vec![
        r#""resourceType":"Provenance""#,
        r#""target":[{"reference":"Patient/example"}]"#,
        r#""recorded":"2026-10-17T12:00:00+02:00""#,
        r#""agent":[{"who":{"reference":"Practitioner/example"}}]"#,
    ];
    members.retain(|member| !member.starts_with(&format!("\"{drop}\"")));
    if !extra.is_empty() {
        members.push(extra);
    }
    format!("{{{}}}", members.join(","))
}

/// A synthetic R4 `CapabilityStatement` with every required element and the
/// `format` member `format` spells, absent when it is empty.
fn capability_statement(format: &str) -> String {
    let mut members = vec![
        r#""resourceType":"CapabilityStatement""#,
        r#""status":"active""#,
        r#""date":"2026-10-17""#,
        r#""kind":"instance""#,
        r#""fhirVersion":"4.0.1""#,
    ];
    if !format.is_empty() {
        members.push(format);
    }
    format!("{{{}}}", members.join(","))
}

type R4Provenance = fhir_types::r4::provenance::Provenance;
type R4CapabilityStatement = fhir_types::r4::capability_statement::CapabilityStatement;

#[test]
fn a_provenance_with_every_required_element_decodes() {
    let decoded = decode::<R4Provenance>(&provenance("", ""), Path::root("Provenance"))
        .expect("the complete Provenance decodes");
    assert_eq!(decoded.target.len(), 1);
    assert_eq!(decoded.agent.len(), 1);
}

#[test]
fn an_absent_required_complex_array_is_refused_at_its_element() {
    for element in ["target", "agent"] {
        let error = decode::<R4Provenance>(&provenance(element, ""), Path::root("Provenance"))
            .expect_err("a 1..* element is required");
        assert_eq!(error.kind, DecodeErrorKind::MissingProperty, "{element}");
        assert_eq!(error.path, format!("Provenance.{element}"));
    }
}

#[test]
fn an_empty_required_complex_array_is_refused_at_its_element() {
    let error = decode::<R4Provenance>(
        &provenance("target", r#""target":[]"#),
        Path::root("Provenance"),
    )
    .expect_err("an array is never empty");
    assert_eq!(error.kind, DecodeErrorKind::Empty);
    assert_eq!(error.path, "Provenance.target");
}

#[test]
fn an_absent_required_primitive_array_is_refused_at_its_element() {
    let error = decode::<R4CapabilityStatement>(
        &capability_statement(""),
        Path::root("CapabilityStatement"),
    )
    .expect_err("CapabilityStatement.format is 1..*");
    assert_eq!(error.kind, DecodeErrorKind::MissingProperty);
    assert_eq!(error.path, "CapabilityStatement.format");
}

#[test]
fn a_required_primitive_array_carried_by_its_sibling_alone_is_present() {
    let sibling = r#""_format":[{"extension":[{"url":"http://example.org/e","valueString":"x"}]}]"#;
    let decoded = decode::<R4CapabilityStatement>(
        &capability_statement(sibling),
        Path::root("CapabilityStatement"),
    )
    .expect("a primitive with only an extension is present");
    assert_eq!(decoded.format.len(), 1);
    let decoded = decode::<R4CapabilityStatement>(
        &capability_statement(r#""format":["json"]"#),
        Path::root("CapabilityStatement"),
    )
    .expect("one format meets the minimum");
    assert_eq!(decoded.format.len(), 1);
}

#[test]
fn a_lenient_path_reads_an_absent_required_array_as_empty() {
    let decoded = decode::<R4Provenance>(&provenance("target", ""), Path::lenient("Provenance"))
        .expect("the lenient reader keeps the rest of the resource");
    assert_eq!(
        decoded.target,
        Vec::<fhir_types::r4::reference::Reference>::new()
    );
}

/// The same refusal in every version, over that version's own `Provenance`,
/// whose `target` is `1..*` in STU3 to R6; `$who` is the agent's member, a
/// choice in STU3.
macro_rules! required_target_case {
    ($version:ident, $who:literal) => {
        #[test]
        fn $version() {
            let agent = format!(r#"[{{"{}":{{"reference":"Practitioner/example"}}}}]"#, $who);
            let text = format!(
                r#"{{"resourceType":"Provenance","recorded":"2026-10-17T12:00:00+02:00","agent":{agent}}}"#
            );
            let error =
                decode::<fhir_types::$version::provenance::Provenance>(&text, Path::root("Provenance"))
                    .expect_err("Provenance.target is 1..*");
            assert_eq!(error.kind, DecodeErrorKind::MissingProperty);
            assert_eq!(error.path, "Provenance.target");
        }
    };
}

mod every_version_refuses_a_provenance_without_target {
    use fhir_types::codec::{DecodeErrorKind, Path};

    use super::decode;

    required_target_case!(stu3, "whoReference");
    required_target_case!(r4, "who");
    required_target_case!(r4b, "who");
    required_target_case!(r5, "who");
    required_target_case!(r6, "who");
}
