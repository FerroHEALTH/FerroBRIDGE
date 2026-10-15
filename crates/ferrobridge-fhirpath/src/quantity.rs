// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `System.Quantity`: an exact decimal and a unit, either a UCUM code or a
//! calendar duration keyword (N1 §4.1.8).
//!
//! Units are compared case-sensitively. Two quantities with the same unit
//! compare and compute; calendar durations convert among themselves by the
//! factors of N1 §5.5.7; a calendar duration and a definite one meet only at
//! the second and below (N1 §6.1.1, §6.2). Two different UCUM units convert
//! by UCUM ([`crate::ucum`]) and are compared in the more granular one; a
//! pair UCUM does not convert, an invalid unit among them, answers empty as
//! N1 §6.1.1 says.

use std::cmp::Ordering;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::ucum::Canonical;

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
}

impl Unit {
    /// The UCUM code of the unit: its own, or the definite unit of a calendar
    /// keyword at and below the second.
    fn definite(&self) -> Option<&str> {
        match self {
            Self::Calendar(calendar) => calendar.subsecond(),
            Self::Ucum(code) => Some(code),
        }
    }

    /// Whether the unit is a special UCUM unit, one on a scale other than a
    /// ratio scale (UCUM §21).
    fn is_special(&self) -> bool {
        matches!(self, Self::Ucum(code) if ucum(code).is_some_and(|unit| unit.is_special()))
    }
}

impl Calendar {
    /// The definite UCUM unit of a calendar keyword at and below the second,
    /// where N1 §6.7 makes calendar and definite durations one.
    fn subsecond(self) -> Option<&'static str> {
        matches!(self, Self::Second | Self::Millisecond).then(|| self.ucum())
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
    /// Both values in one unit, with the size of each operand's unit in it.
    Values {
        left: Decimal,
        right: Decimal,
        unit: Unit,
        sizes: (Decimal, Decimal),
    },
    /// A calendar duration above the second against a definite one.
    Calendrical,
    /// Units that do not convert.
    Unknown,
}

impl Aligned {
    /// The pair with its operands exchanged.
    fn swapped(self) -> Self {
        match self {
            Self::Values {
                left,
                right,
                unit,
                sizes: (left_size, right_size),
            } => Self::Values {
                left: right,
                right: left,
                unit,
                sizes: (right_size, left_size),
            },
            other => other,
        }
    }
}

impl Quantity {
    /// A quantity of `value` in `unit`.
    #[must_use]
    pub const fn new(value: Decimal, unit: Unit) -> Self {
        Self { value, unit }
    }

    /// Both values in the more granular of the two units, when they convert
    /// (N1 §6.1.1: "the comparison will be made using the most granular unit
    /// of either input").
    fn align(&self, other: &Self) -> Aligned {
        if self.unit == other.unit {
            return Aligned::Values {
                left: self.value,
                right: other.value,
                unit: self.unit.clone(),
                sizes: (Decimal::ONE, Decimal::ONE),
            };
        }
        match (&self.unit, &other.unit) {
            (Unit::Calendar(a), Unit::Calendar(b)) => {
                calendar_aligned(self.value, *a, other.value, *b)
            }
            (Unit::Ucum(a), Unit::Ucum(b)) => ucum_aligned(self.value, a, other.value, b),
            (Unit::Calendar(calendar), Unit::Ucum(code)) => {
                mixed_aligned(self.value, *calendar, other.value, code)
            }
            (Unit::Ucum(code), Unit::Calendar(calendar)) => {
                mixed_aligned(other.value, *calendar, self.value, code).swapped()
            }
        }
    }

    /// The quantity in `unit` (N1 §5.5.7): calendar durations by its
    /// factors, UCUM units by UCUM, and a calendar keyword and a UCUM unit
    /// only at and below the second; `None` when they do not convert.
    #[must_use]
    pub fn convert(&self, unit: &Unit) -> Option<Self> {
        if &self.unit == unit {
            return Some(self.clone());
        }
        // NOTE: N1 §5.5.7 relates a calendar keyword to UCUM only as "1 second = 1 's'", so a
        // calendar duration above the second converts to no UCUM unit.
        let value = match (&self.unit, unit) {
            (Unit::Calendar(from), Unit::Calendar(to)) => from.convert(self.value, *to)?,
            (from, to) => ucum(from.definite()?)?
                .convert(self.value, &ucum(to.definite()?)?)
                .ok()?,
        };
        Some(Self::new(value, unit.clone()))
    }

