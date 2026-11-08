// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The token value (search.html §3.1.1.4.10).

use fhir_search::escape::EscapeError;
use fhir_search::token::{OfType, System, Token, TokenError};

#[test]
fn a_bare_code_matches_in_any_system() {
    // GET [base]/Patient?gender=male
    let token = Token::parse("male").unwrap();
    assert_eq!(token, Token::Code(String::from("male")));
    assert_eq!(token.system(), System::Any);
    assert_eq!(token.code(), Some("male"));
}

#[test]
fn a_system_and_code_match_exactly() {
    // GET [base]/Patient?identifier=http://acme.org/patient|2345
    let token = Token::parse("http://acme.org/patient|2345").unwrap();
    assert_eq!(token.system(), System::Is("http://acme.org/patient"));
    assert_eq!(token.code(), Some("2345"));
    let condition = Token::parse("http://acme.org/conditions/codes|ha125").unwrap();
    assert_eq!(condition.code(), Some("ha125"));
}

#[test]
fn a_leading_bar_asks_for_no_system() {
    // identifier=|[root],[root] searches a CDA II.root.
    let token = Token::parse("|2.16.840.1.113883.19.5").unwrap();
    assert_eq!(
        token,
        Token::NoSystem(String::from("2.16.840.1.113883.19.5"))
    );
    assert_eq!(token.system(), System::Absent);
}

#[test]
fn a_trailing_bar_asks_for_any_code_of_the_system() {
    let token = Token::parse("http://snomed.info/sct|").unwrap();
    assert_eq!(token.system(), System::Is("http://snomed.info/sct"));
    assert_eq!(token.code(), None);
}

#[test]
fn an_escaped_bar_belongs_to_its_part() {
    let token = Token::parse(r"urn:oid:2.999.1|a\|1").unwrap();
    assert_eq!(token.system(), System::Is("urn:oid:2.999.1"));
    assert_eq!(token.code(), Some("a|1"));
    assert_eq!(Token::parse(r"a\|b").unwrap().code(), Some("a|b"));
    assert_eq!(
        Token::parse(r"urn:x\,y|c\,d").unwrap(),
        Token::SystemCode {
            system: String::from("urn:x,y"),
            code: String::from("c,d")
        }
    );
}

#[test]
fn a_token_naming_nothing_is_refused() {
    assert_eq!(Token::parse(""), Err(TokenError::Empty));
    assert_eq!(Token::parse("|"), Err(TokenError::Empty));
}

#[test]
fn more_than_one_unescaped_bar_is_refused() {
    assert_eq!(Token::parse("a|b|c"), Err(TokenError::Bars));
    assert_eq!(Token::parse("||"), Err(TokenError::Bars));
    assert_eq!(Token::parse("|||||||"), Err(TokenError::Bars));
}

#[test]
fn an_illegal_escape_in_either_part_is_refused() {
    assert_eq!(
        Token::parse(r"urn:x|a\b"),
        Err(TokenError::Escape(EscapeError::Illegal { at: 1 }))
    );
    assert_eq!(
        Token::parse("urn:x\\"),
        Err(TokenError::Escape(EscapeError::Trailing))
    );
}

#[test]
fn of_type_reads_its_three_parts() {
    // identifier:of-type=http://terminology.hl7.org/CodeSystem/v2-0203|MR|446053
    let value = OfType::parse("http://terminology.hl7.org/CodeSystem/v2-0203|MR|446053").unwrap();
    assert_eq!(
        value.system(),
        "http://terminology.hl7.org/CodeSystem/v2-0203"
    );
    assert_eq!(value.code(), "MR");
    assert_eq!(value.value(), "446053");
}

#[test]
fn of_type_needs_all_three_parts() {
    assert_eq!(OfType::parse("urn:x|MR"), Err(TokenError::OfTypeParts));
    assert_eq!(OfType::parse("urn:x|MR|1|2"), Err(TokenError::OfTypeParts));
    assert_eq!(OfType::parse("urn:x||1"), Err(TokenError::OfTypeEmpty));
    assert_eq!(OfType::parse("|MR|1"), Err(TokenError::OfTypeEmpty));
    assert_eq!(OfType::parse("urn:x|MR|"), Err(TokenError::OfTypeEmpty));
    assert_eq!(OfType::parse(r"urn:x|MR|a\|b").unwrap().value(), "a|b");
}

#[test]
fn debug_never_shows_a_code() {
    for text in ["4711", "urn:x|4711", "|4711"] {
        let shown = format!("{:?}", Token::parse(text).unwrap());
        assert!(!shown.contains("4711"), "{shown}");
    }
    let shown = format!("{:?}", OfType::parse("urn:x|MR|4711").unwrap());
    assert!(!shown.contains("4711"), "{shown}");
    assert!(shown.contains("MR"), "the type is no identifier: {shown}");
}
