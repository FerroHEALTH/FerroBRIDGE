// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_COUNT` against FHIR `integer`, `Quantity` and `Count`.
//!
//! The rows are
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_COUNT.adoc`
//! §Primitive, §Quantity and §Count: `value` is `magnitude`, and nothing else
//! of either side has a counterpart. A `Count` writes `code` `1`, as the page
//! says, with the UCUM system R4 `cnt-3` names beside it
//! (<https://hl7.org/fhir/R4/datatypes.html#Count>).
//!
//! `magnitude` is an `Integer64` and a FHIR `integer` a 32-bit number, so a
//! count outside that range is refused going out, and a `decimal` that is not
//! a whole number is refused coming in. None of the three tables carries
//! `magnitude_status`, so a count whose status is not a point value is
//! refused rather than written as one, and a `comparator` coming in is
//! refused rather than dropped.

use fhir_types::r4::count::Count;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Decimal;
use fhir_types::r4::primitives::Integer;
use fhir_types::r4::primitives::Uri;
use fhir_types::r4::quantity::Quantity;
use openehr_rm::v1_2::data_types::quantity::dv_count::DvCount;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::engine::lens::number::decimal_text;
use crate::engine::lens::number::narrow;
use crate::engine::lens::number::point;
use crate::engine::lens::number::whole;
use crate::engine::lens::quantity::UCUM;
use crate::model::ast::keyword::Direction;

/// The name the `integer` cell carries in a diagnostic.
pub const INTEGER_CELL: &str = "DV_COUNT against integer";

/// The name the `Quantity` cell carries in a diagnostic.
pub const QUANTITY_CELL: &str = "DV_COUNT against Quantity";

/// The name the `Count` cell carries in a diagnostic.
pub const COUNT_CELL: &str = "DV_COUNT against Count";

/// The UCUM unit of a count, "for Count code = 1" (`DV_COUNT.adoc` §Count).
pub const UNITY: &str = "1";

/// `DV_COUNT` against `integer`, `Quantity` and `Count`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CountLens;

impl Lens<DvCount, Integer> for CountLens {
    fn cell(&self) -> &'static str {
        INTEGER_CELL
    }

    fn get(&self, source: &DvCount) -> Result<Integer, LensError> {
        point(source.magnitude_status.as_deref(), INTEGER_CELL)?;
        narrow(source.magnitude, INTEGER_CELL, "DV_COUNT.magnitude").map(Integer::from)
    }

    fn put(&self, view: &Integer, existing: Option<&DvCount>) -> Result<DvCount, LensError> {
        let value = view.value.ok_or(LensError::Missing {
            cell: INTEGER_CELL,
            attribute: "DV_COUNT.magnitude",
            direction: Direction::FhirToOpenehr,
        })?;
        Ok(counted(i64::from(value), existing))
    }

    fn fallbacks(&self, _view: &Integer) -> Vec<Fallback> {
        Vec::new()
    }
}

impl Lens<DvCount, Quantity> for CountLens {
    fn cell(&self) -> &'static str {
        QUANTITY_CELL
    }

    fn get(&self, source: &DvCount) -> Result<Quantity, LensError> {
        point(source.magnitude_status.as_deref(), QUANTITY_CELL)?;
        Ok(Quantity {
            value: Some(Decimal::from(source.magnitude.to_string().as_str())),
            ..Quantity::default()
        })
    }

    fn put(&self, view: &Quantity, existing: Option<&DvCount>) -> Result<DvCount, LensError> {
        unbounded(view.comparator.as_ref(), QUANTITY_CELL)?;
        let text = decimal_text(view.value.as_ref(), QUANTITY_CELL, "DV_COUNT.magnitude")?;
        Ok(counted(
            whole(text, QUANTITY_CELL, "Quantity.value")?,
            existing,
        ))
    }

    fn fallbacks(&self, _view: &Quantity) -> Vec<Fallback> {
        Vec::new()
    }
}

impl Lens<DvCount, Count> for CountLens {
    fn cell(&self) -> &'static str {
        COUNT_CELL
    }

    fn get(&self, source: &DvCount) -> Result<Count, LensError> {
        point(source.magnitude_status.as_deref(), COUNT_CELL)?;
        Ok(Count {
            value: Some(Decimal::from(source.magnitude.to_string().as_str())),
            system: Some(Uri::from(UCUM)),
            code: Some(Code::from(UNITY)),
            ..Count::default()
        })
    }

    fn put(&self, view: &Count, existing: Option<&DvCount>) -> Result<DvCount, LensError> {
        unbounded(view.comparator.as_ref(), COUNT_CELL)?;
        let code = text_of(view.code.as_ref().map(|code| &code.value));
        if let Some(other) = code.filter(|&code| code != UNITY) {
            return Err(LensError::Shape {
                cell: COUNT_CELL,
                attribute: "Count.code",
                found: String::from(other),
                reason: "R4 cnt-3 fixes the code of a Count at 1",
                direction: Direction::FhirToOpenehr,
            });
        }
        let text = decimal_text(view.value.as_ref(), COUNT_CELL, "DV_COUNT.magnitude")?;
        Ok(counted(whole(text, COUNT_CELL, "Count.value")?, existing))
    }

    fn fallbacks(&self, _view: &Count) -> Vec<Fallback> {
        Vec::new()
    }
}

/// Refuses a comparator, which none of the tables gives an openEHR row, so
/// writing the count without it would state a bounded value as exact.
fn unbounded(comparator: Option<&Code>, cell: &'static str) -> Result<(), LensError> {
    match text_of(comparator.map(|code| &code.value)) {
        Some(found) => Err(LensError::Shape {
            cell,
            attribute: "Quantity.comparator",
            found: String::from(found),
            reason: "DV_COUNT.adoc gives comparator no openEHR row",
            direction: Direction::FhirToOpenehr,
        }),
        None => Ok(()),
    }
}

/// Returns the count of `magnitude`, with the attributes no FHIR element
/// carries taken from the held value.
fn counted(magnitude: i64, existing: Option<&DvCount>) -> DvCount {
    DvCount {
        normal_status: existing.and_then(|held| held.normal_status.clone()),
        normal_range: existing.and_then(|held| held.normal_range.clone()),
        other_reference_ranges: existing.and_then(|held| held.other_reference_ranges.clone()),
        magnitude_status: existing.and_then(|held| held.magnitude_status.clone()),
        accuracy: existing.and_then(|held| held.accuracy),
        accuracy_is_percent: existing.and_then(|held| held.accuracy_is_percent),
        magnitude,
    }
}

#[cfg(test)]
mod tests {
    use super::CountLens;
    use crate::engine::lens::Lens;
    use crate::engine::lens::LensError;
    use crate::engine::lens::quantity::UCUM;
    use fhir_types::r4::count::Count;
    use fhir_types::r4::primitives::Code;
    use fhir_types::r4::primitives::Decimal;
    use fhir_types::r4::primitives::Integer;
    use fhir_types::r4::quantity::Quantity;
    use openehr_rm::v1_2::data_types::quantity::dv_count::DvCount;
    use proptest::option;
    use proptest::prop_compose;
    use proptest::prop_oneof;
    use proptest::proptest;
    use proptest::strategy::Just;

    /// Returns a synthetic count of `magnitude`.
    fn count(magnitude: i64) -> DvCount {
        DvCount {
            normal_status: None,
            normal_range: None,
            other_reference_ranges: None,
            magnitude_status: None,
            accuracy: None,
            accuracy_is_percent: None,
            magnitude,
        }
    }

    prop_compose! {
        /// A count inside the FHIR integer range, as a point value.
        fn counted()(
            magnitude in i64::from(i32::MIN)..=i64::from(i32::MAX),
            magnitude_status in prop_oneof![Just(None), Just(Some(String::from("=")))],
            accuracy in option::of(0i32..10),
        ) -> DvCount {
            DvCount { magnitude_status, accuracy: accuracy.map(f64::from), ..count(magnitude) }
        }
    }

    proptest! {
        #[test]
        fn integer_get_put(source in counted()) {
            let view: Integer = CountLens.get(&source).expect("the count travels");
            let back = CountLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn integer_put_get(value in proptest::num::i32::ANY) {
            let view = Integer::from(value);
            let back: DvCount = CountLens.put(&view, None).expect("the view is complete");
            let again: Integer = CountLens.get(&back).expect("the count travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn quantity_get_put(source in counted()) {
            let view: Quantity = CountLens.get(&source).expect("the count travels");
            let back = CountLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn quantity_put_get(source in counted()) {
            let view: Quantity = CountLens.get(&source).expect("the count travels");
            let back: DvCount = CountLens.put(&view, None).expect("the view is complete");
            let again: Quantity = CountLens.get(&back).expect("the count travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn count_get_put(source in counted()) {
            let view: Count = CountLens.get(&source).expect("the count travels");
            let back = CountLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn count_put_get(source in counted()) {
            let view: Count = CountLens.get(&source).expect("the count travels");
            let back: DvCount = CountLens.put(&view, None).expect("the view is complete");
            let again: Count = CountLens.get(&back).expect("the count travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn a_count_writes_the_code_one_in_ucum() {
        // DV_COUNT.adoc §Count: "for Count code = 1"; R4 cnt-3 names UCUM.
        let view: Count = CountLens.get(&count(4)).expect("the count travels");
        assert_eq!(
            (
                view.value.and_then(|value| value.value),
                view.code.and_then(|code| code.value),
                view.system.and_then(|system| system.value),
            ),
            (
                Some(String::from("4")),
                Some(String::from("1")),
                Some(String::from(UCUM)),
            )
        );
    }

    #[test]
    fn a_count_outside_the_fhir_integer_range_is_refused() {
        let error = Lens::<DvCount, Integer>::get(&CountLens, &count(i64::from(i32::MAX) + 1))
            .expect_err("a FHIR integer is 32-bit");
        assert!(
            matches!(error, LensError::Shape { attribute, .. } if attribute == "DV_COUNT.magnitude"),
            "the refusal names the magnitude: {error}"
        );
    }

    #[test]
    fn a_fractional_quantity_is_no_count() {
        let view = Quantity {
            value: Some(Decimal::from("2.5")),
            ..Quantity::default()
        };
        let error = CountLens.put(&view, None).expect_err("a count is whole");
        assert!(
            matches!(error, LensError::Shape { attribute, .. } if attribute == "Quantity.value"),
            "the refusal names the value: {error}"
        );
    }

    #[test]
    fn a_count_coded_otherwise_is_refused() {
        let view = Count {
            value: Some(Decimal::from("2")),
            code: Some(Code::from("mg")),
            ..Count::default()
        };
        let error = CountLens
            .put(&view, None)
            .expect_err("cnt-3 fixes the code");
        assert!(
            matches!(error, LensError::Shape { attribute, .. } if attribute == "Count.code"),
            "the refusal names the code: {error}"
        );
    }

    #[test]
    fn a_comparator_going_in_is_refused() {
        let view = Quantity {
            value: Some(Decimal::from("2")),
            comparator: Some(Code::from("<")),
            ..Quantity::default()
        };
        assert!(CountLens.put(&view, None).is_err());
        let count = Count {
            value: Some(Decimal::from("2")),
            comparator: Some(Code::from(">=")),
            ..Count::default()
        };
        assert!(CountLens.put(&count, None).is_err());
    }

    #[test]
    fn a_bounded_count_has_no_comparator_to_carry_it() {
        let source = DvCount {
            magnitude_status: Some(String::from("<")),
            ..count(4)
        };
        assert!(Lens::<DvCount, Integer>::get(&CountLens, &source).is_err());
        assert!(Lens::<DvCount, Quantity>::get(&CountLens, &source).is_err());
        assert!(Lens::<DvCount, Count>::get(&CountLens, &source).is_err());
    }
}
