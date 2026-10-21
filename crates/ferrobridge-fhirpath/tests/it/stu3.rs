// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The functions FHIRPath 3.0.0 adds as Standard for Trial Use
//! (<http://hl7.org/fhirpath/STU3/>), behind
//! `Environment::with_fhirpath_stu3_functions`; without it the evaluator is N1
//! 2.0.0, the release FHIR R4 fhirpath.html §2.9 cites.

use ferrobridge_fhirpath::ast::Expr;
use ferrobridge_fhirpath::eval::error::EvalError;
use ferrobridge_fhirpath::eval::{Environment, Focus, STU3_FUNCTIONS};
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::named_type;
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
    assert_eq!(texts("{}.split(',')"), Vec::<String>::new());
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

/// The one item an evaluation answers, rendered.
fn shown(expression: &str) -> Option<String> {
    match stu3(expression).expect("evaluates").as_slice() {
        [] => None,
        [item] => Some(item.to_string()),
        other => panic!("expected one item, got {other:?}"),
    }
}

#[test]
fn a_decimal_boundary_is_half_a_unit_of_its_last_place_away() {
    // FHIRPath 3.0.0 #lowboundaryprecision-integer-decimal--date--datetime--time and
    // #highboundaryprecision-integer-decimal--date--datetime--time, the examples.
    for (expression, expected) in [
        ("1.587.lowBoundary()", "1.58650000"),
        ("1.587.lowBoundary(6)", "1.586500"),
        ("1.587.lowBoundary(2)", "1.58"),
        ("1.587.lowBoundary(0)", "1"),
        ("(-1.587).lowBoundary()", "-1.58750000"),
        ("(-1.587).lowBoundary(2)", "-1.59"),
        ("(-1.587).lowBoundary(0)", "-2"),
        ("1.587.highBoundary()", "1.58750000"),
        ("1.587.highBoundary(2)", "1.59"),
        ("1.587.highBoundary(0)", "2"),
        ("(-1.587).highBoundary(6)", "-1.586500"),
        ("(-1.587).highBoundary(2)", "-1.58"),
        ("(-1.587).highBoundary(0)", "-1"),
        ("120.lowBoundary(2)", "119.50"),
    ] {
        assert_eq!(shown(expression).as_deref(), Some(expected), "{expression}");
    }
}

#[test]
fn a_boundary_of_the_last_place_rounds_away_from_the_value() {
    // FHIRPath 3.0.0: 1.587.highBoundary(2) is 1.59, so 0.0034 (0.00335 to 0.00345) is
    // 0.1 to one place at the high end and -0.0034 is -0.1 at the low end.
    assert_eq!(shown("0.0034.highBoundary(1)").as_deref(), Some("0.1"));
    assert_eq!(shown("(-0.0034).lowBoundary(1)").as_deref(), Some("-0.1"));
    assert_eq!(shown("0.0034.lowBoundary(1)").as_deref(), Some("0.0"));
}

#[test]
fn a_precision_beyond_the_greatest_held_or_below_zero_is_empty() {
    // FHIRPath 3.0.0: a precision greater than the implementation's maximum is empty.
    assert_eq!(shown("1.587.lowBoundary(39)"), None);
    assert_eq!(shown("1.587.highBoundary(29)"), None);
    assert_eq!(shown("1.587.lowBoundary(-1)"), None);
}

#[test]
fn a_date_and_time_boundary_fills_the_missing_components() {
    // FHIRPath 3.0.0 lowBoundary() and highBoundary(), the date and time examples; the
    // offset stays the value's own, so an hour without one answers none.
    for (expression, expected) in [
        ("@2014.lowBoundary(6)", "2014-01"),
        ("@2014.highBoundary(6)", "2014-12"),
        ("@2014-01-01T08.lowBoundary(17)", "2014-01-01T08:00:00.000"),
        ("@2014-01-01T08.highBoundary(17)", "2014-01-01T08:59:59.999"),
        (
            "@2014-01-01T08:05+08:00.lowBoundary(17)",
            "2014-01-01T08:05:00.000+08:00",
        ),
        ("@2014-01-01T08.lowBoundary(8)", "2014-01-01"),
        ("@T10:30.lowBoundary(9)", "10:30:00.000"),
        ("@T10:30.highBoundary(9)", "10:30:59.999"),
    ] {
        assert_eq!(shown(expression).as_deref(), Some(expected), "{expression}");
    }
}

#[test]
fn a_quantity_has_no_boundary() {
    // FHIRPath 3.0.0: the function can only be used with Decimal, Date, DateTime and Time.
    assert!(matches!(
        stu3("1.587 'cm'.lowBoundary(8)"),
        Err(EvalError::Type { .. })
    ));
}

