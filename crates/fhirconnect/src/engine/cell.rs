// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Which data-type cell a mapping runs.
//!
//! A cell is one row of the specification's data-type chapter, so it is fixed
//! by the pair of classes a mapping names: the openEHR class the Web Template
//! node carries and the FHIR element the `type` key or the element table
//! names. This module is that lookup, and nothing more: every conversion
//! lives in [`crate::engine::lens`] and runs both ways there.

use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use openehr_rm::v1_2::data_types::quantity::dv_count::DvCount;
use openehr_rm::v1_2::data_types::quantity::dv_quantity::DvQuantity;
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
use openehr_rm::v1_2::data_types::text::dv_text::DvTextData;

use crate::engine::fhir::FhirKind;
use crate::engine::fhir::FhirValue;
use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::boolean::BooleanLens;
use crate::engine::lens::code_phrase::CodePhraseLens;
use crate::engine::lens::coded_text::CodedTextLens;
use crate::engine::lens::count::CountLens;
use crate::engine::lens::date::DateLens;
use crate::engine::lens::date_time::DateTimeLens;
use crate::engine::lens::date_time::PeriodStartLens;
use crate::engine::lens::interval::DateTimeIntervalLens;
use crate::engine::lens::party::IdentifierLens;
use crate::engine::lens::party::PartyLens;
use crate::engine::lens::proportion::PercentLens;
use crate::engine::lens::quantity::QuantityLens;
use crate::engine::lens::term_mapping::Withheld;
use crate::engine::lens::text::TextLens;
use crate::engine::rm::RmValue;
use crate::model::ast::keyword::Direction;

/// What a cell produced, with the one-way rows it took.
#[derive(Debug, Clone, PartialEq)]
pub struct Produced<T> {
    /// The converted value.
    pub value: T,
    /// The rows the cell filled by a rule that runs one way.
    pub fallbacks: Vec<Fallback>,
}

/// Reads an openEHR value into the FHIR element `kind` names.
///
/// # Errors
///
/// Returns [`LensError::NoCell`] when the specification's chapter has no row
/// for the pair, and whatever the cell refuses otherwise.
pub fn get(
    source: &RmValue,
    kind: FhirKind,
    binding: Option<&str>,
) -> Result<FhirValue, LensError> {
    let coded = coded_lens(binding);
    match (source, kind) {
        (RmValue::Text(text), FhirKind::String) => TextLens.get(text).map(FhirValue::String),
        (RmValue::Text(text), FhirKind::Coding) => TextLens
            .get(text)
            .map(|coding| FhirValue::Coding(Box::new(coding))),
        (RmValue::Text(text), FhirKind::CodeableConcept) => TextLens
            .get(text)
            .map(|concept| FhirValue::CodeableConcept(Box::new(concept))),
        (RmValue::CodedText(text), FhirKind::Coding) => coded
            .get(text.as_ref())
            .map(|coding| FhirValue::Coding(Box::new(coding))),
        (RmValue::CodedText(text), FhirKind::CodeableConcept) => coded
            .get(text.as_ref())
            .map(|concept| FhirValue::CodeableConcept(Box::new(concept))),
        (RmValue::CodedText(text), FhirKind::String) => {
            TextLens.get(&plain(text.as_ref())).map(FhirValue::String)
        }
        (RmValue::CodePhrase(code), FhirKind::Coding) => CodePhraseLens
            .get(code)
            .map(|coding| FhirValue::Coding(Box::new(coding))),
        (RmValue::Date(date), FhirKind::Date) => DateLens.get(date.as_ref()).map(FhirValue::Date),
        (RmValue::Boolean(boolean), FhirKind::Boolean) => {
            BooleanLens.get(boolean).map(FhirValue::Boolean)
        }
        (RmValue::DateTime(date), FhirKind::DateTime) => {
            DateTimeLens.get(date.as_ref()).map(FhirValue::DateTime)
        }
        (RmValue::DateTime(date), FhirKind::Period) => PeriodStartLens
            .get(date.as_ref())
            .map(|period| FhirValue::Period(Box::new(period))),
        (RmValue::DateTimeInterval(interval), FhirKind::Period) => DateTimeIntervalLens
            .get(interval.as_ref())
            .map(|period| FhirValue::Period(Box::new(period))),
        (RmValue::Party(party), FhirKind::Reference) => PartyLens
            .get(party)
            .map(|reference| FhirValue::Reference(Box::new(reference))),
        (RmValue::Identifier(identifier), FhirKind::Identifier) => IdentifierLens
            .get(identifier)
            .map(|identifier| FhirValue::Identifier(Box::new(identifier))),
        (RmValue::Proportion(proportion), FhirKind::Quantity) => PercentLens
            .get(proportion.as_ref())
            .map(|quantity| FhirValue::Quantity(Box::new(quantity))),
        (RmValue::Quantity(quantity), FhirKind::Quantity) => QuantityLens
            .get(quantity.as_ref())
            .map(|quantity| FhirValue::Quantity(Box::new(quantity))),
        (RmValue::Quantity(quantity), FhirKind::Decimal) => {
            QuantityLens.get(quantity.as_ref()).map(FhirValue::Decimal)
        }
        (RmValue::Quantity(quantity), FhirKind::Integer) => {
            QuantityLens.get(quantity.as_ref()).map(FhirValue::Integer)
        }
        (RmValue::Count(count), FhirKind::Integer) => {
            CountLens.get(count.as_ref()).map(FhirValue::Integer)
        }
        (RmValue::Count(count), FhirKind::Quantity) => CountLens
            .get(count.as_ref())
            .map(|quantity| FhirValue::Quantity(Box::new(quantity))),
        (RmValue::Count(count), FhirKind::Count) => CountLens
            .get(count.as_ref())
            .map(|count| FhirValue::Count(Box::new(count))),
        (source, kind) => Err(no_cell(source, kind, Direction::OpenehrToFhir)),
    }
}

