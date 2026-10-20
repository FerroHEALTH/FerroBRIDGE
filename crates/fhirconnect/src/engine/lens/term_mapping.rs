// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `TERM_MAPPING` against FHIR `Coding`, the shared half of the text cells.
//!
//! `DV_CODED_TEXT` and `DV_TEXT` both carry their extra codings in
//! `mappings`, and both tables point at the same row set
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/TERM_MAPPING.adoc`):
//! `match` is "hard coded to `=`, other operators not mappable to FHIR", and
//! `target` is a `CODE_PHRASE` read through the `Coding` cell.
//!
//! A mapping whose `match` is not `=` is left out of the FHIR element: a
//! `Coding` beside the others would state an equivalence the data denies. The
//! cell reports it as a [`Withheld`] term, which the engine records as a
//! declared loss, and the rest of the value converts. Going back,
//! [`mappings_of`] keeps every such mapping the target already holds in its
//! place, because FHIR has no counterpart for it.
//!
//! The `DV_TEXT` against `string` and `DV_CODED_TEXT` against `Coding` pairs
//! give `mappings` no FHIR row at all (`DV_TEXT.adoc` §String,
//! `DV_CODED_TEXT.adoc` §Coding), so those cells write none of them and
//! report each one through [`uncarried`].

use fhir_types::r4::coding::Coding;
use openehr_rm::v1_2::data_types::text::term_mapping::TermMapping;

use crate::engine::lens::Lens;
use crate::engine::lens::LensError;
use crate::engine::lens::code_phrase::CodePhraseLens;
use crate::model::ast::keyword::Direction;

/// The `match` operator a mapping FHIR can carry writes.
pub const EQUIVALENT: char = '=';

/// Why a cell left a `TERM_MAPPING` out of the FHIR element it wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cause {
    /// The `match` is `>`, `<` or `?`, which no `Coding` can state.
    Match,
    /// The FHIR element has no `coding` list, so no mapping is written.
    Uncarried,
}

/// One `TERM_MAPPING` a cell left out of the FHIR element it wrote.
///
/// Either its `match` is `>`, `<` or `?` (openEHR RM 1.1.0
/// `TERM_MAPPING.match`), or the element the cell writes carries no coding
/// at all. The term is named by its terminology and its code, as the value
/// holds them, so a consumer can act on it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Withheld {
    match_code: char,
    terminology: String,
    code: String,
    cause: Cause,
}

impl Withheld {
    /// Returns the withheld term of `mapping`, or `None` when its `match` is
    /// `=` and FHIR carries it.
    #[must_use]
    pub fn of(mapping: &TermMapping) -> Option<Self> {
        (mapping.r#match != EQUIVALENT).then(|| Self::with_cause(mapping, Cause::Match))
    }

    /// Returns the term of `mapping` a cell whose FHIR element has no
    /// `coding` list leaves out, whatever its `match`.
    #[must_use]
    pub fn uncarried(mapping: &TermMapping) -> Self {
        Self::with_cause(mapping, Cause::Uncarried)
    }

    /// Returns the term of `mapping` left out for `cause`.
    fn with_cause(mapping: &TermMapping, cause: Cause) -> Self {
        Self {
            match_code: mapping.r#match,
            terminology: mapping.target.terminology_id.value.clone(),
            code: mapping.target.code_string.clone(),
            cause,
        }
    }

    /// Returns the `match` operator of the mapping.
    #[must_use]
    pub const fn match_code(&self) -> char {
        self.match_code
    }

    /// Returns why the cell left the mapping out.
    #[must_use]
    pub const fn cause(&self) -> Cause {
        self.cause
    }

    /// Returns the `terminology_id` value of the mapping's target.
    #[must_use]
    pub fn terminology(&self) -> &str {
        &self.terminology
    }

    /// Returns the `code_string` of the mapping's target.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }
}

/// Returns the mappings of `mappings` FHIR carries as a `Coding`, in order.
pub fn equivalents<'held>(
    mappings: impl IntoIterator<Item = &'held TermMapping>,
) -> impl Iterator<Item = &'held TermMapping> {
    mappings
        .into_iter()
        .filter(|mapping| mapping.r#match == EQUIVALENT)
}

/// Returns the terms of `mappings` a cell leaves out of the FHIR element, in
/// the order the value holds them.
#[must_use]
pub fn withheld<'held>(mappings: impl IntoIterator<Item = &'held TermMapping>) -> Vec<Withheld> {
    mappings.into_iter().filter_map(Withheld::of).collect()
}

/// Returns every term of `mappings`, in the order the value holds them, for
/// a cell whose FHIR element carries no coding.
#[must_use]
pub fn uncarried<'held>(mappings: impl IntoIterator<Item = &'held TermMapping>) -> Vec<Withheld> {
    mappings.into_iter().map(Withheld::uncarried).collect()
}

/// Reads one `TERM_MAPPING` into the `Coding` it stands for.
///
/// A `match` other than `=` states a relation FHIR's `coding` list cannot
/// express, so carrying it would assert an equivalence the data denies. The
/// text cells pass only the mappings [`equivalents`] admits, and report the
/// others through [`withheld`].
///
/// # Errors
///
/// Returns [`LensError::Shape`] for a `match` other than `=`, and whatever
/// the `Coding` cell returns for the target.
pub fn coding_of(mapping: &TermMapping, cell: &'static str) -> Result<Coding, LensError> {
    if mapping.r#match != EQUIVALENT {
        return Err(LensError::Shape {
            cell,
            attribute: "TERM_MAPPING.match",
            found: mapping.r#match.to_string(),
            reason: "FHIR carries no relation beside a coding, so only `=` maps",
            direction: Direction::OpenehrToFhir,
        });
    }
    CodePhraseLens.get(&mapping.target)
}

/// Writes one `Coding` back into the `TERM_MAPPING` it came from.
///
/// `purpose` has no FHIR counterpart, so it is taken from the mapping the
/// target already holds at this position.
///
/// # Errors
///
/// Returns whatever the `Coding` cell returns for the target.
pub fn mapping_of(
    coding: &Coding,
    existing: Option<&TermMapping>,
) -> Result<TermMapping, LensError> {
    Ok(TermMapping {
        r#match: EQUIVALENT,
        purpose: existing.and_then(|mapping| mapping.purpose.clone()),
        target: CodePhraseLens.put(coding, existing.map(|mapping| &mapping.target))?,
    })
}

/// Writes `codings` back into the mappings the target already holds.
///
/// Each coding takes the place of the next `=` mapping `held` carries, whose
/// `purpose` it keeps, and a coding beyond them is appended. A held mapping
/// whose `match` is not `=` stays where it was, since FHIR has no counterpart
/// for it, and a held `=` mapping no coding replaces is dropped.
///
/// # Errors
///
/// Returns whatever the `Coding` cell returns for a target.
pub fn mappings_of<'view>(
    codings: impl IntoIterator<Item = &'view Coding>,
    held: Option<&[TermMapping]>,
) -> Result<Vec<TermMapping>, LensError> {
    let mut codings = codings.into_iter();
    let mut written = Vec::new();
    for mapping in held.unwrap_or_default() {
        if mapping.r#match != EQUIVALENT {
            written.push(mapping.clone());
        } else if let Some(coding) = codings.next() {
            written.push(mapping_of(coding, Some(mapping))?);
        }
    }
    for coding in codings {
        written.push(mapping_of(coding, None)?);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::Withheld;
    use super::coding_of;
    use super::equivalents;
    use super::mapping_of;
    use super::mappings_of;
    use super::withheld;
    use crate::engine::lens::LensError;
    use crate::engine::lens::code_phrase::terminology_id;
    use fhir_types::r4::coding::Coding;
    use fhir_types::r4::primitives::Code;
    use fhir_types::r4::primitives::Uri;
    use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
    use openehr_rm::v1_2::data_types::text::term_mapping::TermMapping;

    /// A mapping carrying `match`, for the two cases below.
    fn mapping(relation: char) -> TermMapping {
        mapping_to(relation, "http://snomed.info/sct", "386661006")
    }

    /// A mapping carrying `match` to `code` of `system`.
    fn mapping_to(relation: char, system: &str, code: &str) -> TermMapping {
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

    /// A coding of `system` carrying `code`.
    fn code_of(system: &str, code: &str) -> Coding {
        Coding {
            system: Some(Uri::from(system)),
            code: Some(Code::from(code)),
            ..Coding::default()
        }
    }

    #[test]
    fn an_equivalent_mapping_is_one_coding() {
        let view = coding_of(&mapping('='), "test").expect("an equivalent mapping");
        assert_eq!(
            view.code.as_ref().and_then(|code| code.value.as_deref()),
            Some("386661006"),
            "the target code travels into the coding"
        );
    }

    #[test]
    fn a_narrower_mapping_is_refused() {
        let error = coding_of(&mapping('<'), "test").expect_err("a narrower relation");
        assert!(
            matches!(error, LensError::Shape { attribute, .. } if attribute == "TERM_MAPPING.match"),
            "the refusal names the operator: {error}"
        );
    }

    #[test]
    fn an_equivalent_mapping_is_withheld_from_nothing() {
        assert_eq!(Withheld::of(&mapping('=')), None, "FHIR carries `=`");
    }

    #[test]
    fn each_relation_fhir_cannot_carry_is_withheld_with_its_term() {
        for relation in ['>', '<', '?'] {
            let held = mapping_to(relation, "http://loinc.org", "LP74849-8");
            let taken = Withheld::of(&held).expect("only `=` maps");
            assert_eq!(taken.match_code(), relation, "the operator is kept");
            assert_eq!(taken.terminology(), "http://loinc.org");
            assert_eq!(taken.code(), "LP74849-8");
        }
    }

    #[test]
    fn the_two_filters_split_the_mappings_in_order() {
        let held = [
            mapping_to('=', "http://loinc.org", "A"),
            mapping_to('<', "http://loinc.org", "B"),
            mapping_to('=', "http://loinc.org", "C"),
            mapping_to('?', "http://loinc.org", "D"),
        ];
        let carried: Vec<&str> = equivalents(&held)
            .map(|mapping| mapping.target.code_string.as_str())
            .collect();
        assert_eq!(carried, ["A", "C"], "the `=` mappings travel in order");
        let taken = withheld(&held);
        let left: Vec<(char, &str)> = taken
            .iter()
            .map(|taken| (taken.match_code(), taken.code()))
            .collect();
        assert_eq!(
            left,
            [('<', "B"), ('?', "D")],
            "the others are withheld in order"
        );
    }

    #[test]
    fn a_written_mapping_is_always_equivalent_and_keeps_its_purpose() {
        let existing = TermMapping {
            purpose: Some(
                openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText {
                    value: String::from("billing"),
                    hyperlink: None,
                    formatting: None,
                    mappings: None,
                    language: None,
                    encoding: None,
                    defining_code: CodePhrase {
                        terminology_id: terminology_id("http://example.org/purpose", ""),
                        code_string: String::from("bill"),
                        preferred_term: None,
                    },
                },
            ),
            ..mapping('=')
        };
        let view = code_of("http://loinc.org", "LP74849-8");
        let written = mapping_of(&view, Some(&existing)).expect("the coding is complete");
        assert_eq!(written.r#match, '=', "the match is hard coded to `=`");
        assert_eq!(
            written.purpose, existing.purpose,
            "purpose has no FHIR counterpart and is carried"
        );
    }

    #[test]
    fn a_held_mapping_fhir_cannot_carry_keeps_its_place_going_back() {
        let held = [
            mapping_to('=', "http://loinc.org", "A"),
            mapping_to('>', "http://loinc.org", "B"),
            mapping_to('=', "http://loinc.org", "C"),
        ];
        let codings = [
            code_of("http://loinc.org", "A2"),
            code_of("http://loinc.org", "C2"),
            code_of("http://loinc.org", "E2"),
        ];
        let written = mappings_of(&codings, Some(&held)).expect("complete codings");
        let rows: Vec<(char, &str)> = written
            .iter()
            .map(|mapping| (mapping.r#match, mapping.target.code_string.as_str()))
            .collect();
        assert_eq!(
            rows,
            [('=', "A2"), ('>', "B"), ('=', "C2"), ('=', "E2")],
            "each coding replaces the next `=` mapping and the `>` one stays"
        );
    }

    #[test]
    fn a_held_equivalent_no_coding_replaces_is_dropped() {
        let held = [
            mapping_to('=', "http://loinc.org", "A"),
            mapping_to('?', "http://loinc.org", "B"),
            mapping_to('=', "http://loinc.org", "C"),
        ];
        let written = mappings_of(&[], Some(&held)).expect("no coding to write");
        let rows: Vec<(char, &str)> = written
            .iter()
            .map(|mapping| (mapping.r#match, mapping.target.code_string.as_str()))
            .collect();
        assert_eq!(rows, [('?', "B")], "only the withheld mapping is carried");
    }
}
