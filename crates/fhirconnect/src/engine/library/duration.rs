// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The R4 units of time and the `DV_DURATION` an amount of one of them is.
//!
//! The conversion is the engine's own `DV_DURATION` cell
//! ([`crate::engine::lens::duration::DurationLens`]), which carries the
//! UCUM to ISO 8601 table of
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_DURATION.adoc`,
//! so a function converts a duration the way a data-type mapping does.
//! `Timing.repeat` names its unit in `units-of-time`, a subset of that
//! table, so this module adds which units a timing admits and holds the
//! openEHR reader's own limits: a fraction stands on the seconds alone.

use fhir_types::r4::duration::Duration;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Decimal;
use fhir_types::r4::primitives::Uri;
use openehr_base::validate::Validate as _;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_duration::DvDuration;

use crate::engine::lens::Lens;
use crate::engine::lens::duration::DurationLens;
use crate::engine::lens::quantity::UCUM;
use crate::engine::library::cell;
use crate::engine::library::refused;
use crate::engine::traverse::functions::MappingCodeError;

/// One unit of the R4 `units-of-time` value set
/// (<https://hl7.org/fhir/R4/valueset-units-of-time.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    /// `s`.
    Second,
    /// `min`.
    Minute,
    /// `h`.
    Hour,
    /// `d`.
    Day,
    /// `wk`.
    Week,
    /// `mo`.
    Month,
    /// `a`.
    Year,
}

/// Every unit, in the order of the value set.
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
}

/// Writes an amount of one unit as a `DV_DURATION`, through the engine's
/// `DV_DURATION` cell.
///
/// The amount is the lexical text of a FHIR `decimal`, kept as written, so
/// the duration reads back to the same amount.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the amount is negative (R4 `tim-4` and
/// `tim-5` hold a duration and a period non-negative) or carries an exponent,
/// which no ISO component can; when it carries a fraction on any unit but
/// the second, the one component an openEHR `Iso8601_duration` gives a
/// fraction (openEHR BASE foundation types, `Iso8601_duration`); and when the
/// cell refuses it.
pub fn to_iso(code: &str, amount: &str, unit: TimeUnit) -> Result<DvDuration, MappingCodeError> {
    let fraction = plain_decimal(amount).ok_or_else(|| {
        refused(
            code,
            format!("the amount `{amount}` is no plain non-negative decimal"),
        )
    })?;
    if fraction && unit != TimeUnit::Second {
        return Err(refused(
            code,
            format!(
                "the amount `{amount}` carries a fraction, and only a duration in seconds keeps \
                 one"
            ),
        ));
    }
    let view = Duration {
        value: Some(Decimal::from(amount)),
        system: Some(Uri::from(UCUM)),
        code: Some(Code::from(unit.ucum())),
        ..Duration::default()
    };
    let duration = Lens::<DvDuration, Duration>::put(&DurationLens, &view, None)
        .map_err(|source| cell(code, source))?;
    if !duration.invariants().is_empty() {
        return Err(refused(
            code,
            format!("`{}` is no ISO 8601 duration", duration.value),
        ));
    }
    Ok(duration)
}

/// Reads a `DV_DURATION` as an amount of one unit of time, through the
/// engine's `DV_DURATION` cell.
///
/// A duration of several components reads in the smallest one present, as
/// the cell reads it.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the duration carries a range, a status
/// or an accuracy, which a FHIR amount of time has no place for, when the
/// cell refuses it, and when it reads in a unit outside `units-of-time`
/// (`ms`).
pub fn from_iso(code: &str, duration: &DvDuration) -> Result<(String, TimeUnit), MappingCodeError> {
    if duration.normal_status.is_some()
        || duration.normal_range.is_some()
        || duration.other_reference_ranges.is_some()
        || duration.accuracy.is_some()
        || duration.accuracy_is_percent.is_some()
    {
        return Err(refused(
            code,
            "the duration carries a range, a status or an accuracy, which a FHIR amount of time \
             has no place for",
        ));
    }
    let view = Lens::<DvDuration, Duration>::get(&DurationLens, duration)
        .map_err(|source| cell(code, source))?;
    let amount = view.value.as_ref().and_then(|value| value.value.as_deref());
    let units = view.code.as_ref().and_then(|units| units.value.as_deref());
    let (Some(amount), Some(unit)) = (amount, units.and_then(TimeUnit::from_ucum)) else {
        return Err(refused(
            code,
            format!("`{}` reads in no unit of `units-of-time`", duration.value),
        ));
    };
    Ok((String::from(amount), unit))
}

/// Returns whether `text` is a plain non-negative decimal with a fraction,
/// `None` when it is no plain decimal at all.
///
/// Plain means the FHIR `decimal` grammar with no sign and no exponent
/// (<https://hl7.org/fhir/R4/datatypes.html#decimal>): no leading zero but a
/// lone one, and digits on both sides of a point.
pub(crate) fn plain_decimal(text: &str) -> Option<bool> {
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

    /// The name the test calls carry.
    const CODE: &str = "test";

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
            let duration = to_iso(CODE, "8", unit).expect("a whole amount renders");
            assert_eq!(duration.value, iso);
            assert_eq!(from_iso(CODE, &duration), Ok((String::from("8"), unit)));
        }
    }

    #[test]
    fn only_seconds_keep_a_fraction() {
        let seconds = to_iso(CODE, "1.5", TimeUnit::Second).expect("seconds keep a fraction");
        assert_eq!(seconds.value, "PT1.5S");
        assert_eq!(
            from_iso(CODE, &seconds),
            Ok((String::from("1.5"), TimeUnit::Second))
        );
        assert!(to_iso(CODE, "1.5", TimeUnit::Hour).is_err());
    }

    #[test]
    fn an_amount_outside_the_plain_grammar_is_refused() {
        for amount in ["-1", "1e2", "01", ".5", "5.", "", "x"] {
            assert!(to_iso(CODE, amount, TimeUnit::Day).is_err(), "{amount}");
        }
    }

    #[test]
    fn a_duration_of_several_components_reads_in_the_smallest() {
        let mut duration = to_iso(CODE, "1", TimeUnit::Hour).expect("a duration builds");
        duration.value = String::from("PT1H30M");
        assert_eq!(
            from_iso(CODE, &duration),
            Ok((String::from("90"), TimeUnit::Minute))
        );
    }

    #[test]
    fn a_duration_that_is_none_or_carries_an_accuracy_is_refused() {
        for value in ["-PT1H", "P", "PT", "10 hours"] {
            let mut duration = to_iso(CODE, "1", TimeUnit::Hour).expect("a duration builds");
            duration.value = String::from(value);
            assert!(from_iso(CODE, &duration).is_err(), "{value}");
        }
        let mut duration = to_iso(CODE, "1", TimeUnit::Hour).expect("a duration builds");
        duration.accuracy = Some(0.0);
        assert!(from_iso(CODE, &duration).is_err());
    }
}