/// Returns the term mappings [`get`] leaves out of the FHIR element `kind`
/// names, in the order the value holds them.
///
/// The cells that carry `mappings` (`DV_TEXT` against `Coding` and
/// `CodeableConcept`, `DV_CODED_TEXT` against `CodeableConcept`) withhold
/// the ones whose `match` is not `=`. The cells whose FHIR element has no
/// coding (`DV_TEXT` and `DV_CODED_TEXT` against `string`, `DV_CODED_TEXT`
/// against `Coding`) withhold every one.
#[must_use]
pub fn withheld(source: &RmValue, kind: FhirKind, binding: Option<&str>) -> Vec<Withheld> {
    match (source, kind) {
        (RmValue::Text(text), FhirKind::String) => {
            Lens::<DvTextData, fhir_types::r4::primitives::String>::withheld(&TextLens, text)
        }
        (RmValue::CodedText(text), FhirKind::String) => {
            Lens::<DvTextData, fhir_types::r4::primitives::String>::withheld(
                &TextLens,
                &plain(text.as_ref()),
            )
        }
        (RmValue::CodedText(text), FhirKind::Coding) => {
            Lens::<DvCodedText, Coding>::withheld(&coded_lens(binding), text.as_ref())
        }
        (RmValue::Text(text), FhirKind::Coding) => {
            Lens::<DvTextData, Coding>::withheld(&TextLens, text)
        }
        (RmValue::Text(text), FhirKind::CodeableConcept) => {
            Lens::<DvTextData, CodeableConcept>::withheld(&TextLens, text)
        }
        (RmValue::CodedText(text), FhirKind::CodeableConcept) => {
            Lens::<DvCodedText, CodeableConcept>::withheld(&coded_lens(binding), text.as_ref())
        }
        (_source, _kind) => Vec::new(),
    }
}

/// Returns the `defining_code` [`get`] leaves out of the FHIR element `kind`
/// names, `None` when the cell carries it.
///
/// Only `DV_CODED_TEXT` against `string` drops it. The chapter writes no
/// `string` row for `DV_CODED_TEXT` (`DV_CODED_TEXT.adoc` §STRING calls the
/// pairing not possible), so the cell is the `DV_TEXT` one of its supertype,
/// whose table has no `defining_code` row, and the engine declares the code.
#[must_use]
pub fn uncarried_code(source: &RmValue, kind: FhirKind) -> Option<&CodePhrase> {
    match (source, kind) {
        (RmValue::CodedText(text), FhirKind::String) => Some(&text.defining_code),
        (_source, _kind) => None,
    }
}

