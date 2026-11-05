// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_DATE_TIME` against FHIR `dateTime`, `instant` and `Period`.
//!
//! The cell is
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_DATE_TIME.adoc`.
//! The `dateTime` table pairs `dateTime` with `value` and marks `accuracy` as
//! having no FHIR counterpart.
//!
//! The value moves byte for byte. Both sides hold a lexical string, the RM
//! because `DV_DATE_TIME.value` is "ISO8601 date/time string" and FHIR because
//! a primitive keeps the text the document carried
//! (<https://hl7.org/fhir/R4/datatypes.html#dateTime>), so `Z`, `+00:00`,
//! `+01:00` and fractional seconds survive. The engine parses no timestamp and
//! adds no offset.

use fhir_types::r4::period::Period;
use fhir_types::r4::primitives::DateTime;
use fhir_types::r4::primitives::Instant;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_date_time::DvDateTime;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::model::ast::keyword::Direction;

/// The name the `dateTime` cell carries in a diagnostic.
pub const DATE_TIME_CELL: &str = "DV_DATE_TIME against dateTime";

/// The name the `instant` cell carries in a diagnostic.
pub const INSTANT_CELL: &str = "DV_DATE_TIME against instant";

/// The name the `Period` cell carries in a diagnostic.
pub const PERIOD_CELL: &str = "DV_DATE_TIME against Period";

/// `DV_DATE_TIME` against `dateTime`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DateTimeLens;

impl Lens<DvDateTime, DateTime> for DateTimeLens {
    fn cell(&self) -> &'static str {
        DATE_TIME_CELL
    }

    fn get(&self, source: &DvDateTime) -> Result<DateTime, LensError> {
        if source.value.is_empty() {
            return Err(LensError::Missing {
                cell: DATE_TIME_CELL,
                attribute: "dateTime",
                direction: Direction::OpenehrToFhir,
            });
        }
        Ok(DateTime::from(source.value.clone()))
    }

    fn put(&self, view: &DateTime, existing: Option<&DvDateTime>) -> Result<DvDateTime, LensError> {
        let value = text_of(Some(&view.value)).ok_or(LensError::Missing {
            cell: DATE_TIME_CELL,
            attribute: "DV_DATE_TIME.value",
            direction: Direction::FhirToOpenehr,
        })?;
        Ok(instant(String::from(value), existing))
    }

    fn fallbacks(&self, _view: &DateTime) -> Vec<Fallback> {
        Vec::new()
    }
}

/// `DV_DATE_TIME` against `instant`.
///
/// `DV_DATE_TIME.adoc` pairs `dateTime` and `Period` and lists no `instant`:
/// no specification governs this pair, it is our own design. The R4
/// package's lexical form of `instant` is a strict subset of the one of
/// `dateTime` (<https://hl7.org/fhir/R4/datatypes.html#primitive>), so going
/// into openEHR the value moves as a `dateTime` does, and going out of
/// openEHR a value outside the `instant` form is refused with the part it
/// lacks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InstantLens;

impl Lens<DvDateTime, Instant> for InstantLens {
    fn cell(&self) -> &'static str {
        INSTANT_CELL
    }

    fn get(&self, source: &DvDateTime) -> Result<Instant, LensError> {
        if source.value.is_empty() {
            return Err(LensError::Missing {
                cell: INSTANT_CELL,
                attribute: "instant",
                direction: Direction::OpenehrToFhir,
            });
        }
        if Instant::check(&source.value).is_err() {
            return Err(LensError::NotAnInstant {
                shortfall: InstantShortfall::of(&source.value),
            });
        }
        Ok(Instant::from(source.value.clone()))
    }

    fn put(&self, view: &Instant, existing: Option<&DvDateTime>) -> Result<DvDateTime, LensError> {
        let value = text_of(Some(&view.value)).ok_or(LensError::Missing {
            cell: INSTANT_CELL,
            attribute: "DV_DATE_TIME.value",
            direction: Direction::FhirToOpenehr,
        })?;
        Ok(instant(String::from(value), existing))
    }

    fn fallbacks(&self, _view: &Instant) -> Vec<Fallback> {
        Vec::new()
    }
}

/// What a `DV_DATE_TIME` lacks that an `instant` requires.
///
/// An `instant` is "known at least to the second" and carries an offset
/// (<https://hl7.org/fhir/R4/datatypes.html#instant>).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstantShortfall {
    /// The value names no full date and time of day.
    PartialDate,
    /// The time of day stops before the seconds.
    NoSeconds,
    /// The value carries no offset.
    NoTimezone,
    /// The value has every part and is still outside the lexical form.
    OutsideForm,
}

impl InstantShortfall {
    /// Returns what `value`, a text outside the `instant` form, lacks.
    #[must_use]
    pub fn of(value: &str) -> Self {
        let Some((date, time)) = value.split_once('T') else {
            return Self::PartialDate;
        };
        if date.len() < "YYYY-MM-DD".len() {
            return Self::PartialDate;
        }
        let zone = time.find(['Z', '+', '-']);
        let clock = zone.and_then(|at| time.get(..at)).unwrap_or(time);
        if clock.matches(':').count() < 2 {
            return Self::NoSeconds;
        }
        if zone.is_none() {
            return Self::NoTimezone;
        }
        Self::OutsideForm
    }

    /// Returns the part the diagnostic names.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PartialDate => "a full date and time of day",
            Self::NoSeconds => "the seconds",
            Self::NoTimezone => "an offset",
            Self::OutsideForm => "the lexical form of instant",
        }
    }
}

impl core::fmt::Display for InstantShortfall {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `Period` collapsed into one `DV_DATE_TIME`.
///
/// The page defines this cell going into openEHR only, and only for a period
/// that carries no `end`: "period.start … if `end` is not provided". It gives
/// no rule for a period that carries both bounds and none for the way back, so
/// both are refusals rather than a guess. An interval target keeps both bounds
/// and is [`crate::engine::lens::interval`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PeriodStartLens;

impl Lens<DvDateTime, Period> for PeriodStartLens {
    fn cell(&self) -> &'static str {
        PERIOD_CELL
    }

    fn get(&self, _source: &DvDateTime) -> Result<Period, LensError> {
        Err(LensError::OneWay {
            cell: PERIOD_CELL,
            defined: Direction::FhirToOpenehr,
            direction: Direction::OpenehrToFhir,
        })
    }

    fn put(&self, view: &Period, existing: Option<&DvDateTime>) -> Result<DvDateTime, LensError> {
        let start = view
            .start
            .as_ref()
            .and_then(|start| text_of(Some(&start.value)));
        let end = view.end.as_ref().and_then(|end| text_of(Some(&end.value)));
        if end.is_some() {
            return Err(LensError::Undefined {
                cell: PERIOD_CELL,
                situation: "a period that carries both bounds",
                reason: "the page gives `start` only when `end` is absent and \
                         leaves the bounded case without a rule",
                direction: Direction::FhirToOpenehr,
            });
        }
        let start = start.ok_or(LensError::Missing {
            cell: PERIOD_CELL,
            attribute: "DV_DATE_TIME.value",
            direction: Direction::FhirToOpenehr,
        })?;
        Ok(instant(String::from(start), existing))
    }

    fn fallbacks(&self, _view: &Period) -> Vec<Fallback> {
        vec![Fallback::new(PERIOD_CELL, "DV_DATE_TIME.value")]
    }
}

/// Returns a `DV_DATE_TIME` of `value`, carrying what FHIR cannot hold.
///
/// `accuracy`, `magnitude_status`, `normal_status`, `normal_range` and
/// `other_reference_ranges` are `-` in the table, so they come from the value
/// the target already holds.
pub(crate) fn instant(value: String, existing: Option<&DvDateTime>) -> DvDateTime {
    DvDateTime {
        normal_status: existing.and_then(|held| held.normal_status.clone()),
        normal_range: existing.and_then(|held| held.normal_range.clone()),
        other_reference_ranges: existing.and_then(|held| held.other_reference_ranges.clone()),
        magnitude_status: existing.and_then(|held| held.magnitude_status.clone()),
        accuracy: existing.and_then(|held| held.accuracy.clone()),
        value,
    }
}

#[cfg(test)]
mod tests {
    use super::DATE_TIME_CELL;
    use super::DateTimeLens;
    use super::INSTANT_CELL;
    use super::InstantLens;
    use super::InstantShortfall;
    use super::PERIOD_CELL;
    use super::PeriodStartLens;
    use super::instant;
    use crate::engine::lens::Lens;
    use crate::engine::lens::LensError;
    use fhir_types::r4::period::Period;
    use fhir_types::r4::primitives::DateTime;
    use fhir_types::r4::primitives::Instant;
    use openehr_rm::v1_2::data_types::quantity::date_time::dv_date_time::DvDateTime;
    use proptest::prop_compose;
    use proptest::proptest;

    /// The lexical forms the round trip must not touch.
    ///
    /// R4 `dateTime` admits a year, a year and month, a date, and a full
    /// timestamp with a mandatory offset
    /// (<https://hl7.org/fhir/R4/datatypes.html#dateTime>); the four offsets
    /// below are the ones a server is most tempted to re-render.
    const LEXICAL: [&str; 8] = [
        "2026",
        "2026-09",
        "2026-09-13",
        "2026-09-13T10:00:00Z",
        "2026-09-13T10:00:00+00:00",
        "2026-09-13T10:00:00+01:00",
        "2026-09-13T10:00:00.123456+02:00",
        "2026-09-13T10:00:00.000-05:30",
    ];

    prop_compose! {
        /// A `DV_DATE_TIME` carrying one of the lexical forms above.
        fn moment()(index in 0usize..LEXICAL.len()) -> DvDateTime {
            let value = LEXICAL.get(index).copied().unwrap_or("2026");
            instant(String::from(value), None)
        }
    }

    proptest! {
        #[test]
        fn get_put(source in moment()) {
            let view: DateTime = DateTimeLens.get(&source).expect("a value is present");
            let back = DateTimeLens.put(&view, Some(&source)).expect("the view carries a value");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn put_get(source in moment()) {
            let view = DateTime::from(source.value.clone());
            let back: DvDateTime = DateTimeLens.put(&view, None).expect("the view carries a value");
            let again: DateTime = DateTimeLens.get(&back).expect("a value is present");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn every_lexical_form_survives_byte_for_byte() {
        for form in LEXICAL {
            let source: DvDateTime = DateTimeLens
                .put(&DateTime::from(form), None)
                .expect("the view carries a value");
            assert_eq!(source.value, form, "the openEHR value changed");
            let view: DateTime = DateTimeLens.get(&source).expect("a value is present");
            assert_eq!(
                view.value.as_deref(),
                Some(form),
                "the FHIR value changed for {form}"
            );
        }
    }

    #[test]
    fn a_date_time_with_no_value_is_refused() {
        let error = DateTimeLens
            .put(&DateTime::default(), None)
            .expect_err("value is mandatory");
        assert!(
            error.to_string().contains(DATE_TIME_CELL),
            "the refusal names the cell: {error}"
        );
    }

    #[test]
    fn a_period_without_an_end_gives_its_start() {
        let view = Period {
            start: Some(DateTime::from("2026-09-13T10:00:00+01:00")),
            ..Period::default()
        };
        let source = PeriodStartLens
            .put(&view, None)
            .expect("a start is present");
        assert_eq!(
            source.value, "2026-09-13T10:00:00+01:00",
            "the start fills the instant"
        );
        assert_eq!(
            PeriodStartLens.fallbacks(&view).len(),
            1,
            "the one-way cell is recorded"
        );
    }

    #[test]
    fn a_period_with_both_bounds_has_no_defined_collapse() {
        let view = Period {
            start: Some(DateTime::from("2026-09-13T10:00:00Z")),
            end: Some(DateTime::from("2026-09-14T10:00:00Z")),
            ..Period::default()
        };
        let error = PeriodStartLens
            .put(&view, None)
            .expect_err("the page gives no rule for both bounds");
        assert!(
            matches!(error, LensError::Undefined { cell, .. } if cell == PERIOD_CELL),
            "the refusal names the undefined cell: {error}"
        );
    }

    #[test]
    fn the_period_cell_does_not_run_towards_fhir() {
        let error = PeriodStartLens
            .get(&instant(String::from("2026-09-13"), None))
            .expect_err("the page defines the cell one way");
        assert!(
            matches!(error, LensError::OneWay { cell, .. } if cell == PERIOD_CELL),
            "the refusal names the one-way cell: {error}"
        );
    }

    #[test]
    fn an_instant_moves_byte_for_byte_both_ways() {
        let form = "2026-09-13T10:00:00.123+02:00";
        let source = InstantLens
            .put(&Instant::from(form), None)
            .expect("the view carries a value");
        assert_eq!(source.value, form, "the openEHR value changed");
        let view = InstantLens.get(&source).expect("the value is an instant");
        assert_eq!(view.value.as_deref(), Some(form), "the instant changed");
    }

    #[test]
    fn a_date_time_short_of_an_instant_is_refused_naming_what_it_lacks() {
        for (form, shortfall) in [
            ("2026-09", InstantShortfall::PartialDate),
            ("2026-09-13", InstantShortfall::PartialDate),
            ("2026-09-13T10:00Z", InstantShortfall::NoSeconds),
            ("2026-09-13T10:00:00", InstantShortfall::NoTimezone),
            ("2026-09T10:00:00Z", InstantShortfall::PartialDate),
            ("2026-09-13T25:00:00Z", InstantShortfall::OutsideForm),
        ] {
            let refused = InstantLens.get(&instant(String::from(form), None));
            assert_eq!(
                refused,
                Err(LensError::NotAnInstant { shortfall }),
                "{form}"
            );
        }
    }

    #[test]
    fn the_refusal_names_the_part_an_instant_lacks() {
        for (shortfall, part) in [
            (InstantShortfall::PartialDate, "a full date and time of day"),
            (InstantShortfall::NoSeconds, "the seconds"),
            (InstantShortfall::NoTimezone, "an offset"),
            (InstantShortfall::OutsideForm, "the lexical form of instant"),
        ] {
            let message = LensError::NotAnInstant { shortfall }.to_string();
            assert!(message.ends_with(part), "{message}");
        }
    }

    #[test]
    fn an_instant_with_no_value_is_refused_both_ways_naming_the_cell() {
        assert_eq!(InstantLens.cell(), INSTANT_CELL);
        let out = InstantLens
            .get(&instant(String::new(), None))
            .expect_err("value is mandatory going out");
        let back = InstantLens
            .put(&Instant::default(), None)
            .expect_err("value is mandatory going in");
        for error in [out, back] {
            assert!(
                matches!(error, LensError::Missing { cell, .. } if cell == INSTANT_CELL),
                "the refusal names the instant cell: {error}"
            );
        }
    }

    #[test]
    fn an_instant_takes_no_one_way_row() {
        let view = Instant::from("2026-09-13T10:00:00Z");
        assert!(
            InstantLens.fallbacks(&view).is_empty(),
            "the instant cell records no fallback"
        );
        let held = instant(String::from("2026-09-13T09:00:00Z"), None);
        let source = InstantLens
            .put(&view, Some(&held))
            .expect("the view carries a value");
        assert_eq!(source.value, "2026-09-13T10:00:00Z", "the view wins");
    }
}
