// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The precision and the boundaries of a date or time value, for the
//! FHIRPath 3.0.0 functions `precision()`, `lowBoundary()` and
//! `highBoundary()` (<http://hl7.org/fhirpath/STU3/#utility-functions>).
//!
//! A precision counts the digits of the value as written: 4 for a year, 6 to
//! the month, 8 to the day, then 10, 12 and 14 to the hour, minute and second
//! of a date and time and 17 with its milliseconds; a time counts 2, 4, 6 and
//! 9 the same way.

use super::{Clock, Date, DateTime, Precision, Seconds, Time};

/// The end of the range of values a partial value stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    /// The least value.
    Low,
    /// The greatest value.
    High,
}

impl Boundary {
    /// `low` at the low end, `high` at the high end.
    fn pick<T>(self, low: T, high: T) -> T {
        match self {
            Self::Low => low,
            Self::High => high,
        }
    }
}

/// The milliseconds a boundary to precision 17 or 9 carries at each end.
const MILLIS: (i32, i32) = (0, 999_000_000);

/// The number of days in `month` of `year`, `None` outside the calendar.
fn days_in(year: i16, month: i8) -> Option<i8> {
    jiff::civil::Date::new(year, month, 1)
        .ok()
        .map(jiff::civil::Date::days_in_month)
}

impl Date {
    /// The digits of precision of the date: 4, 6 or 8.
    #[must_use]
    pub const fn digits(self) -> u8 {
        match self.precision() {
            Precision::Year => 4,
            Precision::Month => 6,
            _ => 8,
        }
    }

    /// The least or greatest date to `digits` of precision (4, 6 or 8),
    /// `None` for any other count.
    #[must_use]
    pub fn boundary(self, end: Boundary, digits: u8) -> Option<Self> {
        let month = || self.month.unwrap_or(end.pick(1, 12));
        Some(match digits {
            4 => Self {
                year: self.year,
                month: None,
                day: None,
            },
            6 => Self {
                year: self.year,
                month: Some(month()),
                day: None,
            },
            8 => {
                let month = month();
                let day = match self.day {
                    Some(day) => day,
                    None => end.pick(Some(1), days_in(self.year, month))?,
                };
                Self {
                    year: self.year,
                    month: Some(month),
                    day: Some(day),
                }
            }
            _ => return None,
        })
    }
}

impl Clock {
    /// The digits of precision of the clock alone: 2, 4, 6 or 9.
    const fn digits(&self) -> u8 {
        match (self.minute, self.second) {
            (None, _) => 2,
            (Some(_), None) => 4,
            (Some(_), Some(Seconds { fraction: None, .. })) => 6,
            (
                Some(_),
                Some(Seconds {
                    fraction: Some(_), ..
                }),
            ) => 9,
        }
    }

    /// The least or greatest clock reading to `digits` of the clock alone
    /// (2, 4, 6 or 9), `None` for any other count.
    fn boundary(clock: Option<&Self>, end: Boundary, digits: u8) -> Option<Self> {
        let hour = clock.map_or(end.pick(0, 23), |clock| clock.hour);
        let minute = clock
            .and_then(|clock| clock.minute)
            .unwrap_or(end.pick(0, 59));
        let second = clock.and_then(|clock| clock.second);
        let whole = second.map_or(end.pick(0, 59), |second| second.whole);
        let fraction = second
            .and_then(|second| second.fraction)
            .unwrap_or((end.pick(MILLIS.0, MILLIS.1), 3));
        Some(match digits {
            2 => Self {
                hour,
                minute: None,
                second: None,
            },
            4 => Self {
                hour,
                minute: Some(minute),
                second: None,
            },
            6 => Self {
                hour,
                minute: Some(minute),
                second: Some(Seconds {
                    whole,
                    fraction: None,
                }),
            },
            9 => Self {
                hour,
                minute: Some(minute),
                second: Some(Seconds {
                    whole,
                    fraction: Some(fraction),
                }),
            },
            _ => return None,
        })
    }
}

impl DateTime {
    /// The digits of precision of the value: 4, 6 or 8 to the day, then 10,
    /// 12, 14 or 17.
    #[must_use]
    pub const fn digits(&self) -> u8 {
        match &self.clock {
            None => self.date.digits(),
            Some(clock) => match clock.digits() {
                9 => 17,
                digits => 8 + digits,
            },
        }
    }

    /// The least or greatest value to `digits` of precision (4, 6, 8, 10,
    /// 12, 14 or 17), `None` for any other count.
    ///
    /// The offset stays the one the value carries, as the FHIRPath 3.0.0
    /// example `@2014-01-01T08.lowBoundary(17)` answers
    /// `@2014-01-01T08:00:00.000`; a value to the day carries none.
    #[must_use]
    pub fn boundary(&self, end: Boundary, digits: u8) -> Option<Self> {
        if digits <= 8 {
            return Some(Self {
                date: self.date.boundary(end, digits)?,
                clock: None,
                offset: None,
            });
        }
        let clock_digits = match digits {
            17 => 9,
            10 | 12 | 14 => digits - 8,
            _ => return None,
        };
        Some(Self {
            date: self.date.boundary(end, 8)?,
            clock: Some(Clock::boundary(self.clock.as_ref(), end, clock_digits)?),
            offset: self.offset,
        })
    }
}

impl Time {
    /// The digits of precision of the time: 2, 4, 6 or 9.
    #[must_use]
    pub const fn digits(&self) -> u8 {
        self.clock.digits()
    }

    /// The least or greatest time to `digits` of precision (2, 4, 6 or 9),
    /// `None` for any other count.
    #[must_use]
    pub fn boundary(&self, end: Boundary, digits: u8) -> Option<Self> {
        Some(Self {
            clock: Clock::boundary(Some(&self.clock), end, digits)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Boundary;
    use crate::temporal::{Date, DateTime, Time};

    #[test]
    fn a_year_widens_to_its_months_and_days() {
        let year = Date::parse("2014").expect("a date");
        assert_eq!(
            year.boundary(Boundary::Low, 6).map(|d| d.to_string()),
            Some(String::from("2014-01"))
        );
        assert_eq!(
            year.boundary(Boundary::High, 8).map(|d| d.to_string()),
            Some(String::from("2014-12-31"))
        );
        assert_eq!(year.boundary(Boundary::High, 5), None);
    }

    #[test]
    fn february_ends_on_its_last_day() {
        let month = Date::parse("2024-02").expect("a date");
        assert_eq!(
            month.boundary(Boundary::High, 8).map(|d| d.to_string()),
            Some(String::from("2024-02-29"))
        );
    }

    #[test]
    fn an_hour_widens_to_the_millisecond() {
        let hour = DateTime::parse("2014-01-01T08").expect("a datetime");
        assert_eq!(hour.digits(), 10);
        assert_eq!(
            hour.boundary(Boundary::Low, 17).map(|d| d.to_string()),
            Some(String::from("2014-01-01T08:00:00.000"))
        );
        assert_eq!(
            hour.boundary(Boundary::High, 17).map(|d| d.to_string()),
            Some(String::from("2014-01-01T08:59:59.999"))
        );
        assert_eq!(
            hour.boundary(Boundary::Low, 8).map(|d| d.to_string()),
            Some(String::from("2014-01-01"))
        );
    }

    #[test]
    fn a_time_widens_and_counts_its_digits() {
        let minute = Time::parse("10:30").expect("a time");
        assert_eq!(minute.digits(), 4);
        assert_eq!(
            minute.boundary(Boundary::High, 9).map(|t| t.to_string()),
            Some(String::from("10:30:59.999"))
        );
    }
}