/// Writes a FHIR element back into the openEHR class `rm_type` names.
///
/// `existing` is the value the target already holds, which carries the
/// attributes the cell's table marks as having no FHIR counterpart.
///
/// # Errors
///
/// Returns [`LensError::NoCell`] when the specification's chapter has no row
/// for the pair, and whatever the cell refuses otherwise.
pub fn put(
    view: &FhirValue,
    rm_type: &str,
    existing: Option<&RmValue>,
    binding: Option<&str>,
) -> Result<Produced<RmValue>, LensError> {
    let coded = coded_lens(binding);
    let value = match (rm_type, view) {
        ("DV_TEXT", FhirValue::String(text)) => {
            TextLens.put(text, held_text(existing)).map(RmValue::Text)
        }
        ("DV_TEXT", FhirValue::Coding(coding)) => TextLens
            .put(coding.as_ref(), held_text(existing))
            .map(RmValue::Text),
        ("DV_TEXT", FhirValue::CodeableConcept(concept)) => TextLens
            .put(concept.as_ref(), held_text(existing))
            .map(RmValue::Text),
        ("DV_CODED_TEXT", FhirValue::Coding(coding)) => coded
            .put(coding.as_ref(), held_coded(existing))
            .map(|text| RmValue::CodedText(Box::new(text))),
        ("DV_CODED_TEXT", FhirValue::CodeableConcept(concept)) => coded
            .put(concept.as_ref(), held_coded(existing))
            .map(|text| RmValue::CodedText(Box::new(text))),
        ("CODE_PHRASE", FhirValue::Coding(coding)) => CodePhraseLens
            .put(coding.as_ref(), held_code_phrase(existing))
            .map(RmValue::CodePhrase),
        ("DV_DATE", FhirValue::Date(date)) => DateLens
            .put(date, held_date(existing))
            .map(|date| RmValue::Date(Box::new(date))),
        ("DV_BOOLEAN", FhirValue::Boolean(boolean)) => BooleanLens
            .put(boolean, held_boolean(existing))
            .map(RmValue::Boolean),
        ("DV_DATE_TIME", FhirValue::DateTime(date)) => DateTimeLens
            .put(date, held_date_time(existing))
            .map(|date| RmValue::DateTime(Box::new(date))),
        ("DV_DATE_TIME", FhirValue::Period(period)) => PeriodStartLens
            .put(period.as_ref(), held_date_time(existing))
            .map(|date| RmValue::DateTime(Box::new(date))),
        ("DV_INTERVAL", FhirValue::Period(period)) => DateTimeIntervalLens
            .put(period.as_ref(), held_interval(existing))
            .map(|interval| RmValue::DateTimeInterval(Box::new(interval))),
        ("PARTY_IDENTIFIED" | "PARTY_PROXY", FhirValue::Reference(reference)) => PartyLens
            .put(reference.as_ref(), held_party(existing))
            .map(RmValue::Party),
        ("DV_IDENTIFIER", FhirValue::Identifier(identifier)) => IdentifierLens
            .put(identifier.as_ref(), held_identifier(existing))
            .map(RmValue::Identifier),
        ("DV_PROPORTION", FhirValue::Quantity(quantity)) => PercentLens
            .put(quantity.as_ref(), held_proportion(existing))
            .map(|proportion| RmValue::Proportion(Box::new(proportion))),
        ("DV_QUANTITY", FhirValue::Quantity(quantity)) => QuantityLens
            .put(quantity.as_ref(), held_quantity(existing))
            .map(|quantity| RmValue::Quantity(Box::new(quantity))),
        ("DV_QUANTITY", FhirValue::Decimal(decimal)) => QuantityLens
            .put(decimal, held_quantity(existing))
            .map(|quantity| RmValue::Quantity(Box::new(quantity))),
        ("DV_QUANTITY", FhirValue::Integer(integer)) => QuantityLens
            .put(integer, held_quantity(existing))
            .map(|quantity| RmValue::Quantity(Box::new(quantity))),
        ("DV_COUNT", FhirValue::Integer(integer)) => CountLens
            .put(integer, held_count(existing))
            .map(|count| RmValue::Count(Box::new(count))),
        ("DV_COUNT", FhirValue::Quantity(quantity)) => CountLens
            .put(quantity.as_ref(), held_count(existing))
            .map(|count| RmValue::Count(Box::new(count))),
        ("DV_COUNT", FhirValue::Count(count)) => CountLens
            .put(count.as_ref(), held_count(existing))
            .map(|count| RmValue::Count(Box::new(count))),
        (rm_type, view) => Err(LensError::NoCell {
            rm_type: String::from(rm_type),
            kind: view.kind().as_str(),
            direction: Direction::FhirToOpenehr,
        }),
    }?;
    Ok(Produced {
        value,
        fallbacks: fallbacks(view, rm_type, existing, binding),
    })
}

