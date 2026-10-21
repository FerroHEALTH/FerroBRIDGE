// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `mappingCode` functions this crate ships, registered at build time.
//!
//! A PROGRAMMED mapping runs "an external bit of code", and "the way how this
//! code is provided and where is up to the vendor"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/concept-type/concept-mappings.adoc`,
//! §Programmed mappings). [`Library`] is this crate's answer: a fixed table of
//! first-party functions, each typed over the generated FHIR model on one
//! side and the `openehr-rm` classes on the other, with the openEHR ports it
//! reads and writes declared so the compiler resolves them against the
//! template at load.
//!
//! What each function reads and writes is our own design, since no
//! specification defines the functions the published mapping library names;
//! each module documents its rows against the R4 element definitions and the
//! data-type pages it follows.

pub mod daily;
pub mod dosage;
pub mod duration;
pub mod ehr_status;
pub mod range;
pub mod ratio;
pub mod timing;

use fhir_types::codec::Json;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::codec::expect_object;
use fhir_types::r4::extension::Extension;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Decimal;
use fhir_types::r4::primitives::String as FhirString;
use fhir_types::r4::primitives::Uri;
use fhir_types::r4::quantity::Quantity;
use fhir_types::r4::range::Range;
use openehr_its::json::from_canonical_value;
use openehr_its::json::to_canonical_value;
use openehr_rm::v1_2::data_types::quantity::dv_interval::DvInterval;
use openehr_rm::v1_2::data_types::quantity::dv_quantity::DvQuantity;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::quantity::QuantityLens;
use crate::engine::lens::quantity::UCUM;
use crate::engine::lens::range::QuantityIntervalLens;
use crate::engine::subject::SubjectIdentifier;
use crate::engine::traverse::functions::Datum;
use crate::engine::traverse::functions::MappingCodeError;
use crate::engine::traverse::functions::MappingFunctions;
use crate::engine::traverse::functions::Ports;
use crate::model::semantic::MappingCodeRegistry;

/// The direction into openEHR of a function that writes the subject of the EHR.
type ToSubject = fn(&Value) -> Result<Vec<SubjectIdentifier>, MappingCodeError>;

/// One registered function: its name, its ports and its two directions.
#[derive(Debug, Clone, Copy)]
struct Function {
    /// The name a `mappingCode` writes.
    code: &'static str,
    /// The openEHR ports, relative to the mapping's `with.openehr`.
    ports: &'static [&'static str],
    /// The direction out of openEHR.
    to_fhir: fn(&Ports) -> Result<Option<Value>, MappingCodeError>,
    /// The direction into openEHR.
    to_openehr: fn(&Value) -> Result<Ports, MappingCodeError>,
    /// The direction into openEHR of a function that writes the subject
    /// reference of the EHR, `None` for one that writes template nodes.
    subject: Option<ToSubject>,
}

/// Every function the library registers, by name.
const FUNCTIONS: &[Function] = &[
    Function {
        code: dosage::ADMINISTRATION_DURATION,
        ports: dosage::ADMINISTRATION_DURATION_PORTS,
        to_fhir: dosage::administration_duration_to_fhir,
        to_openehr: dosage::administration_duration_to_openehr,
        subject: None,
    },
    Function {
        code: timing::NON_DAILY,
        ports: timing::NON_DAILY_PORTS,
        to_fhir: timing::non_daily_to_fhir,
        to_openehr: timing::non_daily_to_openehr,
        subject: None,
    },
    Function {
        code: daily::DAILY,
        ports: daily::PORTS,
        to_fhir: daily::to_fhir,
        to_openehr: daily::to_openehr,
        subject: None,
    },
    Function {
        code: range::DOSE_RANGE,
        ports: range::PORTS,
        to_fhir: range::dose_range_to_fhir,
        to_openehr: range::dose_range_to_openehr,
        subject: None,
    },
    Function {
        code: range::RANGE_TEXT,
        ports: range::PORTS,
        to_fhir: range::range_text_to_fhir,
        to_openehr: range::range_text_to_openehr,
        subject: None,
    },
    Function {
        code: ratio::DOSAGE_RATE,
        ports: ratio::PORTS,
        to_fhir: ratio::dosage_rate_to_fhir,
        to_openehr: ratio::dosage_rate_to_openehr,
        subject: None,
    },
    Function {
        code: ratio::ACTION_RATE,
        ports: ratio::PORTS,
        to_fhir: ratio::action_rate_to_fhir,
        to_openehr: ratio::action_rate_to_openehr,
        subject: None,
    },
    Function {
        code: ehr_status::EXTERNAL_REF,
        ports: &[],
        to_fhir: ehr_status::to_fhir,
        to_openehr: ehr_status::to_openehr,
        subject: Some(ehr_status::to_subject),
    },
];

