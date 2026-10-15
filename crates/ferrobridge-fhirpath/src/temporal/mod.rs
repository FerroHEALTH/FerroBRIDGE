// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `System.Date`, `System.DateTime` and `System.Time`, each with the
//! precision its value was written to (N1 §4.1.5 to §4.1.7).
//!
//! A value keeps every component it was given and no other, so a comparison
//! across precisions can answer "unknown" (N1 §6.1.1, §6.2). Seconds keep
//! their fraction as written, and the second and its fraction are one
//! precision compared as a decimal.

pub mod arithmetic;
mod compare;
pub mod parse;

use std::fmt;

use rust_decimal::Decimal;

/// A calendar date to year, month or day precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    year: i16,
    month: Option<i8>,
    day: Option<i8>,
}

/// The seconds of a clock reading, with the fraction as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seconds {
    whole: i8,
    /// The fraction in nanoseconds, with the number of digits it was written
    /// with; `None` when no fraction was written.
    fraction: Option<(i32, u8)>,
}

/// A time of day to hour, minute, second or fractional-second precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Clock {
    hour: i8,
    minute: Option<i8>,
    second: Option<Seconds>,
}

/// A timezone offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Offset {
    /// `Z`.
    Utc,
    /// `(+|-)hh:mm`, in minutes east of UTC.
    Minutes(i16),
}

impl Offset {
    /// The offset in minutes east of UTC.
    const fn minutes(self) -> i16 {
        match self {
            Self::Utc => 0,
            Self::Minutes(minutes) => minutes,
        }
    }
}

/// A date and time, partial from the year on, with an optional offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    date: Date,
    clock: Option<Clock>,
    offset: Option<Offset>,
}

/// A time of day without a timezone (N1 §4.1.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    clock: Clock,
}

/// The precisions of a date or time value, coarsest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precision {
    /// The year.
    Year,
    /// The month.
    Month,
    /// The day.
    Day,
    /// The hour.
    Hour,
    /// The minute.
    Minute,
    /// The second, with or without a fraction.
    Second,
}

impl Date {
    /// The precision the date carries.
    #[must_use]
    pub const fn precision(self) -> Precision {
        match (self.month, self.day) {
            (None, _) => Precision::Year,
            (Some(_), None) => Precision::Month,
            (Some(_), Some(_)) => Precision::Day,
        }
    }

    /// The date as a `DateTime` with no time components, the implicit
    /// conversion of N1 §5.5.
    #[must_use]
    pub const fn to_date_time(self) -> DateTime {
        DateTime {
            date: self,
            clock: None,
            offset: None,
        }
    }
}

impl Clock {
    /// The precision the clock carries.
    const fn precision(&self) -> Precision {
        match (self.minute, self.second) {
            (None, _) => Precision::Hour,
            (Some(_), None) => Precision::Minute,
            (Some(_), Some(_)) => Precision::Second,
        }
    }

    /// The seconds as a decimal, the one precision N1 §6.1.1 compares them as.
    fn seconds(&self) -> Option<Decimal> {
        self.second.map(|second| {
            let whole = Decimal::from(second.whole);
            match second.fraction {
                Some((nanos, _)) => whole + Decimal::new(i64::from(nanos), 9),
                None => whole,
            }
        })
    }
}

impl DateTime {
    /// The precision the value carries.
    #[must_use]
    pub const fn precision(&self) -> Precision {
        match &self.clock {
            Some(clock) => clock.precision(),
            None => self.date.precision(),
        }
    }

    /// The date part, the conversion `toDate()` makes (N1 §5.5.4).
    #[must_use]
    pub const fn date(&self) -> Date {
        self.date
    }

    /// Whether the value carries a timezone offset.
    #[must_use]
    pub const fn has_offset(&self) -> bool {
        self.offset.is_some()
    }
}

impl Time {
    /// The precision the time carries.
    #[must_use]
    pub const fn precision(&self) -> Precision {
        self.clock.precision()
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}", self.year)?;
        if let Some(month) = self.month {
            write!(f, "-{month:02}")?;
        }
        if let Some(day) = self.day {
            write!(f, "-{day:02}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}", self.hour)?;
        if let Some(minute) = self.minute {
            write!(f, ":{minute:02}")?;
        }
        if let Some(second) = self.second {
            write!(f, ":{:02}", second.whole)?;
            if let Some((nanos, digits)) = second.fraction {
                let all = format!("{nanos:09}");
                let shown = all.get(..usize::from(digits.clamp(1, 9))).unwrap_or(&all);
                write!(f, ".{shown}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Utc => f.write_str("Z"),
            Self::Minutes(minutes) => {
                let sign = if *minutes < 0 { '-' } else { '+' };
                let magnitude = minutes.unsigned_abs();
                write!(
                    f,
                    "{sign}{:02}:{:02}",
                    magnitude.div_euclid(60),
                    magnitude.rem_euclid(60)
                )
            }
        }
    }
}

impl fmt::Display for DateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.date)?;
        if let Some(clock) = &self.clock {
            write!(f, "T{clock}")?;
            if let Some(offset) = &self.offset {
                write!(f, "{offset}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.clock)
    }
}
