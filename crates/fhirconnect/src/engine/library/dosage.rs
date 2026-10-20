// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The functions the `CLUSTER.dosage.v2` mappings of the published library
//! name, over R4 `Dosage` and the `openEHR-EHR-CLUSTER.dosage.v2` archetype.

use fhir_types::codec::Value;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Decimal;
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
use crate::engine::traverse::functions::SELF_PORT;

/// The name of the function between `Timing.repeat.duration` and the
/// administration duration.
pub const ADMINISTRATION_DURATION: &str = "dosageDurationToAdministrationDuration";

/// Its one port, the `DV_DURATION` node the mapping's `with.openehr` names
/// (the archetype's `at0102`).
pub const ADMINISTRATION_DURATION_PORTS: &[&str] = &[SELF_PORT];

/// The class the administration duration holds.
const DV_DURATION: &str = "DV_DURATION";

/// Writes `Timing.repeat.duration` and its `durationUnit` as the
/// administration duration.
///
/// R4 defines `duration` as "How long when it happens" with its unit of time
/// in `durationUnit`, one of `units-of-time`
/// (<https://hl7.org/fhir/R4/datatypes-definitions.html#Timing.repeat.duration>),
/// and `DV_DURATION.adoc` gives the ISO 8601 form of each unit. Reads
/// `duration`, `durationMax` and `durationUnit` of `Timing.repeat`; writes the
/// `DV_DURATION` at the port.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the element is no `Timing.repeat`, when
/// it carries a `durationMax` (a `DV_DURATION` holds one value), a duration
/// with no unit (R4 `tim-1`), a unit outside `units-of-time`, or an amount
/// [`duration::to_iso`] refuses.
pub fn administration_duration_to_openehr(element: &Value) -> Result<Ports, MappingCodeError> {
    let code = ADMINISTRATION_DURATION;
    let repeat: TimingRepeat = decode(code, "Timing.repeat", element)?;
    if repeat.duration_max.is_some() {
        return Err(refused(
            code,
            "`durationMax` states a range, and the administration duration holds one value",
        ));
    }
    let Some(ref amount) = repeat.duration else {
        return Ok(Ports::new());
    };
    let amount = primitive(
        code,
        "duration",
        amount.id.as_ref(),
        &amount.extension,
        amount.value.as_ref(),
    )?;
    let unit = repeat
        .duration_unit
        .as_ref()
        .map(|unit| {
            primitive(
                code,
                "durationUnit",
                unit.id.as_ref(),
                &unit.extension,
                unit.value.as_ref(),
            )
        })
        .transpose()?
        .ok_or_else(|| refused(code, "a duration has a unit (R4 `tim-1`)"))?;
    let unit = TimeUnit::from_ucum(unit)
        .ok_or_else(|| refused(code, format!("`{unit}` is no unit of `units-of-time`")))?;
    let produced = duration::to_iso(amount, unit).map_err(|reason| refused(code, reason))?;
    Ok(Ports::new().with(SELF_PORT, datum(DV_DURATION, &produced)))
}

/// Reads the administration duration back as `Timing.repeat.duration` and
/// its `durationUnit`.
///
/// Reads the `DV_DURATION` at the port; writes `duration` and
/// `durationUnit` of `Timing.repeat`.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the port's value is no `DV_DURATION` or
/// one [`duration::from_iso`] refuses.
pub fn administration_duration_to_fhir(ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
    let code = ADMINISTRATION_DURATION;
    let Some(held) = port_value::<DvDuration>(code, ports, SELF_PORT)? else {
        return Ok(None);
    };
    let (amount, unit) = duration::from_iso(&held).map_err(|reason| refused(code, reason))?;
    let repeat = TimingRepeat {
        duration: Some(Decimal::from(amount)),
        duration_unit: Some(Code::from(unit.ucum())),
        ..TimingRepeat::default()
    };
    encode(code, &repeat).map(Some)
}