    /// Whether two quantities are equal, `None` when unknown (N1 §6.1.1).
    #[must_use]
    pub fn equals(&self, other: &Self) -> Option<bool> {
        match self.align(other) {
            Aligned::Values { left, right, .. } => Some(left == right),
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
        let aligned = match self.align(other) {
            Aligned::Calendrical => definite(self).align(&definite(other)),
            aligned => aligned,
        };
        match aligned {
            Aligned::Values {
                left, right, sizes, ..
            } => equivalent_in(self.value, other.value, left, right, sizes),
            Aligned::Calendrical | Aligned::Unknown => None,
        }
    }

    /// Orders two quantities, `None` when they do not compare (N1 §6.2).
    #[must_use]
    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        match self.align(other) {
            Aligned::Values { left, right, .. } => Some(left.cmp(&right)),
            Aligned::Calendrical | Aligned::Unknown => None,
        }
    }

    /// The sum or difference, in the finer unit (N1 §6.6.3, §6.6.4); `None`
    /// when the units do not convert, either is a special UCUM unit of
    /// another unit than the other's (UCUM §22.1), or the result overflows.
    #[must_use]
    pub fn add(&self, other: &Self, subtract: bool) -> Option<Self> {
        if self.unit != other.unit && (self.unit.is_special() || other.unit.is_special()) {
            return None;
        }
        let Aligned::Values {
            left: a,
            right: b,
            unit,
            ..
        } = self.align(other)
        else {
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

/// The canonical form of a UCUM code, `None` for a code UCUM refuses.
// NOTE: N1 §6.1.1 makes an operation on "quantities with invalid units" empty, so a code UCUM
// refuses is legitimately no unit here rather than an evaluation error.
fn ucum(code: &str) -> Option<Canonical> {
    Canonical::parse(code).ok()
}

/// Both values in the finer of two calendar units.
fn calendar_aligned(a: Decimal, a_unit: Calendar, b: Decimal, b_unit: Calendar) -> Aligned {
    let finer = a_unit.max(b_unit);
    let convert = |value: Decimal, from: Calendar| from.convert(value, finer);
    let sizes = (convert(Decimal::ONE, a_unit), convert(Decimal::ONE, b_unit));
    match (convert(a, a_unit), convert(b, b_unit), sizes) {
        (Some(left), Some(right), (Some(a_size), Some(b_size))) => Aligned::Values {
            left,
            right,
            unit: Unit::Calendar(finer),
            sizes: (a_size, b_size),
        },
        _ => Aligned::Unknown,
    }
}

/// Both values in the more granular of two UCUM units, when UCUM converts
/// them.
fn ucum_aligned(a: Decimal, a_code: &str, b: Decimal, b_code: &str) -> Aligned {
    let (Some(a_unit), Some(b_unit)) = (ucum(a_code), ucum(b_code)) else {
        return Aligned::Unknown;
    };
    let (unit, code) = match a_unit.compare_size(&b_unit) {
        Ok(Ordering::Greater) => (&b_unit, b_code),
        Ok(_) => (&a_unit, a_code),
        Err(_) => return Aligned::Unknown,
    };
    let converted = (
        a_unit.convert(a, unit),
        b_unit.convert(b, unit),
        a_unit.size_in(unit),
        b_unit.size_in(unit),
    );
    match converted {
        (Ok(left), Ok(right), Ok(a_size), Ok(b_size)) => Aligned::Values {
            left,
            right,
            unit: Unit::Ucum(String::from(code)),
            sizes: (a_size, b_size),
        },
        _ => Aligned::Unknown,
    }
}

/// A calendar duration against a UCUM unit: at and below the second against
/// a UCUM time unit no larger than a second they convert as UCUM units;
/// otherwise against a UCUM time unit they are calendrical (N1 §6.1.1).
fn mixed_aligned(a: Decimal, calendar: Calendar, b: Decimal, code: &str) -> Aligned {
    let (Some(unit), Some(second)) = (ucum(code), ucum("s")) else {
        return Aligned::Unknown;
    };
    match (calendar.subsecond(), unit.compare_size(&second)) {
        (Some(definite), Ok(Ordering::Less | Ordering::Equal)) => {
            ucum_aligned(a, definite, b, code)
        }
        (_, Ok(_)) => Aligned::Calendrical,
        (_, Err(_)) => Aligned::Unknown,
    }
}

/// Quantity equivalence (N1 §6.1.2): the values in one unit, each rounded
/// to the precision of the less precise operand.
// NOTE: N1 §6.1.2 is silent on precision across units; no specification governs this: our own
// design, a quantity's precision is one step of its last digit in its own unit, carried across.
fn equivalent_in(
    a: Decimal,
    b: Decimal,
    left: Decimal,
    right: Decimal,
    (a_size, b_size): (Decimal, Decimal),
) -> Option<bool> {
    let step = |value: Decimal, size: Decimal| {
        Decimal::try_new(1, value.normalize().scale())
            .ok()
            .and_then(|unit| unit.checked_mul(size))
    };
    let step = step(a, a_size)?.max(step(b, b_size)?);
    let round = |value: Decimal| {
        value
            .checked_div(step)
            .map(|steps| steps.round_dp_with_strategy(0, RoundingStrategy::MidpointNearestEven))
    };
    Some(round(left)? == round(right)?)
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
    fn commensurable_ucum_units_convert_and_others_answer_unknown() {
        // N1 §6.1.1: the dimensions must be the same, not the unit.
        let gram = q(4, Unit::Ucum(String::from("g")));
        assert_eq!(
            gram.equals(&q(4000, Unit::Ucum(String::from("mg")))),
            Some(true)
        );
        assert_eq!(gram.equals(&q(4000, Unit::Ucum(String::from("mL")))), None);
        assert_eq!(gram.equals(&q(4, Unit::Ucum(String::from("gram")))), None);
    }

    #[test]
    fn the_comparison_is_made_in_the_more_granular_unit() {
        // N1 §6.6.3: 3 'm' + 3 'cm' is 303 'cm'.
        let sum =
            q(3, Unit::Ucum(String::from("m"))).add(&q(3, Unit::Ucum(String::from("cm"))), false);
        assert_eq!(sum, Some(q(303, Unit::Ucum(String::from("cm")))));
        let difference =
            q(3, Unit::Ucum(String::from("m"))).add(&q(3, Unit::Ucum(String::from("cm"))), true);
        assert_eq!(difference, Some(q(297, Unit::Ucum(String::from("cm")))));
    }

    #[test]
    fn equivalence_rounds_to_the_less_precise_operand_in_its_own_unit() {
        // N1 §6.1.2, the suite's testQuantity4: 4 'g' ~ 4040 'mg'.
        let gram = q(4, Unit::Ucum(String::from("g")));
        assert_eq!(
            gram.equivalent(&q(4040, Unit::Ucum(String::from("mg")))),
            Some(true)
        );
        assert_eq!(
            gram.equivalent(&q(4600, Unit::Ucum(String::from("mg")))),
            Some(false)
        );
        assert_eq!(
            gram.equals(&q(4040, Unit::Ucum(String::from("mg")))),
            Some(false)
        );
    }

    #[test]
    fn a_millisecond_meets_a_finer_ucum_time_unit_and_a_day_none() {
        // N1 §6.1.1: calendar and definite durations are one at and below the second.
        let millisecond = q(1, Unit::Calendar(Calendar::Millisecond));
        assert_eq!(
            millisecond.equals(&q(1000, Unit::Ucum(String::from("us")))),
            Some(true)
        );
        let day = q(1, Unit::Calendar(Calendar::Day));
        assert_eq!(
            day.equals(&q(24, Unit::Ucum(String::from("h")))),
            Some(false)
        );
        assert_eq!(day.compare(&q(24, Unit::Ucum(String::from("h")))), None);
        assert_eq!(
            day.equivalent(&q(24, Unit::Ucum(String::from("h")))),
            Some(true)
        );
        assert_eq!(day.equals(&q(1, Unit::Ucum(String::from("g")))), None);
    }

    #[test]
    fn convert_follows_ucum_and_the_calendar_factors() {
        // N1 §5.5.7.
        let inch = q(1, Unit::Ucum(String::from("[in_i]")));
        let centimetres = Quantity::new(Decimal::new(254, 2), Unit::Ucum(String::from("cm")));
        assert_eq!(
            inch.convert(&Unit::Ucum(String::from("cm"))),
            Some(centimetres)
        );
        let second = q(1, Unit::Calendar(Calendar::Second));
        assert_eq!(
            second.convert(&Unit::Ucum(String::from("ms"))),
            Some(q(1000, Unit::Ucum(String::from("ms"))))
        );
        assert_eq!(
            q(2, Unit::Calendar(Calendar::Year)).convert(&Unit::Calendar(Calendar::Month)),
            Some(q(24, Unit::Calendar(Calendar::Month)))
        );
        assert_eq!(
            q(1, Unit::Calendar(Calendar::Day)).convert(&Unit::Ucum(String::from("h"))),
            None
        );
        assert_eq!(inch.convert(&Unit::Ucum(String::from("g"))), None);
    }

    #[test]
    fn a_special_unit_compares_but_adds_only_to_itself() {
        // UCUM §22.1: special units take part in no algebraic operation.
        let celsius = Quantity::new(Decimal::new(37, 0), Unit::Ucum(String::from("Cel")));
        let kelvin = Quantity::new(Decimal::new(31015, 2), Unit::Ucum(String::from("K")));
        assert_eq!(celsius.equals(&kelvin), Some(true));
        assert_eq!(celsius.add(&kelvin, false), None);
        assert_eq!(
            celsius.add(&celsius, false),
            Some(Quantity::new(
                Decimal::new(74, 0),
                Unit::Ucum(String::from("Cel"))
            ))
        );
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