/// Returns the one-way rows the cell would take writing `view` over
/// `existing`.
fn fallbacks(
    view: &FhirValue,
    rm_type: &str,
    existing: Option<&RmValue>,
    binding: Option<&str>,
) -> Vec<Fallback> {
    let coded = coded_lens(binding);
    // NOTE: DV_QUANTITY.adoc §Primitives hardcodes `units` as 1, which is a
    // one-way row only where no held value supplies the units.
    let unheld = held_quantity(existing).is_none();
    match (rm_type, view) {
        ("DV_QUANTITY", FhirValue::Quantity(quantity)) => QuantityLens.fallbacks(quantity.as_ref()),
        ("DV_QUANTITY", FhirValue::Decimal(decimal)) if unheld => QuantityLens.fallbacks(decimal),
        ("DV_QUANTITY", FhirValue::Integer(integer)) if unheld => QuantityLens.fallbacks(integer),
        ("DV_TEXT", FhirValue::String(text)) => TextLens.fallbacks(text),
        ("DV_TEXT", FhirValue::Coding(coding)) => TextLens.fallbacks(coding.as_ref()),
        ("DV_TEXT", FhirValue::CodeableConcept(concept)) => TextLens.fallbacks(concept.as_ref()),
        ("DV_CODED_TEXT", FhirValue::Coding(coding)) => coded.fallbacks(coding.as_ref()),
        ("DV_CODED_TEXT", FhirValue::CodeableConcept(concept)) => coded.fallbacks(concept.as_ref()),
        ("CODE_PHRASE", FhirValue::Coding(coding)) => CodePhraseLens.fallbacks(coding.as_ref()),
        ("DV_DATE", FhirValue::Date(date)) => DateLens.fallbacks(date),
        ("DV_BOOLEAN", FhirValue::Boolean(boolean)) => BooleanLens.fallbacks(boolean),
        ("DV_DATE_TIME", FhirValue::DateTime(date)) => DateTimeLens.fallbacks(date),
        ("DV_DATE_TIME", FhirValue::Period(period)) => PeriodStartLens.fallbacks(period.as_ref()),
        ("DV_INTERVAL", FhirValue::Period(period)) => {
            DateTimeIntervalLens.fallbacks(period.as_ref())
        }
        ("PARTY_IDENTIFIED" | "PARTY_PROXY", FhirValue::Reference(reference)) => {
            PartyLens.fallbacks(reference.as_ref())
        }
        ("DV_IDENTIFIER", FhirValue::Identifier(identifier)) => {
            IdentifierLens.fallbacks(identifier.as_ref())
        }
        ("DV_PROPORTION", FhirValue::Quantity(quantity)) => {
            PercentLens.fallbacks(quantity.as_ref())
        }
        (_rm_type, _view) => Vec::new(),
    }
}

/// Returns the coded-text lens for an element with or without a binding.
fn coded_lens(binding: Option<&str>) -> CodedTextLens {
    binding.map_or_else(CodedTextLens::unbound, CodedTextLens::bound)
}

