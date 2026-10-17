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

/// The logical id of a resource takes the `id` form in every version.
///
/// The published definitions type `Resource.id` as `id`
/// (<https://hl7.org/fhir/R4/resource-definitions.html#Resource.id>), and R4
/// resource.html §2.26.3.3 limits an id to 64 ASCII letters, numerals, `-` and
/// `.`. The 4.0.1 package's `structuredefinition-fhir-type` extension names
/// `string` on `Resource.id` and every copy, and 6.0.0-ballot5 names `string`
/// on 123 of the copies its own `Resource.id` types `id`; the emitter follows
/// the definitions (#482).
#[test]
fn a_resource_id_takes_the_id_form_in_every_version() {
    let body = r#""id":"not a valid id!","#;
    for error in [
        decode::<fhir_types::stu3::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("3.0.2 types CodeSystem.id as id"),
        decode::<fhir_types::r4::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("R4 types CodeSystem.id as id"),
        decode::<fhir_types::r4b::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("4.3.0 types CodeSystem.id as id"),
        decode::<fhir_types::r5::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("5.0.0 types CodeSystem.id as id"),
        decode::<fhir_types::r6::code_system::CodeSystem>(&code_system(body), "CodeSystem")
            .expect_err("R6 types CodeSystem.id as id"),
    ] {
        assert_eq!(error.kind, DecodeErrorKind::BadValue);
        assert_eq!(error.path, "CodeSystem.id");
    }
    let bundle = r#"{"resourceType":"Bundle","id":"not a valid id!","type":"collection"}"#;
    for error in [
        decode::<fhir_types::r4::bundle::Bundle>(bundle, "Bundle").expect_err("R4 Bundle.id"),
        decode::<fhir_types::r6::bundle::Bundle>(bundle, "Bundle").expect_err("R6 Bundle.id"),
    ] {
        assert_eq!(error.kind, DecodeErrorKind::BadValue);
        assert_eq!(error.path, "Bundle.id");
    }
}

/// An id of 64 characters is inside the R4 `id` form and one of 65 is outside it
/// (<https://hl7.org/fhir/R4/datatypes.html#id>, `[A-Za-z0-9\-\.]{1,64}`).
#[test]
fn an_r4_resource_id_is_held_to_sixty_four_characters() {
    let longest = "a".repeat(64);
    let body = format!(r#""id":"{longest}","#);
    let decoded =
        decode::<fhir_types::r4::code_system::CodeSystem>(&code_system(&body), "CodeSystem")
            .expect("64 characters keep the id form");
    assert_eq!(decoded.id.as_deref(), Some(longest.as_str()));
    let body = format!(r#""id":"{longest}a","#);
    let error =
        decode::<fhir_types::r4::code_system::CodeSystem>(&code_system(&body), "CodeSystem")
            .expect_err("65 characters break the id form");
    assert_eq!(error.kind, DecodeErrorKind::BadValue);
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

/// Two element ids outside the `id` form: an element id naming a slice, as a
/// `StructureDefinition` snapshot carries one, and a 65-character value.
const ELEMENT_IDS: [&str; 2] = [
    "Patient.extension:birthPlace",
    "an-element-id-of-sixty-five-characters-which-the-id-form-refuses0",
];

/// `Element.id` and every element inheriting it take the `string` form.
///
/// <https://hl7.org/fhir/R4B/element-definitions.html#Element.id> and
/// <https://hl7.org/fhir/R5/types-definitions.html#Element.id> type
/// `Element.id` as `string` ("This may be any string value that does not
/// contain spaces"), while the 4.3.0 and 5.0.0 snapshots state `id` on the
/// copies `ElementDefinition.id` and `Extension.id` inherit; the emitter
/// follows the definitions (#672). A resource id keeps its own form.
macro_rules! element_id_case {
    ($version:ident, $name:ident) => {
        mod $name {
            use fhir_types::codec::DecodeErrorKind;
            use fhir_types::$version::code_system::CodeSystem;
            use fhir_types::$version::element_definition::ElementDefinition;
            use fhir_types::$version::extension::Extension;

            use super::{ELEMENT_IDS, code_system, decode};

            #[test]
            fn an_element_definition_id_takes_the_string_form() {
                for id in ELEMENT_IDS {
                    let text = format!(r#"{{"id":"{id}","path":"Patient.extension"}}"#);
                    let decoded = decode::<ElementDefinition>(&text, "ElementDefinition")
                        .expect("ElementDefinition.id inherits the string Element.id");
                    assert_eq!(decoded.id.as_deref(), Some(id));
                }
            }

            #[test]
            fn an_extension_id_takes_the_string_form() {
                for id in ELEMENT_IDS {
                    let text = format!(r#"{{"id":"{id}","url":"http://example.org/e"}}"#);
                    let decoded = decode::<Extension>(&text, "Extension")
                        .expect("Extension.id inherits the string Element.id");
                    assert_eq!(decoded.id.as_deref(), Some(id));
                }
            }

            #[test]
            fn a_resource_id_still_takes_the_id_form() {
                for id in ELEMENT_IDS {
                    let body = format!(r#""id":"{id}","#);
                    let error = decode::<CodeSystem>(&code_system(&body), "CodeSystem")
                        .expect_err("CodeSystem.id derives from Resource.id, typed id");
                    assert_eq!(error.kind, DecodeErrorKind::BadValue);
                    assert_eq!(error.path, "CodeSystem.id");
                }
            }
        }
    };
}

element_id_case!(r4b, r4b_element_id);
element_id_case!(r5, r5_element_id);
element_id_case!(r4, r4_element_id);
element_id_case!(r6, r6_element_id);

/// 4.0.1 already types `Element.id` and its copies as `string`, so both
/// element ids decode as before.
#[test]
fn r4_element_ids_keep_the_string_form() {
    for id in ELEMENT_IDS {
        let text = format!(r#"{{"id":"{id}","path":"Patient.extension"}}"#);
        let decoded = decode::<fhir_types::r4::element_definition::ElementDefinition>(
            &text,
            "ElementDefinition",
        )
        .expect("4.0.1 types ElementDefinition.id as string");
        assert_eq!(decoded.id.as_deref(), Some(id));
        let text = format!(r#"{{"id":"{id}","url":"http://example.org/e"}}"#);
        let decoded = decode::<fhir_types::r4::extension::Extension>(&text, "Extension")
            .expect("4.0.1 types Extension.id as string");
        assert_eq!(decoded.id.as_deref(), Some(id));
    }
}
