// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The date value (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.7): a
//! prefix and a date, dateTime or instant to any precision from the year,
//! and the range that precision implies.
//!
//! "The date parameter format is `yyyy-mm-ddThh:mm:ss[Z|(+|-)hh:mm]`" and
//! "any degree of precision can be provided, but it SHALL be populated from
//! the left, except that the minutes SHALL be present if an hour is present,
//! and you SHOULD provide a time zone if the time part is present". Each
//! part is held to the lexical form of the FHIR R4 `dateTime` and `instant`
//! (<https://hl7.org/fhir/R4/datatypes.html#dateTime>): a year from 0001, an
//! offset up to `14:00`, any number of fraction digits.

#[cfg(feature = "jiff")]
mod instant;
mod parse;
mod range;

use core::fmt;

#[cfg(feature = "jiff")]
use jiff::Timestamp;

use crate::escape::{EscapeError, unescape};
use crate::prefix::{Prefix, PrefixError};

/// Why a date value is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DateError {
    /// The value carries an illegal escape.
    #[error(transparent)]
    Escape(#[from] EscapeError),
    /// The value opens with an unknown prefix.
    #[error(transparent)]
    Prefix(#[from] PrefixError),
    /// The value is not of the form `yyyy[-mm[-dd[Thh:mm[:ss[.f+]][Z|±hh:mm]]]]`.
    #[error(
        "the value is not of the form yyyy, yyyy-mm, yyyy-mm-dd or yyyy-mm-ddThh:mm[:ss[.f]][Z|±hh:mm]"
    )]
    Form,
    /// The value has the form and names a day the calendar does not have,
    /// such as 30 February.
    #[error("the value names a day the calendar does not have")]
    Calendar,
    /// The value names the 60th second of a minute, which the FHIR form
    /// admits and no civil clock of this crate holds.
    #[error("the value names a leap second")]
    LeapSecond,
    /// The value is outside the instants this crate represents, years
    /// -9999 to 9999 on the UTC time line.
    #[error("the value is outside the representable instants")]
    Unrepresentable,
}

/// One date value: its prefix and its date.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DateValue {
    prefix: Prefix,
    date: SearchDate,
}

impl DateValue {
    /// Reads one date value as [`split`](crate::escape::split) gave it: its
    /// escapes, its optional prefix and its date.
    ///
    /// # Errors
    ///
    /// A [`DateError`] for an illegal escape, an unknown prefix, or a date
    /// outside the form or the calendar.
    pub fn parse(text: &str) -> Result<Self, DateError> {
        let plain = unescape(text)?;
        let (prefix, rest) = Prefix::split(&plain)?;
        Ok(Self {
            prefix,
            date: SearchDate::parse(rest)?,
        })
    }

    /// The prefix, [`Prefix::Eq`] when none is written.
    #[must_use]
    pub fn prefix(&self) -> Prefix {
        self.prefix
    }

    /// The date.
    #[must_use]
    pub fn date(&self) -> &SearchDate {
        &self.date
    }
}

/// The range a date's precision implies, `[low, high)`, each bound written
/// at the date's own precision with the date's offset.
///
/// "The date 2015-08-12 has a range from 00:00 to 00:00 exclusive"
/// (§3.1.1.4.5), so `high` is the first date after the range. It is `None`
/// when that date is past the year 9999, so the range runs to the end of the
/// calendar.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Range {
    low: SearchDate,
    high: Option<SearchDate>,
}

impl Range {
    /// The first date of the range: the date itself.
    #[must_use]
    pub fn low(&self) -> &SearchDate {
        &self.low
    }

    /// The first date after the range, `None` past the year 9999.
    #[must_use]
    pub fn high(&self) -> Option<&SearchDate> {
        self.high.as_ref()
    }
}

/// The instants a date's range holds, `[from, until)`, on the UTC time line
/// at nanosecond resolution (feature `jiff`).
///
/// `until` is `None` when the range runs past the last representable
/// instant.
#[cfg(feature = "jiff")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Instants {
    from: Timestamp,
    until: Option<Timestamp>,
}

#[cfg(feature = "jiff")]
impl Instants {
    /// The first instant of the range.
    #[must_use]
    pub fn from(self) -> Timestamp {
        self.from
    }

    /// The first instant after the range, `None` when none is representable.
    #[must_use]
    pub fn until(self) -> Option<Timestamp> {
        self.until
    }
}

