// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `System.Quantity`: an exact decimal and a unit, either a UCUM code or a
//! calendar duration keyword (N1 §4.1.8).
//!
//! Units are compared case-sensitively. Two quantities with the same unit
//! compare and compute; calendar durations convert among themselves by the
//! factors of N1 §5.5.7; a calendar duration and a definite one meet only at
//! the second and the millisecond (N1 §6.1.1, §6.2). Every other pair of
//! different units needs UCUM conversion, which this crate does not carry,
//! and answers empty as N1 §4.1.8 permits.

use std::cmp::Ordering;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use rust_decimal::Decimal;

/// A calendar duration keyword (N1 §4.1.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Calendar {
    /// `year`, `years`.
    Year,
    /// `month`, `months`.
    Month,
    /// `week`, `weeks`.
    Week,
    /// `day`, `days`.
    Day,
    /// `hour`, `hours`.
    Hour,
    /// `minute`, `minutes`.
    Minute,
    /// `second`, `seconds`.
    Second,
    /// `millisecond`, `milliseconds`.
    Millisecond,
}

impl Calendar {
    /// The keyword a singular or plural spelling names.
    #[must_use]
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Some(match keyword.strip_suffix('s').unwrap_or(keyword) {
            "year" => Self::Year,
            "month" => Self::Month,
            "week" => Self::Week,
            "day" => Self::Day,
            "hour" => Self::Hour,
            "minute" => Self::Minute,
            "second" => Self::Second,
            "millisecond" => Self::Millisecond,
            _ => return None,
        })
    }

    /// The singular keyword.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Year => "year",
            Self::Month => "month",
            Self::Week => "week",
            Self::Day => "day",
            Self::Hour => "hour",
            Self::Minute => "minute",
            Self::Second => "second",
            Self::Millisecond => "millisecond",
        }
    }

    /// The definite UCUM unit the keyword is equivalent to (N1 §4.1.8).
    #[must_use]
    pub const fn ucum(self) -> &'static str {
        match self {
            Self::Year => "a",
            Self::Month => "mo",
            Self::Week => "wk",
            Self::Day => "d",
            Self::Hour => "h",
            Self::Minute => "min",
            Self::Second => "s",
            Self::Millisecond => "ms",
        }
    }

    /// The length in milliseconds by the calendar factors of N1 §5.5.7, a
    /// year being 365 days and a month 30.
    fn milliseconds(self) -> i64 {
        match self {
            Self::Year => 31_536_000_000,
            Self::Month => 2_592_000_000,
            Self::Week => 604_800_000,
            Self::Day => 86_400_000,
            Self::Hour => 3_600_000,
            Self::Minute => 60_000,
            Self::Second => 1_000,
            Self::Millisecond => 1,
        }
    }

    /// `amount` of `self` expressed in `to`: twelve months to a year, and the
    /// millisecond lengths otherwise (N1 §5.5.7); `None` on overflow.
    #[must_use]
    pub fn convert(self, amount: Decimal, to: Self) -> Option<Decimal> {
        let (numerator, denominator) = match (self, to) {
            (Self::Year, Self::Month) => (12, 1),
            (Self::Month, Self::Year) => (1, 12),
            _ => (self.milliseconds(), to.milliseconds()),
        };
        amount
            .checked_mul(Decimal::from(numerator))?
            .checked_div(Decimal::from(denominator))
    }
}

/// The unit of a quantity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unit {
    /// A calendar duration keyword.
    Calendar(Calendar),
    /// A UCUM code, `'1'` for a plain number.
    Ucum(String),
}

impl Unit {
    /// The unit `'1'`, the default of N1 §5.5.7.
    #[must_use]
    pub fn one() -> Self {
        Self::Ucum(String::from("1"))
    }

    /// The unit a quoted unit string names: a calendar keyword when it is
    /// one, singular or plural, else a UCUM code (N1 §4.1.8: the unit "is
    /// represented as a String that is required to be either a valid UCUM
    /// unit or one of the calendar duration keywords").
    #[must_use]
    pub fn from_code(code: String) -> Self {
        Calendar::from_keyword(&code).map_or(Self::Ucum(code), Self::Calendar)
    }

    /// The calendar keyword or definite time unit at and below the second,
    /// where N1 §6.7 makes calendar and definite durations one.
    fn subsecond(&self) -> Option<Calendar> {
        match self {
            Self::Calendar(Calendar::Second) => Some(Calendar::Second),
            Self::Calendar(Calendar::Millisecond) => Some(Calendar::Millisecond),
            Self::Ucum(code) if code == "s" => Some(Calendar::Second),
            Self::Ucum(code) if code == "ms" => Some(Calendar::Millisecond),
            _ => None,
        }
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Calendar(calendar) => f.write_str(calendar.keyword()),
            Self::Ucum(code) => write!(f, "'{code}'"),
        }
    }
}