/// The registry of the functions this crate ships.
///
/// It answers both questions a `mappingCode` raises: at load, whether the
/// name is registered and which ports it declares
/// ([`MappingCodeRegistry`]); at run time, what the function produces
/// ([`MappingFunctions`]). Pass the same value to the loader, the compiler
/// and [`crate::engine::seam::Seams::with_functions`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Library;

impl Library {
    /// Returns the name of every registered function, in name order.
    pub fn codes() -> impl Iterator<Item = &'static str> {
        let mut codes: Vec<&'static str> = FUNCTIONS.iter().map(|function| function.code).collect();
        codes.sort_unstable();
        codes.into_iter()
    }
}

/// Returns the function `code` names.
fn function(code: &str) -> Result<&'static Function, MappingCodeError> {
    FUNCTIONS
        .iter()
        .find(|function| function.code == code)
        .ok_or_else(|| MappingCodeError::Unknown {
            code: String::from(code),
        })
}

impl MappingCodeRegistry for Library {
    fn contains(&self, code: &str) -> bool {
        function(code).is_ok()
    }

    fn ports(&self, code: &str) -> Option<&'static [&'static str]> {
        function(code).ok().map(|function| function.ports)
    }
}

impl MappingFunctions for Library {
    fn to_fhir(&self, code: &str, ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
        (function(code)?.to_fhir)(ports)
    }

    fn to_openehr(&self, code: &str, element: &Value) -> Result<Ports, MappingCodeError> {
        (function(code)?.to_openehr)(element)
    }

    fn writes_subject(&self, code: &str) -> bool {
        function(code).is_ok_and(|function| function.subject.is_some())
    }

    fn to_subject(
        &self,
        code: &str,
        element: &Value,
    ) -> Result<Vec<SubjectIdentifier>, MappingCodeError> {
        let subject = function(code)?
            .subject
            .ok_or_else(|| MappingCodeError::Unknown {
                code: String::from(code),
            })?;
        subject(element)
    }
}

/// Returns the refusal of `code` for `reason`.
fn refused(code: &str, reason: impl Into<String>) -> MappingCodeError {
    MappingCodeError::Refused {
        code: String::from(code),
        reason: reason.into(),
    }
}

/// Decodes a FHIR element as the generated type `T`, named `element`.
fn decode<T: Json>(
    code: &str,
    element: &'static str,
    value: &Value,
) -> Result<T, MappingCodeError> {
    let mut path = Path::root(element);
    let refuse = |source| MappingCodeError::FhirDecode {
        code: String::from(code),
        element,
        source: Box::new(source),
    };
    let object = expect_object(value, &path).map_err(refuse)?;
    T::from_json(object, &mut path).map_err(refuse)
}

/// Encodes a generated FHIR element as the lexical `Value` the tree writes.
fn encode<T: Json>(code: &str, element: &T) -> Result<Value, MappingCodeError> {
    element
        .to_json()
        .map(Value::Object)
        .map_err(|source| MappingCodeError::FhirEncode {
            code: String::from(code),
            source: Box::new(source),
        })
}

/// Returns the text of a FHIR primitive the function reads.
///
/// A primitive may carry an `id` and extensions beside its value
/// (<https://hl7.org/fhir/R4/json.html#primitive>), and no archetype node
/// has a place for them, so a primitive that carries either is refused.
fn primitive<'value>(
    code: &str,
    element: &str,
    id: Option<&String>,
    extension: &[Extension],
    value: Option<&'value String>,
) -> Result<&'value str, MappingCodeError> {
    if id.is_some() || !extension.is_empty() {
        return Err(refused(
            code,
            format!(
                "`{element}` carries an id or an extension, which the archetype has no place for"
            ),
        ));
    }
    value
        .map(String::as_str)
        .ok_or_else(|| refused(code, format!("`{element}` carries no value")))
}

/// Decodes the value at `port` as the `openehr-rm` type `T`, `None` when the
/// port holds nothing.
fn port_value<T: DeserializeOwned>(
    code: &str,
    ports: &Ports,
    port: &str,
) -> Result<Option<T>, MappingCodeError> {
    let Some(datum) = ports.get(port) else {
        return Ok(None);
    };
    from_canonical_value(datum.value())
        .map(Some)
        .map_err(|source| MappingCodeError::RmDecode {
            code: String::from(code),
            port: String::from(port),
            class: String::from(datum.class()),
            source: Box::new(source),
        })
}

