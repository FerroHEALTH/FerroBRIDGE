// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology server a `conceptmap` translates codes through.
//!
//! A mapping that names a `conceptmap`, or sits in a file whose `spec` names
//! one, has its codes "transformed using the conceptmap"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/concept-type/manual.adoc`,
//! §`ConceptMaps`). The engine makes no call of its own, so the translation
//! is a seam: [`Terminology::translate`] answers `ConceptMap/$translate`
//! (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>) with the
//! generated R4 operation contract, and [`NoTerminology`], the default,
//! refuses every request so a run with no configured server fails closed.
//!
//! The rules the engine applies to an answer live here too, so they are
//! tested apart from a run: only a match whose equivalence is `equivalent` or
//! `equal` counts (<https://hl7.org/fhir/R4/valueset-concept-map-equivalence.html>),
//! and the translated concept's own display wins over the source text.

use core::error::Error;
use core::fmt;

use fhir_types::r4::coding::Coding;
use fhir_types::r4::operations::code_system_lookup::CodeSystemLookupRequest;
use fhir_types::r4::operations::code_system_lookup::CodeSystemLookupResponse;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateRequest;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateResponse;
use fhir_types::r4::primitives;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Uri;

/// The terminology server a run translates codes through.
pub trait Terminology: fmt::Debug + Send + Sync {
    /// Answers `ConceptMap/$translate` for `request`.
    ///
    /// # Errors
    ///
    /// Returns [`TerminologyError`] when no server is configured or the
    /// server did not answer, which is a failure and never a missing
    /// translation.
    fn translate(
        &self,
        request: &ConceptMapTranslateRequest,
    ) -> Result<ConceptMapTranslateResponse, TerminologyError>;

    /// Answers `CodeSystem/$lookup` for the request, `None` when the server does
    /// not know the code.
    ///
    /// The engine asks for the display of a coding that carries none, because
    /// "the value is mandatory in openEHR" and a terminology server is the
    /// recommended source of it (`DV_CODED_TEXT.adoc`). The default refuses
    /// as [`TerminologyError::NotConfigured`].
    ///
    /// # Errors
    ///
    /// Returns [`TerminologyError`] when no server is configured or the
    /// server did not answer, which is a failure and never an unknown code.
    fn lookup(
        &self,
        _request: &CodeSystemLookupRequest,
    ) -> Result<Option<CodeSystemLookupResponse>, TerminologyError> {
        Err(TerminologyError::NotConfigured)
    }

    /// Returns whether a code the server does not know takes `coding.code`
    /// as its display rather than refusing the unit.
    ///
    /// `DV_CODED_TEXT.adoc` offers the code "alternatively", so the choice is
    /// the deployment's and the default refuses.
    fn display_fallback(&self) -> bool {
        false
    }
}

/// The seam with no terminology server behind it.
///
/// Every request is [`TerminologyError::NotConfigured`]: a code is never
/// passed through under a system it was not translated into.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoTerminology;

impl Terminology for NoTerminology {
    fn translate(
        &self,
        _request: &ConceptMapTranslateRequest,
    ) -> Result<ConceptMapTranslateResponse, TerminologyError> {
        Err(TerminologyError::NotConfigured)
    }
}

