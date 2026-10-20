// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The UCUM units of time and the ISO 8601 durations they stand for.
//!
//! "openEHR uses the ISO duration meanwhile, FHIR uses UCUM codes", so the
//! mapping transforms the UCUM code and its amount into
//! `P[nnY][nnM][nnW][nnD][T[nnH][nnM][nnS]]` by the unit table of
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_DURATION.adoc`.
//! A FHIR amount carries one unit, so a duration written with more than one
//! component has no FHIR form and is refused rather than converted.

use openehr_base::containers::present_nonempty;
use openehr_base::validate::Validate as _;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_duration::DvDuration;

/// One unit of the R4 `units-of-time` value set
/// (<https://hl7.org/fhir/R4/valueset-units-of-time.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    /// `s`, ISO `TnnS`.
    Second,
    /// `min`, ISO `TnnM`.
    Minute,
    /// `h`, ISO `TnnH`.
    Hour,
    /// `d`, ISO `nnD`.
    Day,
    /// `wk`, ISO `nnW`.
    Week,
    /// `mo`, ISO `nnM`.
    Month,
    /// `a`, ISO `nnY`.
    Year,
}

/// Every unit, in the order of the rows of `DV_DURATION.adoc`.
const UNITS: [TimeUnit; 7] = [
    TimeUnit::Second,
    TimeUnit::Minute,
    TimeUnit::Hour,
    TimeUnit::Day,
    TimeUnit::Week,
    TimeUnit::Month,
    TimeUnit::Year,
];

impl TimeUnit {
    /// Returns the unit a UCUM code of `units-of-time` names.
    #[must_use]
    pub fn from_ucum(code: &str) -> Option<Self> {
        UNITS.into_iter().find(|unit| unit.ucum() == code)
    }

    /// Returns the UCUM code of the unit.
    #[must_use]
    pub const fn ucum(self) -> &'static str {
        match self {
            Self::Second => "s",
            Self::Minute => "min",
            Self::Hour => "h",
            Self::Day => "d",
            Self::Week => "wk",
            Self::Month => "mo",
            Self::Year => "a",
        }
    }

    /// Returns the ISO 8601 designator of the unit, and whether it sits in
    /// the time part after `T`.
    const fn designator(self) -> (char, bool) {
        match self {
            Self::Second => ('S', true),
            Self::Minute => ('M', true),
            Self::Hour => ('H', true),
            Self::Day => ('D', false),
            Self::Week => ('W', false),
            Self::Month => ('M', false),
            Self::Year => ('Y', false),
        }
    }
}

/// Renders an amount of one unit as the ISO 8601 duration of a `DV_DURATION`.
///
/// The amount is the lexical text of a FHIR `decimal`. It keeps its text, so
/// the duration reads back to the same amount.
///
/// # Errors
///
/// Returns the reason when the amount is negative (R4 `tim-4` and `tim-5`
/// hold a duration and a period non-negative), carries an exponent, which
/// no ISO component can, or a fraction on any unit but the second, which is
/// the one component an openEHR `Iso8601_duration` gives a fraction
/// (openEHR BASE foundation types, `Iso8601_duration.fractional_second`).
pub fn to_iso(amount: &str, unit: TimeUnit) -> Result<DvDuration, String> {
    let fraction = plain_decimal(amount)
        .ok_or_else(|| format!("the amount `{amount}` is no plain non-negative decimal"))?;
    if fraction && unit != TimeUnit::Second {
        return Err(format!(
            "the amount `{amount}` carries a fraction, and only a duration in seconds keeps one"
        ));
    }
    let (designator, time) = unit.designator();
    let value = if time {
        format!("PT{amount}{designator}")
    } else {
        format!("P{amount}{designator}")
    };
    let duration = DvDuration {
        normal_status: None,
        normal_range: None,
        other_reference_ranges: present_nonempty(Vec::new()),
        magnitude_status: None,
        accuracy: None,
        accuracy_is_percent: None,
        value,
    };
    if !duration.invariants().is_empty() {
        return Err(format!("`{}` is no ISO 8601 duration", duration.value));
    }
    Ok(duration)
}

/// Reads the ISO 8601 duration of a `DV_DURATION` as an amount of one unit.
///
/// # Errors
///
/// Returns the reason when the value is no valid duration, is negative,
/// carries more than one component, or carries anything beside its value
/// that the FHIR amount has no place for.
pub fn from_iso(duration: &DvDuration) -> Result<(String, TimeUnit), String> {
    if !duration.invariants().is_empty() {
        return Err(format!("`{}` is no valid DV_DURATION", duration.value));
    }
    if duration.normal_status.is_some()
        || duration.normal_range.is_some()
        || duration.other_reference_ranges.is_some()
        || duration.accuracy.is_some()
        || duration.accuracy_is_percent.is_some()
        || duration
            .magnitude_status
            .as_deref()
            .is_some_and(|status| status != "=")
    {
        return Err(String::from(
            "the duration carries a range, a status or an accuracy, which a FHIR amount of time \
             has no place for",
        ));
    }
    let text = duration.value.as_str();
    let (time, rest) = match text.strip_prefix("PT") {
        Some(rest) => (true, rest),
        None => (
            false,
            text.strip_prefix('P')
                .ok_or_else(|| format!("`{text}` is no positive ISO 8601 duration"))?,
        ),
    };
    let designator = rest
        .chars()
        .last()
        .ok_or_else(|| format!("`{text}` names no component"))?;
    let amount = rest
        .strip_suffix(designator)
        .ok_or_else(|| format!("`{text}` names no component"))?;
    let unit = UNITS
        .into_iter()
        .find(|unit| unit.designator() == (designator, time))
        .ok_or_else(|| format!("`{text}` ends on no unit of time"))?;
    let fraction = plain_decimal(amount).ok_or_else(|| {
        format!("`{text}` carries more than one component, and a FHIR amount carries one unit")
    })?;
    if fraction && unit != TimeUnit::Second {
        return Err(format!(
            "`{text}` carries a fraction on a unit other than the second"
        ));
    }
    Ok((String::from(amount), unit))
}

/// Returns whether `text` is a plain non-negative decimal with a fraction,
/// `None` when it is no plain decimal at all.
///
/// Plain means the FHIR `decimal` grammar with no sign and no exponent
/// (<https://hl7.org/fhir/R4/datatypes.html#decimal>): no leading zero but a
/// lone one, and digits on both sides of a point.
fn plain_decimal(text: &str) -> Option<bool> {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    if !digits(whole) || (whole.len() > 1 && whole.starts_with('0')) {
        return None;
    }
    match fraction {
        Some(fraction) if digits(fraction) => Some(true),
        Some(_) => None,
        None => Some(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unit_of_time_renders_its_row_of_the_table_and_reads_back() {
        for (unit, ucum, iso) in [
            (TimeUnit::Second, "s", "PT8S"),
            (TimeUnit::Minute, "min", "PT8M"),
            (TimeUnit::Hour, "h", "PT8H"),
            (TimeUnit::Day, "d", "P8D"),
            (TimeUnit::Week, "wk", "P8W"),
            (TimeUnit::Month, "mo", "P8M"),
            (TimeUnit::Year, "a", "P8Y"),
        ] {
            assert_eq!(TimeUnit::from_ucum(ucum), Some(unit));
            assert_eq!(unit.ucum(), ucum);
            let duration = to_iso("8", unit).expect("a whole amount renders");
            assert_eq!(duration.value, iso);
            assert_eq!(from_iso(&duration), Ok((String::from("8"), unit)));
        }
    }

    #[test]
    fn only_seconds_keep_a_fraction() {
        let seconds = to_iso("1.5", TimeUnit::Second).expect("seconds keep a fraction");
        assert_eq!(seconds.value, "PT1.5S");
        assert_eq!(
            from_iso(&seconds),
            Ok((String::from("1.5"), TimeUnit::Second))
        );
        assert!(to_iso("1.5", TimeUnit::Hour).is_err());
    }

    #[test]
    fn an_amount_outside_the_plain_grammar_is_refused() {
        for amount in ["-1", "1e2", "01", ".5", "5.", "", "x"] {
            assert!(to_iso(amount, TimeUnit::Day).is_err(), "{amount}");
        }
    }

    #[test]
    fn a_duration_with_two_components_or_a_sign_is_refused() {
        for value in ["PT1H30M", "P1DT2H", "-PT1H", "P", "PT", "10 hours"] {
            let duration = to_iso("1", TimeUnit::Hour)
                .map(|mut duration| {
                    duration.value = String::from(value);
                    duration
                })
                .expect("a duration builds");
            assert!(from_iso(&duration).is_err(), "{value}");
        }
    }

    #[test]
    fn a_duration_that_carries_an_accuracy_is_refused() {
        let mut duration = to_iso("1", TimeUnit::Hour).expect("a duration builds");
        duration.accuracy = Some(0.0);
        assert!(from_iso(&duration).is_err());
    }
}
