// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `\` escape and the split (search.html §3.1.1.4.19).

use fhir_search::escape::{EscapeError, escape, split, unescape};
use proptest::prelude::*;

#[test]
fn an_unescaped_comma_separates_alternatives() {
    // GET [base]/Observation?code=a,b asks for a code of a or b.
    assert_eq!(split("a,b", ','), ["a", "b"]);
}

#[test]
fn an_escaped_comma_is_part_of_the_value() {
    // GET [base]/Observation?code=a\,b asks for the code a,b.
    let parts = split(r"a\,b", ',');
    assert_eq!(parts, [r"a\,b"], "the escape is kept for unescape");
    assert_eq!(unescape(&parts[0]), Ok(String::from("a,b")));
}

#[test]
fn the_value_set_url_examples_read_as_the_specification_says() {
    let parts = split(
        r"http://acme.org/fhir/ValueSet/123,http://acme.org/fhir/ValueSet/124\,125",
        ',',
    );
    let plain: Vec<String> = parts.iter().map(|part| unescape(part).unwrap()).collect();
    assert_eq!(
        plain,
        [
            "http://acme.org/fhir/ValueSet/123",
            "http://acme.org/fhir/ValueSet/124,125"
        ]
    );
    let three = split(
        "http://acme.org/fhir/ValueSet/123,http://acme.org/fhir/ValueSet/124,ValueSet/125",
        ',',
    );
    assert_eq!(three.len(), 3, "a decoded %2C separates like a comma");
}

#[test]
fn a_dollar_escape_is_a_literal_dollar() {
    // param=xx\$xx has the literal value xx$xx.
    assert_eq!(unescape(r"xx\$xx"), Ok(String::from("xx$xx")));
    assert_eq!(split(r"xx\$xx", '$'), [r"xx\$xx"], "no composite split");
    assert_eq!(split("xx$xx", '$'), ["xx", "xx"], "a composite");
}

#[test]
fn an_escaped_backslash_is_one_backslash() {
    // param=xx\\xx indicates a literal value of xx\xx.
    assert_eq!(unescape(r"xx\\xx"), Ok(String::from(r"xx\xx")));
}

#[test]
fn an_escaped_bar_is_a_literal_bar() {
    assert_eq!(unescape(r"a\|b"), Ok(String::from("a|b")));
}

#[test]
fn a_backslash_before_any_other_character_is_illegal() {
    // The parameter value xx\xx is illegal.
    assert_eq!(unescape(r"xx\xx"), Err(EscapeError::Illegal { at: 2 }));
    assert_eq!(unescape(r"\n"), Err(EscapeError::Illegal { at: 0 }));
    assert_eq!(unescape(r"a\ b"), Err(EscapeError::Illegal { at: 1 }));
}

#[test]
fn a_trailing_backslash_is_illegal() {
    assert_eq!(unescape("abc\\"), Err(EscapeError::Trailing));
    assert_eq!(unescape("\\"), Err(EscapeError::Trailing));
    assert_eq!(
        unescape(r"a\\\"),
        Err(EscapeError::Trailing),
        "the third escapes nothing"
    );
}

#[test]
fn an_escaped_backslash_before_a_comma_still_splits() {
    assert_eq!(split(r"a\\,b", ','), [r"a\\", "b"]);
    assert_eq!(
        split(r"a\\\,b", ','),
        [r"a\\\,b"],
        "the third escapes the comma"
    );
}

#[test]
fn empty_parts_are_kept_for_the_reader() {
    assert_eq!(split("", ','), [""]);
    assert_eq!(split(",", ','), ["", ""]);
    assert_eq!(split("a,,b,", ','), ["a", "", "b", ""]);
}

#[test]
fn a_backslash_before_a_multibyte_character_is_illegal_and_names_its_byte() {
    assert_eq!(unescape("é\\é"), Err(EscapeError::Illegal { at: 2 }));
    assert_eq!(split("é\\,😀,ü", ','), ["é\\,😀", "ü"]);
}

#[test]
fn a_value_of_only_escapes_reads_whole() {
    let text = r"\\".repeat(10_000);
    assert_eq!(unescape(&text).map(|plain| plain.len()), Ok(10_000));
    assert_eq!(split(&text, ',').len(), 1);
}

#[test]
fn escape_writes_every_special_character() {
    assert_eq!(escape(r"a\b,c$d|e"), r"a\\b\,c\$d\|e");
    assert_eq!(escape("plain é"), "plain é");
}

#[test]
fn the_error_never_carries_the_value() {
    let shown = unescape(r"secret-mrn-4711\x").unwrap_err().to_string();
    assert!(!shown.contains("4711"), "{shown}");
}

proptest! {
    #[test]
    fn an_escaped_value_survives_the_split_and_unescape(value in any::<String>()) {
        let parts = split(&escape(&value), ',');
        prop_assert_eq!(parts.len(), 1);
        prop_assert_eq!(unescape(&parts[0]), Ok(value));
    }

    #[test]
    fn escaped_alternatives_split_back_apart(values in proptest::collection::vec(any::<String>(), 1..6)) {
        let joined = values.iter().map(|value| escape(value)).collect::<Vec<_>>().join(",");
        let read: Result<Vec<String>, EscapeError> =
            split(&joined, ',').iter().map(|part| unescape(part)).collect();
        prop_assert_eq!(read, Ok(values));
    }

    #[test]
    fn split_and_unescape_never_panic(text in any::<String>(), separator in prop::sample::select(vec![',', '|', '$'])) {
        for part in split(&text, separator) {
            let _outcome: Result<String, EscapeError> = unescape(&part);
        }
    }
}