/// Why a code could not be translated.
#[derive(Debug, thiserror::Error)]
pub enum TerminologyError {
    /// The run carries no terminology server.
    #[error("no terminology server is configured to translate through")]
    NotConfigured,
    /// The server failed, or refused the operation.
    ///
    /// The source carries what the server answered, its status included.
    #[error("the terminology server did not translate the code")]
    Upstream {
        /// The failure the seam reported.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// The coding names no system or no code, so there is nothing to send.
    ///
    /// "If a code is provided, a system must be provided"
    /// (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>).
    #[error("the coding carries no system and code to translate")]
    Uncoded,
    /// The server answered more than one distinct accepted concept.
    #[error("the translation is ambiguous: {concepts} distinct concepts are equivalent")]
    Ambiguous {
        /// How many distinct concepts the accepted matches name.
        concepts: usize,
    },
}

/// Returns the `$translate` request for `coding` through the concept map
/// `url`.
///
/// The request carries the coding's own system, code and version, and the
/// map's canonical as `url`.
///
/// # Errors
///
/// Returns [`TerminologyError::Uncoded`] when the coding names no system or
/// no code.
pub fn request(url: &str, coding: &Coding) -> Result<ConceptMapTranslateRequest, TerminologyError> {
    let system = text(coding.system.as_ref().map(|uri| &uri.value));
    let code = text(coding.code.as_ref().map(|code| &code.value));
    let (Some(system), Some(code)) = (system, code) else {
        return Err(TerminologyError::Uncoded);
    };
    Ok(ConceptMapTranslateRequest {
        url: Some(Uri::from(url)),
        system: Some(Uri::from(system)),
        code: Some(Code::from(code)),
        version: text(coding.version.as_ref().map(|version| &version.value))
            .map(primitives::String::from),
        ..ConceptMapTranslateRequest::default()
    })
}

/// Returns the `$lookup` request for the display of `code` in `system`, at
/// `version` when the coding names one
/// (<https://hl7.org/fhir/R4/codesystem-operation-lookup.html>).
#[must_use]
pub fn lookup_request(system: &str, code: &str, version: Option<&str>) -> CodeSystemLookupRequest {
    CodeSystemLookupRequest {
        system: Some(Uri::from(system)),
        code: Some(Code::from(code)),
        version: version.map(primitives::String::from),
        ..CodeSystemLookupRequest::default()
    }
}

/// Returns the concept `response` translates to, `None` when it names none.
///
/// A match counts only when its equivalence is `equivalent` or `equal`: "the
/// list of matches may include notes of codes for which mapping is
/// specifically excluded, so implementers have to check the
/// `match.equivalence` for each match"
/// (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>), and every
/// other code of <https://hl7.org/fhir/R4/valueset-concept-map-equivalence.html>
/// states a target that does not mean what the source means. A `result` that
/// is not `true` translates to nothing.
///
/// # Errors
///
/// Returns [`TerminologyError::Ambiguous`] when the accepted matches name
/// more than one distinct concept.
pub fn accepted(
    response: &ConceptMapTranslateResponse,
) -> Result<Option<Coding>, TerminologyError> {
    if response.result.value != Some(true) {
        return Ok(None);
    }
    let mut concepts: Vec<&Coding> = Vec::new();
    for found in &response.r#match {
        let equivalence = text(found.equivalence.as_ref().map(|code| &code.value));
        if !matches!(equivalence, Some("equivalent" | "equal")) {
            continue;
        }
        // NOTE: R4 $translate gives `match.concept` 0..1, and an accepted match
        // naming no concept has nothing to write, so it translates to nothing.
        let Some(concept) = found.concept.as_ref() else {
            continue;
        };
        if !concepts.iter().any(|held| same_concept(held, concept)) {
            concepts.push(concept);
        }
    }
    // NOTE: R4 $translate allows "multiple matches of equal or differing
    // equivalence"; no specification picks one, so two refuse: our own design.
    match concepts.as_slice() {
        [] => Ok(None),
        [concept] => Ok(Some((*concept).clone())),
        more => Err(TerminologyError::Ambiguous {
            concepts: more.len(),
        }),
    }
}

/// Returns the coding a translation writes in place of `source`.
///
/// The translated concept carries its own system, version and code, and its
/// own display wins over the source's; the source display is kept only when
/// the concept names none. No specification governs the precedence: our own
/// design, over the R4 rule that the translated concept is "the translation
/// outcome".
#[must_use]
pub fn translated(source: &Coding, concept: &Coding) -> Coding {
    let display = concept
        .display
        .clone()
        .filter(|display| text(Some(&display.value)).is_some())
        .or_else(|| source.display.clone());
    Coding {
        system: concept.system.clone(),
        version: concept.version.clone(),
        code: concept.code.clone(),
        display,
        ..Coding::default()
    }
}

/// Returns whether two codings name the same concept.
fn same_concept(left: &Coding, right: &Coding) -> bool {
    text(left.system.as_ref().map(|uri| &uri.value))
        == text(right.system.as_ref().map(|uri| &uri.value))
        && text(left.code.as_ref().map(|code| &code.value))
            == text(right.code.as_ref().map(|code| &code.value))
}

