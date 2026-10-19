// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DV_TEXT` against FHIR `string`, `Coding` and `CodeableConcept`.
//!
//! The cell is
//! `docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/DV_TEXT.adoc`.
//! The `string` table pairs `value` with `value`; the `Coding` and
//! `CodeableConcept` tables put the codings in `mappings`, "in openEHR each
//! `DV_TEXT` can be annotated with codings using mappings", and pair the
//! rubric with `display` and with `text`.

use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::primitives::String as FhirString;
use openehr_base::containers::NonEmptyVec;
use openehr_rm::v1_2::data_types::text::dv_text::DvTextData;

use crate::engine::lens::Fallback;
use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::text_of;
use crate::engine::lens::term_mapping::EQUIVALENT;
use crate::engine::lens::term_mapping::Withheld;
use crate::engine::lens::term_mapping::coding_of;
use crate::engine::lens::term_mapping::equivalents;
use crate::engine::lens::term_mapping::mappings_of;
use crate::engine::lens::term_mapping::uncarried;
use crate::engine::lens::term_mapping::withheld;
use crate::model::ast::keyword::Direction;

/// The name the `string` cell carries in a diagnostic.
pub const STRING_CELL: &str = "DV_TEXT against string";

/// The name the `Coding` cell carries in a diagnostic.
pub const CODING_CELL: &str = "DV_TEXT against Coding";

/// The name the `CodeableConcept` cell carries in a diagnostic.
pub const CODEABLE_CONCEPT_CELL: &str = "DV_TEXT against CodeableConcept";

/// `DV_TEXT` against the three FHIR elements its page names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextLens;

impl Lens<DvTextData, FhirString> for TextLens {
    fn cell(&self) -> &'static str {
        STRING_CELL
    }

    fn get(&self, source: &DvTextData) -> Result<FhirString, LensError> {
        Ok(FhirString::from(source.value.clone()))
    }

    fn put(
        &self,
        view: &FhirString,
        existing: Option<&DvTextData>,
    ) -> Result<DvTextData, LensError> {
        let value = text_of(Some(&view.value)).ok_or(LensError::Missing {
            cell: STRING_CELL,
            attribute: "DV_TEXT.value",
            direction: Direction::FhirToOpenehr,
        })?;
        Ok(DvTextData {
            value: String::from(value),
            ..carried(existing)
        })
    }

    fn fallbacks(&self, _view: &FhirString) -> Vec<Fallback> {
        Vec::new()
    }

    // NOTE: DV_TEXT.adoc §String gives `mappings` no FHIRpath row, so a string
    // carries none of them and each is a declared loss.
    fn withheld(&self, source: &DvTextData) -> Vec<Withheld> {
        uncarried(source.mappings.iter().flatten())
    }
}

impl Lens<DvTextData, CodeableConcept> for TextLens {
    fn cell(&self) -> &'static str {
        CODEABLE_CONCEPT_CELL
    }

    fn get(&self, source: &DvTextData) -> Result<CodeableConcept, LensError> {
        let mut codings = Vec::new();
        for mapping in equivalents(source.mappings.iter().flatten()) {
            codings.push(coding_of(mapping, CODEABLE_CONCEPT_CELL)?);
        }
        Ok(CodeableConcept {
            coding: codings,
            text: Some(FhirString::from(source.value.clone())),
            ..CodeableConcept::default()
        })
    }

    fn put(
        &self,
        view: &CodeableConcept,
        existing: Option<&DvTextData>,
    ) -> Result<DvTextData, LensError> {
        let value = rubric_of(view).ok_or(LensError::Missing {
            cell: CODEABLE_CONCEPT_CELL,
            attribute: "DV_TEXT.value",
            direction: Direction::FhirToOpenehr,
        })?;
        let held = existing.and_then(|text| text.mappings.as_deref());
        let mappings = mappings_of(&view.coding, held)?;
        Ok(DvTextData {
            value: String::from(value),
            mappings: NonEmptyVec::new(mappings).ok(),
            ..carried(existing)
        })
    }

    fn fallbacks(&self, view: &CodeableConcept) -> Vec<Fallback> {
        if text_of(view.text.as_ref().map(|text| &text.value)).is_some() {
            return Vec::new();
        }
        vec![Fallback::new(CODEABLE_CONCEPT_CELL, "DV_TEXT.value")]
    }

    fn withheld(&self, source: &DvTextData) -> Vec<Withheld> {
        withheld(source.mappings.iter().flatten())
    }
}

