// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The value grammars of the parameter types this facade searches on.
//!
//! Each reader takes one value of one parameter occurrence, after the
//! comma that separates the alternatives of an `OR` has been split off with
//! [`split`], and answers either the value or why it is malformed
//! (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4).

use jiff::ToSpan;
use jiff::civil::Date;
use jiff::civil::DateTime;

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

/// Splits `text` at every `delimiter` no `\` escapes, keeping the escapes.
///
/// "When any of these characters appear in an actual parameter value, they
/// must be prepended by the character `\`, which also must be used to prepend
/// itself" (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.19).
#[must_use]
pub fn split(text: &str, delimiter: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for character in text.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' {
            current.push(character);
            escaped = true;
        } else if character == delimiter {
            parts.push(core::mem::take(&mut current));
        } else {
            current.push(character);
        }
    }
    parts.push(current);
    parts
}

/// Removes the `\` escapes of one value.
///
/// # Errors
///
/// Returns [`ValueError::Malformed`] for a `\` that escapes none of `$`, `,`,
/// `|` and `\`: "The parameter value `xx\xx` is illegal"
/// (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.19).
pub fn unescape(text: &str) -> Result<String, ValueError> {
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            plain.push(character);
            continue;
        }
        match characters.next() {
            Some(escaped @ ('$' | ',' | '|' | '\\')) => plain.push(escaped),
            _ => {
                return Err(ValueError::Malformed(format!(
                    "`{text}` carries a `\\` that escapes none of `$`, `,`, `|` and `\\`"
                )));
            }
        }
    }
    Ok(plain)
}

/// What a token value says about the code system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum System {
    /// `[code]`: any system, or none.
    Any,
    /// `|[code]`: no system.
    Absent,
    /// `[system]|…`: exactly this system.
    Is(String),
}

/// One token value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The system half.
    pub system: System,
    /// The code half, absent in the `[system]|` form.
    pub code: Option<String>,
}

/// Reads one token value: `[code]`, `[system]|[code]`, `|[code]` or
/// `[system]|` (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.10).
///
/// # Errors
///
/// Returns [`ValueError::Malformed`] for a value with more than one unescaped
/// `|`, with neither a system nor a code, or with an illegal escape.
pub fn token(text: &str) -> Result<Token, ValueError> {
    let halves = split(text, '|');
    match halves.as_slice() {
        [code] => Ok(Token {
            system: System::Any,
            code: Some(unescape(code)?),
        }),
        [system, code] => {
            let system = unescape(system)?;
            let code = unescape(code)?;
            if system.is_empty() && code.is_empty() {
                return Err(ValueError::Malformed(String::from(
                    "a token of `|` alone names neither a system nor a code",
                )));
            }
            Ok(Token {
                system: if system.is_empty() {
                    System::Absent
                } else {
                    System::Is(system)
                },
                code: (!code.is_empty()).then_some(code),
            })
        }
        _ => Err(ValueError::Malformed(format!(
            "`{text}` carries more than one unescaped `|`"
        ))),
    }
}

/// How a string value is matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringMatch {
    /// The default: the field equals or starts with the value.
    Start,
    /// `:contains`: the value appears anywhere in the field.
    Contains,
    /// `:exact`: the field is the value.
    Exact,
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

/// The comparison a date value asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefix {
    /// `eq`, the default.
    Equal,
    /// `ne`.
    NotEqual,
    /// `gt`.
    Greater,
    /// `lt`.
    Less,
    /// `ge`.
    GreaterOrEqual,
    /// `le`.
    LessOrEqual,
    /// `sa`.
    StartsAfter,
    /// `eb`.
    EndsBefore,
}

/// One date value: its comparison and the half-open range `[low, high)` its
/// precision spans.
///
/// "Dates with only the year specified are equivalent to an interval that
/// starts at the first instant of January 1st to the last instant of
/// December 31st" (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.7), so
/// every value is a range, and `high` is the first instant after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateValue {
    /// The comparison.
    pub prefix: Prefix,
    /// The first instant of the range, at the value's own precision.
    pub low: String,
    /// The first instant after the range, at the value's own precision.
    pub high: String,
}

