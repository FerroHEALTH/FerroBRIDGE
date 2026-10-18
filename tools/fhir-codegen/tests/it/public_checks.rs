// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The public primitive checks: each string primitive's `check` over its
//! lexical form, and `check_value` over the primitive a type code names
//! (<https://hl7.org/fhir/R4/datatypes.html#primitive>).

use fhir_types::codec::{DecodeErrorKind, Value};
use fhir_types::r4::primitives::{DateTime, Id, Url, check_value};

fn text(text: &str) -> Value {
    Value::String(text.to_owned())
}

/// The kind of the refusal `check_value` answers, `None` when it accepts the
/// value or knows no primitive `code`.
fn refused(code: &str, value: &Value) -> Option<DecodeErrorKind> {
    check_value(code, value)
        .and_then(Result::err)
        .map(|error| error.kind)
}

#[test]
fn a_primitive_s_check_holds_text_to_its_lexical_form() {
    assert_eq!(Url::check("urn:oid:1.2.3"), Ok(()));
    let error = Url::check("North Lab App").expect_err("a url holds no space");
    assert_eq!(error.type_code, "url");
    assert_eq!(Id::check("a-b.c"), Ok(()));
    assert_eq!(Id::check("a b").map_err(|error| error.type_code), Err("id"));
    assert_eq!(DateTime::check("2001-06-15"), Ok(()));
    assert!(DateTime::check("yesterday").is_err());
}

#[test]
fn a_check_reads_its_own_version_s_form() {
    assert!(
        fhir_types::r5::primitives::DateTime::check("2001-06-15T12:00:00").is_ok(),
        "5.0.0 makes the offset optional"
    );
    assert!(
        DateTime::check("2001-06-15T12:00:00").is_err(),
        "4.0.1 requires it"
    );
}

#[test]
fn check_value_holds_a_string_primitive_to_its_form() {
    assert_eq!(check_value("url", &text("urn:oid:1.2.3")), Some(Ok(())));
    let error = check_value("url", &text("North Lab App"))
        .and_then(Result::err)
        .expect("a url holds no space");
    assert_eq!(error.kind, DecodeErrorKind::BadValue);
    assert_eq!(error.path, "url");
    assert_eq!(
        refused("date", &text("1980-00-00")),
        Some(DecodeErrorKind::BadValue)
    );
}

#[test]
fn check_value_holds_a_number_and_a_boolean_to_their_json_type_and_range() {
    assert!(matches!(
        refused("integer", &text("12")),
        Some(DecodeErrorKind::WrongType { .. })
    ));
    assert_eq!(
        check_value("positiveInt", &Value::from(1_u32)),
        Some(Ok(()))
    );
    assert_eq!(
        refused("positiveInt", &Value::from(0_u32)),
        Some(DecodeErrorKind::BadNumber)
    );
    assert_eq!(check_value("boolean", &Value::Bool(true)), Some(Ok(())));
}

#[test]
fn check_value_knows_only_its_own_version_s_primitives() {
    assert_eq!(check_value("Reference", &text("x")), None, "no primitive");
    assert_eq!(check_value("integer64", &text("1")), None, "R5 adds it");
    assert_eq!(
        fhir_types::r5::primitives::check_value("integer64", &text("1")),
        Some(Ok(()))
    );
}
