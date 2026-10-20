// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The function between R4 `Timing.repeat` and a daily timing,
//! `openEHR-EHR-CLUSTER.timing_daily.v1`.
//!
//! R4 reads `frequency` times per `period` of `periodUnit`
//! (<https://hl7.org/fhir/R4/datatypes-definitions.html#Timing.repeat.frequency>).
//! The archetype holds a frequency as a quantity per unit of time (`at0003`,
//! `1/s`, `1/min`, `1/h` or `1/d`, or an interval of them), an interval
//! between doses (`at0014`, a `DV_DURATION`), and a time of day (`at0004`, a
//! `DV_TIME`). A frequency per one unit is the frequency; one event per
//! period of more than one unit is the interval; anything else has no place
//! in the archetype and is refused. No specification defines the function:
//! our own design.

use fhir_types::codec::Value;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Decimal;
use fhir_types::r4::primitives::PositiveInt;
use fhir_types::r4::primitives::Time;
use fhir_types::r4::quantity::Quantity;
use fhir_types::r4::range::Range;
use fhir_types::r4::timing::TimingRepeat;
use openehr_base::containers::present_nonempty;
use openehr_base::validate::Validate as _;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_duration::DvDuration;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_time::DvTime;
use openehr_rm::v1_2::data_types::quantity::dv_interval::DvInterval;
use openehr_rm::v1_2::data_types::quantity::dv_quantity::DvQuantity;

use crate::engine::library::class_at;
use crate::engine::library::datum;
use crate::engine::library::decode;
use crate::engine::library::duration;
use crate::engine::library::duration::TimeUnit;
use crate::engine::library::dv_interval;
use crate::engine::library::dv_quantity;
use crate::engine::library::encode;
use crate::engine::library::port_value;
use crate::engine::library::primitive;
use crate::engine::library::quantity_of;
use crate::engine::library::range_of;
use crate::engine::library::refused;
use crate::engine::library::ucum_amount;
use crate::engine::library::ucum_quantity;
use crate::engine::traverse::functions::MappingCodeError;
use crate::engine::traverse::functions::Ports;

/// The name of the function.
pub const DAILY: &str = "timingToDaily";

/// The frequency, the archetype's `at0003`.
const FREQUENCY: &str = "items[at0003]";

/// The interval between doses, the archetype's `at0014`.
const INTERVAL: &str = "items[at0014]";

/// The time of day, the archetype's `at0004`.
const TIME_OF_DAY: &str = "items[at0004]";

/// The ports of the function.
pub const PORTS: &[&str] = &[FREQUENCY, INTERVAL, TIME_OF_DAY];

/// The class of a frequency held as one quantity.
const DV_QUANTITY: &str = "DV_QUANTITY";

/// The class of a frequency held as a range, as the Web Template names it.
const DV_INTERVAL: &str = "DV_INTERVAL<DV_QUANTITY>";

/// The class of the interval between doses.
const DV_DURATION: &str = "DV_DURATION";

/// The class of the time of day.
const DV_TIME: &str = "DV_TIME";

/// The units a daily period is counted in.
const DAILY_UNITS: [TimeUnit; 4] = [
    TimeUnit::Second,
    TimeUnit::Minute,
    TimeUnit::Hour,
    TimeUnit::Day,
];

/// The one period a frequency per unit of time is counted over.
const ONE: &str = "1";