/// How much of a date a value writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Precision {
    /// `yyyy`.
    Year,
    /// `yyyy-mm`.
    Month,
    /// `yyyy-mm-dd`.
    Day,
    /// `yyyy-mm-ddThh:mm`.
    Minute,
    /// `yyyy-mm-ddThh:mm:ss`.
    Second,
    /// `yyyy-mm-ddThh:mm:ss.f`, with this many fraction digits.
    Fraction(usize),
}

/// The offset a time writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Offset {
    /// `Z`.
    Utc,
    /// `+hh:mm`.
    Plus {
        /// The hours, 0 to 14.
        hours: u8,
        /// The minutes, 0 to 59.
        minutes: u8,
    },
    /// `-hh:mm`.
    Minus {
        /// The hours, 0 to 14.
        hours: u8,
        /// The minutes, 0 to 59.
        minutes: u8,
    },
}

impl Offset {
    /// The offset from UTC in seconds, east positive.
    #[must_use]
    pub fn seconds(self) -> i32 {
        match self {
            Self::Utc => 0,
            Self::Plus { hours, minutes } => i32::from(hours) * 3600 + i32::from(minutes) * 60,
            Self::Minus { hours, minutes } => -(i32::from(hours) * 3600 + i32::from(minutes) * 60),
        }
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Utc => f.write_str("Z"),
            Self::Plus { hours, minutes } => write!(f, "+{hours:02}:{minutes:02}"),
            Self::Minus { hours, minutes } => write!(f, "-{hours:02}:{minutes:02}"),
        }
    }
}

/// The date of a date value, to the precision it was written in.
///
/// `Display` writes it back as it was read. `Debug` shows its precision and
/// offset and never the date, which may be a birth date.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SearchDate {
    year: i16,
    month: Option<i8>,
    day: Option<i8>,
    time: Option<Clock>,
}

/// The time part of a [`SearchDate`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Clock {
    hour: i8,
    minute: i8,
    second: Option<Second>,
    offset: Option<Offset>,
}

/// The seconds of a [`Clock`], with the fraction digits as written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Second {
    whole: i8,
    fraction: Option<String>,
}

impl SearchDate {
    /// How much of a date the value writes.
    #[must_use]
    pub fn precision(&self) -> Precision {
        match (&self.month, &self.day, &self.time) {
            (None, _, _) => Precision::Year,
            (Some(_), None, _) => Precision::Month,
            (Some(_), Some(_), None) => Precision::Day,
            (Some(_), Some(_), Some(clock)) => match &clock.second {
                None => Precision::Minute,
                Some(Second { fraction: None, .. }) => Precision::Second,
                Some(Second {
                    fraction: Some(digits),
                    ..
                }) => Precision::Fraction(digits.len()),
            },
        }
    }

    /// The calendar day the date starts on: its day, or the first day of
    /// its month or year (feature `jiff`).
    ///
    /// A consumer that compares days, such as a birth date's, reads a range
    /// as `range().low().first_day()` up to `range().high()`'s.
    ///
    /// # Panics
    ///
    /// Never: every date this crate holds is a calendar date jiff represents.
    #[cfg(feature = "jiff")]
    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "parse admits years 0001 to 9999 and checks the day against the Gregorian calendar, and next() steps within it, all of which jiff's Date holds"
    )]
    pub fn first_day(&self) -> jiff::civil::Date {
        jiff::civil::Date::new(self.year, self.month.unwrap_or(1), self.day.unwrap_or(1))
            .expect("a date read by parse or stepped by next should be a calendar date")
    }

    /// The offset the time writes, `None` for a date with no time or a time
    /// with no offset.
    #[must_use]
    pub fn offset(&self) -> Option<Offset> {
        self.time.as_ref().and_then(|clock| clock.offset)
    }
}

impl fmt::Display for SearchDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}", self.year)?;
        if let Some(month) = self.month {
            write!(f, "-{month:02}")?;
        }
        if let Some(day) = self.day {
            write!(f, "-{day:02}")?;
        }
        if let Some(clock) = &self.time {
            write!(f, "T{:02}:{:02}", clock.hour, clock.minute)?;
            if let Some(second) = &clock.second {
                write!(f, ":{:02}", second.whole)?;
                if let Some(digits) = &second.fraction {
                    write!(f, ".{digits}")?;
                }
            }
            if let Some(offset) = clock.offset {
                write!(f, "{offset}")?;
            }
        }
        Ok(())
    }
}

impl fmt::Debug for SearchDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SearchDate")
            .field("precision", &self.precision())
            .field("offset", &self.offset())
            .finish_non_exhaustive()
    }
}