/// Returns the rubric a `CodeableConcept` gives `DV_TEXT.value`.
///
/// `text` when the concept carries one, else the first coding's `display`,
/// else its `code`. The last two are one-way rows, recorded by
/// [`Lens::fallbacks`].
fn rubric_of(view: &CodeableConcept) -> Option<&str> {
    // NOTE: DV_TEXT.adoc §CodeableConcept gives `value` (1..1) no fallback;
    // FerroBRIDGE reads that silence by DV_CODED_TEXT.adoc §CodeableConcept,
    // which inherits from DV_TEXT and falls back to `coding.display`, `coding.code`.
    let first = view.coding.first();
    text_of(view.text.as_ref().map(|text| &text.value))
        .or_else(|| {
            text_of(
                first
                    .and_then(|coding| coding.display.as_ref())
                    .map(|display| &display.value),
            )
        })
        .or_else(|| {
            text_of(
                first
                    .and_then(|coding| coding.code.as_ref())
                    .map(|code| &code.value),
            )
        })
}

impl Lens<DvTextData, Coding> for TextLens {
    fn cell(&self) -> &'static str {
        CODING_CELL
    }

    fn get(&self, source: &DvTextData) -> Result<Coding, LensError> {
        let mut codings = equivalents(source.mappings.iter().flatten());
        let first = codings.next();
        if codings.next().is_some() {
            return Err(LensError::Shape {
                cell: CODING_CELL,
                attribute: "DV_TEXT.mappings",
                found: String::from("more than one `=` term mapping"),
                reason: "one Coding carries one code, so the extra mappings have nowhere to go",
                direction: Direction::OpenehrToFhir,
            });
        }
        let mut coding = match first {
            Some(mapping) => coding_of(mapping, CODING_CELL)?,
            None => Coding::default(),
        };
        coding.display = Some(FhirString::from(source.value.clone()));
        Ok(coding)
    }

    fn put(&self, view: &Coding, existing: Option<&DvTextData>) -> Result<DvTextData, LensError> {
        let value = text_of(view.display.as_ref().map(|display| &display.value)).ok_or(
            LensError::Missing {
                cell: CODING_CELL,
                attribute: "DV_TEXT.value",
                direction: Direction::FhirToOpenehr,
            },
        )?;
        let held = existing.and_then(|text| text.mappings.as_deref());
        let preferred = equivalents(held.unwrap_or_default())
            .next()
            .and_then(|mapping| mapping.target.preferred_term.clone());
        let coded = text_of(view.system.as_ref().map(|uri| &uri.value)).is_some()
            || text_of(view.code.as_ref().map(|code| &code.value)).is_some();
        let mut mappings = mappings_of(coded.then_some(view), held)?;
        // NOTE: DV_TEXT.adoc §Coding pairs `display` with `value`, so the
        // display is the rubric here and the target's preferred term,
        // which FHIR has nowhere to put, comes from the held mapping.
        if let Some(written) = mappings
            .iter_mut()
            .find(|mapping| mapping.r#match == EQUIVALENT)
        {
            written.target.preferred_term = preferred;
        }
        Ok(DvTextData {
            value: String::from(value),
            mappings: NonEmptyVec::new(mappings).ok(),
            ..carried(existing)
        })
    }

    fn fallbacks(&self, _view: &Coding) -> Vec<Fallback> {
        Vec::new()
    }

    fn withheld(&self, source: &DvTextData) -> Vec<Withheld> {
        withheld(source.mappings.iter().flatten())
    }
}

