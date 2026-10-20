// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two functions of the `CLUSTER.dosage.v2` mappings that carry an R4
//! `Range`: the dose as an interval of quantities, and the rate as text.

use fhir_types::codec::Value;
use fhir_types::r4::quantity::Quantity;
use fhir_types::r4::range::Range;
use openehr_rm::v1_2::data_types::quantity::dv_interval::DvInterval;
use openehr_rm::v1_2::data_types::quantity::dv_quantity::DvQuantity;
use openehr_rm::v1_2::data_types::text::dv_text::DvTextData;

use crate::engine::library::class_at;
use crate::engine::library::datum;
use crate::engine::library::decode;
use crate::engine::library::duration::plain_decimal;
use crate::engine::library::dv_interval;
use crate::engine::library::encode;
use crate::engine::library::port_value;
use crate::engine::library::range_of;
use crate::engine::library::refused;
use crate::engine::library::ucum_amount;
use crate::engine::library::ucum_quantity;
use crate::engine::traverse::functions::MappingCodeError;
use crate::engine::traverse::functions::Ports;
use crate::engine::traverse::functions::SELF_PORT;

/// The name of the function between a dose `Range` and the dose interval.
pub const DOSE_RANGE: &str = "dosageQuantityToRange";

/// The name of the function between a rate `Range` and the rate as text.
pub const RANGE_TEXT: &str = "rangeToText";

/// The one port of both, the `ELEMENT` the mapping's `with.openehr` names
/// (the archetype's `at0144` dose and `at0134` rate).
pub const PORTS: &[&str] = &[SELF_PORT];

/// The class of the dose interval, as the Web Template names it.
const INTERVAL: &str = "DV_INTERVAL<DV_QUANTITY>";

/// The class of the rate as text.
const TEXT: &str = "DV_TEXT";

/// What separates the two bounds of a range written as text.
const BETWEEN: &str = " - ";

/// Writes a dose `Range` as an interval of quantities.
///
/// `low` and `high` are `lower` and `upper`, both included and bounded
/// where they are present, by the rows of `DV_INTERVAL.adoc`; each bound
/// converts through the engine's `DV_QUANTITY` cell. A side the range leaves
/// out is unbounded. Reads `Range.low` and `Range.high`; writes the
/// `DV_INTERVAL<DV_QUANTITY>` at the port.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the element is no `Range`, carries an
/// id or an extension, names neither bound, or a bound the cell refuses.
pub fn dose_range_to_openehr(element: &Value) -> Result<Ports, MappingCodeError> {
    let code = DOSE_RANGE;
    let range = plain_range(code, element)?;
    if range.low.is_none() && range.high.is_none() {
        return Err(refused(code, "the range names neither bound"));
    }
    let interval = dv_interval(code, &range)?;
    Ok(Ports::new().with(SELF_PORT, datum(INTERVAL, &interval)))
}

/// Reads a dose interval back as a `Range`.
///
/// A dose the element holds as one `DV_QUANTITY` is no range, so it writes
/// nothing here. Reads the port; writes `Range.low` and `Range.high`.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the interval does not decode, excludes
/// a bound it holds (`DV_INTERVAL.adoc` includes both), names a bound and
/// calls the side unbounded, or holds a bound with a comparator, which an R4
/// `SimpleQuantity` does not carry (`sqty-1`).
pub fn dose_range_to_fhir(ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
    let code = DOSE_RANGE;
    if class_at(ports, SELF_PORT) != Some(INTERVAL) {
        return Ok(None);
    }
    let Some(interval) = port_value::<DvInterval<DvQuantity>>(code, ports, SELF_PORT)? else {
        return Ok(None);
    };
    // NOTE: no specification governs this: our own design, a side whose flag and
    // limit disagree is refused rather than read as one of the two.
    if interval.lower.is_none() != interval.lower_unbounded
        || interval.upper.is_none() != interval.upper_unbounded
    {
        return Err(refused(
            code,
            "a side of the interval holds no bound and is not unbounded",
        ));
    }
    encode(code, &range_of(code, &interval)?).map(Some)
}

