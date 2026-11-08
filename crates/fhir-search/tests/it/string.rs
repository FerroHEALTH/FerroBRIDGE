// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The string match (search.html §3.1.1.4.8).

use fhir_search::string::{StringMatch, fold};

#[test]
fn the_modifier_picks_the_match() {
    assert_eq!(StringMatch::from_modifier(None), Some(StringMatch::Start));
    assert_eq!(
        StringMatch::from_modifier(Some("contains")),
        Some(StringMatch::Contains)
    );
    assert_eq!(
        StringMatch::from_modifier(Some("exact")),
        Some(StringMatch::Exact)
    );
    assert_eq!(StringMatch::from_modifier(Some("text")), None);
    assert_eq!(StringMatch::from_modifier(Some("missing")), None);
    assert_eq!(
        StringMatch::from_modifier(Some("Exact")),
        None,
        "modifiers are case sensitive"
    );
    for matching in [
        StringMatch::Start,
        StringMatch::Contains,
        StringMatch::Exact,
    ] {
        assert_eq!(
            StringMatch::from_modifier(matching.modifier()),
            Some(matching)
        );
    }
}

#[test]
fn given_eve_starts_eve_and_evelyn() {
    // [base]/Patient?given=eve includes "Eve" and "Evelyn".
    for field in ["Eve", "Evelyn"] {
        assert!(StringMatch::Start.matches(field, "eve"), "{field}");
    }
    assert!(!StringMatch::Start.matches("Severine", "eve"));
}

#[test]
fn given_contains_eve_also_finds_severine() {
    // [base]/Patient?given:contains=eve also includes "Severine".
    for field in ["Eve", "Evelyn", "Severine"] {
        assert!(StringMatch::Contains.matches(field, "eve"), "{field}");
    }
    assert!(!StringMatch::Contains.matches("Adam", "eve"));
}

#[test]
fn given_exact_eve_is_only_eve() {
    // [base]/Patient?given:exact=Eve does not include "eve" or "EVE".
    assert!(StringMatch::Exact.matches("Eve", "Eve"));
    for field in ["eve", "EVE", "Evelyn", "Ève"] {
        assert!(!StringMatch::Exact.matches(field, "Eve"), "{field}");
    }
}

#[test]
fn the_default_match_ignores_case_and_accents() {
    assert!(StringMatch::Start.matches("Séverine", "SEVE"));
    assert!(StringMatch::Start.matches("Carreño Quiñones", "carreno"));
    assert!(StringMatch::Contains.matches("Carreño Quiñones", "quinones"));
    assert!(
        StringMatch::Start.matches("Ève", "eve"),
        "a precomposed letter decomposes"
    );
    assert!(
        StringMatch::Start.matches("E\u{300}ve", "eve"),
        "a combining grave"
    );
}

#[test]
fn the_default_match_ignores_punctuation_and_extra_whitespace() {
    assert!(StringMatch::Start.matches("van  der\tBerg", "van der berg"));
    assert!(StringMatch::Start.matches("O'Brien", "obrien"));
    assert!(StringMatch::Start.matches("  Eve", "eve"));
}

#[test]
fn fold_writes_the_compared_form() {
    assert_eq!(fold("  Ça  va,\tMÜLLER! "), "ca va muller");
    assert_eq!(fold(""), "");
    assert_eq!(fold("!!!"), "");
    assert_eq!(
        fold("ΣΊΣΥΦΟΣ"),
        "σισυφοσ",
        "Greek lowercases and drops its tonos"
    );
}

#[test]
fn an_empty_value_starts_every_field() {
    assert!(StringMatch::Start.matches("anything", ""));
}
