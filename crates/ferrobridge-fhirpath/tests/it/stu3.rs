// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The functions FHIRPath 3.0.0 adds as Standard for Trial Use
//! (<http://hl7.org/fhirpath/STU3/>), behind
//! `Environment::with_fhirpath_stu3_functions`; without it the evaluator is N1
//! 2.0.0, the release FHIR R4 fhirpath.html §2.9 cites.

use ferrobridge_fhirpath::eval::error::EvalError;
use ferrobridge_fhirpath::eval::{Environment, Focus, STU3_FUNCTIONS};
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::value::{Collection, Item};

fn stu3(expression: &str) -> Result<Collection<'static>, EvalError> {
    Expression::parse(expression)
        .expect("the expression should parse")
        .evaluate(
            &Environment::new(&R4).with_fhirpath_stu3_functions(),
            Focus::none(),
        )
}

fn n1(expression: &str) -> Result<Collection<'static>, EvalError> {
    Expression::parse(expression)
        .expect("the expression should parse")
        .evaluate(&Environment::new(&R4), Focus::none())
}

fn text(expression: &str) -> Option<String> {
    match stu3(expression).expect("evaluates").as_slice() {
        [] => None,
        [Item::String(value)] => Some(value.clone()),
        other => panic!("expected one string, got {other:?}"),
    }
}

fn texts(expression: &str) -> Vec<String> {
    stu3(expression)
        .expect("evaluates")
        .into_iter()
        .map(|item| match item {
            Item::String(value) => value,
            other => panic!("expected strings, got {other:?}"),
        })
        .collect()
}

fn truth(expression: &str) -> Option<bool> {
    match stu3(expression).expect("evaluates").as_slice() {
        [] => None,
        [Item::Boolean(value)] => Some(*value),
        other => panic!("expected one Boolean, got {other:?}"),
    }
}

fn argument_error(expression: &str) -> bool {
    matches!(stu3(expression), Err(EvalError::Argument { .. }))
}

#[test]
fn every_stu3_function_is_undefined_without_the_opt_in() {
    // FHIR R4 fhirpath.html §2.9 cites FHIRPath release 2 (N1), which defines none of them.
    for name in STU3_FUNCTIONS {
        let expression = format!("'x'.{name}()");
        assert!(
            matches!(n1(&expression), Err(EvalError::UnknownFunction { name: found }) if found == name),
            "{expression}"
        );
    }
}

#[test]
fn trim_removes_the_whitespace_of_both_ends() {
    // FHIRPath 3.0.0 #trim--string: whitespace as the Whitespace lexical category defines it.
    assert_eq!(text("' 123456 '.trim()").as_deref(), Some("123456"));
    assert_eq!(text("'  '.trim()").as_deref(), Some(""));
    assert_eq!(text("'\\t a b\\r\\n'.trim()").as_deref(), Some("a b"));
    assert_eq!(text("{}.trim()"), None);
}

#[test]
fn split_answers_every_part_between_separators() {
    // FHIRPath 3.0.0 #splitseparator-string--collection.
    assert_eq!(texts("('A,B,C').split(',')"), ["A", "B", "C"]);
    assert_eq!(texts("('ABC').split(',')"), ["ABC"]);
    assert_eq!(texts("'A,,C'.split(',')"), ["A", "", "C"]);
    assert!(texts("{}.split(',')").is_empty());
}

#[test]
fn split_refuses_an_empty_separator() {
    // No specification governs an empty separator: our own design refuses it.
    assert!(argument_error("'abc'.split('')"));
}

#[test]
fn join_concatenates_with_the_separator() {
    // FHIRPath 3.0.0 #joinseparator-string--string.
    assert_eq!(text("('A' | 'B' | 'C').join()").as_deref(), Some("ABC"));
    assert_eq!(
        text("('A' | 'B' | 'C').join(',')").as_deref(),
        Some("A,B,C")
    );
    assert_eq!(text("{}.join(',')"), None);
}

#[test]
fn join_refuses_an_item_that_is_no_string() {
    // FHIRPath 3.0.0 #joinseparator-string--string: the input is a collection of strings.
    assert!(matches!(
        stu3("('A' | 1).join(',')"),
        Err(EvalError::Type { .. })
    ));
}

