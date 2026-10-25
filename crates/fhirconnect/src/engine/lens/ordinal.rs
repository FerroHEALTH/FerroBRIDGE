// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_ORDINAL` against FHIR `CodeableConcept`.
//!
//! The cell is
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_ORDINAL.adoc`
//! §`CodeableConcept`: `coding.code` pairs with `symbol`, and `value` has no
//! FHIR row because it "has to be maintained by the archetype itself". The
//! symbol travels through the `DV_CODED_TEXT` cell, and going into openEHR
//! the value is the one the template's `C_DV_ORDINAL` lists for the symbol's
//! code (the openEHR OPT 1.4 XML schema, `C_DV_ORDINAL`). A symbol the list
//! does not hold, or a node whose template lists no options, is refused: a
//! value the archetype does not define would be invented.

use fhir_types::r4::codeable_concept::CodeableConcept;
use openehr_mapping_core::index::ordinals::OrdinalOption;
use openehr_rm::v1_2::data_types::quantity::dv_ordinal::DvOrdinal;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::coded_text::CodedTextLens;
use crate::engine::lens::term_mapping::Withheld;
use crate::model::ast::keyword::Direction;

/// The name the cell carries in a diagnostic.
pub const CELL: &str = "DV_ORDINAL against CodeableConcept";

/// `DV_ORDINAL` against `CodeableConcept`, with the options the template
/// constrains at the node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrdinalLens<'o> {
    coded: CodedTextLens,
    options: &'o [OrdinalOption],
}

impl<'o> OrdinalLens<'o> {
    /// Creates the lens over the symbol cell `coded` and the template's
    /// `options`, in template order.
    #[must_use]
    pub const fn new(coded: CodedTextLens, options: &'o [OrdinalOption]) -> Self {
        Self { coded, options }
    }

    /// Returns the value the template lists for `symbol`.
    fn value_of(&self, symbol: &DvCodedText) -> Result<i32, LensError> {
        if self.options.is_empty() {
            return Err(LensError::Undefined {
                cell: CELL,
                situation: "a node whose template lists no ordinal options",
                reason: "the page keeps `value` maintained by the archetype itself, \
                         and this template states none",
                direction: Direction::FhirToOpenehr,
            });
        }
        let code = &symbol.defining_code;
        self.options
            .iter()
            .find(|option| {
                option.code() == code.code_string
                    && option.terminology() == code.terminology_id.value
            })
            .map(OrdinalOption::value)
            .ok_or_else(|| LensError::Shape {
                cell: CELL,
                attribute: "DV_ORDINAL.symbol",
                found: format!("{}::{}", code.terminology_id.value, code.code_string),
                reason: "the template lists no ordinal for this symbol",
                direction: Direction::FhirToOpenehr,
            })
    }
}

impl Lens<DvOrdinal, CodeableConcept> for OrdinalLens<'_> {
    fn cell(&self) -> &'static str {
        CELL
    }

    fn get(&self, source: &DvOrdinal) -> Result<CodeableConcept, LensError> {
        self.coded.get(&source.symbol)
    }

    fn put(
        &self,
        view: &CodeableConcept,
        existing: Option<&DvOrdinal>,
    ) -> Result<DvOrdinal, LensError> {
        let symbol = self.coded.put(view, existing.map(|held| &held.symbol))?;
        let value = self.value_of(&symbol)?;
        Ok(DvOrdinal {
            normal_status: existing.and_then(|held| held.normal_status.clone()),
            normal_range: existing.and_then(|held| held.normal_range.clone()),
            other_reference_ranges: existing.and_then(|held| held.other_reference_ranges.clone()),
            symbol,
            value,
        })
    }

    fn fallbacks(&self, view: &CodeableConcept) -> Vec<Fallback> {
        Lens::<DvCodedText, CodeableConcept>::fallbacks(&self.coded, view)
            .into_iter()
            .map(|taken| Fallback::new(CELL, taken.field()))
            .collect()
    }

    fn withheld(&self, source: &DvOrdinal) -> Vec<Withheld> {
        Lens::<DvCodedText, CodeableConcept>::withheld(&self.coded, &source.symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::CELL;
    use super::OrdinalLens;
    use crate::engine::lens::Lens;
    use crate::engine::lens::LensError;
    use crate::engine::lens::coded_text::CodedTextLens;
    use crate::engine::lens::coded_text::tests::codeable_concept;
    use crate::engine::lens::coded_text::tests::coded_text;
    use fhir_types::r4::codeable_concept::CodeableConcept;
    use openehr_mapping_core::index::ordinals::OrdinalOption;
    use openehr_rm::v1_2::data_types::quantity::dv_ordinal::DvOrdinal;
    use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
    use proptest::prop_compose;
    use proptest::proptest;

    /// The option the template lists for `symbol`, with `value`.
    fn option_of(symbol: &DvCodedText, value: i32) -> OrdinalOption {
        OrdinalOption::new(
            value,
            &symbol.defining_code.code_string,
            &symbol.defining_code.terminology_id.value,
        )
    }

    prop_compose! {
        /// A `DV_ORDINAL` whose symbol the cell carries losslessly.
        fn ordinal()(symbol in coded_text(), value in -5..10_i32) -> DvOrdinal {
            DvOrdinal {
                normal_status: None,
                normal_range: None,
                other_reference_ranges: None,
                symbol,
                value,
            }
        }
    }

    proptest! {
        #[test]
        fn get_put(source in ordinal()) {
            let options = [option_of(&source.symbol, source.value)];
            let lens = OrdinalLens::new(CodedTextLens::unbound(), &options);
            let view: CodeableConcept = lens.get(&source).expect("the symbol is carried");
            let back = lens.put(&view, Some(&source)).expect("the template lists the symbol");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn put_get(view in codeable_concept(), value in -5..10_i32) {
            let coded = CodedTextLens::unbound();
            let symbol: DvCodedText = coded.put(&view, None).expect("the concept is carried");
            let options = [option_of(&symbol, value)];
            let lens = OrdinalLens::new(coded, &options);
            let source = lens.put(&view, None).expect("the template lists the symbol");
            assert_eq!(value, source.value, "the value comes from the template");
            let back: CodeableConcept = lens.get(&source).expect("the value is complete");
            assert_eq!(back, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn a_symbol_the_template_does_not_list_is_refused(view in codeable_concept()) {
            let options = [OrdinalOption::new(1, "at-not-listed", "nowhere")];
            let lens = OrdinalLens::new(CodedTextLens::unbound(), &options);
            let refused = lens.put(&view, None).expect_err("no option names the symbol");
            assert!(
                matches!(refused, LensError::Shape { cell: CELL, attribute: "DV_ORDINAL.symbol", .. }),
                "{refused:?}"
            );
        }

        #[test]
        fn a_node_with_no_options_is_refused(view in codeable_concept()) {
            let lens = OrdinalLens::new(CodedTextLens::unbound(), &[]);
            let refused = lens.put(&view, None).expect_err("the template lists nothing");
            assert!(matches!(refused, LensError::Undefined { cell: CELL, .. }), "{refused:?}");
        }
    }
}
