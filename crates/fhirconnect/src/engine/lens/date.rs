// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_DATE` against FHIR `date`.
//!
//! The cell is
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_DATE.adoc`.
//! The table pairs `date` with `value` and marks `accuracy` as having no FHIR
//! counterpart.
//!
//! The value moves byte for byte. Both sides hold a lexical string, the RM
//! because `DV_DATE.value` is an "ISO8601 date string" and FHIR because a
//! primitive keeps the text the document carried
//! (<https://hl7.org/fhir/R4/datatypes.html#date>), so a partial date such as
//! `1970` or `1970-01` survives. The engine parses no date.

use fhir_types::r4::primitives::Date;
use openehr_rm::v1_2::data_types::quantity::date_time::dv_date::DvDate;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::model::ast::keyword::Direction;

/// The name the `date` cell carries in a diagnostic.
pub const DATE_CELL: &str = "DV_DATE against date";

/// `DV_DATE` against `date`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DateLens;

impl Lens<DvDate, Date> for DateLens {
    fn cell(&self) -> &'static str {
        DATE_CELL
    }

    fn get(&self, source: &DvDate) -> Result<Date, LensError> {
        if source.value.is_empty() {
            return Err(LensError::Missing {
                cell: DATE_CELL,
                attribute: "date",
                direction: Direction::OpenehrToFhir,
            });
        }
        Ok(Date::from(source.value.clone()))
    }

    fn put(&self, view: &Date, existing: Option<&DvDate>) -> Result<DvDate, LensError> {
        let value = text_of(Some(&view.value)).ok_or(LensError::Missing {
            cell: DATE_CELL,
            attribute: "DV_DATE.value",
            direction: Direction::FhirToOpenehr,
        })?;
        // NOTE: `DV_DATE.adoc` marks `accuracy` `-`; the other attributes come
        // from the held value for the same reason (`DV_ORDERED`, RM 1.1.0).
        Ok(DvDate {
            normal_status: existing.and_then(|held| held.normal_status.clone()),
            normal_range: existing.and_then(|held| held.normal_range.clone()),
            other_reference_ranges: existing.and_then(|held| held.other_reference_ranges.clone()),
            magnitude_status: existing.and_then(|held| held.magnitude_status.clone()),
            accuracy: existing.and_then(|held| held.accuracy.clone()),
            value: String::from(value),
        })
    }

    fn fallbacks(&self, _view: &Date) -> Vec<Fallback> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use fhir_types::r4::primitives::Date;
    use openehr_rm::v1_2::data_types::quantity::date_time::dv_date::DvDate;

    use super::DATE_CELL;
    use super::DateLens;
    use crate::engine::lens::Lens;
    use crate::engine::lens::LensError;

    /// Returns a `DV_DATE` of `value` with nothing else set.
    fn date(value: &str) -> DvDate {
        DvDate {
            normal_status: None,
            normal_range: None,
            other_reference_ranges: None,
            magnitude_status: None,
            accuracy: None,
            value: String::from(value),
        }
    }

    #[test]
    fn a_partial_date_moves_byte_for_byte_both_ways() {
        for value in ["1970", "1970-01", "1970-01-01"] {
            let fhir = DateLens.get(&date(value)).expect("a date reads");
            assert_eq!(fhir.value.as_deref(), Some(value));
            let back = DateLens.put(&fhir, None).expect("a date writes");
            assert_eq!(back, date(value), "GetPut holds for {value}");
        }
    }

    #[test]
    fn putget_holds_and_the_held_status_survives() {
        let mut held = date("1970-01-01");
        held.magnitude_status = Some(String::from("~"));
        let view = Date::from("1971-02-03");
        let written = DateLens.put(&view, Some(&held)).expect("a date writes");
        assert_eq!(written.magnitude_status.as_deref(), Some("~"));
        assert_eq!(DateLens.get(&written).expect("a date reads"), view);
    }

    #[test]
    fn an_empty_value_is_refused_both_ways() {
        assert!(matches!(
            DateLens.get(&date("")),
            Err(LensError::Missing {
                cell: DATE_CELL,
                ..
            })
        ));
        assert!(matches!(
            DateLens.put(&Date::default(), None),
            Err(LensError::Missing {
                cell: DATE_CELL,
                ..
            })
        ));
    }
}