/// A quantity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quantity {
    /// The value.
    pub value: Decimal,
    /// The unit.
    pub unit: Unit,
}

/// How two quantities line up for a comparison.
enum Aligned {
    /// Both values in one unit.
    Values(Decimal, Decimal, Unit),
    /// A calendar duration above the second against a definite one.
    Calendrical,
    /// Different units this crate cannot convert.
    Unknown,
}

impl Quantity {
    /// A quantity of `value` in `unit`.
    #[must_use]
    pub const fn new(value: Decimal, unit: Unit) -> Self {
        Self { value, unit }
    }

    /// Both values in the finer of the two units, when they convert.
    fn align(&self, other: &Self) -> Aligned {
        if self.unit == other.unit {
            return Aligned::Values(self.value, other.value, self.unit.clone());
        }
        if let (Some(a), Some(b)) = (self.unit.subsecond(), other.unit.subsecond()) {
            return calendar_aligned(self.value, a, other.value, b, |c| {
                Unit::Ucum(String::from(c.ucum()))
            });
        }
        match (&self.unit, &other.unit) {
            (Unit::Calendar(a), Unit::Calendar(b)) => {
                calendar_aligned(self.value, *a, other.value, *b, Unit::Calendar)
            }
            (Unit::Calendar(_), Unit::Ucum(code)) | (Unit::Ucum(code), Unit::Calendar(_))
                if is_time_unit(code) =>
            {
                Aligned::Calendrical
            }
            // TODO(#459): convert between different UCUM units; until then
            // such a pair answers empty, as N1 §4.1.8 permits.
            _ => Aligned::Unknown,
        }
    }

    /// Whether two quantities are equal, `None` when unknown (N1 §6.1.1).
    #[must_use]
    pub fn equals(&self, other: &Self) -> Option<bool> {
        match self.align(other) {
            Aligned::Values(a, b, _) => Some(a == b),
            Aligned::Calendrical => Some(false),
            Aligned::Unknown => None,
        }
    }

    /// Whether two quantities are equivalent, `None` when unknown (N1
    /// §6.1.2): a calendar duration is equivalent to its definite unit.
    #[must_use]
    pub fn equivalent(&self, other: &Self) -> Option<bool> {
        let definite = |q: &Self| match &q.unit {
            Unit::Calendar(calendar) => {
                Self::new(q.value, Unit::Ucum(String::from(calendar.ucum())))
            }
            Unit::Ucum(_) => q.clone(),
        };
        match self.align(other) {
            Aligned::Values(a, b, _) => Some(equivalent_decimals(a, b)),
            Aligned::Calendrical => match definite(self).align(&definite(other)) {
                Aligned::Values(a, b, _) => Some(equivalent_decimals(a, b)),
                _ => None,
            },
            Aligned::Unknown => None,
        }
    }

    /// Orders two quantities, `None` when they do not compare (N1 §6.2).
    #[must_use]
    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        match self.align(other) {
            Aligned::Values(a, b, _) => Some(a.cmp(&b)),
            Aligned::Calendrical | Aligned::Unknown => None,
        }
    }

    /// The sum or difference, in the finer unit (N1 §6.6.3, §6.6.4); `None`
    /// when the units do not convert or the result overflows.
    #[must_use]
    pub fn add(&self, other: &Self, subtract: bool) -> Option<Self> {
        let Aligned::Values(a, b, unit) = self.align(other) else {
            return None;
        };
        let value = if subtract {
            a.checked_sub(b)?
        } else {
            a.checked_add(b)?
        };
        Some(Self::new(value, unit))
    }

    /// The product (N1 §6.6.1), its unit the UCUM product of the operands'.
    #[must_use]
    pub fn multiply(&self, other: &Self) -> Option<Self> {
        let value = self.value.checked_mul(other.value)?;
        let unit = match (&self.unit, &other.unit) {
            (Unit::Ucum(one), unit) | (unit, Unit::Ucum(one)) if one == "1" => unit.clone(),
            (Unit::Ucum(a), Unit::Ucum(b)) => Unit::Ucum(format!("{}.{}", group(a), group(b))),
            _ => return None,
        };
        Some(Self::new(value, unit))
    }

    /// The quotient (N1 §6.6.2), its unit the UCUM quotient of the operands';
    /// `None` on a zero divisor.
    #[must_use]
    pub fn divide(&self, other: &Self) -> Option<Self> {
        let value = self.value.checked_div(other.value)?;
        let unit = match (&self.unit, &other.unit) {
            (a, b) if a == b => Unit::one(),
            (unit, Unit::Ucum(one)) if one == "1" => unit.clone(),
            (Unit::Ucum(a), Unit::Ucum(b)) => Unit::Ucum(format!("{}/{}", group(a), group(b))),
            _ => return None,
        };
        Some(Self::new(value, unit))
    }

    /// Reads the string form `toQuantity()` converts (N1 §5.5.7): a number,
    /// then a quoted UCUM unit or a calendar keyword, or no unit for `'1'`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        static FORM: LazyLock<Option<Regex>> = LazyLock::new(|| {
            Regex::new(r"^((?:\+|-)?\d+(?:\.\d+)?)\s*(?:'([^']+)'|([a-zA-Z]+))?$").ok()
        });
        let captures = FORM.as_ref()?.captures(text)?;
        let value = captures.get(1)?.as_str();
        let value = value
            .strip_prefix('+')
            .unwrap_or(value)
            .parse::<Decimal>()
            .ok()?;
        let unit = match (captures.get(2), captures.get(3)) {
            (Some(code), _) => Unit::from_code(String::from(code.as_str())),
            (None, Some(keyword)) => Unit::Calendar(Calendar::from_keyword(keyword.as_str())?),
            (None, None) => Unit::one(),
        };
        Some(Self::new(value, unit))
    }
}

