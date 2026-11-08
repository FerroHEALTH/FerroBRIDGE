// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The range a date's precision implies (§3.1.1.4.5 and §3.1.1.4.7).

use jiff::ToSpan;
use jiff::civil::{Date, DateTime};

use super::{Clock, Range, SearchDate, Second};

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
        // NOTE: no specification governs this: our own design; the fields were
        // checked at parse, so jiff refuses only a step past 9999-12-31.
        let (Some(month), Some(day)) = (self.month, self.day) else {
            return self.next_month_or_year();
        };
        let Some(clock) = &self.time else {
            let next = Date::new(self.year, month, day)
                .ok()?
                .checked_add(1.day())
                .ok()?;
            return Self::on(next, None);
        };
        let whole = clock.second.as_ref().map_or(0, |second| second.whole);
        let now = DateTime::new(self.year, month, day, clock.hour, clock.minute, whole, 0).ok()?;
        let (next, fraction) = match &clock.second {
            None => (now.checked_add(1.minute()).ok()?, None),
            Some(Second { fraction: None, .. }) => (now.checked_add(1.second()).ok()?, None),
            Some(Second {
                fraction: Some(digits),
                ..
            }) => match increment(digits) {
                Some(digits) => (now, Some(digits)),
                None => (
                    now.checked_add(1.second()).ok()?,
                    Some("0".repeat(digits.len())),
                ),
            },
        };
        let second = clock.second.as_ref().map(|_| Second {
            whole: next.second(),
            fraction,
        });
        Self::on(
            next.date(),
            Some(Clock {
                hour: next.hour(),
                minute: next.minute(),
                second,
                offset: clock.offset,
            }),
        )
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

    /// The date of `date` with `time`, `None` past the year 9999.
    fn on(date: Date, time: Option<Clock>) -> Option<Self> {
        (date.year() <= LAST_YEAR).then_some(Self {
            year: date.year(),
            month: Some(date.month()),
            day: Some(date.day()),
            time,
        })
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