/// Writes a daily `Timing.repeat` as its frequency or interval and its time
/// of day.
///
/// Reads `frequency`, `frequencyMax`, `period`, `periodMax`, `periodUnit`
/// and `timeOfDay`; writes the frequency (`period` of one unit), or the
/// interval (one event per `period` of more than one unit), and the time of
/// day.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the element is no `Timing.repeat`, when
/// it carries a `periodMax`, a frequency with no period, a period with no
/// unit (R4 `tim-2`) or in a unit longer than a day, more than one event
/// over a period of more than one unit, or more than one time of day (the
/// archetype holds one).
pub fn to_openehr(element: &Value) -> Result<Ports, MappingCodeError> {
    let code = DAILY;
    let repeat: TimingRepeat = decode(code, "Timing.repeat", element)?;
    if repeat.period_max.is_some() {
        return Err(refused(
            code,
            "`periodMax` states a range of periods, and the archetype holds one",
        ));
    }
    let frequency = count(code, "frequency", repeat.frequency.as_ref())?;
    let frequency_max = count(code, "frequencyMax", repeat.frequency_max.as_ref())?;
    let mut ports = match repeat.period {
        Some(ref period) => {
            let period = primitive(
                code,
                "period",
                period.id.as_ref(),
                &period.extension,
                period.value.as_ref(),
            )?;
            frequency_ports(code, &repeat, period, frequency, frequency_max)?
        }
        None if frequency.is_some() || frequency_max.is_some() => {
            return Err(refused(
                code,
                "a frequency states the period it counts over",
            ));
        }
        None => Ports::new(),
    };
    match repeat.time_of_day.as_slice() {
        [] => {}
        [time] => {
            let value = primitive(
                code,
                "timeOfDay",
                time.id.as_ref(),
                &time.extension,
                time.value.as_ref(),
            )?;
            let held = DvTime {
                normal_status: None,
                normal_range: None,
                other_reference_ranges: present_nonempty(Vec::new()),
                magnitude_status: None,
                accuracy: None,
                value: String::from(value),
            };
            if !held.invariants().is_empty() {
                return Err(refused(code, format!("`{value}` is no DV_TIME")));
            }
            ports = ports.with(TIME_OF_DAY, datum(DV_TIME, &held));
        }
        _ => {
            return Err(refused(
                code,
                "the timing names more than one time of day, and the archetype holds one",
            ));
        }
    }
    Ok(ports)
}

/// Returns the port a period and its frequency write.
fn frequency_ports(
    code: &str,
    repeat: &TimingRepeat,
    period: &str,
    frequency: Option<u32>,
    frequency_max: Option<u32>,
) -> Result<Ports, MappingCodeError> {
    let unit = repeat
        .period_unit
        .as_ref()
        .map(|unit| {
            primitive(
                code,
                "periodUnit",
                unit.id.as_ref(),
                &unit.extension,
                unit.value.as_ref(),
            )
        })
        .transpose()?
        .ok_or_else(|| refused(code, "a period has a unit (R4 `tim-2`)"))?;
    let unit = TimeUnit::from_ucum(unit)
        .filter(|unit| DAILY_UNITS.contains(unit))
        .ok_or_else(|| {
            refused(
                code,
                format!("`{unit}` is no unit of a daily period, which is `s`, `min`, `h` or `d`"),
            )
        })?;
    // NOTE: R4 Timing.repeat.frequency.meaningWhenMissing, with no frequency
    // "the event occurs once per period".
    let events = frequency.unwrap_or(1);
    let per = format!("1/{}", unit.ucum());
    if period == ONE {
        let lower = dv_quantity(code, &ucum_quantity(&events.to_string(), &per))?;
        let Some(most) = frequency_max else {
            return Ok(Ports::new().with(FREQUENCY, datum(DV_QUANTITY, &lower)));
        };
        let range = dv_interval(
            code,
            &Range {
                low: Some(ucum_quantity(&events.to_string(), &per)),
                high: Some(ucum_quantity(&most.to_string(), &per)),
                ..Range::default()
            },
        )?;
        return Ok(Ports::new().with(FREQUENCY, datum(DV_INTERVAL, &range)));
    }
    if events != 1 || frequency_max.is_some() {
        return Err(refused(
            code,
            "more than one event over a period of more than one unit has no place in the \
             archetype, which holds a frequency per unit or an interval between single events",
        ));
    }
    let interval = duration::to_iso(code, period, unit)?;
    Ok(Ports::new().with(INTERVAL, datum(DV_DURATION, &interval)))
}

