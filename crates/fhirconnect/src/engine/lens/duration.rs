// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_DURATION` against FHIR `Duration`, through the UCUM to ISO 8601 bridge.
//!
//! The cell is
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_DURATION.adoc`:
//! FHIR writes a duration as a `value` and a UCUM `code` from
//! `CommonUCUMCodesForDuration`, openEHR as one ISO 8601 duration string, and
//! the page's table names the designator each of the eight codes becomes.
//! [`BRIDGE`] is that table. A code outside it is a typed refusal, because the
//! bridge is closed.
//!
//! Going out, a duration of one component reads as that component's code. A
//! duration of several components reads in the smallest one present, each
//! larger one converted by the factors UCUM fixes (`wk` is 7 `d`, `d` is 24
//! `h`, `h` is 60 `min`, `min` is 60 `s`; `a` is 12 `mo`); one that mixes
//! years or months with weeks, days or a time has no single code and is
//! refused. No specification governs that reading: our own design.
//!
//! The table gives `comparator`, `unit` and `system` no openEHR row. A
//! comparator going in would be dropped and the duration read as exact, so it
//! is refused; R4 `drt-1` fixes the system at UCUM
//! (<https://hl7.org/fhir/R4/datatypes.html#Duration>), so any other is
//! refused too.

use fhir_types::r4::duration::Duration;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Decimal;
use fhir_types::r4::primitives::Uri;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_duration::DvDuration;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::engine::lens::number::decimal_text;
use crate::engine::lens::number::point;
use crate::engine::lens::quantity::UCUM;
use crate::model::ast::keyword::Direction;

/// The name the `Duration` cell carries in a diagnostic.
pub const CELL: &str = "DV_DURATION against Duration";

/// One ISO 8601 duration component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Component {
    /// `nY`.
    Years,
    /// `nM` before the `T`.
    Months,
    /// `nW`.
    Weeks,
    /// `nD`.
    Days,
    /// `nH`.
    Hours,
    /// `nM` after the `T`.
    Minutes,
    /// `nS`.
    Seconds,
}

impl Component {
    /// Returns the designator letter the component is written with.
    #[must_use]
    pub const fn designator(self) -> char {
        match self {
            Self::Years => 'Y',
            Self::Months | Self::Minutes => 'M',
            Self::Weeks => 'W',
            Self::Days => 'D',
            Self::Hours => 'H',
            Self::Seconds => 'S',
        }
    }

    /// Returns whether the component is written after the `T`.
    #[must_use]
    pub const fn is_time(self) -> bool {
        matches!(self, Self::Hours | Self::Minutes | Self::Seconds)
    }

    /// Returns the next smaller component and how many of it make one of
    /// this, `None` for the smallest of its group.
    const fn smaller(self) -> Option<(Self, u128)> {
        match self {
            Self::Years => Some((Self::Months, 12)),
            Self::Weeks => Some((Self::Days, 7)),
            Self::Days => Some((Self::Hours, 24)),
            Self::Hours => Some((Self::Minutes, 60)),
            Self::Minutes => Some((Self::Seconds, 60)),
            Self::Months | Self::Seconds => None,
        }
    }
}

/// The bridge of `DV_DURATION.adoc`, one row per UCUM code.
///
/// Each row of `CommonUCUMCodesForDuration` names the component the code is
/// written as and the power of ten its value is divided by on the way (`ms`
/// is a fraction of a second).
pub const BRIDGE: [(&str, Component, u32); 8] = [
    ("ms", Component::Seconds, 3),
    ("s", Component::Seconds, 0),
    ("min", Component::Minutes, 0),
    ("h", Component::Hours, 0),
    ("d", Component::Days, 0),
    ("wk", Component::Weeks, 0),
    ("mo", Component::Months, 0),
    ("a", Component::Years, 0),
];

/// `DV_DURATION` against `Duration`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DurationLens;

impl Lens<DvDuration, Duration> for DurationLens {
    fn cell(&self) -> &'static str {
        CELL
    }

    fn get(&self, source: &DvDuration) -> Result<Duration, LensError> {
        point(source.magnitude_status.as_deref(), CELL)?;
        let (value, code) = reading(&source.value).ok_or_else(|| LensError::Shape {
            cell: CELL,
            attribute: "DV_DURATION.value",
            found: source.value.clone(),
            reason: "the duration has no single UCUM code of CommonUCUMCodesForDuration",
            direction: Direction::OpenehrToFhir,
        })?;
        Ok(Duration {
            value: Some(Decimal::from(value.as_str())),
            system: Some(Uri::from(UCUM)),
            code: Some(Code::from(code)),
            ..Duration::default()
        })
    }

    fn put(&self, view: &Duration, existing: Option<&DvDuration>) -> Result<DvDuration, LensError> {
        if let Some(comparator) = text_of(view.comparator.as_ref().map(|code| &code.value)) {
            return Err(LensError::Shape {
                cell: CELL,
                attribute: "Duration.comparator",
                found: String::from(comparator),
                reason: "the table gives comparator no openEHR row",
                direction: Direction::FhirToOpenehr,
            });
        }
        let system = text_of(view.system.as_ref().map(|system| &system.value));
        if let Some(other) = system.filter(|&system| system != UCUM) {
            return Err(LensError::Shape {
                cell: CELL,
                attribute: "Duration.system",
                found: String::from(other),
                reason: "R4 drt-1 fixes a Duration's system at UCUM",
                direction: Direction::FhirToOpenehr,
            });
        }
        let code =
            text_of(view.code.as_ref().map(|code| &code.value)).ok_or(LensError::Missing {
                cell: CELL,
                attribute: "Duration.code",
                direction: Direction::FhirToOpenehr,
            })?;
        let &(code, component, shift) = BRIDGE
            .iter()
            .find(|&&(listed, _, _)| listed == code)
            .ok_or_else(|| LensError::Shape {
                cell: CELL,
                attribute: "Duration.code",
                found: String::from(code),
                reason: "the code is outside the CommonUCUMCodesForDuration bridge",
                direction: Direction::FhirToOpenehr,
            })?;
        let text = decimal_text(view.value.as_ref(), CELL, "DV_DURATION.value")?;
        let amount = shifted(text, shift).ok_or_else(|| LensError::Shape {
            cell: CELL,
            attribute: "Duration.value",
            found: String::from(text),
            reason: "an ISO 8601 duration component is a non-negative decimal",
            direction: Direction::FhirToOpenehr,
        })?;
        let written = iso(&amount, component);
        // NOTE: no specification governs this: our own design, a held duration
        // that reads as the same value and code keeps the components it was
        // written with.
        let value = existing
            .map(|held| held.value.as_str())
            .filter(|&held| reading(held).is_some_and(|(read, unit)| read == text && unit == code))
            .map_or(written, String::from);
        Ok(DvDuration {
            normal_status: existing.and_then(|held| held.normal_status.clone()),
            normal_range: existing.and_then(|held| held.normal_range.clone()),
            other_reference_ranges: existing.and_then(|held| held.other_reference_ranges.clone()),
            magnitude_status: existing.and_then(|held| held.magnitude_status.clone()),
            accuracy: existing.and_then(|held| held.accuracy),
            accuracy_is_percent: existing.and_then(|held| held.accuracy_is_percent),
            value,
        })
    }

    fn fallbacks(&self, view: &Duration) -> Vec<Fallback> {
        let shifted = text_of(view.code.as_ref().map(|code| &code.value))
            .and_then(|code| BRIDGE.iter().find(|&&(listed, _, _)| listed == code))
            .is_some_and(|&(_, _, shift)| shift > 0);
        if shifted {
            return vec![Fallback::new(CELL, "DV_DURATION.value")];
        }
        Vec::new()
    }
}

/// Returns the value and the UCUM code an ISO 8601 duration reads as, `None`
/// when no single code of [`BRIDGE`] states it.
#[must_use]
pub fn reading(duration: &str) -> Option<(String, &'static str)> {
    let parts = components(duration)?;
    let (&(smallest, ref fraction), larger) = parts.split_last()?;
    let mut whole: u128 = 0;
    let mut unit = parts.first()?.0;
    for &(component, ref amount) in larger {
        whole = descend(whole, unit, component)?.checked_add(amount.parse().ok()?)?;
        unit = component;
    }
    whole = descend(whole, unit, smallest)?;
    let (integer, decimals) = match fraction.split_once('.') {
        Some((integer, decimals)) => (integer, Some(decimals)),
        None => (fraction.as_str(), None),
    };
    let total = whole.checked_add(integer.parse::<u128>().ok()?)?;
    let mut value = total.to_string();
    if let Some(decimals) = decimals {
        value.push('.');
        value.push_str(decimals);
    }
    let code = BRIDGE
        .iter()
        .find(|&&(_, component, shift)| component == smallest && shift == 0)
        .map(|&(code, _, _)| code)?;
    Some((value, code))
}

/// Returns `amount` of `from` counted in `to`, `None` when `to` is not
/// reached from `from` by the factors UCUM fixes or the count overflows.
fn descend(amount: u128, from: Component, to: Component) -> Option<u128> {
    let mut unit = from;
    let mut count = amount;
    while unit != to {
        let (next, factor) = unit.smaller()?;
        count = count.checked_mul(factor)?;
        unit = next;
    }
    Some(count)
}

/// Returns the components of an ISO 8601 duration in order, `None` for text
/// that is no duration of the `P[nY][nM][nW][nD][T[nH][nM][nS]]` form the
/// page names.
fn components(duration: &str) -> Option<Vec<(Component, String)>> {
    let body = duration.strip_prefix('P')?;
    let (date, time) = match body.split_once('T') {
        Some((date, time)) if !time.is_empty() => (date, Some(time)),
        Some(_) => return None,
        None => (body, None),
    };
    let mut parts = Vec::new();
    let date_order = [
        Component::Years,
        Component::Months,
        Component::Weeks,
        Component::Days,
    ];
    let time_order = [Component::Hours, Component::Minutes, Component::Seconds];
    read_group(date, &date_order, &mut parts)?;
    if let Some(time) = time {
        read_group(time, &time_order, &mut parts)?;
    }
    let fractional = parts
        .iter()
        .rev()
        .skip(1)
        .any(|(_, amount)| amount.contains('.'));
    (!parts.is_empty() && !fractional).then_some(parts)
}

/// Reads the components of one side of the `T`, in the order `order` admits.
fn read_group(text: &str, order: &[Component], parts: &mut Vec<(Component, String)>) -> Option<()> {
    let mut rest = text;
    let mut allowed = order.iter();
    while !rest.is_empty() {
        let end = rest.find(|character: char| !(character.is_ascii_digit() || character == '.'))?;
        let (amount, tail) = rest.split_at(end);
        let mut letters = tail.chars();
        let designator = letters.next()?;
        let component = allowed.find(|component| component.designator() == designator)?;
        if !is_amount(amount) {
            return None;
        }
        parts.push((*component, String::from(amount)));
        rest = letters.as_str();
    }
    Some(())
}

/// Returns whether `text` is a non-negative decimal with no exponent.
fn is_amount(text: &str) -> bool {
    let (integer, fraction) = match text.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (text, None),
    };
    !integer.is_empty()
        && integer.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.is_none_or(|digits| {
            !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// Returns `text` divided by ten to the power `shift`, written as a decimal,
/// `None` for text that is no non-negative decimal.
fn shifted(text: &str, shift: u32) -> Option<String> {
    if !is_amount(text) {
        return None;
    }
    if shift == 0 {
        return Some(String::from(text));
    }
    let (integer, fraction) = text.split_once('.').unwrap_or((text, ""));
    let places = usize::try_from(shift).ok()?;
    let padded = format!("{integer:0>width$}", width = places.checked_add(1)?);
    let split = padded.len().checked_sub(places)?;
    let (head, tail) = (padded.get(..split)?, padded.get(split..)?);
    let head = head.trim_start_matches('0');
    let head = if head.is_empty() { "0" } else { head };
    let decimals = format!("{tail}{fraction}");
    let decimals = decimals.trim_end_matches('0');
    if decimals.is_empty() {
        return Some(String::from(head));
    }
    Some(format!("{head}.{decimals}"))
}

/// Writes `amount` of `component` as an ISO 8601 duration.
fn iso(amount: &str, component: Component) -> String {
    let time = if component.is_time() { "T" } else { "" };
    format!("P{time}{amount}{}", component.designator())
}

#[cfg(test)]
mod tests {
    use super::BRIDGE;
    use super::DurationLens;
    use super::reading;
    use super::shifted;
    use crate::engine::lens::Lens;
    use crate::engine::lens::LensError;
    use crate::engine::lens::quantity::UCUM;
    use fhir_types::r4::duration::Duration;
    use fhir_types::r4::primitives::Code;
    use fhir_types::r4::primitives::Decimal;
    use fhir_types::r4::primitives::Uri;
    use openehr_rm::v1_2::data_types::quantity::date_time::dv_duration::DvDuration;
    use proptest::prop_compose;
    use proptest::proptest;
    use proptest::sample::select;

    /// Returns a `DV_DURATION` of `value` with nothing else set.
    fn duration(value: &str) -> DvDuration {
        DvDuration {
            normal_status: None,
            normal_range: None,
            other_reference_ranges: None,
            magnitude_status: None,
            accuracy: None,
            accuracy_is_percent: None,
            value: String::from(value),
        }
    }

    /// Returns the FHIR `Duration` of `value` in `code`.
    fn fhir(value: &str, code: &str) -> Duration {
        Duration {
            value: Some(Decimal::from(value)),
            system: Some(Uri::from(UCUM)),
            code: Some(Code::from(code)),
            ..Duration::default()
        }
    }

    #[test]
    fn each_code_of_the_bridge_writes_its_designator() {
        // DV_DURATION.adoc: the CommonUCUMCodesForDuration table.
        let expected = [
            ("ms", "1500", "PT1.5S"),
            ("s", "45", "PT45S"),
            ("min", "30", "PT30M"),
            ("h", "2", "PT2H"),
            ("d", "3", "P3D"),
            ("wk", "4", "P4W"),
            ("mo", "6", "P6M"),
            ("a", "1", "P1Y"),
        ];
        assert_eq!(BRIDGE.len(), expected.len(), "one case per bridge entry");
        for (code, value, iso) in expected {
            let written = DurationLens
                .put(&fhir(value, code), None)
                .expect("the code is in the bridge");
            assert_eq!(written.value, iso, "{value} {code}");
        }
    }

    #[test]
    fn each_designator_reads_as_its_code() {
        for (iso, value, code) in [
            ("PT45S", "45", "s"),
            ("PT30M", "30", "min"),
            ("PT2H", "2", "h"),
            ("P3D", "3", "d"),
            ("P4W", "4", "wk"),
            ("P6M", "6", "mo"),
            ("P1Y", "1", "a"),
        ] {
            assert_eq!(reading(iso), Some((String::from(value), code)), "{iso}");
        }
    }

    #[test]
    fn several_components_read_in_the_smallest() {
        assert_eq!(reading("PT1H30M"), Some((String::from("90"), "min")));
        assert_eq!(reading("P1DT2H"), Some((String::from("26"), "h")));
        assert_eq!(reading("P1Y6M"), Some((String::from("18"), "mo")));
        assert_eq!(reading("PT1M30.5S"), Some((String::from("90.5"), "s")));
    }

    #[test]
    fn a_duration_no_single_code_states_is_refused() {
        for iso in [
            "P1Y2D",
            "P1MT1H",
            "PT1.5H30M",
            "P",
            "PT",
            "1D",
            "-P1D",
            "P1H",
            "PT1D",
        ] {
            assert_eq!(reading(iso), None, "{iso}");
            assert!(DurationLens.get(&duration(iso)).is_err(), "{iso}");
        }
    }

    #[test]
    fn a_code_outside_the_bridge_is_a_typed_refusal() {
        let error = DurationLens
            .put(&fhir("3", "mg"), None)
            .expect_err("the bridge is closed");
        assert!(
            matches!(error, LensError::Shape { attribute, .. } if attribute == "Duration.code"),
            "the refusal names the code: {error}"
        );
    }

    #[test]
    fn a_comparator_or_another_system_is_refused_going_in() {
        let bounded = Duration {
            comparator: Some(Code::from("<")),
            ..fhir("3", "d")
        };
        assert!(DurationLens.put(&bounded, None).is_err());
        let foreign = Duration {
            system: Some(Uri::from(
                "http://example.org/fhir/CodeSystem/synthetic-units",
            )),
            ..fhir("3", "d")
        };
        assert!(DurationLens.put(&foreign, None).is_err());
    }

    #[test]
    fn milliseconds_are_a_one_way_row() {
        assert_eq!(DurationLens.fallbacks(&fhir("500", "ms")).len(), 1);
        assert_eq!(
            DurationLens.fallbacks(&fhir("5", "s")),
            Vec::<crate::engine::lens::Fallback>::new()
        );
        assert_eq!(shifted("500", 3).as_deref(), Some("0.5"));
        assert_eq!(shifted("5", 3).as_deref(), Some("0.005"));
        assert_eq!(shifted("1000", 3).as_deref(), Some("1"));
        assert_eq!(shifted("1.5", 3).as_deref(), Some("0.0015"));
    }

    prop_compose! {
        /// A duration of one or several components that one code states.
        fn held()(
            hours in 0u32..100,
            minutes in 0u32..60,
            shape in select(vec!["H", "HM", "M", "D", "DH", "W", "Y", "YM"]),
        ) -> DvDuration {
            let value = match shape {
                "H" => format!("PT{hours}H"),
                "HM" => format!("PT{hours}H{minutes}M"),
                "M" => format!("PT{minutes}M"),
                "D" => format!("P{hours}D"),
                "DH" => format!("P{hours}DT{minutes}H"),
                "W" => format!("P{hours}W"),
                "Y" => format!("P{hours}Y"),
                _ => format!("P{hours}Y{minutes}M"),
            };
            duration(&value)
        }
    }

    proptest! {
        #[test]
        fn get_put(source in held()) {
            let view = DurationLens.get(&source).expect("one code states it");
            let back = DurationLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn put_get(source in held()) {
            let view = DurationLens.get(&source).expect("one code states it");
            let back = DurationLens.put(&view, None).expect("the view is complete");
            let again = DurationLens.get(&back).expect("one code states it");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }
}
