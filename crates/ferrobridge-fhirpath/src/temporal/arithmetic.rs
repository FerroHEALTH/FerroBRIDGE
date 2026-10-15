// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Adding a time-valued quantity to a date, a datetime or a time (N1 §6.7).
//!
//! Calendar semantics apply above the second and definite ones at and below
//! it. A quantity finer than the value's precision is first converted to that
//! precision by the factors of N1 §5.5.7 and truncated, so `@2014 + 23
//! months` is `@2015`; above the second the decimal part of the quantity is
//! dropped. A day that the resulting month lacks becomes the month's last.

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use super::{Clock, Date, DateTime, Precision, Seconds, Time};
use crate::quantity::Calendar;

/// Why a date or time could not be moved by a quantity.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ArithmeticError {
    /// The unit does not apply to the value, as an hour does not to a date
    /// (N1 §6.7.1).
    #[error("a {value} cannot be moved by a quantity in {unit}")]
    Unit {
        /// The kind of value.
        value: &'static str,
        /// The unit's keyword.
        unit: &'static str,
    },
    /// The result leaves the range 0001 to 9999 (N1 §6.7).
    #[error("the result lies outside the years 0001 to 9999")]
    Range,
    /// The calendar arithmetic itself failed, as for a span past its limits.
    #[error("the calendar arithmetic failed")]
    Calendar {
        /// The calendar library's failure.
        #[source]
        source: jiff::Error,
    },
}

/// The calendar keyword matching a precision; a second with a fraction is a
/// millisecond.
const fn step(precision: Precision, fraction: bool) -> Calendar {
    match precision {
        Precision::Year => Calendar::Year,
        Precision::Month => Calendar::Month,
        Precision::Day => Calendar::Day,
        Precision::Hour => Calendar::Hour,
        Precision::Minute => Calendar::Minute,
        Precision::Second if fraction => Calendar::Millisecond,
        Precision::Second => Calendar::Second,
    }
}

/// The whole number of `unit` steps to apply to a value whose finest
/// precision is `finest`: the quantity converted to that precision when it is
/// finer, then truncated (N1 §6.7.1).
fn steps(
    amount: Decimal,
    unit: Calendar,
    finest: Calendar,
) -> Result<(i64, Calendar), ArithmeticError> {
    let (amount, unit) = if unit == Calendar::Week {
        (
            amount
                .checked_mul(Decimal::from(7))
                .ok_or(ArithmeticError::Range)?,
            Calendar::Day,
        )
    } else {
        (amount, unit)
    };
    let (amount, unit) = if unit > finest {
        (
            unit.convert(amount, finest).ok_or(ArithmeticError::Range)?,
            finest,
        )
    } else if unit == Calendar::Second && finest == Calendar::Millisecond {
        (
            amount
                .checked_mul(Decimal::from(1000))
                .ok_or(ArithmeticError::Range)?,
            Calendar::Millisecond,
        )
    } else {
        (amount, unit)
    };
    let whole = amount.trunc().to_i64().ok_or(ArithmeticError::Range)?;
    Ok((whole, unit))
}

/// A span of `count` steps of `unit`.
fn span(count: i64, unit: Calendar) -> Result<jiff::Span, ArithmeticError> {
    let span = jiff::Span::new();
    match unit {
        Calendar::Year => span.try_years(count),
        Calendar::Month => span.try_months(count),
        Calendar::Week => span.try_weeks(count),
        Calendar::Day => span.try_days(count),
        Calendar::Hour => span.try_hours(count),
        Calendar::Minute => span.try_minutes(count),
        Calendar::Second => span.try_seconds(count),
        Calendar::Millisecond => span.try_milliseconds(count),
    }
    .map_err(|source| ArithmeticError::Calendar { source })
}

/// The date moved by whole years, months or days.
fn move_date(date: Date, count: i64, unit: Calendar) -> Result<Date, ArithmeticError> {
    let civil = jiff::civil::Date::new(date.year, date.month.unwrap_or(1), date.day.unwrap_or(1))
        .map_err(|source| ArithmeticError::Calendar { source })?;
    let moved = civil
        .checked_add(span(count, unit)?)
        .map_err(|source| ArithmeticError::Calendar { source })?;
    in_range(Date {
        year: moved.year(),
        month: date.month.map(|_| moved.month()),
        day: date.day.map(|_| moved.day()),
    })
}

/// The date when its year lies in 0001 to 9999.
fn in_range(date: Date) -> Result<Date, ArithmeticError> {
    if (1..=9999).contains(&date.year) {
        Ok(date)
    } else {
        Err(ArithmeticError::Range)
    }
}

/// The nanoseconds a clock's fraction carries.
fn nanos(clock: &Clock) -> i32 {
    clock
        .second
        .and_then(|second| second.fraction)
        .map_or(0, |(nanos, _)| nanos)
}

/// The clock with the components it carries taken from `time`, a fraction
/// widened to milliseconds when milliseconds were added.
fn rewrite(clock: Clock, time: jiff::civil::Time, millis: bool) -> Clock {
    Clock {
        hour: time.hour(),
        minute: clock.minute.map(|_| time.minute()),
        second: clock.second.map(|second| Seconds {
            whole: time.second(),
            fraction: second.fraction.map(|(_, digits)| {
                let digits = if millis { digits.max(3) } else { digits };
                (time.subsec_nanosecond(), digits)
            }),
        }),
    }
}

