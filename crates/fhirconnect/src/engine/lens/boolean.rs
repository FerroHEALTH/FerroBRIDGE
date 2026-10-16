// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_BOOLEAN` against FHIR `boolean`.
//!
//! The data-type chapter leaves this cell out: "Primitive mappings like
//! integers to integers, strings to string etc. are left out due to being
//! straight forward" (`types-of-mappings/data-type/data-mappings.adoc`).
//! `DV_BOOLEAN.value` is a `Boolean`
//! (<https://specifications.openehr.org/releases/RM/Release-1.1.0/data_types.html#_dv_boolean_class>)
//! and FHIR `boolean` is `true` or `false`
//! (<https://hl7.org/fhir/R4/datatypes.html#boolean>), so the one attribute
//! moves unchanged and neither side has an attribute the other lacks.

use fhir_types::r4::primitives::Boolean;
use openehr_rm::v1_2::data_types::basic::dv_boolean::DvBoolean;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::model::ast::keyword::Direction;

/// The name the `boolean` cell carries in a diagnostic.
pub const BOOLEAN_CELL: &str = "DV_BOOLEAN against boolean";

/// `DV_BOOLEAN` against `boolean`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BooleanLens;

impl Lens<DvBoolean, Boolean> for BooleanLens {
    fn cell(&self) -> &'static str {
        BOOLEAN_CELL
    }

    fn get(&self, source: &DvBoolean) -> Result<Boolean, LensError> {
        Ok(Boolean::from(source.value))
    }

    fn put(&self, view: &Boolean, _existing: Option<&DvBoolean>) -> Result<DvBoolean, LensError> {
        let value = view.value.ok_or(LensError::Missing {
            cell: BOOLEAN_CELL,
            attribute: "DV_BOOLEAN.value",
            direction: Direction::FhirToOpenehr,
        })?;
        Ok(DvBoolean { value })
    }

    fn fallbacks(&self, _view: &Boolean) -> Vec<Fallback> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use fhir_types::r4::primitives::Boolean;
    use openehr_rm::v1_2::data_types::basic::dv_boolean::DvBoolean;

    use super::BOOLEAN_CELL;
    use super::BooleanLens;
    use crate::engine::lens::Lens;
    use crate::engine::lens::LensError;

    #[test]
    fn both_values_round_trip_both_ways() {
        for value in [true, false] {
            let fhir = BooleanLens
                .get(&DvBoolean { value })
                .expect("a boolean reads");
            assert_eq!(fhir.value, Some(value));
            assert_eq!(
                BooleanLens.put(&fhir, None).expect("a boolean writes"),
                DvBoolean { value }
            );
        }
    }

    #[test]
    fn a_boolean_with_no_value_is_refused() {
        assert!(matches!(
            BooleanLens.put(&Boolean::default(), None),
            Err(LensError::Missing {
                cell: BOOLEAN_CELL,
                ..
            })
        ));
    }
}