/// Reads one date value with its optional prefix
/// (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.5 and §3.1.1.4.7).
///
/// The value is `yyyy`, `yyyy-mm`, `yyyy-mm-dd`, or a date with a time of
/// hours and minutes, optional seconds and fraction, and an optional `Z` or
/// `±hh:mm` offset. The bounds keep the value's offset as written, so the CDR
/// compares them as the client meant them.
///
/// # Errors
///
/// Returns [`ValueError::Unsupported`] for the `ap` prefix and
/// [`ValueError::Malformed`] for anything outside the grammar or a date the
/// calendar does not have.
pub fn date(text: &str) -> Result<DateValue, ValueError> {
    let (prefix, rest) = prefix_of(text)?;
    let malformed = || {
        ValueError::Malformed(format!(
            "`{text}` is no date of the form yyyy, yyyy-mm, yyyy-mm-dd or yyyy-mm-ddThh:mm[:ss][Z|±hh:mm]"
        ))
    };
    let (low, high) = match rest.split_once('T') {
        None => date_range(rest).ok_or_else(malformed)?,
        Some((day, time)) => time_range(day, time).ok_or_else(malformed)?,
    };
    Ok(DateValue { prefix, low, high })
}

/// Splits the two-letter prefix off a date value.
fn prefix_of(text: &str) -> Result<(Prefix, &str), ValueError> {
    let Some(head) = text.get(..2) else {
        return Ok((Prefix::Equal, text));
    };
    let rest = text.get(2..).unwrap_or_default();
    let prefix = match head {
        "eq" => Prefix::Equal,
        "ne" => Prefix::NotEqual,
        "gt" => Prefix::Greater,
        "lt" => Prefix::Less,
        "ge" => Prefix::GreaterOrEqual,
        "le" => Prefix::LessOrEqual,
        "sa" => Prefix::StartsAfter,
        "eb" => Prefix::EndsBefore,
        "ap" => {
            return Err(ValueError::Unsupported(String::from(
                "the `ap` prefix leaves its range to the server, and this server does not approximate",
            )));
        }
        _ => return Ok((Prefix::Equal, text)),
    };
    Ok((prefix, rest))
}

/// Returns the range of a date with no time part.
fn date_range(text: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = text.split('-').collect();
    match parts.as_slice() {
        [year] => {
            let start = Date::new(digits(year, 4)?, 1, 1).ok()?;
            let next = start.checked_add(1.year()).ok()?;
            Some((
                format!("{:04}", start.year()),
                format!("{:04}", next.year()),
            ))
        }
        [year, month] => {
            let start = Date::new(digits(year, 4)?, digits(month, 2)?, 1).ok()?;
            let next = start.checked_add(1.month()).ok()?;
            Some((
                format!("{:04}-{:02}", start.year(), start.month()),
                format!("{:04}-{:02}", next.year(), next.month()),
            ))
        }
        [year, month, day] => {
            let start = Date::new(digits(year, 4)?, digits(month, 2)?, digits(day, 2)?).ok()?;
            let next = start.checked_add(1.day()).ok()?;
            Some((start.to_string(), next.to_string()))
        }
        _ => None,
    }
}

/// Returns the range of a date with a time part.
fn time_range(day: &str, time: &str) -> Option<(String, String)> {
    let date: Date = day.parse().ok()?;
    if day.len() != 10 {
        return None;
    }
    let split_at = time.find(['Z', '+', '-']).unwrap_or(time.len());
    let clock = time.get(..split_at)?;
    let offset = time.get(split_at..)?;
    if !(offset.is_empty() || offset == "Z" || is_offset(offset)) {
        return None;
    }
    let fields: Vec<&str> = clock.split(':').collect();
    let (hour, minute, second) = match fields.as_slice() {
        [hour, minute] => (*hour, *minute, None),
        [hour, minute, second] => (*hour, *minute, Some(*second)),
        _ => return None,
    };
    let (whole, fraction) = match second {
        Some(text) => match text.split_once('.') {
            Some((whole, fraction)) => (Some(whole), Some(fraction)),
            None => (Some(text), None),
        },
        None => (None, None),
    };
    let start = DateTime::from_parts(
        date,
        jiff::civil::Time::new(
            digits(hour, 2)?,
            digits(minute, 2)?,
            whole.map_or(Some(0), |text| digits(text, 2))?,
            fraction_nanos(fraction)?,
        )
        .ok()?,
    );
    let step = match (whole, fraction) {
        (None, _) => 1.minute(),
        (Some(_), None) => 1.second(),
        (Some(_), Some(text)) => {
            let unit = 10_i64.checked_pow(9_u32.checked_sub(u32::try_from(text.len()).ok()?)?)?;
            unit.nanoseconds()
        }
    };
    let next = start.checked_add(step).ok()?;
    Some((
        format!("{}{offset}", render(start, whole, fraction)),
        format!("{}{offset}", render(next, whole, fraction)),
    ))
}

