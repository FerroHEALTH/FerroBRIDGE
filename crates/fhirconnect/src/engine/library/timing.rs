// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The functions the timing mappings of the published library name, over R4
//! `Timing.repeat` and the `openEHR-EHR-CLUSTER.timing_daily.v1` and
//! `openEHR-EHR-CLUSTER.timing_nondaily.v1` archetypes.

use fhir_types::codec::Value;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Decimal;
use fhir_types::r4::primitives::PositiveInt;
use fhir_types::r4::timing::TimingRepeat;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_duration::DvDuration;

use crate::engine::library::datum;
use crate::engine::library::decode;
use crate::engine::library::duration;
use crate::engine::library::duration::TimeUnit;
use crate::engine::library::encode;
use crate::engine::library::port_value;
use crate::engine::library::primitive;
use crate::engine::library::refused;
use crate::engine::traverse::functions::MappingCodeError;
use crate::engine::traverse::functions::Ports;

/// The name of the function between `Timing.repeat` and a non-daily timing.
pub const NON_DAILY: &str = "timingNonDaily";

/// The interval between doses of a non-daily timing, the archetype's
/// `at0002`, a `DV_DURATION`.
const NON_DAILY_PERIOD: &str = "items[at0002]";

/// The ports of the non-daily timing.
pub const NON_DAILY_PORTS: &[&str] = &[NON_DAILY_PERIOD];

/// The class the interval holds.
const DV_DURATION: &str = "DV_DURATION";

/// The units a non-daily period is counted in.
const NON_DAILY_UNITS: [TimeUnit; 3] = [TimeUnit::Week, TimeUnit::Month, TimeUnit::Year];

/// Writes a `Timing.repeat` that occurs once per period as the interval of a
/// non-daily timing.
///
/// R4 reads `frequency` times per `period` of `periodUnit`
/// (<https://hl7.org/fhir/R4/datatypes-definitions.html#Timing.repeat.frequency>),
/// and the non-daily archetype holds the interval between doses alone, so
/// only one event per period has a place. Reads `frequency`,
/// `frequencyMax`, `period`, `periodMax` and `periodUnit`; writes the
/// interval.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the element is no `Timing.repeat`, when
/// it occurs more than once per period, states a `frequencyMax` or a
/// `periodMax`, carries a period with no unit (R4 `tim-2`) or in a unit of a
/// day or less, or a period [`duration::to_iso`] refuses.
pub fn non_daily_to_openehr(element: &Value) -> Result<Ports, MappingCodeError> {
    let code = NON_DAILY;
    let repeat: TimingRepeat = decode(code, "Timing.repeat", element)?;
    once_per_period(code, &repeat)?;
    if repeat.period_max.is_some() {
        return Err(refused(
            code,
            "`periodMax` states a range of intervals, and the archetype holds one",
        ));
    }
    let Some(ref period) = repeat.period else {
        return Ok(Ports::new());
    };
    let amount = primitive(
        code,
        "period",
        period.id.as_ref(),
        &period.extension,
        period.value.as_ref(),
    )?;
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
        .filter(|unit| NON_DAILY_UNITS.contains(unit))
        .ok_or_else(|| {
            refused(
                code,
                format!("`{unit}` is no unit of a non-daily period, which is `wk`, `mo` or `a`"),
            )
        })?;
    let produced = duration::to_iso(code, amount, unit)?;
    Ok(Ports::new().with(NON_DAILY_PERIOD, datum(DV_DURATION, &produced)))
}

/// Reads the interval of a non-daily timing back as a `Timing.repeat` that
/// occurs once per period.
///
/// R4 assumes one event per period when no frequency is stated, "but systems
/// SHOULD always be specific about this"
/// (`Timing.repeat.frequency.meaningWhenMissing`), so `frequency` is written.
/// Reads the interval; writes `frequency`, `period` and `periodUnit`.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the interval is no `DV_DURATION`, one
/// [`duration::from_iso`] refuses, or one counted in a unit of a day or less.
pub fn non_daily_to_fhir(ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
    let code = NON_DAILY;
    let Some(held) = port_value::<DvDuration>(code, ports, NON_DAILY_PERIOD)? else {
        return Ok(None);
    };
    let (amount, unit) = duration::from_iso(code, &held)?;
    if !NON_DAILY_UNITS.contains(&unit) {
        return Err(refused(
            code,
            format!(
                "`{}` is an interval of a day or less, which is a daily timing",
                held.value
            ),
        ));
    }
    let repeat = TimingRepeat {
        frequency: Some(PositiveInt::from(1)),
        period: Some(Decimal::from(amount)),
        period_unit: Some(Code::from(unit.ucum())),
        ..TimingRepeat::default()
    };
    encode(code, &repeat).map(Some)
}

/// Refuses a `Timing.repeat` that occurs other than once per period.
fn once_per_period(code: &str, repeat: &TimingRepeat) -> Result<(), MappingCodeError> {
    if repeat.frequency_max.is_some() {
        return Err(refused(
            code,
            "`frequencyMax` states a range of events per period, and the archetype holds one \
             interval",
        ));
    }
    let Some(ref frequency) = repeat.frequency else {
        return Ok(());
    };
    if !frequency.extension.is_empty() || frequency.id.is_some() {
        return Err(refused(
            code,
            "`frequency` carries an id or an extension, which the archetype has no place for",
        ));
    }
    if frequency.value != Some(1) {
        return Err(refused(
            code,
            "the timing occurs more than once per period, and the archetype holds the interval \
             between single events",
        ));
    }
    Ok(())
}
