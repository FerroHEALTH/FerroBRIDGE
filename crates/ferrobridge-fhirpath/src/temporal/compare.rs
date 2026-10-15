// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Equality, equivalence and ordering of dates and times across precisions.
//!
//! N1 §6.1.1 compares precision by precision from the coarsest: a difference
//! decides, a precision one value has and the other lacks answers "unknown",
//! and two values ending at the same precision are equal. The second and its
//! fraction are one precision compared as a decimal.

use std::cmp::Ordering;

use rust_decimal::Decimal;

use super::{Date, DateTime, Time};

/// The components of a value, coarsest first, `None` past its precision.
type Components = [Option<Decimal>; 6];

/// The components of a date.
fn date_components(date: Date) -> Components {
    [
        Some(Decimal::from(date.year)),
        date.month.map(Decimal::from),
        date.day.map(Decimal::from),
        None,
        None,
        None,
    ]
}

/// The components of a datetime, shifted to UTC when `to_utc` holds and the
/// value carries an offset and an hour.
fn date_time_components(value: &DateTime, to_utc: bool) -> Option<Components> {
    let Some(clock) = &value.clock else {
        return Some(date_components(value.date));
    };
    let shifted = match (to_utc, value.offset) {
        (true, Some(offset)) => shift(value, offset.minutes())?,
        _ => *value,
    };
    let date = shifted.date;
    let clock_shifted = shifted.clock.unwrap_or(*clock);
    let mut components = date_components(date);
    components[3] = Some(Decimal::from(clock_shifted.hour));
    components[4] = clock_shifted.minute.map(Decimal::from);
    components[5] = clock_shifted.seconds();
    Some(components)
}

/// The value moved by `-minutes`, keeping which components it carries.
fn shift(value: &DateTime, minutes: i16) -> Option<DateTime> {
    let clock = value.clock?;
    let civil = jiff::civil::DateTime::new(
        value.date.year,
        value.date.month?,
        value.date.day?,
        clock.hour,
        clock.minute.unwrap_or(0),
        0,
        0,
    )
    .ok()?;
    let moved = civil
        .checked_sub(jiff::Span::new().minutes(i64::from(minutes)))
        .ok()?;
    let mut result = *value;
    result.date.year = moved.year();
    result.date.month = Some(moved.month());
    result.date.day = Some(moved.day());
    if let Some(shifted) = result.clock.as_mut() {
        shifted.hour = moved.hour();
        if shifted.minute.is_some() {
            shifted.minute = Some(moved.minute());
        }
    }
    Some(result)
}

/// The component walk of N1 §6.1.1: `Some` with the order at the first
/// difference or at a shared final precision, `None` when one value stops
/// before the other.
fn walk(left: &Components, right: &Components) -> Option<Ordering> {
    for (a, b) in left.iter().zip(right) {
        match (a, b) {
            (Some(a), Some(b)) => {
                if a != b {
                    return Some(a.cmp(b));
                }
            }
            (None, None) => return Some(Ordering::Equal),
            _ => return None,
        }
    }
    Some(Ordering::Equal)
}

/// Whether the two values carry the same precision, the second and its
/// fraction counting as one.
fn same_precision(left: &Components, right: &Components) -> bool {
    left.iter()
        .zip(right)
        .all(|(a, b)| a.is_some() == b.is_some())
}

impl Date {
    /// Orders two dates, `None` when the answer is unknown (N1 §6.2).
    #[must_use]
    pub fn compare(self, other: Self) -> Option<Ordering> {
        walk(&date_components(self), &date_components(other))
    }

    /// Whether two dates are equivalent: equal at the same precision (N1
    /// §6.1.2).
    #[must_use]
    pub fn equivalent(self, other: Self) -> bool {
        let (left, right) = (date_components(self), date_components(other));
        same_precision(&left, &right) && walk(&left, &right) == Some(Ordering::Equal)
    }
}

