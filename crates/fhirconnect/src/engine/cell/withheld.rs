// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a cell does not carry to FHIR: the term mappings it withholds and
//! the code phrase the FHIR element has no place for.

use super::{coded_lens, plain};
use crate::engine::fhir::FhirKind;
use crate::engine::lens::Lens;
use crate::engine::lens::ordinal::OrdinalLens;
use crate::engine::lens::term_mapping::Withheld;
use crate::engine::lens::text::TextLens;
use crate::engine::rm::RmValue;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
use openehr_rm::v1_2::data_types::text::dv_text::DvTextData;

/// Returns the term mappings [`super::get`] leaves out of the FHIR element `kind`
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
        (RmValue::Ordinal(ordinal), FhirKind::CodeableConcept) => {
            OrdinalLens::new(coded_lens(binding), &[]).withheld(ordinal.as_ref())
        }
        (_source, _kind) => Vec::new(),
    }
}

/// Returns the `defining_code` [`super::get`] leaves out of the FHIR element `kind`
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