/// Returns the refusal for a pair the data-type chapter has no row for.
fn no_cell(source: &RmValue, kind: FhirKind, direction: Direction) -> LensError {
    LensError::NoCell {
        rm_type: String::from(source.rm_type()),
        kind: kind.as_str(),
        direction,
    }
}

/// Returns the plain text of a coded one, for the `string` cell.
///
/// `DV_CODED_TEXT` is a subtype of `DV_TEXT`
/// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/data_types.html#_dv_coded_text_class>),
/// so the `DV_TEXT` rows apply to it unchanged.
fn plain(coded: &DvCodedText) -> DvTextData {
    DvTextData {
        value: coded.value.clone(),
        hyperlink: coded.hyperlink.clone(),
        formatting: coded.formatting.clone(),
        mappings: coded.mappings.clone(),
        language: coded.language.clone(),
        encoding: coded.encoding.clone(),
    }
}

/// Returns the held `DV_TEXT`, when the target holds one.
fn held_text(existing: Option<&RmValue>) -> Option<&DvTextData> {
    match existing {
        Some(RmValue::Text(text)) => Some(text),
        _other => None,
    }
}

/// Returns the held `DV_CODED_TEXT`, when the target holds one.
fn held_coded(existing: Option<&RmValue>) -> Option<&DvCodedText> {
    match existing {
        Some(RmValue::CodedText(text)) => Some(text.as_ref()),
        _other => None,
    }
}

/// Returns the held `CODE_PHRASE`, when the target holds one.
fn held_code_phrase(existing: Option<&RmValue>) -> Option<&CodePhrase> {
    match existing {
        Some(RmValue::CodePhrase(code)) => Some(code),
        _other => None,
    }
}

