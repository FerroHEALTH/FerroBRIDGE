// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_QUANTITY` against FHIR `Quantity`, `decimal` and `integer`.
//!
//! The rows are
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_QUANTITY.adoc`
//! §Quantity and §Primitives. `value` is `magnitude`, written to the places
//! `precision` declares; `code` (or `unit` when no code exists) is `units`;
//! `system` is `units_system`; `comparator` is `magnitude_status`.
//!
//! Three of the table's rows need a decision of our own, each recorded on the
//! data-type defects upstream report:
//!
//! - The table maps `system` to both `units_system` and `units_display_name`.
//!   A display name is no system, so `units_display_name` is read as the
//!   counterpart of `unit`, the human form R4 gives that element
//!   (<https://hl7.org/fhir/R4/datatypes.html#Quantity>): `unit` is the display
//!   name when the value carries one and `units` otherwise, which is what the
//!   table's "both code and unit are populated" writes.
//! - R4 `qty-3` requires a `system` beside a `code`, and the reference model
//!   assumes UCUM when `units_system` is not set, so a value with no
//!   `units_system` writes the UCUM system.
//! - `magnitude_status` admits `~`, which no `comparator` states, so a value
//!   with it is refused rather than written as a point value.

use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Decimal;
use fhir_types::r4::primitives::Integer;
use fhir_types::r4::primitives::String as FhirString;
use fhir_types::r4::primitives::Uri;
use fhir_types::r4::quantity::Quantity;
use openehr_rm::v1_2::data_types::quantity::dv_quantity::DvQuantity;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::engine::lens::number::POINT;
use crate::engine::lens::number::decimal_text;
use crate::engine::lens::number::exact;
use crate::engine::lens::number::point;
use crate::engine::lens::number::precision_of;
use crate::engine::lens::number::real;
use crate::engine::lens::number::render;
use crate::model::ast::keyword::Direction;

/// The name the `Quantity` cell carries in a diagnostic.
pub const CELL: &str = "DV_QUANTITY against Quantity";

/// The name the `decimal` cell carries in a diagnostic.
pub const DECIMAL_CELL: &str = "DV_QUANTITY against decimal";

/// The name the `integer` cell carries in a diagnostic.
pub const INTEGER_CELL: &str = "DV_QUANTITY against integer";

/// The UCUM system URI (<https://hl7.org/fhir/R4/datatypes.html#Quantity>).
pub const UCUM: &str = "http://unitsofmeasure.org";

/// The `units` a primitive writes, "hardcoded as 1, due to being required"
/// (`DV_QUANTITY.adoc` §Primitives).
pub const UNITY: &str = "1";

/// The comparators `Quantity.comparator` admits (R4 `quantity-comparator`).
const COMPARATORS: [&str; 4] = ["<", "<=", ">=", ">"];

/// `DV_QUANTITY` against `Quantity`, `decimal` and `integer`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuantityLens;

impl Lens<DvQuantity, Quantity> for QuantityLens {
    fn cell(&self) -> &'static str {
        CELL
    }

    fn get(&self, source: &DvQuantity) -> Result<Quantity, LensError> {
        let comparator = match source.magnitude_status.as_deref() {
            None | Some(POINT) => None,
            Some(status) if COMPARATORS.contains(&status) => Some(Code::from(status)),
            Some(other) => {
                return Err(LensError::Shape {
                    cell: CELL,
                    attribute: "DV_QUANTITY.magnitude_status",
                    found: String::from(other),
                    reason: "Quantity.comparator states only <, <=, >= and >",
                    direction: Direction::OpenehrToFhir,
                });
            }
        };
        let unit = source
            .units_display_name
            .as_deref()
            .unwrap_or(&source.units);
        let value = exact(
            source.magnitude,
            source.precision,
            CELL,
            "DV_QUANTITY.precision",
        )?;
        Ok(Quantity {
            value: Some(Decimal::from(value.as_str())),
            comparator,
            unit: Some(FhirString::from(unit)),
            system: Some(Uri::from(source.units_system.as_deref().unwrap_or(UCUM))),
            code: Some(Code::from(source.units.as_str())),
            ..Quantity::default()
        })
    }

    fn put(&self, view: &Quantity, existing: Option<&DvQuantity>) -> Result<DvQuantity, LensError> {
        let code = text_of(view.code.as_ref().map(|code| &code.value));
        let unit = text_of(view.unit.as_ref().map(|unit| &unit.value));
        let units = code.or(unit).ok_or(LensError::Missing {
            cell: CELL,
            attribute: "DV_QUANTITY.units",
            direction: Direction::FhirToOpenehr,
        })?;
        let held_display = existing.and_then(|held| held.units_display_name.as_deref());
        let units_display_name = match unit {
            Some(shown) if shown != units => Some(String::from(shown)),
            Some(shown) => held_display.filter(|&held| held == shown).map(String::from),
            None => None,
        };
        let system = text_of(view.system.as_ref().map(|system| &system.value));
        let units_system = match system {
            Some(UCUM) => existing
                .and_then(|held| held.units_system.as_deref())
                .filter(|&held| held == UCUM)
                .map(String::from),
            Some(other) => Some(String::from(other)),
            None => None,
        };
        let magnitude_status = match text_of(view.comparator.as_ref().map(|code| &code.value)) {
            Some(comparator) => Some(String::from(comparator)),
            None => existing
                .and_then(|held| held.magnitude_status.as_deref())
                .filter(|&held| held == POINT)
                .map(String::from),
        };
        let text = decimal_text(view.value.as_ref(), CELL, "DV_QUANTITY.magnitude")?;
        let magnitude = real(text, CELL, "Quantity.value")?;
        Ok(DvQuantity {
            magnitude,
            precision: precision_of(
                text,
                magnitude,
                existing.is_some(),
                existing.and_then(|held| held.precision),
            ),
            units: String::from(units),
            units_system,
            units_display_name,
            magnitude_status,
            ..carried(existing)
        })
    }

    fn fallbacks(&self, view: &Quantity) -> Vec<Fallback> {
        let code = text_of(view.code.as_ref().map(|code| &code.value));
        let unit = text_of(view.unit.as_ref().map(|unit| &unit.value));
        match (code, unit) {
            (None, Some(_)) => vec![Fallback::new(CELL, "DV_QUANTITY.units")],
            _ => Vec::new(),
        }
    }
}

impl Lens<DvQuantity, Decimal> for QuantityLens {
    fn cell(&self) -> &'static str {
        DECIMAL_CELL
    }

    fn get(&self, source: &DvQuantity) -> Result<Decimal, LensError> {
        point(source.magnitude_status.as_deref(), DECIMAL_CELL)?;
        let value = exact(
            source.magnitude,
            source.precision,
            DECIMAL_CELL,
            "DV_QUANTITY.precision",
        )?;
        Ok(Decimal::from(value.as_str()))
    }

    fn put(&self, view: &Decimal, existing: Option<&DvQuantity>) -> Result<DvQuantity, LensError> {
        let text = decimal_text(Some(view), DECIMAL_CELL, "DV_QUANTITY.magnitude")?;
        let magnitude = real(text, DECIMAL_CELL, "decimal")?;
        Ok(DvQuantity {
            magnitude,
            precision: precision_of(
                text,
                magnitude,
                existing.is_some(),
                existing.and_then(|held| held.precision),
            ),
            ..primitive(existing)
        })
    }

    fn fallbacks(&self, _view: &Decimal) -> Vec<Fallback> {
        vec![Fallback::new(DECIMAL_CELL, "DV_QUANTITY.units")]
    }
}

impl Lens<DvQuantity, Integer> for QuantityLens {
    fn cell(&self) -> &'static str {
        INTEGER_CELL
    }

    fn get(&self, source: &DvQuantity) -> Result<Integer, LensError> {
        point(source.magnitude_status.as_deref(), INTEGER_CELL)?;
        // NOTE: no specification governs this: our own design, the shortest
        // rendering of a whole magnitude has no fraction, so it reads as an i32
        // exactly when the magnitude is whole and inside the FHIR range.
        let shortest = render(source.magnitude, None);
        let value = shortest
            .parse::<i32>()
            .map_err(|_refused| LensError::Shape {
                cell: INTEGER_CELL,
                attribute: "DV_QUANTITY.magnitude",
                found: shortest.clone(),
                reason: "a FHIR integer is a whole signed 32-bit number",
                direction: Direction::OpenehrToFhir,
            })?;
        Ok(Integer::from(value))
    }

    fn put(&self, view: &Integer, existing: Option<&DvQuantity>) -> Result<DvQuantity, LensError> {
        let value = view.value.ok_or(LensError::Missing {
            cell: INTEGER_CELL,
            attribute: "DV_QUANTITY.magnitude",
            direction: Direction::FhirToOpenehr,
        })?;
        Ok(DvQuantity {
            magnitude: f64::from(value),
            precision: existing.map_or(Some(0), |held| held.precision),
            ..primitive(existing)
        })
    }

    fn fallbacks(&self, _view: &Integer) -> Vec<Fallback> {
        vec![Fallback::new(INTEGER_CELL, "DV_QUANTITY.units")]
    }
}

/// Returns the value a primitive writes into, with the attributes it has no
/// counterpart for taken from the held value and `units` the specification's
/// `1` when nothing is held.
fn primitive(existing: Option<&DvQuantity>) -> DvQuantity {
    DvQuantity {
        units: existing.map_or_else(|| String::from(UNITY), |held| held.units.clone()),
        units_system: existing.and_then(|held| held.units_system.clone()),
        units_display_name: existing.and_then(|held| held.units_display_name.clone()),
        magnitude_status: existing.and_then(|held| held.magnitude_status.clone()),
        ..carried(existing)
    }
}

/// Returns the attributes no cell of the type has a FHIR counterpart for,
/// taken from the held value.
fn carried(existing: Option<&DvQuantity>) -> DvQuantity {
    DvQuantity {
        normal_status: existing.and_then(|held| held.normal_status.clone()),
        normal_range: existing.and_then(|held| held.normal_range.clone()),
        other_reference_ranges: existing.and_then(|held| held.other_reference_ranges.clone()),
        magnitude_status: None,
        accuracy: existing.and_then(|held| held.accuracy),
        accuracy_is_percent: existing.and_then(|held| held.accuracy_is_percent),
        magnitude: 0.0,
        precision: None,
        units: String::new(),
        units_system: None,
        units_display_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::QuantityLens;
    use super::UCUM;
    use crate::engine::lens::Lens;
    use crate::engine::lens::LensError;
    use fhir_types::r4::primitives::Code;
    use fhir_types::r4::primitives::Decimal;
    use fhir_types::r4::primitives::Integer;
    use fhir_types::r4::primitives::String as FhirString;
    use fhir_types::r4::quantity::Quantity;
    use openehr_rm::v1_2::data_types::quantity::dv_quantity::DvQuantity;
    use proptest::option;
    use proptest::prop_compose;
    use proptest::prop_oneof;
    use proptest::proptest;
    use proptest::strategy::Just;
    use proptest::strategy::Strategy;

    /// Returns a synthetic quantity of `magnitude` in `units`.
    fn quantity(magnitude: f64, units: &str) -> DvQuantity {
        DvQuantity {
            normal_status: None,
            normal_range: None,
            other_reference_ranges: None,
            magnitude_status: None,
            accuracy: None,
            accuracy_is_percent: None,
            magnitude,
            precision: None,
            units: String::from(units),
            units_system: None,
            units_display_name: None,
        }
    }

    /// The statuses a comparator states, a point value written both ways.
    fn status() -> impl Strategy<Value = Option<String>> {
        prop_oneof![
            Just(None),
            Just(Some(String::from("="))),
            Just(Some(String::from("<"))),
            Just(Some(String::from("<="))),
            Just(Some(String::from(">="))),
            Just(Some(String::from(">"))),
        ]
    }

    prop_compose! {
        /// A quantity in a synthetic unit, written to a declared precision.
        fn measured()(
            hundredths in -100_000i32..100_000,
            precision in option::of(2i32..5),
            units in prop_oneof![Just("mg"), Just("mm[Hg]"), Just("1")],
            system in option::of(prop_oneof![
                Just(String::from(UCUM)),
                Just(String::from("http://example.org/fhir/CodeSystem/synthetic-units")),
            ]),
            display in option::of(prop_oneof![Just(String::from("milligram")), Just(String::from("mg"))]),
            magnitude_status in status(),
            accuracy in option::of(0i32..10),
        ) -> DvQuantity {
            DvQuantity {
                magnitude: f64::from(hundredths) / 100.0,
                precision,
                units_system: system,
                units_display_name: display,
                magnitude_status,
                accuracy: accuracy.map(f64::from),
                ..quantity(0.0, units)
            }
        }
    }

    prop_compose! {
        /// A whole quantity, which an integer carries.
        fn whole()(value in -100_000i32..100_000, precision in option::of(0i32..3)) -> DvQuantity {
            DvQuantity { precision, ..quantity(f64::from(value), "1") }
        }
    }

    proptest! {
        #[test]
        fn quantity_get_put(source in measured()) {
            let view: Quantity = QuantityLens.get(&source).expect("the quantity travels");
            let back = QuantityLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn quantity_put_get(source in measured()) {
            let view: Quantity = QuantityLens.get(&source).expect("the quantity travels");
            let back: DvQuantity = QuantityLens.put(&view, None).expect("the view is complete");
            let again: Quantity = QuantityLens.get(&back).expect("the quantity travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn decimal_get_put(source in measured().prop_filter("a point value", |held| {
            held.magnitude_status.as_deref().is_none_or(|status| status == "=")
        })) {
            let view: Decimal = QuantityLens.get(&source).expect("the magnitude travels");
            let back = QuantityLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn decimal_put_get(source in measured().prop_filter("a point value", |held| {
            held.magnitude_status.as_deref().is_none_or(|status| status == "=")
        })) {
            let view: Decimal = QuantityLens.get(&source).expect("the magnitude travels");
            let back: DvQuantity = QuantityLens.put(&view, None).expect("the view is complete");
            let again: Decimal = QuantityLens.get(&back).expect("the magnitude travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn integer_get_put(source in whole()) {
            let view: Integer = QuantityLens.get(&source).expect("a whole magnitude travels");
            let back = QuantityLens.put(&view, Some(&source)).expect("the view is complete");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn integer_put_get(value in -100_000i32..100_000) {
            let view = Integer::from(value);
            let back: DvQuantity = QuantityLens.put(&view, None).expect("the view is complete");
            let again: Integer = QuantityLens.get(&back).expect("a whole magnitude travels");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn a_quantity_writes_its_units_as_code_and_unit_with_the_ucum_system() {
        // DV_QUANTITY.adoc §Quantity: "both code and unit are populated
        // openEHR -> FHIR"; R4 qty-3 asks for the system beside the code.
        let view: Quantity = QuantityLens
            .get(&quantity(7.5, "mg"))
            .expect("the quantity travels");
        assert_eq!(
            (
                view.value.and_then(|value| value.value),
                view.code.and_then(|code| code.value),
                view.unit.and_then(|unit| unit.value),
                view.system.and_then(|system| system.value),
            ),
            (
                Some(String::from("7.5")),
                Some(String::from("mg")),
                Some(String::from("mg")),
                Some(String::from(UCUM)),
            )
        );
    }

    #[test]
    fn an_approximate_quantity_has_no_comparator() {
        let source = DvQuantity {
            magnitude_status: Some(String::from("~")),
            ..quantity(7.0, "mg")
        };
        let error = Lens::<DvQuantity, Quantity>::get(&QuantityLens, &source)
            .expect_err("no comparator states an approximate value");
        assert!(
            matches!(error, LensError::Shape { attribute, .. } if attribute == "DV_QUANTITY.magnitude_status"),
            "the refusal names the status: {error}"
        );
    }

    #[test]
    fn a_quantity_with_no_unit_has_no_units() {
        let view = Quantity {
            value: Some(Decimal::from("7")),
            ..Quantity::default()
        };
        let error = QuantityLens
            .put(&view, None)
            .expect_err("units is mandatory");
        assert_eq!(
            error,
            LensError::Missing {
                cell: super::CELL,
                attribute: "DV_QUANTITY.units",
                direction: crate::model::ast::keyword::Direction::FhirToOpenehr,
            }
        );
    }

    #[test]
    fn a_unit_with_no_code_is_the_units_by_a_one_way_row() {
        let view = Quantity {
            value: Some(Decimal::from("7")),
            unit: Some(FhirString::from("mg")),
            ..Quantity::default()
        };
        let back = QuantityLens.put(&view, None).expect("the unit stands in");
        assert_eq!(back.units, "mg");
        assert_eq!(QuantityLens.fallbacks(&view).len(), 1);
        let coded = Quantity {
            code: Some(Code::from("mg")),
            ..view
        };
        assert!(QuantityLens.fallbacks(&coded).is_empty());
    }

    #[test]
    fn a_fractional_magnitude_has_no_integer() {
        let error = Lens::<DvQuantity, Integer>::get(&QuantityLens, &quantity(2.5, "1"))
            .expect_err("an integer is whole");
        assert!(
            matches!(error, LensError::Shape { .. }),
            "the refusal names the magnitude: {error}"
        );
    }

    #[test]
    fn a_primitive_writes_the_unity_units_when_nothing_is_held() {
        // DV_QUANTITY.adoc §Primitives: units "hardcoded as 1".
        let back = QuantityLens
            .put(&Integer::from(3), None)
            .expect("the integer travels");
        assert_eq!(back.units, super::UNITY);
        assert_eq!(back.precision, Some(0));
    }
}
