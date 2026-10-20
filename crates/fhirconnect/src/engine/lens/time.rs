// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_TIME` against FHIR `time`.
//!
//! The cell is
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_TIME.adoc`:
//! `value` is `value`, "transformed" between FHIR's `hh:mm:ss` and ISO 8601,
//! and `accuracy` has no FHIR counterpart.
//!
//! The text moves unchanged where both sides can hold it. R4 `time` is
//! `hh:mm:ss` with an optional fraction and no zone
//! (<https://hl7.org/fhir/R4/datatypes.html#time>); ISO 8601 also writes the
//! basic form `hhmmss`, which the cell transforms into the extended form. A
//! time with an offset, or one that stops before the seconds, has no R4
//! `time` that states it, so it is refused rather than written with its zone
//! dropped or seconds invented.

use fhir_types::r4::primitives::Time;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_time::DvTime;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::model::ast::keyword::Direction;

/// The name the `time` cell carries in a diagnostic.
pub const CELL: &str = "DV_TIME against time";

/// `DV_TIME` against `time`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeLens;

impl Lens<DvTime, Time> for TimeLens {
    fn cell(&self) -> &'static str {
        CELL
    }

    fn get(&self, source: &DvTime) -> Result<Time, LensError> {
        extended(&source.value)
            .map(|text| Time::from(text.as_str()))
            .ok_or_else(|| LensError::Shape {
                cell: CELL,
                attribute: "DV_TIME.value",
                found: source.value.clone(),
                reason: "an R4 time is hh:mm:ss with no zone",
                direction: Direction::OpenehrToFhir,
            })
    }

    fn put(&self, view: &Time, existing: Option<&DvTime>) -> Result<DvTime, LensError> {
        let text = text_of(Some(&view.value)).ok_or(LensError::Missing {
            cell: CELL,
            attribute: "DV_TIME.value",
            direction: Direction::FhirToOpenehr,
        })?;
        // NOTE: no specification governs this: our own design, a held value
        // in the basic form that names the same time keeps its form.
        let value = existing
            .map(|held| held.value.as_str())
            .filter(|&held| extended(held).as_deref() == Some(text))
            .unwrap_or(text);
        Ok(DvTime {
            normal_status: existing.and_then(|held| held.normal_status.clone()),
            normal_range: existing.and_then(|held| held.normal_range.clone()),
            other_reference_ranges: existing.and_then(|held| held.other_reference_ranges.clone()),
            magnitude_status: existing.and_then(|held| held.magnitude_status.clone()),
            accuracy: existing.and_then(|held| held.accuracy.clone()),
            value: String::from(value),
        })
    }

    fn fallbacks(&self, _view: &Time) -> Vec<Fallback> {
        Vec::new()
    }
}

/// Returns the R4 `time` an ISO 8601 time names, `None` when it carries a
/// zone or stops before the seconds.
///
/// The extended form `hh:mm:ss[.f]` is returned as written and the basic form
/// `hhmmss[.f]` gains its separators; both are the openEHR `Iso8601_time`
/// forms (<https://specifications.openehr.org/releases/BASE/Release-1.2.0/foundation_types.html#_iso8601_time_class>).
fn extended(value: &str) -> Option<String> {
    let (clock, fraction) = match value.split_once(['.', ',']) {
        Some((clock, fraction)) => (clock, Some(fraction)),
        None => (value, None),
    };
    if let Some(fraction) = fraction
        && (fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let digits: String = match clock.len() {
        8 if clock.as_bytes().get(2) == Some(&b':') && clock.as_bytes().get(5) == Some(&b':') => {
            clock
                .chars()
                .filter(|&character| character != ':')
                .collect()
        }
        6 => String::from(clock),
        _ => return None,
    };
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (hours, rest) = digits.split_at(2);
    let (minutes, seconds) = rest.split_at(2);
    let fits = hours <= "23" && minutes <= "59" && seconds <= "60";
    if !fits {
        return None;
    }
    let mut text = format!("{hours}:{minutes}:{seconds}");
    if let Some(fraction) = fraction {
        text.push('.');
        text.push_str(fraction);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::TimeLens;
    use super::extended;
    use crate::engine::lens::Lens;
    use fhir_types::r4::primitives::Time;
    use openehr_rm::v1_2::data_types::quantity::date_time::dv_time::DvTime;
    use proptest::prop_compose;
    use proptest::proptest;

    /// Returns a `DV_TIME` of `value` with nothing else set.
    fn time(value: &str) -> DvTime {
        DvTime {
            normal_status: None,
            normal_range: None,
            other_reference_ranges: None,
            magnitude_status: None,
            accuracy: None,
            value: String::from(value),
        }
    }

    prop_compose! {
        /// A time in the extended or the basic form, with or without a fraction.
        fn clock()(
            hours in 0u8..24,
            minutes in 0u8..60,
            seconds in 0u8..60,
            fraction in proptest::option::of(0u16..1000),
            basic in proptest::bool::ANY,
            status in proptest::option::of(proptest::sample::select(vec!["=", "~"])),
        ) -> DvTime {
            let separator = if basic { "" } else { ":" };
            let clock = format!("{hours:02}{separator}{minutes:02}{separator}{seconds:02}");
            let value = match fraction {
                Some(fraction) => format!("{clock}.{fraction:03}"),
                None => clock,
            };
            DvTime { magnitude_status: status.map(String::from), ..time(&value) }
        }
    }

    proptest! {
        #[test]
        fn get_put(source in clock()) {
            let view = TimeLens.get(&source).expect("the time travels");
            let back = TimeLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn put_get(source in clock()) {
            let view = TimeLens.get(&source).expect("the time travels");
            let back = TimeLens.put(&view, None).expect("the view is complete");
            let again = TimeLens.get(&back).expect("the time travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn the_basic_form_is_transformed_into_the_fhir_form() {
        // DV_TIME.adoc: the value "must be transformed".
        assert_eq!(extended("101530").as_deref(), Some("10:15:30"));
        assert_eq!(extended("10:15:30.25").as_deref(), Some("10:15:30.25"));
    }

    #[test]
    fn a_time_fhir_cannot_state_is_refused() {
        for value in [
            "10:15",
            "10",
            "10:15:30Z",
            "10:15:30+01:00",
            "25:00:00",
            "10:15:30.",
        ] {
            assert!(
                TimeLens.get(&time(value)).is_err(),
                "{value} has no R4 time"
            );
        }
    }

    #[test]
    fn an_empty_time_is_refused_going_in() {
        assert!(TimeLens.put(&Time::default(), None).is_err());
    }
}