/// Returns the datum of a value of `class`, as its canonical JSON.
fn datum<T: Serialize>(class: &str, value: &T) -> Datum {
    Datum::new(class, to_canonical_value(value))
}

/// Reads a `DV_QUANTITY` as a FHIR `Quantity` through the engine's own
/// `DV_QUANTITY` cell, so a function converts a quantity by the rows of
/// `DV_QUANTITY.adoc` and nothing of its own.
fn quantity_of(code: &str, value: &DvQuantity) -> Result<Quantity, MappingCodeError> {
    Lens::<DvQuantity, Quantity>::get(&QuantityLens, value).map_err(|source| cell(code, source))
}

/// Writes a FHIR `Quantity` as a `DV_QUANTITY` through the engine's own
/// `DV_QUANTITY` cell.
fn dv_quantity(code: &str, view: &Quantity) -> Result<DvQuantity, MappingCodeError> {
    Lens::<DvQuantity, Quantity>::put(&QuantityLens, view, None)
        .map_err(|source| cell(code, source))
}

/// Reads a `DV_INTERVAL<DV_QUANTITY>` as a FHIR `Range` through the
/// engine's own `DV_INTERVAL<DV_QUANTITY>` cell (`DV_INTERVAL.adoc`).
fn range_of(code: &str, value: &DvInterval<DvQuantity>) -> Result<Range, MappingCodeError> {
    Lens::<DvInterval<DvQuantity>, Range>::get(&QuantityIntervalLens, value)
        .map_err(|source| cell(code, source))
}

/// Writes a FHIR `Range` as a `DV_INTERVAL<DV_QUANTITY>` through the
/// engine's own `DV_INTERVAL<DV_QUANTITY>` cell.
fn dv_interval(code: &str, view: &Range) -> Result<DvInterval<DvQuantity>, MappingCodeError> {
    Lens::<DvInterval<DvQuantity>, Range>::put(&QuantityIntervalLens, view, None)
        .map_err(|source| cell(code, source))
}

/// Returns the refusal of `code` for a cell's refusal.
fn cell(code: &str, source: LensError) -> MappingCodeError {
    MappingCodeError::Cell {
        code: String::from(code),
        source: Box::new(source),
    }
}

/// Returns the value and the UCUM code of a `Quantity` a function writes as
/// plain text.
///
/// The text keeps the value and the code alone, so a quantity carrying
/// anything else would not read back: a comparator, a system other than
/// UCUM, a `unit` other than its code, an id or an extension, or a value
/// outside the plain decimal grammar is refused.
fn ucum_amount<'quantity>(
    code: &str,
    quantity: &'quantity Quantity,
) -> Result<(&'quantity str, &'quantity str), MappingCodeError> {
    let text = |value: Option<&'quantity String>| value.map(String::as_str);
    let value = text(
        quantity
            .value
            .as_ref()
            .and_then(|value| value.value.as_ref()),
    );
    let units = text(
        quantity
            .code
            .as_ref()
            .and_then(|units| units.value.as_ref()),
    );
    let unit = text(quantity.unit.as_ref().and_then(|unit| unit.value.as_ref()));
    let system = text(
        quantity
            .system
            .as_ref()
            .and_then(|system| system.value.as_ref()),
    );
    let (Some(value), Some(units)) = (value, units) else {
        return Err(refused(code, "a quantity carries a value and a UCUM code"));
    };
    if quantity.comparator.is_some()
        || quantity.id.is_some()
        || !quantity.extension.is_empty()
        || system.is_some_and(|system| system != UCUM)
        || unit.is_some_and(|unit| unit != units)
        || units.contains(' ')
        || duration::plain_decimal(value).is_none()
    {
        return Err(refused(
            code,
            "a quantity here is a plain UCUM value and code alone, with no comparator, no other \
             unit text and no extension",
        ));
    }
    Ok((value, units))
}

/// Returns a UCUM `Quantity` of `value` in `units`, with the code as its
/// `unit` too.
fn ucum_quantity(value: &str, units: &str) -> Quantity {
    Quantity {
        value: Some(Decimal::from(value)),
        unit: Some(FhirString::from(units)),
        system: Some(Uri::from(UCUM)),
        code: Some(Code::from(units)),
        ..Quantity::default()
    }
}

/// Returns the class of the value at `port`, `None` when it holds none.
fn class_at<'ports>(ports: &'ports Ports, port: &str) -> Option<&'ports str> {
    ports.get(port).map(Datum::class)
}
