// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reads a date to the lexical form of §3.1.1.4.7.

use super::{Clock, DateError, Offset, SearchDate, Second};

impl SearchDate {
    /// Reads one date with no prefix and no escapes.
    ///
    /// # Errors
    ///
    /// [`DateError::Form`], [`DateError::Calendar`] or
    /// [`DateError::LeapSecond`] for a value outside the form, the calendar
    /// or the clock.
    pub fn parse(text: &str) -> Result<Self, DateError> {
        let (day_part, time_part) = match text.split_once('T') {
            Some((day, time)) => (day, Some(time)),
            None => (text, None),
        };
        let mut fields = day_part.split('-');
        let year: i16 = digits(fields.next(), 4).ok_or(DateError::Form)?;
        let month: Option<i8> = fields
            .next()
            .map(|month| digits(Some(month), 2).ok_or(DateError::Form))
            .transpose()?;
        let day: Option<i8> = fields
            .next()
            .map(|day| digits(Some(day), 2).ok_or(DateError::Form))
            .transpose()?;
        if fields.next().is_some() || year == 0 {
            return Err(DateError::Form);
        }
        if month.is_some_and(|month| !(1..=12).contains(&month)) {
            return Err(DateError::Form);
        }
        if let (Some(month), Some(day)) = (month, day) {
            if !(1..=31).contains(&day) {
                return Err(DateError::Form);
            }
            if day > super::range::days_in_month(year, month) {
                return Err(DateError::Calendar);
            }
        }
        let time = match time_part {
            None => None,
            Some(_) if day.is_none() => return Err(DateError::Form),
            Some(time) => Some(clock(time)?),
        };
        Ok(SearchDate {
            year,
            month,
            day,
            time,
        })
    }
}

/// Reads the time part after the `T`: `hh:mm[:ss[.f+]]` and an optional
/// offset.
fn clock(text: &str) -> Result<Clock, DateError> {
    let (written, offset) = match text.find(['Z', '+', '-']) {
        Some(at) => {
            let (written, offset) = text.split_at_checked(at).ok_or(DateError::Form)?;
            (written, Some(self::offset(offset)?))
        }
        None => (text, None),
    };
    let fields: Vec<&str> = written.split(':').collect();
    let (hour, minute, second) = match fields.as_slice() {
        [hour, minute] => (*hour, *minute, None),
        [hour, minute, second] => (*hour, *minute, Some(*second)),
        _ => return Err(DateError::Form),
    };
    let hour: i8 = digits(Some(hour), 2).ok_or(DateError::Form)?;
    let minute: i8 = digits(Some(minute), 2).ok_or(DateError::Form)?;
    if hour > 23 || minute > 59 {
        return Err(DateError::Form);
    }
    let second = second.map(self::second).transpose()?;
    Ok(Clock {
        hour,
        minute,
        second,
        offset,
    })
}

/// Reads `ss` or `ss.f+`.
fn second(text: &str) -> Result<Second, DateError> {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let whole: i8 = digits(Some(whole), 2).ok_or(DateError::Form)?;
    let fraction = match fraction {
        Some(digits) if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) => {
            return Err(DateError::Form);
        }
        Some(digits) => Some(digits.to_owned()),
        None => None,
    };
    match whole {
        0..=59 => Ok(Second { whole, fraction }),
        60 => Err(DateError::LeapSecond),
        _ => Err(DateError::Form),
    }
}

/// Reads `Z` or `±hh:mm`, an offset of at most `14:00` as the FHIR
/// `dateTime` form admits.
fn offset(text: &str) -> Result<Offset, DateError> {
    if text == "Z" {
        return Ok(Offset::Utc);
    }
    let (sign, rest) = match text.split_at_checked(1) {
        Some(("+", rest)) => (true, rest),
        Some(("-", rest)) => (false, rest),
        _ => return Err(DateError::Form),
    };
    let (hours, minutes) = rest.split_once(':').ok_or(DateError::Form)?;
    let hours: u8 = digits(Some(hours), 2).ok_or(DateError::Form)?;
    let minutes: u8 = digits(Some(minutes), 2).ok_or(DateError::Form)?;
    if !(hours <= 13 && minutes <= 59 || hours == 14 && minutes == 0) {
        return Err(DateError::Form);
    }
    Ok(if sign {
        Offset::Plus { hours, minutes }
    } else {
        Offset::Minus { hours, minutes }
    })
}

/// The number `text` spells when it is exactly `width` ASCII digits.
fn digits<T: core::str::FromStr>(text: Option<&str>, width: usize) -> Option<T> {
    let text = text?;
    if text.len() == width && text.bytes().all(|byte| byte.is_ascii_digit()) {
        // NOTE: no specification governs this: our own design; `width` ASCII
        // digits fit every caller's type, so a failed parse is input outside the form.
        text.parse().ok()
    } else {
        None
    }
}