impl DateTime {
    /// The components both values are compared by, `None` when one carries
    /// an offset and the other does not while both carry an hour.
    ///
    /// NOTE: N1 §6.1.1 leaves a default offset to the implementation; this one supplies
    /// none, so a time with an offset and one without compare as unknown.
    fn aligned(&self, other: &Self) -> Option<(Components, Components)> {
        let timed = self.clock.is_some() && other.clock.is_some();
        if timed && self.offset.is_some() != other.offset.is_some() {
            return None;
        }
        let to_utc = timed && self.offset.is_some();
        Some((
            date_time_components(self, to_utc)?,
            date_time_components(other, to_utc)?,
        ))
    }

    /// Orders two datetimes, `None` when the answer is unknown (N1 §6.2).
    #[must_use]
    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        let (left, right) = self.aligned(other)?;
        walk(&left, &right)
    }

    /// Whether two datetimes are equivalent: equal at the same precision (N1
    /// §6.1.2).
    #[must_use]
    pub fn equivalent(&self, other: &Self) -> bool {
        match self.aligned(other) {
            Some((left, right)) => {
                same_precision(&left, &right) && walk(&left, &right) == Some(Ordering::Equal)
            }
            None => false,
        }
    }
}

impl Time {
    /// The components of the time, in the hour, minute and second slots.
    fn components(&self) -> Components {
        [
            None,
            None,
            None,
            Some(Decimal::from(self.clock.hour)),
            self.clock.minute.map(Decimal::from),
            self.clock.seconds(),
        ]
    }

    /// Orders two times, `None` when the answer is unknown (N1 §6.2).
    #[must_use]
    pub fn compare(&self, other: &Self) -> Option<Ordering> {
        let (left, right) = (self.components(), other.components());
        let tail = |c: &Components| [c[3], c[4], c[5], None, None, None];
        walk(&tail(&left), &tail(&right))
    }

    /// Whether two times are equivalent: equal at the same precision (N1
    /// §6.1.2).
    #[must_use]
    pub fn equivalent(&self, other: &Self) -> bool {
        self.precision() == other.precision() && self.compare(other) == Some(Ordering::Equal)
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::super::{DateTime, Time};

    fn dt(text: &str) -> DateTime {
        DateTime::parse(text).expect("the datetime should read")
    }

    #[test]
    fn the_specification_examples_of_equality_hold() {
        assert_eq!(dt("2012").compare(&dt("2012")), Some(Ordering::Equal));
        assert_eq!(dt("2012").compare(&dt("2013")), Some(Ordering::Less));
        assert_eq!(dt("2012-01").compare(&dt("2012")), None);
        assert_eq!(
            dt("2012-01-01T10:30:31").compare(&dt("2012-01-01T10:30")),
            None
        );
        assert_eq!(
            dt("2012-01-01T10:30:31.0").compare(&dt("2012-01-01T10:30:31")),
            Some(Ordering::Equal)
        );
        assert_eq!(
            dt("2012-01-01T10:30:31.1").compare(&dt("2012-01-01T10:30:31")),
            Some(Ordering::Greater)
        );
    }

    #[test]
    fn offsets_are_aligned_before_comparing() {
        assert_eq!(
            dt("2017-11-05T01:30:00.0-04:00").compare(&dt("2017-11-05T01:15:00.0-05:00")),
            Some(Ordering::Less)
        );
        assert_eq!(
            dt("2017-11-05T01:30:00.0-04:00").compare(&dt("2017-11-05T00:30:00.0-05:00")),
            Some(Ordering::Equal)
        );
        assert_eq!(
            dt("2012-04-15T15:00:00Z").compare(&dt("2012-04-15T10:00:00")),
            None
        );
    }

    #[test]
    fn equivalence_needs_the_same_precision() {
        assert!(!dt("2012-01").equivalent(&dt("2012")));
        assert!(dt("2012-01-01T10:30:31.0").equivalent(&dt("2012-01-01T10:30:31")));
        let (a, b) = (
            Time::parse("10:30").expect("time"),
            Time::parse("10:30:00").expect("time"),
        );
        assert_eq!(a.compare(&b), None);
        assert!(!a.equivalent(&b));
    }
}
