// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The numbers the quantity cells read and write.
//!
//! FHIR keeps a `decimal` in its lexical form
//! (<https://hl7.org/fhir/R4/datatypes.html#decimal>), and the reference model
//! declares the magnitudes of `DV_QUANTITY` and `DV_PROPORTION` a `Real` with
//! a separate `precision`, "the number of decimal places"
//! (<https://specifications.openehr.org/releases/RM/Release-1.1.0/data_types.html#_dv_quantity_class>).
//! These helpers move between the two, and between FHIR's 32-bit `integer`
//! and the reference model's `Integer64`, refusing what does not fit.

use fhir_types::r4::primitives::Decimal;

use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::model::ast::keyword::Direction;

/// Renders a real to the number of decimal places a precision declares.
///
/// -1 means no limit, so a negative or absent precision renders the shortest
/// form that reads back.
#[must_use]
pub(crate) fn render(value: f64, precision: Option<i32>) -> String {
    match precision.and_then(|places| usize::try_from(places).ok()) {
        Some(places) => format!("{value:.places$}"),
        None => format!("{value}"),
    }
}

/// Renders a magnitude to its declared precision, refusing one that the
/// precision would round.
///
/// A magnitude with more places than its `precision` declares would reach
/// FHIR as a different number, so it is refused rather than rounded.
///
/// # Errors
///
/// Returns [`LensError::Shape`] naming `attribute` when the rendered text
/// does not read back as the magnitude.
pub(crate) fn exact(
    value: f64,
    precision: Option<i32>,
    cell: &'static str,
    attribute: &'static str,
) -> Result<String, LensError> {
    let text = render(value, precision);
    match text.parse::<f64>() {
        Ok(read) if read.to_bits() == value.to_bits() => Ok(text),
        _ => Err(LensError::Shape {
            cell,
            attribute,
            found: render(value, None),
            reason: "the magnitude has more decimal places than its precision declares",
            direction: Direction::OpenehrToFhir,
        }),
    }
}

/// Returns the decimal places a lexical decimal was written with, `None` for
/// one written with an exponent, whose places the text does not state.
#[must_use]
pub(crate) fn places(text: &str) -> Option<i32> {
    if text.contains(['e', 'E']) {
        return None;
    }
    let Some((_whole, fraction)) = text.split_once('.') else {
        return Some(0);
    };
    i32::try_from(fraction.chars().count()).ok()
}

/// Returns the precision a magnitude read from `text` keeps: the held
/// precision when one is held and renders the same text, the places of the
/// text otherwise.
#[must_use]
pub(crate) fn precision_of(
    text: &str,
    magnitude: f64,
    held: bool,
    held_precision: Option<i32>,
) -> Option<i32> {
    if held && render(magnitude, held_precision) == text {
        return held_precision;
    }
    places(text)
}

/// Returns the lexical text of a FHIR `decimal`, refusing an absent one.
///
/// # Errors
///
/// Returns [`LensError::Missing`] naming `attribute` when the decimal carries
/// no value.
pub(crate) fn decimal_text<'view>(
    value: Option<&'view Decimal>,
    cell: &'static str,
    attribute: &'static str,
) -> Result<&'view str, LensError> {
    value
        .and_then(|decimal| text_of(Some(&decimal.value)))
        .ok_or(LensError::Missing {
            cell,
            attribute,
            direction: Direction::FhirToOpenehr,
        })
}

/// Reads a FHIR `decimal` as a reference-model `Real`.
///
/// # Errors
///
/// Returns [`LensError::Shape`] when the text is no finite number.
pub(crate) fn real(
    text: &str,
    cell: &'static str,
    attribute: &'static str,
) -> Result<f64, LensError> {
    // NOTE: the reference model declares the magnitude a `Real`, so the openEHR
    // side is binary floating point by definition and the lexical form survives
    // only through `precision`.
    text.parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
        .ok_or_else(|| LensError::Shape {
            cell,
            attribute,
            found: String::from(text),
            reason: "the reference model takes a real number",
            direction: Direction::FhirToOpenehr,
        })
}

/// Reads a FHIR `decimal` as a reference-model `Integer64`.
///
/// # Errors
///
/// Returns [`LensError::Shape`] when the text is no whole number in range.
pub(crate) fn whole(
    text: &str,
    cell: &'static str,
    attribute: &'static str,
) -> Result<i64, LensError> {
    text.parse::<i64>().map_err(|_refused| LensError::Shape {
        cell,
        attribute,
        found: String::from(text),
        reason: "the reference model takes a whole number",
        direction: Direction::FhirToOpenehr,
    })
}

/// Narrows a reference-model `Integer64` to a FHIR `integer`.
///
/// # Errors
///
/// Returns [`LensError::Shape`] when the value is outside the 32-bit range
/// FHIR fixes (<https://hl7.org/fhir/R4/datatypes.html#integer>).
pub(crate) fn narrow(
    value: i64,
    cell: &'static str,
    attribute: &'static str,
) -> Result<i32, LensError> {
    i32::try_from(value).map_err(|_refused| LensError::Shape {
        cell,
        attribute,
        found: value.to_string(),
        reason: "a FHIR integer is a signed 32-bit number",
        direction: Direction::OpenehrToFhir,
    })
}

/// Refuses a `magnitude_status` a FHIR element with no comparator would
/// falsify.
///
/// No status and `=` both mean a point value (openEHR RM 1.1.0
/// `DV_QUANTIFIED.magnitude_status`), which a bare number states; any other
/// status would be lost.
///
/// # Errors
///
/// Returns [`LensError::Shape`] for a status other than `=`.
pub(crate) fn point(status: Option<&str>, cell: &'static str) -> Result<(), LensError> {
    match status {
        None | Some(POINT) => Ok(()),
        Some(other) => Err(LensError::Shape {
            cell,
            attribute: "DV_QUANTIFIED.magnitude_status",
            found: String::from(other),
            reason: "the FHIR element states a point value and carries no comparator",
            direction: Direction::OpenehrToFhir,
        }),
    }
}

/// The `magnitude_status` that states a point value.
pub(crate) const POINT: &str = "=";

#[cfg(test)]
mod tests {
    use super::narrow;
    use super::places;
    use super::point;
    use super::precision_of;
    use super::render;

    #[test]
    fn a_precision_fixes_the_places_rendered() {
        assert_eq!(render(1.5, Some(2)), "1.50");
        assert_eq!(render(1.5, None), "1.5");
        assert_eq!(render(1.5, Some(-1)), "1.5");
    }

    #[test]
    fn the_places_are_read_from_the_text() {
        assert_eq!(places("1.50"), Some(2));
        assert_eq!(places("7"), Some(0));
        assert_eq!(places("1.5e3"), None);
    }

    #[test]
    fn a_held_precision_that_renders_the_text_is_kept() {
        assert_eq!(precision_of("1.5", 1.5, true, None), None);
        assert_eq!(precision_of("1.50", 1.5, true, None), Some(2));
        assert_eq!(precision_of("1.5", 1.5, false, None), Some(1));
    }

    #[test]
    fn an_integer64_outside_the_fhir_range_is_refused() {
        assert_eq!(narrow(7, "cell", "attribute"), Ok(7));
        assert!(narrow(i64::from(i32::MAX) + 1, "cell", "attribute").is_err());
    }

    #[test]
    fn only_a_point_status_has_no_comparator() {
        assert!(point(None, "cell").is_ok());
        assert!(point(Some("="), "cell").is_ok());
        assert!(point(Some("~"), "cell").is_err());
    }
}
