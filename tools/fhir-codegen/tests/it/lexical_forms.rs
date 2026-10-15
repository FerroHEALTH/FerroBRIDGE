// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The lexical form the generated codec holds every primitive value to.
//!
//! A form is the `regex` extension of the primitive's `value` element in that
//! version's own package, anchored to the whole value
//! (<https://hl7.org/fhir/R5/datatypes.html#primitive>: a form is published per
//! type and is qualified with the anchors of the engine reading it). The
//! packages disagree between versions, so every case below is asserted per
//! version rather than shared.

use fhir_types::codec::{DecodeError, DecodeErrorKind, Json, Value, expect_object};

/// Decodes `text` as `T`, reporting every path from `root`.
fn decode<T: Json>(text: &str, root: &str) -> Result<T, DecodeError> {
    let value: Value = serde_json::from_str(text).expect("the fixture is JSON");
    let mut path = fhir_types::codec::Path::root(root);
    let object = expect_object(&value, &path).expect("the fixture is an object");
    T::from_json(object, &mut path)
}

/// A synthetic `CodeSystem` carrying `body` beside the two elements every
/// version requires, `status` and `content`.
fn code_system(body: &str) -> String {
    format!(r#"{{"resourceType":"CodeSystem",{body}"status":"active","content":"complete"}}"#)
}

/// The same five cases per version, over that version's own `CodeSystem`.
macro_rules! lexical_form_cases {
    ($version:ident) => {
        mod $version {
            use fhir_types::$version::code_system::CodeSystem;
            use fhir_types::codec::DecodeErrorKind;

            use super::{code_system, decode};

            #[test]
            fn a_date_time_outside_its_form_is_refused() {
                let error = decode::<CodeSystem>(&code_system(r#""date":"yesterday","#), "CodeSystem")
                    .expect_err("yesterday is no dateTime");
                assert_eq!(error.kind, DecodeErrorKind::BadValue);
                assert_eq!(error.path, "CodeSystem.date");
            }

            #[test]
            fn every_date_time_precision_the_form_admits_decodes() {
                for value in [
                    "2001",
                    "2001-06",
                    "2001-06-15",
                    "2001-06-15T12:00:00.123+02:00",
                ] {
                    let text = code_system(&format!(r#""date":"{value}","#));
                    let decoded = decode::<CodeSystem>(&text, "CodeSystem")
                        .unwrap_or_else(|error| panic!("{value} decodes: {error}"));
                    assert_eq!(
                        decoded.date.and_then(|date| date.value).as_deref(),
                        Some(value),
                        "the value survives decoding"
                    );
                }
            }

            #[test]
            fn a_property_value_outside_its_form_is_refused() {
                let concept = r#""concept":[{"code":"a","property":[{"code":"p","valueDateTime":"2001-6"}]}],"#;
                let error = decode::<CodeSystem>(&code_system(concept), "CodeSystem")
                    .expect_err("2001-6 is no dateTime: the month is two digits");
                assert_eq!(error.kind, DecodeErrorKind::BadValue);
                // A choice element reports the path of the element, not of the
                // type-suffixed key it arrived under
                // (<https://hl7.org/fhir/R4B/formats.html#choice>).
                assert_eq!(error.path, "CodeSystem.concept[0].property[0].value");
            }

            #[test]
            fn a_code_with_a_leading_space_is_refused() {
                let text = format!(
                    r#"{{"resourceType":"CodeSystem","status":" active","content":"complete"}}"#
                );
                let error = decode::<CodeSystem>(&text, "CodeSystem")
                    .expect_err("a code carries no leading whitespace");
                assert_eq!(error.kind, DecodeErrorKind::BadValue);
                assert_eq!(error.path, "CodeSystem.status");
            }
        }
    };
}

/// The `uri` case per version whose package publishes a form for `uri.value`.
macro_rules! uri_form_case {
    ($version:ident, $module:ident) => {
        mod $module {
            use fhir_types::codec::DecodeErrorKind;
            use fhir_types::$version::code_system::CodeSystem;

            use super::{code_system, decode};

            #[test]
            fn a_uri_holding_a_space_is_refused() {
                let error = decode::<CodeSystem>(
                    &code_system(r#""url":"http://example.org/a b","#),
                    "CodeSystem",
                )
                .expect_err("a uri holds no whitespace");
                assert_eq!(error.kind, DecodeErrorKind::BadValue);
                assert_eq!(error.path, "CodeSystem.url");
            }
        }
    };
}

lexical_form_cases!(stu3);
lexical_form_cases!(r4);
lexical_form_cases!(r4b);
lexical_form_cases!(r5);
lexical_form_cases!(r6);
uri_form_case!(r4, r4_uri);
uri_form_case!(r4b, r4b_uri);
uri_form_case!(r5, r5_uri);
uri_form_case!(r6, r6_uri);

/// 3.0.2 publishes no lexical form for `uri.value`, so the STU3 codec holds a
/// `uri` to none: the form is read from each version's own package.
#[test]
fn stu3_publishes_no_uri_form() {
    let decoded = decode::<fhir_types::stu3::code_system::CodeSystem>(
        &code_system(r#""url":"http://example.org/a b","#),
        "CodeSystem",
    )
    .expect("hl7.fhir.r3.core 3.0.2 states no structuredefinition-regex on uri.value");
    assert_eq!(
        decoded.url.and_then(|url| url.value).as_deref(),
        Some("http://example.org/a b")
    );
}

/// The resource id keeps the form the element's own type names, per package.
///
/// The `structuredefinition-fhir-type` extension of a resource's `id` element
/// names `id` in 4.3.0 and 5.0.0 and `string` in 4.0.1; 6.0.0-ballot5 names
/// `string` on `CodeSystem.id` and `id` on `Bundle.id`; 3.0.2 types
/// `CodeSystem.id` with the code `id` itself. The emitter reads the
/// element, so each version refuses exactly what its own package describes.
#[test]
fn a_resource_id_keeps_the_form_its_package_states() {
    let body = r#""id":"not a valid id!","#;
    for error in [
        decode::<fhir_types::r4b::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("4.3.0 types CodeSystem.id as id"),
        decode::<fhir_types::r5::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("5.0.0 types CodeSystem.id as id"),
        decode::<fhir_types::stu3::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("3.0.2 types CodeSystem.id as id"),
    ] {
        assert_eq!(error.kind, DecodeErrorKind::BadValue);
        assert_eq!(error.path, "CodeSystem.id");
    }
    let r4 = decode::<fhir_types::r4::code_system::CodeSystem>(&code_system(body), "CodeSystem")
        .expect("4.0.1 types CodeSystem.id as string");
    assert_eq!(r4.id.as_deref(), Some("not a valid id!"));
    let r6 = decode::<fhir_types::r6::code_system::CodeSystem>(&code_system(body), "CodeSystem")
        .expect("6.0.0-ballot5 types CodeSystem.id as string");
    assert_eq!(r6.id.as_deref(), Some("not a valid id!"));
    let bundle = r#"{"resourceType":"Bundle","id":"not a valid id!","type":"collection"}"#;
    let error = decode::<fhir_types::r6::bundle::Bundle>(bundle, "Bundle")
        .expect_err("6.0.0-ballot5 types Bundle.id as id");
    assert_eq!(error.kind, DecodeErrorKind::BadValue);
    assert_eq!(error.path, "Bundle.id");
}

/// A `dateTime` carrying a time without an offset, as each package states it.
///
/// 3.0.2, 4.0.1, 4.3.0 and 6.0.0-ballot5 put the offset inside the time group of the
/// `dateTime` form, so a time without one is outside the form; 5.0.0 moves the
/// offset out of that group and makes it optional, so the same value keeps the
/// 5.0.0 form.
#[test]
fn a_time_without_an_offset_follows_each_package() {
    let body = r#""date":"2001-06-15T12:00:00","#;
    for error in [
        decode::<fhir_types::stu3::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("3.0.2 requires the offset"),
        decode::<fhir_types::r4::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("4.0.1 requires the offset"),
        decode::<fhir_types::r4b::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("4.3.0 requires the offset"),
        decode::<fhir_types::r6::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("6.0.0-ballot5 requires the offset"),
    ] {
        assert_eq!(error.kind, DecodeErrorKind::BadValue);
        assert_eq!(error.path, "CodeSystem.date");
    }
    let r5 = decode::<fhir_types::r5::code_system::CodeSystem>(&code_system(body), "CodeSystem")
        .expect("5.0.0 makes the offset optional");
    assert_eq!(
        r5.date.and_then(|date| date.value).as_deref(),
        Some("2001-06-15T12:00:00")
    );
}
