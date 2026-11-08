// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The comparison prefix (search.html §3.1.1.4.5).

use fhir_search::prefix::{Prefix, PrefixError};

#[test]
fn no_prefix_is_eq() {
    assert_eq!(Prefix::split("2013-01-14"), Ok((Prefix::Eq, "2013-01-14")));
    assert_eq!(Prefix::split("100"), Ok((Prefix::Eq, "100")));
    assert_eq!(Prefix::split("-5"), Ok((Prefix::Eq, "-5")));
    assert_eq!(Prefix::split(""), Ok((Prefix::Eq, "")));
    assert_eq!(Prefix::default(), Prefix::Eq);
}

#[test]
fn each_of_the_nine_prefixes_splits_off() {
    for prefix in Prefix::ALL {
        let text = format!("{prefix}2013");
        assert_eq!(Prefix::split(&text), Ok((prefix, "2013")), "{text}");
    }
    assert_eq!(
        Prefix::split("ap2013"),
        Ok((Prefix::Ap, "2013")),
        "ap is a prefix"
    );
}

#[test]
fn an_unknown_letter_prefix_is_refused() {
    for text in ["xx2013", "GT2013", "Eq2013", "g2013", "yesterday", "e5"] {
        assert_eq!(Prefix::split(text), Err(PrefixError::Unknown), "{text}");
    }
}

#[test]
fn a_prefix_alone_leaves_an_empty_value() {
    assert_eq!(Prefix::split("gt"), Ok((Prefix::Gt, "")));
}

#[test]
fn a_multibyte_opening_is_no_prefix() {
    assert_eq!(Prefix::split("é2013"), Ok((Prefix::Eq, "é2013")));
}