/// The unit in parentheses when it is itself a product or a quotient, so a
/// composed UCUM unit keeps its grouping.
fn group(unit: &str) -> String {
    if unit.contains(['.', '/']) {
        format!("({unit})")
    } else {
        String::from(unit)
    }
}

/// Whether a UCUM code is one of the definite time units N1 §4.1.8 relates
/// to a calendar keyword.
fn is_time_unit(code: &str) -> bool {
    ["a", "mo", "wk", "d", "h", "min", "s", "ms"].contains(&code)
}

/// Both values in the finer of two calendar units.
fn calendar_aligned(
    a: Decimal,
    a_unit: Calendar,
    b: Decimal,
    b_unit: Calendar,
    unit: impl Fn(Calendar) -> Unit,
) -> Aligned {
    let finer = a_unit.max(b_unit);
    let convert = |value: Decimal, from: Calendar| from.convert(value, finer);
    match (convert(a, a_unit), convert(b, b_unit)) {
        (Some(a), Some(b)) => Aligned::Values(a, b, unit(finer)),
        _ => Aligned::Unknown,
    }
}

/// Decimal equivalence: equal after rounding to the precision of the less
/// precise operand, trailing zeroes not counting (N1 §6.1.2).
#[must_use]
pub fn equivalent_decimals(a: Decimal, b: Decimal) -> bool {
    let scale = a.normalize().scale().min(b.normalize().scale());
    a.round_dp(scale) == b.round_dp(scale)
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.unit {
            Unit::Calendar(calendar) if self.value.abs() == Decimal::ONE => {
                write!(f, "{} {}", self.value, calendar.keyword())
            }
            Unit::Calendar(calendar) => write!(f, "{} {}s", self.value, calendar.keyword()),
            Unit::Ucum(code) => write!(f, "{} '{code}'", self.value),
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{Calendar, Quantity, Unit};

    fn q(value: i64, unit: Unit) -> Quantity {
        Quantity::new(Decimal::from(value), unit)
    }

    #[test]
    fn calendar_durations_convert_among_themselves() {
        let week = q(1, Unit::Calendar(Calendar::Week));
        assert_eq!(
            q(7, Unit::Calendar(Calendar::Day)).equals(&week),
            Some(true)
        );
        assert_eq!(
            q(24, Unit::Calendar(Calendar::Month)).compare(&q(2, Unit::Calendar(Calendar::Year))),
            Some(std::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn a_calendar_year_is_unequal_but_equivalent_to_a_ucum_year() {
        let year = q(1, Unit::Calendar(Calendar::Year));
        let annum = q(1, Unit::Ucum(String::from("a")));
        assert_eq!(year.equals(&annum), Some(false));
        assert_eq!(year.equivalent(&annum), Some(true));
        assert_eq!(year.compare(&annum), None);
    }

    #[test]
    fn a_second_is_one_s() {
        let second = q(1, Unit::Calendar(Calendar::Second));
        assert_eq!(
            second.equals(&q(1, Unit::Ucum(String::from("s")))),
            Some(true)
        );
        assert_eq!(
            second.equals(&q(1000, Unit::Ucum(String::from("ms")))),
            Some(true)
        );
    }

    #[test]
    fn different_ucum_units_answer_unknown() {
        let gram = q(4, Unit::Ucum(String::from("g")));
        assert_eq!(gram.equals(&q(4000, Unit::Ucum(String::from("mg")))), None);
    }

    #[test]
    fn the_string_form_reads() {
        assert_eq!(
            Quantity::parse("1 day"),
            Some(q(1, Unit::Calendar(Calendar::Day)))
        );
        assert_eq!(
            Quantity::parse("1 'wk'"),
            Some(q(1, Unit::Ucum(String::from("wk"))))
        );
        assert_eq!(Quantity::parse("1 wk"), None);
        assert_eq!(Quantity::parse("1"), Some(q(1, Unit::one())));
    }
}
