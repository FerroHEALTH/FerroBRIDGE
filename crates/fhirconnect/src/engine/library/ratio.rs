// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two functions that carry a dosage rate as an R4 `Ratio`.
//!
//! One is for `Dosage.doseAndRate.rate[x]` and one for
//! `MedicationAdministration.dosage.rate[x]`, both against the rate and the
//! administration duration of `openEHR-EHR-CLUSTER.dosage.v2`.
//!
//! R4 reads a rate `Ratio` as an amount over a time: "Where a rate such as
//! 500ml over 2 hours is specified, the use of rateRatio may be more
//! semantically correct than specifying using a rateQuantity of 250
//! mg/hour" (<https://hl7.org/fhir/R4/datatypes-definitions.html#Dosage.doseAndRate.rate_x_>).
//! The archetype holds the rate per unit of time and the duration of the
//! administration, so the numerator is the rate times the duration and the
//! denominator is the duration. The two are exact in both directions or
//! refused: no amount is rounded. No specification defines the functions:
//! our own design.

use core::str::FromStr;

use fhir_types::codec::Value;
use fhir_types::r4::ratio::Ratio;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_duration::DvDuration;
use openehr_rm::v1_2::data_types::quantity::dv_quantity::DvQuantity;
use rust_decimal::Decimal;

use crate::engine::library::class_at;
use crate::engine::library::datum;
use crate::engine::library::decode;
use crate::engine::library::duration;
use crate::engine::library::duration::TimeUnit;
use crate::engine::library::dv_quantity;
use crate::engine::library::encode;
use crate::engine::library::port_value;
use crate::engine::library::quantity_of;
use crate::engine::library::refused;
use crate::engine::library::ucum_amount;
use crate::engine::library::ucum_quantity;
use crate::engine::traverse::functions::MappingCodeError;
use crate::engine::traverse::functions::Ports;

/// The name of the function over `Dosage.doseAndRate.rate[x]`.
pub const DOSAGE_RATE: &str = "ratio_to_dosage";

/// The name of the function over `MedicationAdministration.dosage.rate[x]`.
pub const ACTION_RATE: &str = "ratio_to_dosage_action";

/// The rate per unit of time, the archetype's `at0134`.
const RATE: &str = "items[at0134]";

/// The duration of the administration, the archetype's `at0102`.
const DURATION: &str = "items[at0102]";

/// The ports of both functions.
pub const PORTS: &[&str] = &[RATE, DURATION];

/// The class of the rate.
const DV_QUANTITY: &str = "DV_QUANTITY";

/// The class of the duration.
const DV_DURATION: &str = "DV_DURATION";

/// The significant digits a `rust_decimal::Decimal` holds
/// (<https://docs.rs/rust_decimal/latest/rust_decimal/struct.Decimal.html>).
const SIGNIFICANT_DIGITS: usize = 28;

/// Writes a rate `Ratio` of the dosage as its rate and duration.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the element is no `Ratio` the rate and
/// the duration carry exactly, by the rules of this module.
pub fn dosage_rate_to_openehr(element: &Value) -> Result<Ports, MappingCodeError> {
    to_openehr(DOSAGE_RATE, element)
}

/// Reads a dosage's rate and duration back as a rate `Ratio`.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the rate and the duration have no exact
/// `Ratio` form, by the rules of this module.
pub fn dosage_rate_to_fhir(ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
    to_fhir(DOSAGE_RATE, ports)
}

/// Writes a rate `Ratio` of an administration as its rate and duration.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the element is no `Ratio` the rate and
/// the duration carry exactly, by the rules of this module.
pub fn action_rate_to_openehr(element: &Value) -> Result<Ports, MappingCodeError> {
    to_openehr(ACTION_RATE, element)
}

/// Reads an administration's rate and duration back as a rate `Ratio`.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the rate and the duration have no exact
/// `Ratio` form, by the rules of this module.
pub fn action_rate_to_fhir(ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
    to_fhir(ACTION_RATE, ports)
}