/// Writes a rate `Range` as the text of the rate.
///
/// The rate element admits a `DV_QUANTITY` or a `DV_TEXT` and no interval,
/// so a range is written as text, `<low> <code> - <high> <code>`, with each
/// bound's value and UCUM code as FHIR carries them, which reads back to
/// the same range. No specification governs the text: our own design. Reads
/// `Range.low` and `Range.high`; writes the `DV_TEXT` at the port.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the element is no `Range`, carries an
/// id or an extension, lacks a bound, or holds a bound the text cannot carry:
/// one with no value or code, a comparator, a system other than UCUM, or a
/// `unit` other than its code.
pub fn range_text_to_openehr(element: &Value) -> Result<Ports, MappingCodeError> {
    let code = RANGE_TEXT;
    let range = plain_range(code, element)?;
    let (Some(low), Some(high)) = (range.low.as_ref(), range.high.as_ref()) else {
        return Err(refused(code, "a range written as text names both bounds"));
    };
    let text = format!("{}{BETWEEN}{}", written(code, low)?, written(code, high)?);
    let value = DvTextData {
        value: text,
        hyperlink: None,
        formatting: None,
        mappings: None,
        language: None,
        encoding: None,
    };
    Ok(Ports::new().with(SELF_PORT, datum(TEXT, &value)))
}

/// Reads the text of a rate back as a `Range`.
///
/// A rate the element holds as a `DV_QUANTITY` is no range, so it writes
/// nothing here. Reads the port; writes `Range.low` and `Range.high`, each
/// with its value, its code as `code` and `unit`, and the UCUM system.
///
/// # Errors
///
/// Returns [`MappingCodeError`] when the text is not the form
/// [`range_text_to_openehr`] writes, or carries a hyperlink, formatting,
/// term mappings, a language or an encoding, which a `Range` has no place
/// for.
pub fn range_text_to_fhir(ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
    let code = RANGE_TEXT;
    if class_at(ports, SELF_PORT) != Some(TEXT) {
        return Ok(None);
    }
    let Some(text) = port_value::<DvTextData>(code, ports, SELF_PORT)? else {
        return Ok(None);
    };
    if text.hyperlink.is_some()
        || text.formatting.is_some()
        || text.mappings.is_some()
        || text.language.is_some()
        || text.encoding.is_some()
    {
        return Err(refused(
            code,
            "the text carries a hyperlink, formatting, mappings, a language or an encoding, which \
             a Range has no place for",
        ));
    }
    let (low, high) = text.value.split_once(BETWEEN).ok_or_else(|| {
        refused(
            code,
            format!("`{}` is no range written as text", text.value),
        )
    })?;
    let range = Range {
        low: Some(read(code, low)?),
        high: Some(read(code, high)?),
        ..Range::default()
    };
    encode(code, &range).map(Some)
}

/// Decodes a `Range`, refusing one that carries an id or an extension.
fn plain_range(code: &str, element: &Value) -> Result<Range, MappingCodeError> {
    let range: Range = decode(code, "Range", element)?;
    if range.id.is_some() || !range.extension.is_empty() {
        return Err(refused(
            code,
            "the range carries an id or an extension, which the archetype has no place for",
        ));
    }
    Ok(range)
}

/// Returns one bound of a range as the text writes it, `<value> <code>`.
fn written(code: &str, bound: &Quantity) -> Result<String, MappingCodeError> {
    let (value, units) = ucum_amount(code, bound)?;
    Ok(format!("{value} {units}"))
}

/// Reads one bound of a range written as text.
fn read(code: &str, bound: &str) -> Result<Quantity, MappingCodeError> {
    let (value, units) = bound
        .split_once(' ')
        .filter(|(value, units)| {
            plain_decimal(value).is_some() && !units.is_empty() && !units.contains(' ')
        })
        .ok_or_else(|| {
            refused(
                code,
                format!("`{bound}` is no bound written as `<value> <code>`"),
            )
        })?;
    Ok(ucum_quantity(value, units))
}