impl Date {
    /// The date moved by `amount` of `unit` (N1 §6.7.1).
    ///
    /// # Errors
    ///
    /// Returns [`ArithmeticError::Unit`] for a unit below the day and
    /// [`ArithmeticError::Range`] for a result outside 0001 to 9999.
    pub fn add(self, amount: Decimal, unit: Calendar) -> Result<Self, ArithmeticError> {
        if unit > Calendar::Day {
            return Err(ArithmeticError::Unit {
                value: "date",
                unit: unit.keyword(),
            });
        }
        let (count, unit) = steps(amount, unit, step(self.precision(), false))?;
        move_date(self, count, unit)
    }
}

impl DateTime {
    /// The datetime moved by `amount` of `unit` (N1 §6.7.1), keeping its
    /// precision and offset.
    ///
    /// # Errors
    ///
    /// Returns [`ArithmeticError::Range`] for a result outside 0001 to 9999.
    pub fn add(&self, amount: Decimal, unit: Calendar) -> Result<Self, ArithmeticError> {
        let fraction = self
            .clock
            .and_then(|clock| clock.second)
            .is_some_and(|second| second.fraction.is_some());
        let (count, unit) = steps(amount, unit, step(self.precision(), fraction))?;
        let Some(clock) = self.clock else {
            return Ok(Self {
                date: move_date(self.date, count, unit)?,
                ..*self
            });
        };
        let civil = jiff::civil::DateTime::new(
            self.date.year,
            self.date.month.unwrap_or(1),
            self.date.day.unwrap_or(1),
            clock.hour,
            clock.minute.unwrap_or(0),
            clock.second.map_or(0, |second| second.whole),
            nanos(&clock),
        )
        .map_err(|source| ArithmeticError::Calendar { source })?;
        let moved = civil
            .checked_add(span(count, unit)?)
            .map_err(|source| ArithmeticError::Calendar { source })?;
        Ok(Self {
            date: in_range(Date {
                year: moved.year(),
                month: Some(moved.month()),
                day: Some(moved.day()),
            })?,
            clock: Some(rewrite(clock, moved.time(), unit == Calendar::Millisecond)),
            offset: self.offset,
        })
    }
}

impl Time {
    /// The time moved by `amount` of `unit` (N1 §6.7.1).
    ///
    /// NOTE: N1 §6.7 does not say what happens past midnight; no specification
    /// governs this: our own design, the time wraps around the day.
    ///
    /// # Errors
    ///
    /// Returns [`ArithmeticError::Unit`] for a unit above the hour.
    pub fn add(&self, amount: Decimal, unit: Calendar) -> Result<Self, ArithmeticError> {
        if unit < Calendar::Hour {
            return Err(ArithmeticError::Unit {
                value: "time",
                unit: unit.keyword(),
            });
        }
        let fraction = self
            .clock
            .second
            .is_some_and(|second| second.fraction.is_some());
        let (count, unit) = steps(amount, unit, step(self.precision(), fraction))?;
        let civil = jiff::civil::Time::new(
            self.clock.hour,
            self.clock.minute.unwrap_or(0),
            self.clock.second.map_or(0, |second| second.whole),
            nanos(&self.clock),
        )
        .map_err(|source| ArithmeticError::Calendar { source })?;
        let moved = civil.wrapping_add(span(count, unit)?);
        Ok(Self {
            clock: rewrite(self.clock, moved, unit == Calendar::Millisecond),
        })
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::super::{Date, DateTime};
    use crate::quantity::Calendar;

    fn date(text: &str) -> Date {
        Date::parse(text).expect("the date should read")
    }

    #[test]
    fn partial_dates_take_the_quantity_at_their_precision() {
        let add = |text: &str, n: i64, unit| {
            date(text)
                .add(Decimal::from(n), unit)
                .expect("the sum should exist")
                .to_string()
        };
        assert_eq!(add("2014", 24, Calendar::Month), "2016");
        assert_eq!(add("2014", 23, Calendar::Month), "2015");
        assert_eq!(add("2016", 365, Calendar::Day), "2017");
        assert_eq!(add("2019-03-01", 24, Calendar::Month), "2021-03-01");
        assert_eq!(add("1973-12-25", 1, Calendar::Week), "1974-01-01");
    }

    #[test]
    fn a_missing_day_becomes_the_last_of_the_month() {
        let sum = date("2015-01-31").add(Decimal::ONE, Calendar::Month);
        assert_eq!(
            sum.ok().map(|d| d.to_string()),
            Some(String::from("2015-02-28"))
        );
    }

    #[test]
    fn a_datetime_keeps_its_offset_and_fraction() {
        let value = DateTime::parse("1973-12-25T00:00:00.000+10:00").expect("datetime");
        let sum = value.add(Decimal::from(10), Calendar::Millisecond);
        assert_eq!(
            sum.ok().map(|d| d.to_string()),
            Some(String::from("1973-12-25T00:00:00.010+10:00"))
        );
    }

    #[test]
    fn an_hour_does_not_move_a_date() {
        assert!(
            date("2015-01-01")
                .add(Decimal::ONE, Calendar::Hour)
                .is_err()
        );
    }
}
