// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_INTERVAL<DV_QUANTITY>` against FHIR `Range`.
//!
//! The cell is the `Range` table of
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_INTERVAL.adoc`:
//! `low` is `lower` and `high` is `upper`, each bound a quantity converted by
//! the `DV_QUANTITY` cell, and the table fixes both bounds included and
//! neither unbounded.
//!
//! The table writes the unbounded flags false "since not infinite", which
//! leaves an absent bound undefined. The reference model states an unbounded
//! side as the absence of its bound, so an absent `low` or `high` is the
//! unbounded side and is not included, as the `Period` cell reads it. An
//! excluded bound has no `Range` that states it, and a bound with a comparator
//! has no `SimpleQuantity` (R4 `sqty-1`), so both are refused going out.

use fhir_types::r4::quantity::Quantity;
use fhir_types::r4::range::Range;
use openehr_rm::v1_2::data_types::quantity::dv_interval::DvInterval;
use openehr_rm::v1_2::data_types::quantity::dv_quantity::DvQuantity;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::number::point;
use crate::engine::lens::quantity::QuantityLens;
use crate::model::ast::keyword::Direction;

/// The name this cell carries in a diagnostic.
pub const CELL: &str = "DV_INTERVAL<DV_QUANTITY> against Range";

/// `DV_INTERVAL<DV_QUANTITY>` against `Range`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuantityIntervalLens;

impl Lens<DvInterval<DvQuantity>, Range> for QuantityIntervalLens {
    fn cell(&self) -> &'static str {
        CELL
    }

    fn get(&self, source: &DvInterval<DvQuantity>) -> Result<Range, LensError> {
        Ok(Range {
            low: limit(
                source.lower.as_ref(),
                source.lower_included,
                "DV_INTERVAL.lower_included",
            )?,
            high: limit(
                source.upper.as_ref(),
                source.upper_included,
                "DV_INTERVAL.upper_included",
            )?,
            ..Range::default()
        })
    }

    fn put(
        &self,
        view: &Range,
        existing: Option<&DvInterval<DvQuantity>>,
    ) -> Result<DvInterval<DvQuantity>, LensError> {
        let lower = view
            .low
            .as_ref()
            .map(|low| QuantityLens.put(low, existing.and_then(|held| held.lower.as_ref())))
            .transpose()?;
        let upper = view
            .high
            .as_ref()
            .map(|high| QuantityLens.put(high, existing.and_then(|held| held.upper.as_ref())))
            .transpose()?;
        Ok(DvInterval {
            lower_unbounded: lower.is_none(),
            upper_unbounded: upper.is_none(),
            lower_included: lower.is_some(),
            upper_included: upper.is_some(),
            lower,
            upper,
        })
    }

    fn fallbacks(&self, view: &Range) -> Vec<Fallback> {
        [view.low.as_ref(), view.high.as_ref()]
            .into_iter()
            .flatten()
            .flat_map(|bound| QuantityLens.fallbacks(bound))
            .collect()
    }
}

/// Returns the `SimpleQuantity` one bound reads as, refusing an excluded
/// bound and one with a comparator.
fn limit(
    bound: Option<&DvQuantity>,
    included: bool,
    attribute: &'static str,
) -> Result<Option<Quantity>, LensError> {
    let Some(bound) = bound else {
        return Ok(None);
    };
    if !included {
        return Err(LensError::Shape {
            cell: CELL,
            attribute,
            found: String::from("false"),
            reason: "the table fixes a Range bound included",
            direction: Direction::OpenehrToFhir,
        });
    }
    point(bound.magnitude_status.as_deref(), CELL)?;
    QuantityLens.get(bound).map(Some)
}

#[cfg(test)]
mod tests {
    use super::QuantityIntervalLens;
    use crate::engine::lens::Lens;
    use fhir_types::r4::range::Range;
    use openehr_rm::v1_2::data_types::quantity::dv_interval::DvInterval;
    use openehr_rm::v1_2::data_types::quantity::dv_quantity::DvQuantity;
    use proptest::option;
    use proptest::proptest;

    /// Returns a synthetic quantity of `magnitude` milligrams.
    fn milligrams(magnitude: i32) -> DvQuantity {
        DvQuantity {
            normal_status: None,
            normal_range: None,
            other_reference_ranges: None,
            magnitude_status: None,
            accuracy: None,
            accuracy_is_percent: None,
            magnitude: f64::from(magnitude),
            precision: Some(0),
            units: String::from("mg"),
            units_system: None,
            units_display_name: None,
        }
    }

    /// Returns the interval over the bounds given, each included.
    fn interval(lower: Option<i32>, upper: Option<i32>) -> DvInterval<DvQuantity> {
        DvInterval {
            lower_unbounded: lower.is_none(),
            upper_unbounded: upper.is_none(),
            lower_included: lower.is_some(),
            upper_included: upper.is_some(),
            lower: lower.map(milligrams),
            upper: upper.map(milligrams),
        }
    }

    proptest! {
        #[test]
        fn get_put(lower in option::of(0i32..100), upper in option::of(100i32..200)) {
            let source = interval(lower, upper);
            let view: Range = QuantityIntervalLens.get(&source).expect("the interval travels");
            let back = QuantityIntervalLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn put_get(lower in option::of(0i32..100), upper in option::of(100i32..200)) {
            let view: Range = QuantityIntervalLens.get(&interval(lower, upper)).expect("the interval travels");
            let back = QuantityIntervalLens.put(&view, None).expect("the view is complete");
            let again: Range = QuantityIntervalLens.get(&back).expect("the interval travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn an_excluded_bound_has_no_range() {
        // DV_INTERVAL.adoc §Range: lower_included and upper_included are true.
        let source = DvInterval {
            lower_included: false,
            ..interval(Some(1), Some(2))
        };
        assert!(QuantityIntervalLens.get(&source).is_err());
    }

    #[test]
    fn a_bound_with_a_comparator_has_no_simple_quantity() {
        let mut source = interval(Some(1), None);
        if let Some(ref mut lower) = source.lower {
            lower.magnitude_status = Some(String::from(">"));
        }
        assert!(QuantityIntervalLens.get(&source).is_err());
    }
}