/// Reads a daily timing back as a `Timing.repeat`.
///
/// A frequency of `n` per unit `u` reads as `frequency` `n` per `period` 1
/// `u`, a range of them adds `frequencyMax`, an interval of `p` units reads
/// as `frequency` 1 per `period` `p`, and the time of day as `timeOfDay`.
/// Reads the three ports; writes those members.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the composition holds both a frequency
/// and an interval, a frequency that is no positive whole count per
/// `1/s`, `1/min`, `1/h` or `1/d`, a range of them not both bounded and
/// included in one unit, an interval longer than a day per unit, or a time
/// of day that is no R4 `time` or carries more than its value.
pub fn to_fhir(ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
    let code = DAILY;
    let mut repeat = TimingRepeat::default();
    match (class_at(ports, FREQUENCY), class_at(ports, INTERVAL)) {
        (Some(_), Some(_)) => {
            return Err(refused(
                code,
                "the timing holds both a frequency and an interval, and a Timing.repeat states \
                 one",
            ));
        }
        (Some(DV_QUANTITY), None) => {
            if let Some(held) = port_value::<DvQuantity>(code, ports, FREQUENCY)? {
                let (events, unit) = per_unit(code, &quantity_of(code, &held)?)?;
                repeat.frequency = Some(PositiveInt::from(events));
                repeat.period = Some(Decimal::from(ONE));
                repeat.period_unit = Some(Code::from(unit.ucum()));
            }
        }
        (Some(_), None) => {
            if let Some(held) = port_value::<DvInterval<DvQuantity>>(code, ports, FREQUENCY)? {
                let range = range_of(code, &held)?;
                let (Some(lower), Some(upper)) = (range.low.as_ref(), range.high.as_ref()) else {
                    return Err(refused(
                        code,
                        "a range of frequencies has both bounds, each included",
                    ));
                };
                let (events, unit) = per_unit(code, lower)?;
                let (most, most_unit) = per_unit(code, upper)?;
                if unit != most_unit {
                    return Err(refused(
                        code,
                        "the bounds of a range of frequencies count per different units",
                    ));
                }
                repeat.frequency = Some(PositiveInt::from(events));
                repeat.frequency_max = Some(PositiveInt::from(most));
                repeat.period = Some(Decimal::from(ONE));
                repeat.period_unit = Some(Code::from(unit.ucum()));
            }
        }
        (None, Some(_)) => {
            if let Some(held) = port_value::<DvDuration>(code, ports, INTERVAL)? {
                let (period, unit) = duration::from_iso(code, &held)?;
                if !DAILY_UNITS.contains(&unit) {
                    return Err(refused(
                        code,
                        format!(
                            "`{}` is longer than a day, which is a non-daily timing",
                            held.value
                        ),
                    ));
                }
                repeat.frequency = Some(PositiveInt::from(1));
                repeat.period = Some(Decimal::from(period));
                repeat.period_unit = Some(Code::from(unit.ucum()));
            }
        }
        (None, None) => {}
    }
    if let Some(held) = port_value::<DvTime>(code, ports, TIME_OF_DAY)? {
        if held.normal_status.is_some()
            || held.normal_range.is_some()
            || held.other_reference_ranges.is_some()
            || held.accuracy.is_some()
            || held
                .magnitude_status
                .as_deref()
                .is_some_and(|status| status != "=")
        {
            return Err(refused(
                code,
                "the time of day carries a range, a status or an accuracy, which an R4 `time` has \
                 no place for",
            ));
        }
        Time::check(&held.value)
            .map_err(|error| refused(code, format!("`{}` is no R4 `time`: {error}", held.value)))?;
        repeat.time_of_day = vec![Time::from(held.value)];
    }
    if repeat == TimingRepeat::default() {
        return Ok(None);
    }
    encode(code, &repeat).map(Some)
}

/// Returns the count a `positiveInt` holds, refusing one that carries an id
/// or an extension.
fn count(
    code: &str,
    element: &str,
    held: Option<&PositiveInt>,
) -> Result<Option<u32>, MappingCodeError> {
    let Some(held) = held else {
        return Ok(None);
    };
    if held.id.is_some() || !held.extension.is_empty() {
        return Err(refused(
            code,
            format!(
                "`{element}` carries an id or an extension, which the archetype has no place for"
            ),
        ));
    }
    held.value
        .map(Some)
        .ok_or_else(|| refused(code, format!("`{element}` carries no value")))
}

/// Returns the whole count and the unit of time of a frequency quantity
/// `<n> 1/<unit>`.
fn per_unit(code: &str, quantity: &Quantity) -> Result<(u32, TimeUnit), MappingCodeError> {
    let (value, units) = ucum_amount(code, quantity)?;
    let unit = units
        .strip_prefix("1/")
        .and_then(TimeUnit::from_ucum)
        .filter(|unit| DAILY_UNITS.contains(unit))
        .ok_or_else(|| {
            refused(
                code,
                format!("`{units}` is no frequency per `s`, `min`, `h` or `d`"),
            )
        })?;
    let events = value
        .parse::<u32>()
        .ok()
        .filter(|events| *events > 0)
        .ok_or_else(|| {
            refused(
                code,
                format!("`{value}` is no positive whole number of events"),
            )
        })?;
    Ok((events, unit))
}