/// Returns the text a primitive carries, `None` when it carries none.
fn text(value: Option<&Option<String>>) -> Option<&str> {
    value
        .and_then(Option::as_deref)
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::NoTerminology;
    use super::Terminology;
    use super::TerminologyError;
    use super::accepted;
    use super::request;
    use super::translated;
    use fhir_types::r4::coding::Coding;
    use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateRequest;
    use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateResponse;
    use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateResponseMatch;
    use fhir_types::r4::primitives;
    use fhir_types::r4::primitives::Boolean;
    use fhir_types::r4::primitives::Code;
    use fhir_types::r4::primitives::Uri;

    fn coding(system: &str, code: &str, display: Option<&str>) -> Coding {
        Coding {
            system: Some(Uri::from(system)),
            code: Some(Code::from(code)),
            display: display.map(primitives::String::from),
            ..Coding::default()
        }
    }

    fn found(equivalence: &str, concept: Coding) -> ConceptMapTranslateResponseMatch {
        ConceptMapTranslateResponseMatch {
            equivalence: Some(Code::from(equivalence)),
            concept: Some(concept),
            ..ConceptMapTranslateResponseMatch::default()
        }
    }

    fn answer(
        result: bool,
        matches: Vec<ConceptMapTranslateResponseMatch>,
    ) -> ConceptMapTranslateResponse {
        ConceptMapTranslateResponse {
            result: Boolean {
                value: Some(result),
                ..Boolean::default()
            },
            r#match: matches,
            ..ConceptMapTranslateResponse::default()
        }
    }

    #[test]
    fn no_terminology_fails_closed() {
        let outcome = NoTerminology.translate(&ConceptMapTranslateRequest::default());
        assert!(
            matches!(outcome, Err(TerminologyError::NotConfigured)),
            "a run with no server refuses: {outcome:?}"
        );
    }

    #[test]
    fn the_request_carries_the_source_system_code_and_version() {
        let mut source = coding("http://example.org/source", "S-1", Some("Source"));
        source.version = Some(primitives::String::from("2026"));
        let sent =
            request("http://example.org/ConceptMap/a", &source).expect("the coding is coded");
        assert_eq!(
            sent.url.and_then(|uri| uri.value).as_deref(),
            Some("http://example.org/ConceptMap/a")
        );
        assert_eq!(
            sent.system.and_then(|uri| uri.value).as_deref(),
            Some("http://example.org/source")
        );
        assert_eq!(
            sent.code.and_then(|code| code.value).as_deref(),
            Some("S-1")
        );
        assert_eq!(
            sent.version.and_then(|version| version.value).as_deref(),
            Some("2026")
        );
        assert_eq!(sent.targetsystem, None, "the map alone names the target");
    }

    #[test]
    fn a_coding_with_no_system_is_refused_before_any_call() {
        let source = Coding {
            code: Some(Code::from("S-1")),
            ..Coding::default()
        };
        assert!(matches!(
            request("http://example.org/ConceptMap/a", &source),
            Err(TerminologyError::Uncoded)
        ));
    }

    #[test]
    fn only_equivalent_and_equal_count() {
        let target = coding("http://example.org/target", "T-1", Some("Target"));
        for code in ["equivalent", "equal"] {
            let concept = accepted(&answer(true, vec![found(code, target.clone())]))
                .expect("one concept is no ambiguity");
            assert_eq!(concept, Some(target.clone()), "{code} is a translation");
        }
        for code in [
            "relatedto",
            "wider",
            "subsumes",
            "narrower",
            "specializes",
            "inexact",
            "unmatched",
            "disjoint",
        ] {
            let concept = accepted(&answer(true, vec![found(code, target.clone())]))
                .expect("no accepted concept is no ambiguity");
            assert_eq!(concept, None, "{code} is no translation");
        }
    }

    #[test]
    fn a_false_result_translates_to_nothing() {
        let target = coding("http://example.org/target", "T-1", None);
        let concept = accepted(&answer(false, vec![found("equivalent", target)]))
            .expect("a false result is no ambiguity");
        assert_eq!(concept, None);
    }

    #[test]
    fn two_distinct_accepted_concepts_are_ambiguous() {
        let first = coding("http://example.org/target", "T-1", None);
        let second = coding("http://example.org/target", "T-2", None);
        let outcome = accepted(&answer(
            true,
            vec![
                found("equivalent", first.clone()),
                found("equal", second),
                found("equal", first),
            ],
        ));
        assert!(
            matches!(outcome, Err(TerminologyError::Ambiguous { concepts: 2 })),
            "two concepts refuse: {outcome:?}"
        );
    }

    #[test]
    fn the_translated_display_wins_over_the_source_display() {
        let source = coding("http://example.org/source", "S-1", Some("Source text"));
        let concept = coding("http://example.org/target", "T-1", Some("Target display"));
        let written = translated(&source, &concept);
        assert_eq!(written, concept);
        let bare = coding("http://example.org/target", "T-1", None);
        assert_eq!(
            translated(&source, &bare).display,
            Some(primitives::String::from("Source text")),
            "the source text stands in only when the concept names no display"
        );
    }
}
