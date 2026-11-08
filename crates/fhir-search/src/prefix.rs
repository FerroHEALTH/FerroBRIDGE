// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The comparison prefix of a number, date or quantity value
//! (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.5).

use core::fmt;

/// A comparison prefix; [`Prefix::Eq`] when none is written ("If no prefix
/// is present, the prefix eq is assumed", §3.1.1.4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Prefix {
    /// `eq`: the range of the search value fully contains the range of the
    /// target value.
    #[default]
    Eq,
    /// `ne`: the range of the search value does not fully contain the range
    /// of the target value.
    Ne,
    /// `gt`: the range above the search value overlaps the target's range.
    Gt,
    /// `lt`: the range below the search value overlaps the target's range.
    Lt,
    /// `ge`: as `gt`, or as `eq`.
    Ge,
    /// `le`: as `lt`, or as `eq`.
    Le,
    /// `sa`: the target starts after the range of the search value.
    Sa,
    /// `eb`: the target ends before the range of the search value.
    Eb,
    /// `ap`: the target is approximately the search value, by a margin the
    /// specification recommends and leaves to the system.
    Ap,
}

/// Why a value's prefix is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PrefixError {
    /// The value opens with a letter and with none of the nine prefixes.
    #[error("the value opens with none of the prefixes eq, ne, gt, lt, ge, le, sa, eb and ap")]
    Unknown,
}

impl Prefix {
    /// The nine prefixes, in the order §3.1.1.4.5 lists them.
    pub const ALL: [Self; 9] = [
        Self::Eq,
        Self::Ne,
        Self::Gt,
        Self::Lt,
        Self::Ge,
        Self::Le,
        Self::Sa,
        Self::Eb,
        Self::Ap,
    ];

    /// The two letters a value writes the prefix as.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Gt => "gt",
            Self::Lt => "lt",
            Self::Ge => "ge",
            Self::Le => "le",
            Self::Sa => "sa",
            Self::Eb => "eb",
            Self::Ap => "ap",
        }
    }

    /// Splits the prefix off `text`, the value of an ordered parameter type.
    ///
    /// A value of a number, a date or a quantity opens with a digit or a
    /// sign, so a value that opens with an ASCII letter opens with a prefix
    /// or is refused. The prefixes are lowercase.
    ///
    /// # Errors
    ///
    /// [`PrefixError::Unknown`] for a value that opens with an ASCII letter
    /// and none of the nine prefixes.
    pub fn split(text: &str) -> Result<(Self, &str), PrefixError> {
        for prefix in Self::ALL {
            if let Some(rest) = text.strip_prefix(prefix.code()) {
                return Ok((prefix, rest));
            }
        }
        if text.starts_with(|character: char| character.is_ascii_alphabetic()) {
            Err(PrefixError::Unknown)
        } else {
            Ok((Self::Eq, text))
        }
    }
}

impl fmt::Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
