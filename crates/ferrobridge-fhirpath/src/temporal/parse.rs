// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading dates and times from a literal, a string and a FHIR primitive.
//!
//! The forms are the grammar's `DATEFORMAT`, `TIMEFORMAT` and
//! `TIMEZONEOFFSETFORMAT` (<https://hl7.org/fhirpath/N1/grammar.html>), which
//! are also the lexical forms of the FHIR `date`, `dateTime`, `instant` and
//! `time` primitives (<https://hl7.org/fhir/R4/datatypes.html#primitive>). A
//! value whose components name no calendar date or clock reading (month 13,
//! 30 February, hour 24) is refused.

use super::{Clock, Date, DateTime, Offset, Seconds, Time};

/// A date or time literal (N1 §4.1.5 to §4.1.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Temporal {
    /// `@YYYY-MM-DD`.
    Date(Date),
    /// `@YYYY-MM-DDThh:mm:ss.fff(+|-)hh:mm`.
    DateTime(DateTime),
    /// `@Thh:mm:ss.fff`.
    Time(Time),
}

/// A cursor over ASCII text.
struct Cursor<'t> {
    bytes: &'t [u8],
    at: usize,
}

impl<'t> Cursor<'t> {
    const fn new(text: &'t str) -> Self {
        Self {
            bytes: text.as_bytes(),
            at: 0,
        }
    }

    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.at = self.at.saturating_add(1);
            true
        } else {
            false
        }
    }

    /// Reads exactly `n` digits as a number.
    fn digits(&mut self, n: usize) -> Option<i32> {
        let end = self.at.checked_add(n)?;
        let run = self.bytes.get(self.at..end)?;
        let mut value: i32 = 0;
        for byte in run {
            if !byte.is_ascii_digit() {
                return None;
            }
            value = value.checked_mul(10)?.checked_add(i32::from(byte - b'0'))?;
        }
        self.at = end;
        Some(value)
    }

    /// Reads one or more digits as a fraction: nanoseconds and digit count.
    fn fraction(&mut self) -> Option<(i32, u8)> {
        let start = self.at;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at = self.at.saturating_add(1);
        }
        let run = self.bytes.get(start..self.at)?;
        if run.is_empty() {
            return None;
        }
        let digits = u8::try_from(run.len()).unwrap_or(u8::MAX);
        let mut nanos: i32 = 0;
        for (index, byte) in run.iter().take(9).enumerate() {
            let scale = 10_i32.checked_pow(u32::try_from(8_usize.checked_sub(index)?).ok()?)?;
            nanos = nanos.checked_add(i32::from(byte - b'0').checked_mul(scale)?)?;
        }
        Some((nanos, digits))
    }
}

/// Reads `YYYY(-MM(-DD)?)?`.
fn date(cursor: &mut Cursor<'_>) -> Option<Date> {
    let year = i16::try_from(cursor.digits(4)?).ok()?;
    let mut parsed = Date {
        year,
        month: None,
        day: None,
    };
    if cursor.eat(b'-') {
        parsed.month = Some(i8::try_from(cursor.digits(2)?).ok()?);
        if cursor.eat(b'-') {
            parsed.day = Some(i8::try_from(cursor.digits(2)?).ok()?);
        }
    }
    valid_date(parsed).then_some(parsed)
}

/// Whether the components name a calendar date of the Gregorian calendar
/// from 0001 to 9999 (N1 §4.1.5).
fn valid_date(value: Date) -> bool {
    if !(1..=9999).contains(&value.year) {
        return false;
    }
    match (value.month, value.day) {
        (None, _) => true,
        (Some(month), None) => (1..=12).contains(&month),
        (Some(month), Some(day)) => jiff::civil::Date::new(value.year, month, day).is_ok(),
    }
}

/// Reads `hh(:mm(:ss(.fff)?)?)?`.
fn clock(cursor: &mut Cursor<'_>) -> Option<Clock> {
    let hour = i8::try_from(cursor.digits(2)?).ok()?;
    let mut parsed = Clock {
        hour,
        minute: None,
        second: None,
    };
    if cursor.eat(b':') {
        parsed.minute = Some(i8::try_from(cursor.digits(2)?).ok()?);
        if cursor.eat(b':') {
            let whole = i8::try_from(cursor.digits(2)?).ok()?;
            let fraction = if cursor.eat(b'.') {
                Some(cursor.fraction()?)
            } else {
                None
            };
            parsed.second = Some(Seconds { whole, fraction });
        }
    }
    let valid = (0..=23).contains(&parsed.hour)
        && parsed
            .minute
            .is_none_or(|minute| (0..=59).contains(&minute))
        && parsed
            .second
            .is_none_or(|second| (0..=59).contains(&second.whole));
    valid.then_some(parsed)
}

