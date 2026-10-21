// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A scalar `Integer` or `Real` attribute against FHIR `integer` or
//! `decimal`.
//!
//! The data-type chapter leaves primitive pairs out "due to being straight
//! forward" (`types-of-mappings/data-type/data-mappings.adoc`), so these cells
//! carry the number and refuse what the target cannot state. FHIR `integer` is
//! a signed 32-bit number (<https://hl7.org/fhir/R4/datatypes.html#integer>)
//! while the reference model's `Integer` and `Integer64` are wider, so a value
//! outside that range is refused going out. FHIR keeps a `decimal` in its
//! lexical form, precision included
//! (<https://hl7.org/fhir/R4/datatypes.html#decimal>), while a reference-model
//! `Real` is a binary number with no precision of its own, so the openEHR side
//! is the canonical JSON number and a decimal whose text that number does not
//! render (`1.50`, `1e3`) is refused going in rather than rounded.

use core::str::FromStr;

use fhir_types::r4::primitives::Decimal;
use fhir_types::r4::primitives::Integer;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::number::decimal_text;
use crate::engine::lens::number::narrow;
use crate::model::ast::keyword::Direction;

/// The name the `integer` cell carries in a diagnostic.
pub const INTEGER_CELL: &str = "Integer against integer";

/// The name the `decimal` cell carries in a diagnostic.
pub const REAL_CELL: &str = "Real against decimal";

/// A reference-model `Integer` or `Integer64` attribute against `integer`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IntegerLens;

impl Lens<i64, Integer> for IntegerLens {
    fn cell(&self) -> &'static str {
        INTEGER_CELL
    }

    fn get(&self, source: &i64) -> Result<Integer, LensError> {
        narrow(*source, INTEGER_CELL, "Integer").map(Integer::from)
    }

    fn put(&self, view: &Integer, _existing: Option<&i64>) -> Result<i64, LensError> {
        view.value.map(i64::from).ok_or(LensError::Missing {
            cell: INTEGER_CELL,
            attribute: "Integer",
            direction: Direction::FhirToOpenehr,
        })
    }

    fn fallbacks(&self, _view: &Integer) -> Vec<Fallback> {
        Vec::new()
    }
}

/// A reference-model `Real` attribute, as its canonical JSON number, against
/// `decimal`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RealLens;

impl Lens<serde_json::Number, Decimal> for RealLens {
    fn cell(&self) -> &'static str {
        REAL_CELL
    }

    fn get(&self, source: &serde_json::Number) -> Result<Decimal, LensError> {
        Ok(Decimal::from(source.to_string().as_str()))
    }

    fn put(
        &self,
        view: &Decimal,
        _existing: Option<&serde_json::Number>,
    ) -> Result<serde_json::Number, LensError> {
        let text = decimal_text(Some(view), REAL_CELL, "Real")?;
        let refuse = |reason: &'static str| LensError::Shape {
            cell: REAL_CELL,
            attribute: "Real",
            found: String::from(text),
            reason,
            direction: Direction::FhirToOpenehr,
        };
        let number = serde_json::Number::from_str(text)
            .map_err(|_refused| refuse("the reference model takes a real number"))?;
        // NOTE: RM 1.1.0 foundation types give `Real` no precision, so the
        // decimal's places survive only where the number renders them.
        if number.to_string() != text {
            return Err(refuse(
                "a reference-model Real keeps no precision, and this number reads back as a \
                 different decimal",
            ));
        }
        Ok(number)
    }

    fn fallbacks(&self, _view: &Decimal) -> Vec<Fallback> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::IntegerLens;
    use super::RealLens;
    use crate::engine::lens::Lens;
    use fhir_types::r4::primitives::Decimal;
    use fhir_types::r4::primitives::Integer;
    use proptest::proptest;

    proptest! {
        #[test]
        fn integer_get_put(source in i64::from(i32::MIN)..=i64::from(i32::MAX)) {
            let view = IntegerLens.get(&source).expect("a 32-bit value travels");
            let back = IntegerLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source}");
        }

        #[test]
        fn integer_put_get(value: i32) {
            let view = Integer::from(value);
            let source = IntegerLens.put(&view, None).expect("the view is complete");
            let back = IntegerLens.get(&source).expect("a 32-bit value travels");
            assert_eq!(back, view, "PutGet failed for {value}");
        }

        #[test]
        fn real_get_put(value in -1.0e9f64..1.0e9f64) {
            let source = serde_json::Number::from_f64(value).expect("a finite real");
            let view = RealLens.get(&source).expect("a real travels");
            let back = RealLens.put(&view, Some(&source)).expect("the canonical text reads back");
            assert_eq!(back, source, "GetPut failed for {source}");
        }

        #[test]
        fn real_put_get(whole in -100_000i32..100_000, places in 1u32..999) {
            let text = format!("{whole}.{places}");
            let number = serde_json::Number::from_f64(text.parse::<f64>().expect("a decimal"))
                .expect("a finite real");
            let view = Decimal::from(number.to_string().as_str());
            let source = RealLens.put(&view, None).expect("the canonical text reads back");
            let back = RealLens.get(&source).expect("a real travels");
            assert_eq!(back, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn an_integer_outside_32_bits_is_refused() {
        // R4 datatypes §integer: a signed 32-bit number.
        let error = IntegerLens
            .get(&(i64::from(i32::MAX) + 1))
            .expect_err("2^31 is no FHIR integer");
        assert!(error.to_string().contains("32-bit"), "{error}");
    }

    #[test]
    fn an_integer_with_no_value_is_refused() {
        let error = IntegerLens
            .put(&Integer::default(), None)
            .expect_err("an integer with only an extension carries no number");
        assert!(error.to_string().contains("Integer"), "{error}");
    }

    #[test]
    fn a_decimal_whose_places_a_real_cannot_keep_is_refused() {
        // R4 datatypes §decimal: the precision of the decimal is significant.
        for text in ["1.50", "1e3", "007"] {
            let error = RealLens
                .put(&Decimal::from(text), None)
                .expect_err("the Real would read back as another decimal");
            assert!(error.to_string().contains(text), "{error}");
        }
    }

    #[test]
    fn a_decimal_with_no_value_is_refused() {
        assert!(RealLens.put(&Decimal::default(), None).is_err());
    }

    #[test]
    fn a_whole_decimal_is_a_real() {
        let source = RealLens
            .put(&Decimal::from("42"), None)
            .expect("42 renders as itself");
        assert_eq!(source.to_string(), "42");
    }
}
