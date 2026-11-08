// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The range a date's precision implies (§3.1.1.4.5 and §3.1.1.4.7).

use super::{Clock, Range, SearchDate};

/// The last year a FHIR date writes.
const LAST_YEAR: i16 = 9999;

impl SearchDate {
    /// The range the date's precision implies.
    ///
    /// "Dates with only the year specified are equivalent to an interval
    /// that starts at the first instant of January 1st to the last instant
    /// of December 31st" (§3.1.1.4.7), and the same holds for a month, a day,
    /// a minute, a second and each fraction digit, so `2026` is `[2026,
    /// 2027)` and `2026-09-15T09:00:00.25` is `[…00.25, …00.26)`. The bounds
    /// keep the date's precision and offset, so a caller that compares
    /// lexically compares like with like.
    #[must_use]
    pub fn range(&self) -> Range {
        Range {
            low: self.clone(),
            high: self.next(),
        }
    }

    /// The first date after this one at the same precision, `None` past the
    /// year 9999.
    fn next(&self) -> Option<Self> {
        let (Some(month), Some(day)) = (self.month, self.day) else {
            return self.next_month_or_year();
        };
        let Some(clock) = &self.time else {
            let (year, month, day) = next_day(self.year, month, day)?;
            return Some(Self {
                year,
                month: Some(month),
                day: Some(day),
                time: None,
            });
        };
        let mut next = clock.clone();
        let into_day = match &mut next.second {
            None => step_minute(&mut next),
            Some(second) => {
                let read = second
                    .fraction
                    .as_deref()
                    .map(|digits| (increment(digits), digits.len()));
                let carried = match read {
                    None => true,
                    Some((Some(raised), _)) => {
                        second.fraction = Some(raised);
                        false
                    }
                    Some((None, width)) => {
                        second.fraction = Some("0".repeat(width));
                        true
                    }
                };
                carried && step_second(&mut next)
            }
        };
        let (year, month, day) = if into_day {
            next_day(self.year, month, day)?
        } else {
            (self.year, month, day)
        };
        Some(Self {
            year,
            month: Some(month),
            day: Some(day),
            time: Some(next),
        })
    }

    /// The next month or year of a date written to the month or the year.
    fn next_month_or_year(&self) -> Option<Self> {
        let (year, month) = match self.month {
            None => (self.year.checked_add(1)?, None),
            Some(12) => (self.year.checked_add(1)?, Some(1)),
            Some(month) => (self.year, Some(month.checked_add(1)?)),
        };
        (year <= LAST_YEAR).then_some(Self {
            year,
            month,
            day: None,
            time: None,
        })
    }
}

/// Moves `clock` on by one second, and answers whether that carried into
/// the next day.
fn step_second(clock: &mut Clock) -> bool {
    let Some(second) = clock.second.as_mut() else {
        return step_minute(clock);
    };
    if second.whole < 59 {
        second.whole += 1;
        return false;
    }
    second.whole = 0;
    step_minute(clock)
}

/// Moves `clock` on by one minute, and answers whether that carried into the
/// next day.
fn step_minute(clock: &mut Clock) -> bool {
    if clock.minute < 59 {
        clock.minute += 1;
        return false;
    }
    clock.minute = 0;
    if clock.hour < 23 {
        clock.hour += 1;
        return false;
    }
    clock.hour = 0;
    true
}

/// The day after `year-month-day`, `None` past 9999-12-31.
fn next_day(year: i16, month: i8, day: i8) -> Option<(i16, i8, i8)> {
    if day < days_in_month(year, month) {
        return Some((year, month, day + 1));
    }
    if month < 12 {
        return Some((year, month + 1, 1));
    }
    (year < LAST_YEAR).then_some((year + 1, 1, 1))
}

/// The days of `month` in `year` in the proleptic Gregorian calendar, the
/// calendar of XML Schema and so of the FHIR date types.
pub(super) fn days_in_month(year: i16, month: i8) -> i8 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The fraction digits one unit of the last digit after `digits`, `None`
/// when every digit carries over into the next second.
fn increment(digits: &str) -> Option<String> {
    let mut bytes = digits.as_bytes().to_vec();
    let raised = bytes.iter().rposition(|byte| *byte != b'9')?;
    for (at, byte) in bytes.iter_mut().enumerate().skip(raised) {
        *byte = if at == raised { *byte + 1 } else { b'0' };
    }
    Some(bytes.into_iter().map(char::from).collect())
}
