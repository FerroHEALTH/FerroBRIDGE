// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What this facade does with the values of the parameter types it searches
//! on, over the FHIR R4 search value grammar of `fhir-search`
//! (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4).

use fhir_search::date::{DateError, DateValue};
use fhir_search::escape::EscapeError;
use fhir_search::prefix::Prefix;
use fhir_search::string::StringMatch;
use fhir_search::token::TokenError;

/// Why one value is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ValueError {
    /// The value does not follow its type's grammar.
    #[error("{0}")]
    Malformed(String),
    /// The value is well formed and asks for something this facade does not
    /// do.
    #[error("{0}")]
    Unsupported(String),
}

impl From<EscapeError> for ValueError {
    fn from(error: EscapeError) -> Self {
        Self::Malformed(error.to_string())
    }
}

impl From<TokenError> for ValueError {
    fn from(error: TokenError) -> Self {
        Self::Malformed(error.to_string())
    }
}

impl From<DateError> for ValueError {
    fn from(error: DateError) -> Self {
        match error {
            DateError::LeapSecond | DateError::Unrepresentable => {
                Self::Unsupported(error.to_string())
            }
            _ => Self::Malformed(error.to_string()),
        }
    }
}

/// Reads one date value, refusing the `ap` prefix.
///
/// # Errors
///
/// Returns [`ValueError::Malformed`] for a value outside the grammar and
/// [`ValueError::Unsupported`] for `ap`, whose margin R4 leaves to the server
/// (§3.1.1.4.5), and for a leap second the CDR's clock does not hold.
pub fn date(text: &str) -> Result<DateValue, ValueError> {
    let value = DateValue::parse(text)?;
    if value.prefix() == Prefix::Ap {
        return Err(ValueError::Unsupported(String::from(
            "the `ap` prefix leaves its range to the server, and this server does not approximate",
        )));
    }
    Ok(value)
}

/// The characters AQL `LIKE` reads as wildcards (QUERY AQL §3.8.1.1).
const LIKE_WILDCARDS: [char; 2] = ['*', '?'];

/// Returns the `LIKE` pattern of one string value matched as `matching`, or
/// `None` for an exact match, which compares with `=`.
///
/// # Errors
///
/// Returns [`ValueError::Unsupported`] for a value carrying `*` or `?` under a
/// pattern match: AQL `LIKE` reads both as wildcards and defines no escape for
/// them (QUERY AQL §3.8.1.1), so the value cannot be matched literally.
pub fn like_pattern(value: &str, matching: StringMatch) -> Result<Option<String>, ValueError> {
    if matching == StringMatch::Exact {
        return Ok(None);
    }
    if value.contains(LIKE_WILDCARDS) {
        return Err(ValueError::Unsupported(format!(
            "`{value}` carries `*` or `?`, which AQL LIKE reads as wildcards with no escape; search it with :exact"
        )));
    }
    Ok(Some(match matching {
        StringMatch::Contains => format!("*{value}*"),
        StringMatch::Start | StringMatch::Exact => format!("{value}*"),
    }))
}

#[cfg(test)]
mod tests {
    use fhir_search::prefix::Prefix;
    use fhir_search::string::StringMatch;

    use super::{ValueError, date, like_pattern};

    #[test]
    fn a_malformed_date_and_the_ap_prefix_are_refused_apart() {
        for text in [
            "2026-02-30",
            "26",
            "2026-9",
            "2026-09-15T09",
            "yesterday",
            "2026-09-15T09:00+2",
        ] {
            assert!(
                matches!(date(text), Err(ValueError::Malformed(_))),
                "{text} is malformed"
            );
        }
        assert!(matches!(date("ap2026"), Err(ValueError::Unsupported(_))));
        assert!(matches!(
            date("2016-12-31T23:59:60Z"),
            Err(ValueError::Unsupported(_))
        ));
        assert_eq!(
            Ok(Prefix::Ge),
            date("ge2026-12").map(|value| value.prefix())
        );
    }

    #[test]
    fn a_string_pattern_starts_contains_or_compares_exactly() {
        assert_eq!(
            Ok(Some(String::from("diab*"))),
            like_pattern("diab", StringMatch::Start)
        );
        assert_eq!(
            Ok(Some(String::from("*abet*"))),
            like_pattern("abet", StringMatch::Contains)
        );
        assert_eq!(Ok(None), like_pattern("Diabetes", StringMatch::Exact));
        assert!(matches!(
            like_pattern("a*b", StringMatch::Start),
            Err(ValueError::Unsupported(_))
        ));
    }
}