/// Reads `Z` or `(+|-)hh:mm`.
fn offset(cursor: &mut Cursor<'_>) -> Option<Offset> {
    let start = cursor.at;
    let parsed = signed_offset(cursor);
    if parsed.is_none() {
        cursor.at = start;
    }
    parsed
}

/// Reads `Z` or `(+|-)hh:mm`, `None` for anything else, the cursor then
/// left wherever reading stopped.
fn signed_offset(cursor: &mut Cursor<'_>) -> Option<Offset> {
    if cursor.eat(b'Z') {
        return Some(Offset::Utc);
    }
    let sign: i16 = if cursor.eat(b'+') {
        1
    } else if cursor.eat(b'-') {
        -1
    } else {
        return None;
    };
    let hours = i16::try_from(cursor.digits(2)?).ok()?;
    if !cursor.eat(b':') {
        return None;
    }
    let minutes = i16::try_from(cursor.digits(2)?).ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some(Offset::Minutes(
        sign.checked_mul(hours.checked_mul(60)?.checked_add(minutes)?)?,
    ))
}

/// Reads the text of a date or time literal, the part after `@`.
#[must_use]
pub fn literal(text: &str) -> Option<Temporal> {
    let mut cursor = Cursor::new(text);
    if cursor.eat(b'T') {
        let parsed = clock(&mut cursor)?;
        return cursor
            .done()
            .then_some(Temporal::Time(Time { clock: parsed }));
    }
    let parsed = date(&mut cursor)?;
    if cursor.done() {
        return Some(Temporal::Date(parsed));
    }
    if !cursor.eat(b'T') {
        return None;
    }
    if cursor.done() {
        return Some(Temporal::DateTime(parsed.to_date_time()));
    }
    let time = clock(&mut cursor)?;
    let zone = offset(&mut cursor);
    cursor.done().then_some(Temporal::DateTime(DateTime {
        date: parsed,
        clock: Some(time),
        offset: zone,
    }))
}

impl Date {
    /// Reads `YYYY-MM-DD`, partial or whole, the string form `toDate()`
    /// converts (N1 §5.5.4).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut cursor = Cursor::new(text);
        let parsed = date(&mut cursor)?;
        cursor.done().then_some(parsed)
    }
}

impl DateTime {
    /// Reads `YYYY-MM-DDThh:mm:ss.fff(+|-)hh:mm`, partial or whole: the string
    /// form `toDateTime()` converts (N1 §5.5.5) and the lexical form of the
    /// FHIR `date`, `dateTime` and `instant` primitives.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut cursor = Cursor::new(text);
        let parsed = date(&mut cursor)?;
        if cursor.done() {
            return Some(parsed.to_date_time());
        }
        if !cursor.eat(b'T') {
            return None;
        }
        let time = clock(&mut cursor)?;
        let zone = offset(&mut cursor);
        cursor.done().then_some(Self {
            date: parsed,
            clock: Some(time),
            offset: zone,
        })
    }
}

impl Time {
    /// Reads `hh:mm:ss.fff`, partial or whole: the string form `toTime()`
    /// converts (N1 §5.5.9) and the lexical form of the FHIR `time` primitive.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut cursor = Cursor::new(text);
        let parsed = clock(&mut cursor)?;
        cursor.done().then_some(Self { clock: parsed })
    }
}

#[cfg(test)]
mod tests {
    use super::{DateTime, Temporal, Time, literal};

    #[test]
    fn every_partial_form_reads_and_writes_back() {
        for text in [
            "2015",
            "2015-02",
            "2015-02-04",
            "2015-02-04T14",
            "2015-02-04T14:34",
            "2015-02-04T14:34:28",
            "2015-02-04T14:34:28.123",
            "2015-02-04T14:34:28Z",
            "2015-02-04T14:34:28+10:00",
            "2017-11-05T01:30:00.0-04:00",
        ] {
            let parsed = DateTime::parse(text).expect("the form should read");
            assert_eq!(parsed.to_string(), text);
        }
    }

    #[test]
    fn a_literal_names_its_kind() {
        assert!(matches!(literal("2015"), Some(Temporal::Date(_))));
        assert!(matches!(literal("2015T"), Some(Temporal::DateTime(_))));
        assert!(matches!(literal("T14:30"), Some(Temporal::Time(_))));
    }

    #[test]
    fn impossible_values_are_refused() {
        assert!(DateTime::parse("2015-02-30").is_none());
        assert!(DateTime::parse("2015-13").is_none());
        assert!(Time::parse("24:00").is_none());
        assert!(DateTime::parse("2015-02-04T10:00+25:00").is_none());
    }
}