/// Returns the held `DV_DATE`, when the target holds one.
fn held_date(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::quantity::date_time::dv_date::DvDate> {
    match existing {
        Some(RmValue::Date(date)) => Some(date.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_BOOLEAN`, when the target holds one.
fn held_boolean(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::basic::dv_boolean::DvBoolean> {
    match existing {
        Some(RmValue::Boolean(boolean)) => Some(boolean),
        _other => None,
    }
}

/// Returns the held `DV_DATE_TIME`, when the target holds one.
fn held_date_time(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::quantity::date_time::dv_date_time::DvDateTime> {
    match existing {
        Some(RmValue::DateTime(date)) => Some(date.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_INTERVAL<DV_DATE_TIME>`, when the target holds one.
fn held_interval(
    existing: Option<&RmValue>,
) -> Option<
    &openehr_rm::v1_2::data_types::quantity::dv_interval::DvInterval<
        openehr_rm::v1_2::data_types::quantity::date_time::dv_date_time::DvDateTime,
    >,
> {
    match existing {
        Some(RmValue::DateTimeInterval(interval)) => Some(interval.as_ref()),
        _other => None,
    }
}

/// Returns the held `PARTY_IDENTIFIED`, when the target holds one.
fn held_party(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::common::generic::party_identified::PartyIdentifiedData> {
    match existing {
        Some(RmValue::Party(party)) => Some(party),
        _other => None,
    }
}

/// Returns the held `DV_IDENTIFIER`, when the target holds one.
fn held_identifier(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::basic::dv_identifier::DvIdentifier> {
    match existing {
        Some(RmValue::Identifier(identifier)) => Some(identifier),
        _other => None,
    }
}

/// Returns the held `DV_QUANTITY`, when the target holds one.
fn held_quantity(existing: Option<&RmValue>) -> Option<&DvQuantity> {
    match existing {
        Some(RmValue::Quantity(quantity)) => Some(quantity.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_COUNT`, when the target holds one.
fn held_count(existing: Option<&RmValue>) -> Option<&DvCount> {
    match existing {
        Some(RmValue::Count(count)) => Some(count.as_ref()),
        _other => None,
    }
}

/// Returns the held `DV_PROPORTION`, when the target holds one.
fn held_proportion(
    existing: Option<&RmValue>,
) -> Option<&openehr_rm::v1_2::data_types::quantity::dv_proportion::DvProportion> {
    match existing {
        Some(RmValue::Proportion(proportion)) => Some(proportion.as_ref()),
        _other => None,
    }
}

#[cfg(test)]
mod tests {
    use super::get;
    use super::put;
    use super::withheld;
    use crate::engine::fhir::FhirKind;
    use crate::engine::fhir::FhirValue;
    use crate::engine::lens::term_mapping::Cause;
    use crate::engine::rm::RmValue;
    use openehr_base::v1_3::base_types::identification::terminology_id::TerminologyId;
    use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;

    /// Returns a synthetic `CODE_PHRASE`.
    fn code() -> CodePhrase {
        CodePhrase {
            terminology_id: TerminologyId {
                value: String::from("local"),
            },
            code_string: String::from("at0017"),
            preferred_term: None,
        }
    }

    #[test]
    fn a_code_phrase_against_a_coding_runs_its_cell() {
        let produced = get(&RmValue::CodePhrase(code()), FhirKind::Coding, None)
            .expect("the CODE_PHRASE cell carries the pair");
        assert_eq!(produced.kind(), FhirKind::Coding);
    }

    #[test]
    fn a_pair_the_chapter_has_no_row_for_is_refused() {
        let error = get(&RmValue::CodePhrase(code()), FhirKind::Period, None)
            .expect_err("no row pairs a CODE_PHRASE with a Period");
        assert!(
            error.to_string().contains("CODE_PHRASE"),
            "the refusal names the class: {error}"
        );
    }

    #[test]
    fn the_target_class_decides_the_cell_going_back() {
        let coding = fhir_types::r4::coding::Coding {
            system: Some(fhir_types::r4::primitives::Uri::from("local")),
            code: Some(fhir_types::r4::primitives::Code::from("at0017")),
            ..fhir_types::r4::coding::Coding::default()
        };
        let produced = put(
            &FhirValue::Coding(Box::new(coding)),
            "CODE_PHRASE",
            None,
            None,
        )
        .expect("the CODE_PHRASE cell carries the pair");
        assert_eq!(produced.value.rm_type(), "CODE_PHRASE");
        assert!(
            produced.fallbacks.is_empty(),
            "the cell took no one-way row"
        );
    }

    #[test]
    fn a_quantity_and_a_count_run_their_cells_both_ways() {
        // DV_QUANTITY.adoc §Quantity and §Primitives, DV_COUNT.adoc §Primitive,
        // §Quantity and §Count.
        let quantity = RmValue::from_canonical(
            "DV_QUANTITY",
            "/synthetic",
            &serde_json::json!({
                "_type": "DV_QUANTITY", "magnitude": 7.25, "precision": 2, "units": "mg",
            }),
        )
        .expect("a DV_QUANTITY");
        let count = RmValue::from_canonical(
            "DV_COUNT",
            "/synthetic",
            &serde_json::json!({ "_type": "DV_COUNT", "magnitude": 3 }),
        )
        .expect("a DV_COUNT");
        for (value, kinds) in [
            (
                &quantity,
                [FhirKind::Quantity, FhirKind::Decimal, FhirKind::Quantity],
            ),
            (
                &count,
                [FhirKind::Integer, FhirKind::Quantity, FhirKind::Count],
            ),
        ] {
            for kind in kinds {
                let view = get(value, kind, None).expect("the chapter has the row");
                assert_eq!(view.kind(), kind);
                let back =
                    put(&view, value.rm_type(), Some(value), None).expect("the row runs back");
                assert_eq!(&back.value, value, "{kind:?} round trips");
            }
        }
        let error = get(&quantity, FhirKind::Integer, None).expect_err("7.25 is no integer");
        assert!(error.to_string().contains("DV_QUANTITY"), "{error}");
    }

    #[test]
    fn a_primitive_declares_the_unity_units_only_when_nothing_is_held() {
        let view = FhirValue::Integer(fhir_types::r4::primitives::Integer::from(3));
        let fresh = put(&view, "DV_QUANTITY", None, None).expect("the integer travels");
        assert_eq!(fresh.fallbacks.len(), 1, "units is the specification's 1");
        let held =
            put(&view, "DV_QUANTITY", Some(&fresh.value), None).expect("the integer travels");
        assert!(held.fallbacks.is_empty(), "the held units return");
    }

    /// Returns two synthetic `TERM_MAPPING`s, one `=` and one `>`.
    fn mappings() -> serde_json::Value {
        let term = |relation: &str, code: &str| {
            serde_json::json!({
                "_type": "TERM_MAPPING",
                "match": relation,
                "target": {
                    "_type": "CODE_PHRASE",
                    "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "http://example.org/fhir/CodeSystem/synthetic"},
                    "code_string": code,
                },
            })
        };
        serde_json::json!([term("=", "SYN-001"), term(">", "SYN-002")])
    }

    /// Returns a synthetic `DV_TEXT` annotated with [`mappings`].
    fn text() -> RmValue {
        let value = serde_json::json!({
            "_type": "DV_TEXT",
            "value": "a synthetic rubric",
            "mappings": mappings(),
        });
        RmValue::from_canonical("DV_TEXT", "/synthetic", &value).expect("a DV_TEXT")
    }

    /// Returns a synthetic `DV_CODED_TEXT` annotated with [`mappings`].
    fn coded_text() -> RmValue {
        let value = serde_json::json!({
            "_type": "DV_CODED_TEXT",
            "value": "a synthetic rubric",
            "mappings": mappings(),
            "defining_code": {
                "_type": "CODE_PHRASE",
                "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "local"},
                "code_string": "at0017",
            },
        });
        RmValue::from_canonical("DV_CODED_TEXT", "/synthetic", &value).expect("a DV_CODED_TEXT")
    }

    /// Returns the cause and code of every term `withheld` reports.
    fn reported(source: &RmValue, kind: FhirKind) -> Vec<(Cause, String)> {
        withheld(source, kind, None)
            .iter()
            .map(|taken| (taken.cause(), String::from(taken.code())))
            .collect()
    }

    #[test]
    fn a_cell_with_no_coding_withholds_every_term_mapping() {
        // DV_TEXT.adoc §String and DV_CODED_TEXT.adoc §Coding give `mappings`
        // no FHIRpath row.
        let every = vec![
            (Cause::Uncarried, String::from("SYN-001")),
            (Cause::Uncarried, String::from("SYN-002")),
        ];
        assert_eq!(reported(&text(), FhirKind::String), every);
        assert_eq!(reported(&coded_text(), FhirKind::String), every);
        assert_eq!(reported(&coded_text(), FhirKind::Coding), every);
    }

    #[test]
    fn a_coded_text_read_as_a_string_names_its_defining_code() {
        // DV_CODED_TEXT.adoc §STRING: the chapter writes no string row, so the
        // DV_TEXT cell carries the rubric and the code is declared.
        let source = coded_text();
        let dropped = super::uncarried_code(&source, FhirKind::String).map(|code| {
            (
                code.terminology_id.value.as_str(),
                code.code_string.as_str(),
            )
        });
        assert_eq!(dropped, Some(("local", "at0017")));
        let produced = get(&source, FhirKind::String, None).expect("the DV_TEXT cell runs");
        assert_eq!(produced.kind(), FhirKind::String);
    }

    #[test]
    fn a_cell_with_a_coding_drops_no_defining_code() {
        for kind in [FhirKind::Coding, FhirKind::CodeableConcept] {
            assert_eq!(super::uncarried_code(&coded_text(), kind), None);
        }
        assert_eq!(super::uncarried_code(&text(), FhirKind::String), None);
    }

    #[test]
    fn a_cell_with_codings_withholds_only_what_no_coding_states() {
        // TERM_MAPPING.adoc: `match` is "hard coded to `=`".
        let relation = vec![(Cause::Match, String::from("SYN-002"))];
        assert_eq!(reported(&text(), FhirKind::Coding), relation);
        assert_eq!(reported(&text(), FhirKind::CodeableConcept), relation);
        assert_eq!(reported(&coded_text(), FhirKind::CodeableConcept), relation);
    }
}