#[test]
fn precision_counts_the_digits_as_written() {
    // FHIRPath 3.0.0 #precision--integer, the examples.
    for (expression, expected) in [
        ("1.58700.precision()", "5"),
        ("100.precision()", "0"),
        ("@2014.precision()", "4"),
        ("@2014-01-05T10:30:00.000.precision()", "17"),
        ("@T10:30.precision()", "4"),
        ("@T10:30:00.000.precision()", "9"),
    ] {
        assert_eq!(shown(expression).as_deref(), Some(expected), "{expression}");
    }
}

#[test]
fn comparable_answers_whether_the_units_are_commensurable() {
    // FHIRPath 3.0.0 #comparableother--quantity--boolean.
    assert_eq!(truth("1 'cm'.comparable(1 '[in_i]')"), Some(true));
    assert_eq!(truth("1 'cm'.comparable(1 's')"), Some(false));
    assert_eq!(truth("1 'cm'.comparable(1 '[s]')"), Some(false));
    assert_eq!(truth("1 'cm'.comparable({})"), None);
    assert_eq!(truth("1 'cm'.comparable(1)"), None);
}

fn all_shown(expression: &str) -> Vec<String> {
    stu3(expression)
        .expect("evaluates")
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn sort_orders_by_value_or_by_its_keys() {
    // FHIRPath 3.0.0 #sortkeyselector-this--any-asc--desc--keyselector-this--any-asc--desc---collection,
    // the examples.
    assert_eq!(all_shown("(3 | 1 | 2).sort()"), ["1", "2", "3"]);
    assert_eq!(all_shown("(3 | 1 | 2).sort($this)"), ["1", "2", "3"]);
    assert_eq!(all_shown("(3 | 1 | 2).sort($this desc)"), ["3", "2", "1"]);
    assert_eq!(all_shown("('c' | 'a' | 'b').sort()"), ["a", "b", "c"]);
    assert_eq!(
        all_shown("('c' | 'a' | 'b').sort($this desc)"),
        ["c", "b", "a"]
    );
    assert_eq!(all_shown("('3' | '1' | '10').sort()"), ["1", "10", "3"]);
    assert_eq!(all_shown("(3 | 1 | 2).sort($this asc)"), ["1", "2", "3"]);
}

#[test]
fn sort_takes_later_keys_only_on_a_tie() {
    // FHIRPath 3.0.0 sort(): a later key decides only where the earlier keys are equal.
    assert_eq!(
        all_shown("('b2' | 'a1' | 'b1' | 'a2').sort(substring(0, 1) desc, substring(1, 1))"),
        ["b1", "b2", "a1", "a2"]
    );
}

#[test]
fn sort_puts_an_empty_key_first() {
    // FHIRPath 3.0.0 sort(): an empty value is lower than all other values.
    assert_eq!(
        all_shown("('bb' | 'a' | 'ccc').sort(iif($this = 'a', {}, length()))"),
        ["a", "bb", "ccc"]
    );
}

#[test]
fn sort_refuses_a_key_of_several_items_and_mixed_types() {
    // FHIRPath 3.0.0 sort(): a key of more than one item, and incompatible types, are errors.
    assert!(matches!(
        stu3("(1 | 2).sort(1 | 2)"),
        Err(EvalError::Singleton { .. })
    ));
    assert!(matches!(
        stu3("(1 | 'a').sort()"),
        Err(EvalError::Type { .. })
    ));
}

#[test]
fn a_sort_key_carries_its_written_direction() {
    // FHIRPath 3.0.0 grammar: `'sort' '(' (sortArgument (',' sortArgument)*)? ')'`.
    let parsed = Expression::parse("name.sort(family desc, given asc, text)").expect("parses");
    let Expr::Call { name, args, .. } = parsed.tree() else {
        panic!("a call, got {:?}", parsed.tree());
    };
    assert_eq!(name, "sort");
    assert!(matches!(
        args.as_slice(),
        [
            Expr::SortKey {
                descending: true,
                ..
            },
            Expr::SortKey {
                descending: false,
                ..
            },
            Expr::Member { .. },
        ]
    ));
    assert!(matches!(
        Expression::parse("sort").expect("parses").tree(),
        Expr::Member { .. }
    ));
}

#[test]
fn a_sort_key_is_checked_against_the_items_it_sorts() {
    // FHIRPath 3.0.0 sort() is a scoped function: each key selector sees an item as $this.
    let patient = named_type(&R4, "Patient").expect("Patient");
    let parsed = Expression::parse("Patient.name.sort(family desc)").expect("parses");
    assert!(parsed.check(&R4, patient).is_ok());
    let wrong = Expression::parse("Patient.name.sort(gender)").expect("parses");
    assert!(wrong.check(&R4, patient).is_err());
}

#[test]
fn a_descending_string_key_is_desc_never_unary_minus() {
    // FHIRPath 3.0.0 grammar: sortArgument is `expression ('asc' | 'desc')?`, and N1 §6.6
    // defines unary minus on numbers and quantities alone.
    assert!(matches!(
        stu3("('a' | 'b').sort(-$this)"),
        Err(EvalError::Type { .. })
    ));
}