/// Writes a rate `Ratio` as the rate it divides to and the duration in its
/// denominator.
///
/// The numerator `n a` over the denominator `d t`, `t` a unit of time, is the
/// rate `n/d a/t` and the duration `d t`. Reads `Ratio.numerator` and
/// `Ratio.denominator`; writes the rate and the duration.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the element is no `Ratio`, carries an
/// id or an extension, names one side alone (R4 `rat-1`), a side
/// `ucum_amount` refuses, a denominator in no unit of time or of zero, a
/// quotient the decimals cannot hold exactly, or a duration
/// [`duration::to_iso`] refuses.
fn to_openehr(code: &str, element: &Value) -> Result<Ports, MappingCodeError> {
    let ratio: Ratio = decode(code, "Ratio", element)?;
    if ratio.id.is_some() || !ratio.extension.is_empty() {
        return Err(refused(
            code,
            "the ratio carries an id or an extension, which the archetype has no place for",
        ));
    }
    let (numerator, denominator) = match (ratio.numerator.as_ref(), ratio.denominator.as_ref()) {
        (None, None) => return Ok(Ports::new()),
        (Some(numerator), Some(denominator)) => (numerator, denominator),
        _ => {
            return Err(refused(
                code,
                "a ratio names both a numerator and a denominator (R4 `rat-1`)",
            ));
        }
    };
    let (amount, amount_units) = ucum_amount(code, numerator)?;
    let (time, time_units) = ucum_amount(code, denominator)?;
    let unit = TimeUnit::from_ucum(time_units).ok_or_else(|| {
        refused(
            code,
            format!("the denominator's unit `{time_units}` is no unit of time"),
        )
    })?;
    let rate = exact(code, amount, time, Decimal::checked_div)?.normalize();
    // NOTE: no specification governs this: our own design, a quotient that fills every
    // significant digit `rust_decimal` holds was rounded, so it is no exact rate.
    let filled = rate.mantissa().unsigned_abs().to_string().len() >= SIGNIFICANT_DIGITS;
    if filled || rate.checked_mul(number(code, time)?) != Some(number(code, amount)?) {
        return Err(refused(
            code,
            format!("{amount} over {time} has no exact decimal rate"),
        ));
    }
    let rate = dv_quantity(
        code,
        &ucum_quantity(&rate.to_string(), &format!("{amount_units}/{time_units}")),
    )?;
    let duration = duration::to_iso(code, time, unit)?;
    Ok(Ports::new()
        .with(RATE, datum(DV_QUANTITY, &rate))
        .with(DURATION, datum(DV_DURATION, &duration)))
}

/// Reads the rate and the duration back as the `Ratio` of the amount given
/// over the duration.
///
/// The rate `r a/t` over the duration `d t` is the numerator `r*d a` over the
/// denominator `d t`. Nothing is written unless both are held. Reads the rate
/// and the duration; writes `Ratio.numerator` and `Ratio.denominator`.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the rate is text, when the rate's
/// units are no `<amount>/<time>` whose time is the duration's unit, when
/// the duration [`duration::from_iso`] refuses, or when the product
/// overflows the decimals.
fn to_fhir(code: &str, ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
    let (Some(rate_class), Some(_)) = (class_at(ports, RATE), class_at(ports, DURATION)) else {
        return Ok(None);
    };
    if rate_class != DV_QUANTITY {
        return Err(refused(
            code,
            "the rate is text, and a rate with a duration reads as a Ratio of quantities",
        ));
    }
    let (Some(rate), Some(held)) = (
        port_value::<DvQuantity>(code, ports, RATE)?,
        port_value::<DvDuration>(code, ports, DURATION)?,
    ) else {
        return Ok(None);
    };
    let rate = quantity_of(code, &rate)?;
    let (rate_value, rate_units) = ucum_amount(code, &rate)?;
    let (time, unit) = duration::from_iso(code, &held)?;
    let amount_units = rate_units
        .strip_suffix(unit.ucum())
        .and_then(|units| units.strip_suffix('/'))
        .filter(|units| !units.is_empty())
        .ok_or_else(|| {
            refused(
                code,
                format!(
                    "the rate's units `{rate_units}` are no amount per `{}`, the unit of the \
                     duration `{}`",
                    unit.ucum(),
                    held.value
                ),
            )
        })?;
    let amount = exact(code, rate_value, &time, Decimal::checked_mul)?;
    let ratio = Ratio {
        numerator: Some(ucum_quantity(&amount.normalize().to_string(), amount_units)),
        denominator: Some(ucum_quantity(&time, unit.ucum())),
        ..Ratio::default()
    };
    encode(code, &ratio).map(Some)
}

/// Reads a plain decimal as an exact decimal.
fn number(code: &str, text: &str) -> Result<Decimal, MappingCodeError> {
    Decimal::from_str(text)
        .map_err(|error| refused(code, format!("`{text}` is no exact decimal: {error}")))
}

/// Applies `operation` to two plain decimals, refusing an overflow or a
/// division by zero.
fn exact(
    code: &str,
    left: &str,
    right: &str,
    operation: impl Fn(Decimal, Decimal) -> Option<Decimal>,
) -> Result<Decimal, MappingCodeError> {
    operation(number(code, left)?, number(code, right)?).ok_or_else(|| {
        refused(
            code,
            format!("{left} and {right} have no exact decimal result"),
        )
    })
}
