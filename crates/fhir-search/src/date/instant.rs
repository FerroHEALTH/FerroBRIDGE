// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The instants a date's range holds, and the `ap` approximation.

use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, ToSpan};

use super::{DateError, Instants, SearchDate};

/// The digits of a fraction an instant holds.
const NANO_DIGITS: usize = 9;

impl SearchDate {
    /// The instants the date's range holds.
    ///
    /// A date with an offset is read at that offset. A date with none, a
    /// date with no time or a time without an offset, is read in `zone`:
    /// "where both search parameters and resource element date times do not
    /// have time zones, the servers local time zone should be assumed"
    /// (§3.1.1.4.7), so `zone` is the server's local zone. A range finer
    /// than a nanosecond holds the nanosecond instants at or after its low
    /// bound and before its high bound, so both bounds round up to the next
    /// nanosecond.
    ///
    /// # Errors
    ///
    /// [`DateError::Unrepresentable`] when the range starts outside the
    /// instants jiff represents.
    pub fn instants(&self, zone: &TimeZone) -> Result<Instants, DateError> {
        let range = self.range();
        let from = range.low.instant(zone).ok_or(DateError::Unrepresentable)?;
        // NOTE: no specification governs this: our own design; a high bound
        // past the last instant leaves the range open, since no later instant exists.
        let until = range.high.and_then(|high| high.instant(zone));
        Ok(Instants { from, until })
    }

    /// The instants an `ap` value matches: the range widened on each side by
    /// a tenth of the gap between `now` and the range, nothing when `now`
    /// lies within it.
    ///
    /// "The recommended value for the approximation is ... for a date, 10%
    /// of the gap between now and the date, but systems may choose other
    /// values where appropriate" (§3.1.1.4.5). The gap is measured from the
    /// nearer bound of the range. No specification fixes that reading: our
    /// own design.
    ///
    /// # Errors
    ///
    /// [`DateError::Unrepresentable`] as for [`SearchDate::instants`].
    pub fn approximate(&self, zone: &TimeZone, now: Timestamp) -> Result<Instants, DateError> {
        let exact = self.instants(zone)?;
        let gap = if now < exact.from {
            exact.from.duration_since(now)
        } else {
            match exact.until {
                Some(until) if now >= until => now.duration_since(until),
                _ => SignedDuration::ZERO,
            }
        };
        let margin = gap / 10;
        // NOTE: no specification governs this: our own design; a margin past
        // either end of the time line stops there, since no instant lies beyond.
        let from = exact.from.checked_sub(margin).unwrap_or(Timestamp::MIN);
        let until = exact.until.and_then(|until| until.checked_add(margin).ok());
        Ok(Instants { from, until })
    }

    /// The first instant at or after this date's first instant, at
    /// nanosecond resolution, or `None` outside the instants jiff holds.
    fn instant(&self, zone: &TimeZone) -> Option<Timestamp> {
        // NOTE: no specification governs this: our own design; the fields were
        // checked at parse, so jiff refuses only a date outside its time line.
        let (seconds, nanos, round_up) =
            match self.time.as_ref().and_then(|clock| clock.second.as_ref()) {
                None => (0, 0, false),
                Some(second) => {
                    let digits = second.fraction.as_deref().unwrap_or("");
                    let (kept, rest) = digits.split_at_checked(NANO_DIGITS).unwrap_or((digits, ""));
                    let nanos: i32 = format!("{kept:0<NANO_DIGITS$}").parse().ok()?;
                    (second.whole, nanos, rest.bytes().any(|byte| byte != b'0'))
                }
            };
        let (hour, minute) = self
            .time
            .as_ref()
            .map_or((0, 0), |clock| (clock.hour, clock.minute));
        let civil = DateTime::new(
            self.year,
            self.month.unwrap_or(1),
            self.day.unwrap_or(1),
            hour,
            minute,
            seconds,
            nanos,
        )
        .ok()?;
        let civil = if round_up {
            civil.checked_add(1.nanosecond()).ok()?
        } else {
            civil
        };
        let instant = match self.offset() {
            Some(offset) => jiff::tz::Offset::from_seconds(offset.seconds())
                .ok()?
                .to_timestamp(civil)
                .ok()?,
            None => civil.to_zoned(zone.clone()).ok()?.timestamp(),
        };
        Some(instant)
    }
}