/// Returns the attributes the three tables mark as having no FHIR counterpart.
///
/// `hyperlink`, `formatting`, `language` and `encoding` are `-` in every row,
/// so they are taken from the value the target already holds.
fn carried(existing: Option<&DvTextData>) -> DvTextData {
    DvTextData {
        value: String::new(),
        hyperlink: existing.and_then(|held| held.hyperlink.clone()),
        formatting: existing.and_then(|held| held.formatting.clone()),
        mappings: None,
        language: existing.and_then(|held| held.language.clone()),
        encoding: existing.and_then(|held| held.encoding.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::CODEABLE_CONCEPT_CELL;
    use super::CODING_CELL;
    use super::TextLens;
    use crate::engine::lens::Fallback;
    use crate::engine::lens::Lens;
    use crate::engine::lens::LensError;
    use crate::engine::lens::code_phrase::terminology_id;
    use crate::engine::lens::code_phrase::tests::code_phrase;
    use crate::engine::lens::code_phrase::tests::coding;
    use fhir_types::r4::codeable_concept::CodeableConcept;
    use fhir_types::r4::coding::Coding;
    use fhir_types::r4::primitives::Code;
    use fhir_types::r4::primitives::String as FhirString;
    use fhir_types::r4::primitives::Uri;
    use openehr_base::containers::NonEmptyVec;
    use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
    use openehr_rm::v1_2::data_types::text::dv_text::DvTextData;
    use openehr_rm::v1_2::data_types::text::term_mapping::TermMapping;
    use proptest::collection::vec;
    use proptest::prop_compose;
    use proptest::proptest;

    prop_compose! {
        /// A `DV_TEXT` carrying `count` codings in its mappings.
        fn text_with(count: core::ops::Range<usize>)(
            value in "[A-Za-z ]{1,12}",
            targets in vec(code_phrase(), count),
        ) -> DvTextData {
            DvTextData {
                value,
                hyperlink: None,
                formatting: None,
                mappings: NonEmptyVec::new(
                    targets
                        .into_iter()
                        .map(|target| TermMapping { r#match: '=', purpose: None, target })
                        .collect::<Vec<TermMapping>>(),
                )
                .ok(),
                language: None,
                encoding: None,
            }
        }
    }

    prop_compose! {
        /// A `CodeableConcept` carrying a text and any number of codings.
        fn codeable_concept()(
            codings in vec(coding(), 0..3),
            text in "[A-Za-z ]{1,12}",
        ) -> CodeableConcept {
            CodeableConcept {
                coding: codings,
                text: Some(FhirString::from(text)),
                ..CodeableConcept::default()
            }
        }
    }

    proptest! {
        #[test]
        fn string_get_put(source in text_with(0..1)) {
            let view: FhirString = TextLens.get(&source).expect("the value travels");
            let back = TextLens.put(&view, Some(&source)).expect("the string carries a value");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn string_put_get(text in "[A-Za-z ]{1,12}") {
            let view = FhirString::from(text);
            let source: DvTextData = TextLens.put(&view, None).expect("the string carries a value");
            let back: FhirString = TextLens.get(&source).expect("the value travels");
            assert_eq!(back, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn codeable_concept_get_put(source in text_with(0..3)) {
            let view: CodeableConcept = TextLens.get(&source).expect("the generated subset is carried");
            let back = TextLens.put(&view, Some(&source)).expect("the view carries a text");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn codeable_concept_put_get(view in codeable_concept()) {
            let source: DvTextData = TextLens.put(&view, None).expect("the generated subset is carried");
            let back: CodeableConcept = TextLens.get(&source).expect("the value is complete");
            assert_eq!(back, view, "PutGet failed for {view:?}");
        }

        #[test]
        fn a_concept_without_text_comes_back_with_text_added(
            codings in vec(coding(), 1..3),
        ) {
            let view = CodeableConcept { coding: codings, ..CodeableConcept::default() };
            let source: DvTextData = TextLens.put(&view, None).expect("the first coding names the value");
            let back: CodeableConcept = TextLens.get(&source).expect("the value is complete");
            assert_eq!(back.coding, view.coding, "the codings come back unchanged");
            assert_eq!(
                back.text.map(|text| text.value),
                Some(Some(source.value.clone())),
                "the rubric comes back as an added text"
            );
            assert_eq!(
                TextLens.fallbacks(&view),
                vec![Fallback::new(CODEABLE_CONCEPT_CELL, "DV_TEXT.value")],
                "the one-way row is recorded"
            );
        }

        #[test]
        fn coding_get_put(source in text_with(1..2)) {
            let view: Coding = TextLens.get(&source).expect("one mapping is one coding");
            let back = TextLens.put(&view, Some(&source)).expect("the coding carries a display");
            assert_eq!(back, source, "GetPut failed for {source:?}");
        }

        #[test]
        fn coding_put_get(source in text_with(1..2)) {
            let view: Coding = TextLens.get(&source).expect("one mapping is one coding");
            let back: DvTextData = TextLens.put(&view, None).expect("the coding carries a display");
            let again: Coding = TextLens.get(&back).expect("one mapping is one coding");
            assert_eq!(again, view, "PutGet failed for {view:?}");
        }
    }

    #[test]
    fn a_text_with_two_mappings_has_no_single_coding() {
        let source = DvTextData {
            value: String::from("Fever"),
            hyperlink: None,
            formatting: None,
            mappings: NonEmptyVec::new(vec![
                TermMapping {
                    r#match: '=',
                    purpose: None,
                    target: CodePhrase {
                        terminology_id: terminology_id("http://loinc.org", ""),
                        code_string: String::from("LP74849-8"),
                        preferred_term: None,
                    },
                },
                TermMapping {
                    r#match: '=',
                    purpose: None,
                    target: CodePhrase {
                        terminology_id: terminology_id("http://snomed.info/sct", ""),
                        code_string: String::from("386661006"),
                        preferred_term: None,
                    },
                },
            ])
            .ok(),
            language: None,
            encoding: None,
        };
        let error = <TextLens as Lens<DvTextData, Coding>>::get(&TextLens, &source)
            .expect_err("two mappings do not fit one coding");
        assert!(
            error.to_string().contains(CODING_CELL),
            "the refusal names the cell: {error}"
        );
    }

    #[test]
    fn a_display_only_coding_leaves_the_mappings_empty() {
        let view = Coding {
            display: Some(FhirString::from("Fever")),
            ..Coding::default()
        };
        let source: DvTextData = TextLens
            .put(&view, None)
            .expect("the coding carries a display");
        assert_eq!(source.value, "Fever", "the display fills the value");
        assert!(
            source.mappings.is_none(),
            "a coding with no code annotates nothing"
        );
    }

    /// A `TERM_MAPPING` with `relation` to `code` of `system`.
    fn term(relation: char, system: &str, code: &str) -> TermMapping {
        TermMapping {
            r#match: relation,
            purpose: None,
            target: CodePhrase {
                terminology_id: terminology_id(system, ""),
                code_string: String::from(code),
                preferred_term: None,
            },
        }
    }

    /// A `DV_TEXT` annotated with `mappings`.
    fn annotated(mappings: Vec<TermMapping>) -> DvTextData {
        DvTextData {
            value: String::from("Fever"),
            hyperlink: None,
            formatting: None,
            mappings: NonEmptyVec::new(mappings).ok(),
            language: None,
            encoding: None,
        }
    }

    /// Returns the codes a list of codings carries.
    fn codes(codings: &[Coding]) -> Vec<Option<&str>> {
        codings
            .iter()
            .map(|coding| coding.code.as_ref().and_then(|code| code.value.as_deref()))
            .collect()
    }

    // TERM_MAPPING.adoc: `match` is "hard coded to `=`, other operators not mappable
    // to FHIR", so the mapping writes no coding and the rest of the value converts.
    #[test]
    fn a_concept_leaves_out_a_term_mapping_fhir_cannot_carry() {
        for relation in ['>', '<', '?'] {
            let source = annotated(vec![
                term(relation, "http://snomed.info/sct", "386661006"),
                term('=', "http://loinc.org", "LP74849-8"),
            ]);
            let view: CodeableConcept = TextLens.get(&source).expect("the mapping is withheld");
            assert_eq!(
                codes(&view.coding),
                [Some("LP74849-8")],
                "only the `=` mapping travels past `{relation}`"
            );
            let taken = Lens::<DvTextData, CodeableConcept>::withheld(&TextLens, &source);
            let rows: Vec<(char, &str, &str)> = taken
                .iter()
                .map(|term| (term.match_code(), term.terminology(), term.code()))
                .collect();
            assert_eq!(rows, [(relation, "http://snomed.info/sct", "386661006")]);
            let back = TextLens
                .put(&view, Some(&source))
                .expect("the view carries a text");
            assert_eq!(back, source, "GetPut holds past `{relation}`");
        }
    }

    #[test]
    fn a_coding_leaves_out_a_term_mapping_fhir_cannot_carry() {
        for relation in ['>', '<', '?'] {
            let source = annotated(vec![
                term('=', "http://loinc.org", "LP74849-8"),
                term(relation, "http://snomed.info/sct", "386661006"),
            ]);
            let view: Coding = TextLens
                .get(&source)
                .expect("one `=` mapping is one coding");
            assert_eq!(
                codes(core::slice::from_ref(&view)),
                [Some("LP74849-8")],
                "the `=` mapping is the coding past `{relation}`"
            );
            assert_eq!(
                Lens::<DvTextData, Coding>::withheld(&TextLens, &source).len(),
                1,
                "the `{relation}` mapping is withheld"
            );
            let back = TextLens
                .put(&view, Some(&source))
                .expect("the coding carries a display");
            assert_eq!(back, source, "GetPut holds past `{relation}`");
        }
    }

    #[test]
    fn a_coding_of_only_a_withheld_mapping_carries_the_rubric() {
        let source = annotated(vec![term('<', "http://snomed.info/sct", "386661006")]);
        let view: Coding = TextLens.get(&source).expect("the mapping is withheld");
        assert_eq!(view.code, None, "no code travels");
        assert_eq!(
            view.display
                .as_ref()
                .and_then(|display| display.value.as_deref()),
            Some("Fever"),
            "the rubric is the display"
        );
        let back = TextLens
            .put(&view, Some(&source))
            .expect("the coding carries a display");
        assert_eq!(back, source, "the held mapping returns");
    }

    #[test]
    fn equivalent_term_mappings_are_withheld_from_nothing() {
        let source = annotated(vec![
            term('=', "http://snomed.info/sct", "386661006"),
            term('=', "http://loinc.org", "LP74849-8"),
        ]);
        let view: CodeableConcept = TextLens.get(&source).expect("both mappings travel");
        assert_eq!(codes(&view.coding), [Some("386661006"), Some("LP74849-8")]);
        assert!(Lens::<DvTextData, CodeableConcept>::withheld(&TextLens, &source).is_empty());
    }
    /// A `CodeableConcept` with no `text` whose first coding carries `display`
    /// and `code` as given, followed by a second, fully described coding.
    fn untexted(display: Option<&str>, code: Option<&str>) -> CodeableConcept {
        CodeableConcept {
            coding: vec![
                Coding {
                    system: Some(Uri::from("http://snomed.info/sct")),
                    code: code.map(Code::from),
                    display: display.map(FhirString::from),
                    ..Coding::default()
                },
                Coding {
                    system: Some(Uri::from("http://snomed.info/sct")),
                    code: Some(Code::from("386661006")),
                    display: Some(FhirString::from("Second")),
                    ..Coding::default()
                },
            ],
            ..CodeableConcept::default()
        }
    }

    #[test]
    fn the_text_wins_over_the_first_display() {
        let view = CodeableConcept {
            text: Some(FhirString::from("Fever")),
            ..untexted(Some("Pyrexia"), Some("386661006"))
        };
        let source: DvTextData = TextLens.put(&view, None).expect("the text names the value");
        assert_eq!(source.value, "Fever", "the text fills the value");
        assert!(
            <TextLens as Lens<DvTextData, CodeableConcept>>::fallbacks(&TextLens, &view).is_empty(),
            "a text takes no fallback"
        );
    }

    #[test]
    fn a_missing_text_falls_back_to_the_first_display() {
        let view = untexted(Some("Pyrexia"), Some("386661006"));
        let source: DvTextData = TextLens
            .put(&view, None)
            .expect("the display names the value");
        assert_eq!(source.value, "Pyrexia", "the first display fills the value");
    }

    #[test]
    fn a_missing_display_falls_back_to_the_first_code() {
        let view = untexted(None, Some("39065001"));
        let source: DvTextData = TextLens.put(&view, None).expect("the code names the value");
        assert_eq!(source.value, "39065001", "the first code fills the value");
        assert_eq!(
            TextLens.fallbacks(&view),
            vec![Fallback::new(CODEABLE_CONCEPT_CELL, "DV_TEXT.value")],
            "the one-way row is recorded"
        );
    }

    #[test]
    fn a_concept_with_nothing_to_name_the_value_is_refused() {
        let view = untexted(None, None);
        let error = <TextLens as Lens<DvTextData, CodeableConcept>>::put(&TextLens, &view, None)
            .expect_err("no text, display or code on the first coding");
        assert!(
            matches!(
                error,
                LensError::Missing {
                    cell: CODEABLE_CONCEPT_CELL,
                    attribute: "DV_TEXT.value",
                    ..
                }
            ),
            "the refusal names the cell and the attribute: {error}"
        );
    }
}