/// Renders a bound at the precision its value was written in.
fn render(instant: DateTime, second: Option<&str>, fraction: Option<&str>) -> String {
    let minutes = format!(
        "{}T{:02}:{:02}",
        instant.date(),
        instant.hour(),
        instant.minute()
    );
    match (second, fraction) {
        (None, _) => minutes,
        (Some(_), None) => format!("{minutes}:{:02}", instant.second()),
        (Some(_), Some(text)) => {
            let nanos = format!("{:09}", instant.subsec_nanosecond());
            let kept = nanos.get(..text.len()).unwrap_or(&nanos);
            format!("{minutes}:{:02}.{kept}", instant.second())
        }
    }
}

/// Returns the nanoseconds a fraction of up to nine digits names.
fn fraction_nanos(fraction: Option<&str>) -> Option<i32> {
    let Some(text) = fraction else {
        return Some(0);
    };
    if text.is_empty() || text.len() > 9 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let padded = format!("{text:0<9}");
    padded.parse().ok()
}

/// Whether `text` is a `+hh:mm` or `-hh:mm` offset.
fn is_offset(text: &str) -> bool {
    let bytes = text.as_bytes();
    matches!(
        bytes,
        [b'+' | b'-', h1, h2, b':', m1, m2]
            if [h1, h2, m1, m2].iter().all(|byte| byte.is_ascii_digit())
    )
}

/// Returns the number `text` spells when it is exactly `width` ASCII digits.
fn digits<T: core::str::FromStr>(text: &str, width: usize) -> Option<T> {
    if text.len() == width && text.bytes().all(|byte| byte.is_ascii_digit()) {
        text.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Prefix, StringMatch, System, ValueError, date, like_pattern, split, token, unescape,
    };

    #[test]
    fn a_value_splits_at_unescaped_commas_only() {
        assert_eq!(vec!["a", "b\\,c", "d"], split("a,b\\,c,d", ','));
        assert_eq!(Ok(String::from("b,c")), unescape("b\\,c"));
        assert!(unescape("x\\y").is_err(), "an escape of y is illegal");
    }

    #[test]
    fn the_four_token_forms_read() {
        let code = token("44054006").expect("a bare code");
        assert_eq!(System::Any, code.system);
        assert_eq!(Some(String::from("44054006")), code.code);
        let both = token("http://snomed.info/sct|44054006").expect("system and code");
        assert_eq!(
            System::Is(String::from("http://snomed.info/sct")),
            both.system
        );
        let absent = token("|44054006").expect("no system");
        assert_eq!(System::Absent, absent.system);
        let system = token("http://snomed.info/sct|").expect("any code of a system");
        assert_eq!(None, system.code);
        assert!(token("|").is_err());
        assert!(token("a|b|c").is_err());
        let escaped = token("a\\|b").expect("an escaped bar is part of the code");
        assert_eq!(Some(String::from("a|b")), escaped.code);
    }

    #[test]
    fn a_partial_date_spans_its_whole_precision() {
        let year = date("2026").expect("a year");
        assert_eq!(("2026", "2027"), (year.low.as_str(), year.high.as_str()));
        let month = date("ge2026-12").expect("a month");
        assert_eq!(Prefix::GreaterOrEqual, month.prefix);
        assert_eq!(
            ("2026-12", "2027-01"),
            (month.low.as_str(), month.high.as_str())
        );
        let day = date("2024-02-29").expect("a leap day");
        assert_eq!(
            ("2024-02-29", "2024-03-01"),
            (day.low.as_str(), day.high.as_str())
        );
    }

    #[test]
    fn a_time_keeps_its_offset_and_its_precision() {
        let minute = date("lt2026-09-15T23:59Z").expect("a minute");
        assert_eq!(Prefix::Less, minute.prefix);
        assert_eq!("2026-09-15T23:59Z", minute.low);
        assert_eq!("2026-09-16T00:00Z", minute.high);
        let second = date("2026-09-15T09:00:00+02:00").expect("a second");
        assert_eq!("2026-09-15T09:00:01+02:00", second.high);
        let fraction = date("2026-09-15T09:00:00.25").expect("a fraction");
        assert_eq!("2026-09-15T09:00:00.25", fraction.low);
        assert_eq!("2026-09-15T09:00:00.26", fraction.high);
    }

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