#[test]
fn encode_and_decode_round_trip_every_format() {
    // FHIRPath 3.0.0 #encodeformat--string--string and #decodeformat--string--string (RFC 4648).
    assert_eq!(text("'test'.encode('base64')").as_deref(), Some("dGVzdA=="));
    assert_eq!(text("'test'.encode('hex')").as_deref(), Some("74657374"));
    assert_eq!(
        text("'subjects?_d'.encode('urlbase64')").as_deref(),
        Some("c3ViamVjdHM_X2Q=")
    );
    assert_eq!(text("'dGVzdA=='.decode('base64')").as_deref(), Some("test"));
    assert_eq!(text("'74657374'.decode('hex')").as_deref(), Some("test"));
    assert_eq!(
        text("'c3ViamVjdHM_X2Q='.decode('urlbase64')").as_deref(),
        Some("subjects?_d")
    );
}

#[test]
fn encode_ascii_replaces_every_character_above_127() {
    // FHIRPath 3.0.0 #encodeformat--string--string: ascii is lossy.
    assert_eq!(text("'café'.encode('ascii')").as_deref(), Some("caf?"));
}

#[test]
fn decode_of_bytes_that_are_no_utf8_is_empty() {
    // FHIRPath 3.0.0 #decodeformat--string--string: not valid UTF-8, the result is empty.
    assert_eq!(text("'ff'.decode('hex')"), None);
}

#[test]
fn an_unknown_format_or_target_is_refused() {
    // FHIRPath 3.0.0 lists the formats and targets; no other is defined.
    assert!(argument_error("'test'.encode('rot13')"));
    assert!(argument_error("'test'.decode('ascii')"));
    assert!(argument_error("'test'.escape('xml')"));
    assert!(argument_error("'zz'.decode('hex')"));
}

#[test]
fn an_empty_format_answers_empty() {
    // FHIRPath 3.0.0: if no format or target is specified, the result is empty.
    assert_eq!(text("'test'.encode({})"), None);
    assert_eq!(text("'test'.escape({})"), None);
}

#[test]
fn escape_and_unescape_html_and_json() {
    // FHIRPath 3.0.0 #escapetarget--string--string and #unescapetarget--string--string.
    assert_eq!(
        text("'\"1<2\"'.escape('html')").as_deref(),
        Some("&quot;1&lt;2&quot;")
    );
    assert_eq!(text("'A&B'.escape('html')").as_deref(), Some("A&amp;B"));
    assert_eq!(
        text("'\"1<2\"'.escape('json')").as_deref(),
        Some("\\\"1<2\\\"")
    );
    assert_eq!(
        text("'&quot;1&lt;2&quot;'.unescape('html')").as_deref(),
        Some("\"1<2\"")
    );
    assert_eq!(
        text("'\\\\\"1<2\\\\\"'.unescape('json')").as_deref(),
        Some("\"1<2\"")
    );
}

#[test]
fn unescape_json_replaces_only_the_escapes() {
    // FHIRPath 3.0.0 #unescapetarget--string--string with the JSON string escapes (RFC 8259 §7).
    assert_eq!(
        text("'\\\"1<2\\\"'.unescape('json')").as_deref(),
        Some("\"1<2\"")
    );
    assert_eq!(
        text("'a\\\\u00e9\\\\n'.unescape('json')").as_deref(),
        Some("aé\n")
    );
    assert_eq!(
        text("'\\\\ud83d\\\\udd25'.unescape('json')").as_deref(),
        Some("\u{1f525}")
    );
    assert!(argument_error("'\\\\q'.unescape('json')"));
    assert!(argument_error("'\\\\ud83d'.unescape('json')"));
}

#[test]
fn matches_full_matches_the_whole_string() {
    // FHIRPath 3.0.0 #matchesfullregex--string-flags--string--boolean.
    let url = "'http://fhir.org/guides/cqf/common/Library/FHIR-ModelInfo|4.0.1'";
    assert_eq!(truth(&format!("{url}.matchesFull('Library')")), Some(false));
    assert_eq!(
        truth(&format!("{url}.matchesFull('.*Library.*')")),
        Some(true)
    );
    assert_eq!(truth("'N8000123123'.matchesFull('N[0-9]{8}')"), Some(false));
    assert_eq!(truth("'N8000123123'.matchesFull('N[0-9]{10}')"), Some(true));
    assert_eq!(truth("{}.matchesFull('a')"), None);
}

#[test]
fn matches_full_takes_the_i_and_m_flags_and_refuses_others() {
    // FHIRPath 3.0.0 #matchesfullregex--string-flags--string--boolean: flags i and m.
    assert_eq!(truth("'ABC'.matchesFull('abc', 'i')"), Some(true));
    assert_eq!(truth("'ABC'.matchesFull('abc')"), Some(false));
    assert!(argument_error("'ABC'.matchesFull('abc', 'x')"));
}
