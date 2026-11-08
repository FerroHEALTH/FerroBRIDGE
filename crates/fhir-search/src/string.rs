// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How a string value matches a field (<https://hl7.org/fhir/R4/search.html>,
//! §3.1.1.4.8).

use unicode_normalization::UnicodeNormalization as _;
use unicode_normalization::char::is_combining_mark;

/// How a string value matches a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StringMatch {
    /// No modifier: the field equals or starts with the value, both
    /// [`fold`]ed.
    #[default]
    Start,
    /// `:contains`: the value appears anywhere in the field, both
    /// [`fold`]ed.
    Contains,
    /// `:exact`: the field is the value, "including casing and accents".
    Exact,
}

impl StringMatch {
    /// The match a string parameter's modifier asks for, or `None` for a
    /// modifier that is no string match (`:missing`, `:text` and the rest of
    /// §3.1.1.4.4), which the caller reads or refuses.
    #[must_use]
    pub fn from_modifier(modifier: Option<&str>) -> Option<Self> {
        match modifier {
            None => Some(Self::Start),
            Some("contains") => Some(Self::Contains),
            Some("exact") => Some(Self::Exact),
            Some(_) => None,
        }
    }

    /// The modifier that asks for this match, `None` for the default.
    #[must_use]
    pub const fn modifier(self) -> Option<&'static str> {
        match self {
            Self::Start => None,
            Self::Contains => Some("contains"),
            Self::Exact => Some("exact"),
        }
    }

    /// Whether `field` matches the unescaped search `value`.
    #[must_use]
    pub fn matches(self, field: &str, value: &str) -> bool {
        match self {
            Self::Start => fold(field).starts_with(&fold(value)),
            Self::Contains => fold(field).contains(&fold(value)),
            Self::Exact => field == value,
        }
    }
}

/// Returns `text` in the form the default and the `:contains` match compare.
///
/// The search "is insensitive to casing and included combining characters",
/// and "punctuation and non-significant whitespace (e.g. repeated space
/// characters, tab vs space) should also be ignored" (§3.1.1.4.8). So the
/// text is decomposed (NFD) and loses its combining marks, its letters are
/// lowercased, every character that is neither alphanumeric nor whitespace
/// is dropped, and each run of whitespace becomes one space, with none at
/// either end.
#[must_use]
pub fn fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    let mut gap = false;
    for character in text
        .nfd()
        .filter(|character| !is_combining_mark(*character))
    {
        if character.is_whitespace() {
            gap = true;
        } else if character.is_alphanumeric() {
            if gap && !folded.is_empty() {
                folded.push(' ');
            }
            gap = false;
            folded.extend(character.to_lowercase());
        }
    }
    folded
}
