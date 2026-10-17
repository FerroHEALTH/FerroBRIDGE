// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIRconnect terminology seam, served by the configured terminology
//! server.
//!
//! A mapping that names a `conceptmap` has its codes "transformed using the
//! conceptmap" (`types-of-mappings/concept-type/manual.adoc`, §`ConceptMaps`),
//! and the engine asks [`Terminology::translate`] for each distinct code. The
//! seam is synchronous and [`Client`] is async, so [`Translator::run`]
//! answers the engine from codes it resolved before the run: a run that meets
//! a code not yet resolved is discarded, the codes it met are resolved
//! through `$translate`, and the run is repeated. Every run adds at least one
//! code, so the repetition ends. No specification governs this: our own
//! design, which keeps every blocking wait off the runtime.
//!
//! FHIRconnect names no direction for a concept map. A map is read as
//! authored from openEHR to FHIR: the FHIR-bound run translates forward, and
//! the openEHR-bound run sends `reverse=true`, "return all the codes that
//! might be mapped to this code"
//! (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>).
//!
//! With no `[terminology]` server the engine keeps its default,
//! [`NoTerminology`], which refuses every code: a code is never passed
//! through under a system it was not translated into.

use core::error::Error;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use ferrobridge_term::client::Client;
use ferrobridge_term::concept::{Concept, Match};
use ferrobridge_term::outcome::{BatchOutcome, Request, TranslateOutcome, Translation};
use fhir_types::r4::coding::Coding;
use fhir_types::r4::operations::concept_map_translate::{
    ConceptMapTranslateRequest, ConceptMapTranslateResponse, ConceptMapTranslateResponseMatch,
};
use fhir_types::r4::primitives::{Boolean, Canonical, Code, String as FhirString, Uri};
use fhirconnect::engine::terminology::{NoTerminology, Terminology, TerminologyError};

/// The direction of one engine run, which decides `reverse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// A composition into FHIR: the map is read forward.
    ToFhir,
    /// FHIR into a composition: the map is read with `reverse=true`.
    ToOpenehr,
}

/// What the server answered for one code, shared across the repeated runs.
type Answer = Result<ConceptMapTranslateResponse, Arc<dyn Error + Send + Sync>>;

/// The terminology server the engine runs translate through, when one is
/// configured.
#[derive(Debug, Clone, Default)]
pub struct Translator {
    /// The client of the `[terminology]` server.
    client: Option<Client>,
}

impl Translator {
    /// Returns the translator over `client`, `None` when no `[terminology]`
    /// server is configured.
    #[must_use]
    pub const fn new(client: Option<Client>) -> Self {
        Self { client }
    }

    /// Returns whether a terminology server is configured.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    /// Runs `run` with every code it translates resolved first.
    ///
    /// `run` is called again after each round of `$translate` calls, so it
    /// must be a pure function of its input; only the result of the run that
    /// met no unresolved code is returned. With no server `run` is called
    /// once over [`NoTerminology`].
    ///
    /// # Errors
    ///
    /// Returns the error `run` returns. A code the server refused, or that
    /// the call never reached an answer for, reaches the engine as
    /// [`TerminologyError::Upstream`] carrying the client's error, so the run
    /// refuses with it.
    pub async fn run<R, E>(
        &self,
        direction: Direction,
        mut run: impl FnMut(&dyn Terminology) -> Result<R, E>,
    ) -> Result<R, E> {
        let Some(client) = self.client.as_ref() else {
            return run(&NoTerminology);
        };
        let mut answers: BTreeMap<Translation, Answer> = BTreeMap::new();
        settle(client, direction, &mut answers, &mut run).await
    }

    /// Resolves every code `probe` translates, for a final run over the
    /// [`Resolved`] seam this returns.
    ///
    /// This is [`Translator::run`] for a run with a side effect of its own,
    /// such as binding the identity of a created resource: `probe` repeats
    /// without it, and only the final run over [`Resolved`] has it.
    pub async fn resolve<R, E>(
        &self,
        direction: Direction,
        mut probe: impl FnMut(&dyn Terminology) -> Result<R, E>,
    ) -> Resolved {
        let mut answers: BTreeMap<Translation, Answer> = BTreeMap::new();
        if let Some(client) = self.client.as_ref() {
            let _probed: Result<R, E> = settle(client, direction, &mut answers, &mut probe).await;
        }
        Resolved {
            direction,
            configured: self.client.is_some(),
            answers,
        }
    }
}

/// Repeats `run` over the codes resolved so far, resolving the ones each
/// run meets, and returns the result of the first run that meets none.
async fn settle<R, E>(
    client: &Client,
    direction: Direction,
    answers: &mut BTreeMap<Translation, Answer>,
    run: &mut impl FnMut(&dyn Terminology) -> Result<R, E>,
) -> Result<R, E> {
    loop {
        let missing = {
            let prepared = Prepared {
                direction,
                answers,
                missing: Mutex::new(BTreeSet::new()),
            };
            let result = run(&prepared);
            let missing = prepared.missing();
            if missing.is_empty() {
                return result;
            }
            missing
        };
        resolve(client, missing, answers).await;
    }
}

/// The codes [`Translator::resolve`] resolved, as the seam of the final run.
#[derive(Debug)]
pub struct Resolved {
    /// The direction of the run.
    direction: Direction,
    /// Whether a terminology server is configured.
    configured: bool,
    /// Every code resolved.
    answers: BTreeMap<Translation, Answer>,
}

impl Terminology for Resolved {
    fn translate(
        &self,
        request: &ConceptMapTranslateRequest,
    ) -> Result<ConceptMapTranslateResponse, TerminologyError> {
        if !self.configured {
            return Err(TerminologyError::NotConfigured);
        }
        let translation = translation(self.direction, request)?;
        match self.answers.get(&translation) {
            Some(Ok(response)) => Ok(response.clone()),
            Some(Err(error)) => Err(TerminologyError::Upstream {
                source: Box::new(Arc::clone(error)),
            }),
            None => Err(TerminologyError::Upstream {
                source: Box::new(Unresolved),
            }),
        }
    }
}

/// A code the final run met that the probe runs never did.
#[derive(Debug, thiserror::Error)]
#[error("the run translated a code its probe runs never met")]
pub struct Unresolved;

/// The seam one run reads: the codes resolved so far, and the codes the run
/// met that are not.
#[derive(Debug)]
struct Prepared<'a> {
    /// The direction of the run.
    direction: Direction,
    /// Every code resolved so far.
    answers: &'a BTreeMap<Translation, Answer>,
    /// The codes this run met that are not resolved yet.
    missing: Mutex<BTreeSet<Translation>>,
}

impl Prepared<'_> {
    /// Returns the codes this run met that are not resolved yet.
    fn missing(self) -> BTreeSet<Translation> {
        self.missing
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Terminology for Prepared<'_> {
    fn translate(
        &self,
        request: &ConceptMapTranslateRequest,
    ) -> Result<ConceptMapTranslateResponse, TerminologyError> {
        let translation = translation(self.direction, request)?;
        match self.answers.get(&translation) {
            Some(Ok(response)) => Ok(response.clone()),
            Some(Err(error)) => Err(TerminologyError::Upstream {
                source: Box::new(Arc::clone(error)),
            }),
            None => {
                self.missing
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(translation);
                // NOTE: no specification governs this: our own design; this run
                // is discarded, and a false `result` lets it reach further codes.
                Ok(unresolved())
            }
        }
    }
}

/// Returns the `$translate` question `request` asks in `direction`.
///
/// # Errors
///
/// Returns [`TerminologyError::Uncoded`] when the request names no system or
/// no code, and [`TerminologyError::Upstream`] when it carries a parameter
/// this client does not send, which is refused rather than dropped.
fn translation(
    direction: Direction,
    request: &ConceptMapTranslateRequest,
) -> Result<Translation, TerminologyError> {
    if let Some(parameter) = uncarried(request) {
        return Err(TerminologyError::Upstream {
            source: Box::new(Uncarried { parameter }),
        });
    }
    let system = request.system.as_ref().and_then(|uri| uri.value.as_deref());
    let code = request.code.as_ref().and_then(|code| code.value.as_deref());
    let (Some(system), Some(code)) = (system, code) else {
        return Err(TerminologyError::Uncoded);
    };
    let mut translation = Translation::new(system, code);
    if let Some(version) = request
        .version
        .as_ref()
        .and_then(|text| text.value.as_deref())
    {
        translation = translation.with_version(version);
    }
    if let Some(url) = request.url.as_ref().and_then(|uri| uri.value.as_deref()) {
        translation = translation.with_concept_map(url);
    }
    if let Some(target) = request
        .targetsystem
        .as_ref()
        .and_then(|uri| uri.value.as_deref())
    {
        translation = translation.with_target_system(target);
    }
    let reverse = request
        .reverse
        .as_ref()
        .and_then(|reverse| reverse.value)
        .unwrap_or(false);
    if reverse || direction == Direction::ToOpenehr {
        translation = translation.with_reverse(true);
    }
    Ok(translation)
}

/// Returns the first parameter `request` sets that [`Translation`] cannot
/// carry.
fn uncarried(request: &ConceptMapTranslateRequest) -> Option<&'static str> {
    [
        ("conceptMap", request.concept_map.is_some()),
        ("conceptMapVersion", request.concept_map_version.is_some()),
        ("source", request.source.is_some()),
        ("coding", request.coding.is_some()),
        ("codeableConcept", request.codeable_concept.is_some()),
        ("target", request.target.is_some()),
        ("dependency", !request.dependency.is_empty()),
    ]
    .into_iter()
    .find_map(|(parameter, set)| set.then_some(parameter))
}

/// A `$translate` parameter the engine set that this client does not send.
#[derive(Debug, thiserror::Error)]
#[error("the terminology client does not send the $translate parameter {parameter}")]
pub struct Uncarried {
    /// The parameter, as the operation names it.
    parameter: &'static str,
}

/// An answer that names no translation.
fn unresolved() -> ConceptMapTranslateResponse {
    ConceptMapTranslateResponse {
        result: Boolean::from(false),
        ..ConceptMapTranslateResponse::default()
    }
}

/// Resolves every code of `missing` and records the answers.
///
/// One code is one call; several are one `batch`, answered by position. A
/// batch that failed as a whole is the answer of every code it carried.
async fn resolve(
    client: &Client,
    missing: BTreeSet<Translation>,
    answers: &mut BTreeMap<Translation, Answer>,
) {
    let missing: Vec<Translation> = missing.into_iter().collect();
    if let [one] = missing.as_slice() {
        let answer = client
            .translate(one)
            .await
            .map_err(shared)
            .and_then(|outcome| response(&outcome));
        answers.insert(one.clone(), answer);
        return;
    }
    let requests: Vec<Request> = missing.iter().cloned().map(Request::Translate).collect();
    match client.batch(&requests).await {
        Ok(entries) => {
            for (translation, entry) in missing.into_iter().zip(entries) {
                let answer = match entry {
                    Ok(BatchOutcome::Translate(outcome)) => response(&outcome),
                    Ok(_) => Err(shared(UnexpectedAnswer)),
                    Err(error) => Err(shared(error)),
                };
                answers.insert(translation, answer);
            }
        }
        Err(error) => {
            let error = shared(error);
            for translation in missing {
                answers.insert(translation, Err(Arc::clone(&error)));
            }
        }
    }
}

/// Returns `error` in the form every run that meets its code shares.
fn shared(error: impl Error + Send + Sync + 'static) -> Arc<dyn Error + Send + Sync> {
    Arc::new(error)
}

/// A batch entry answered with an outcome of another operation.
#[derive(Debug, thiserror::Error)]
#[error("the terminology server answered a $translate entry with another operation's outcome")]
pub struct UnexpectedAnswer;

/// Returns the R4 `$translate` answer `outcome` carries.
///
/// The client carries each match's equivalence, concept and source; `product`
/// and the other `out` parts the client does not decode are absent here, and
/// the engine reads none of them.
fn response(outcome: &TranslateOutcome) -> Answer {
    match outcome {
        TranslateOutcome::Translated { matches, message } => Ok(ConceptMapTranslateResponse {
            result: Boolean::from(true),
            message: message.as_deref().map(FhirString::from),
            r#match: matches.iter().map(matched).collect(),
            ..ConceptMapTranslateResponse::default()
        }),
        TranslateOutcome::NoMatch { message } => Ok(ConceptMapTranslateResponse {
            result: Boolean::from(false),
            message: message.as_deref().map(FhirString::from),
            ..ConceptMapTranslateResponse::default()
        }),
        _ => Err(shared(UnexpectedAnswer)),
    }
}

/// Returns one R4 `match` part.
fn matched(found: &Match) -> ConceptMapTranslateResponseMatch {
    ConceptMapTranslateResponseMatch {
        equivalence: found
            .equivalence
            .as_ref()
            .map(|equivalence| Code::from(equivalence.as_str())),
        concept: found.concept.as_ref().map(coding),
        source: found.source.as_deref().map(Canonical::from),
        ..ConceptMapTranslateResponseMatch::default()
    }
}

/// Returns the R4 `Coding` of `concept`.
fn coding(concept: &Concept) -> Coding {
    Coding {
        system: concept.system.as_deref().map(Uri::from),
        version: concept.version.as_deref().map(FhirString::from),
        code: concept.code.as_deref().map(Code::from),
        display: concept.display.as_deref().map(FhirString::from),
        ..Coding::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{Direction, Translator, translation};
    use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateRequest;
    use fhir_types::r4::primitives::{Code, Uri};
    use fhirconnect::engine::terminology::{Terminology, TerminologyError};

    fn request() -> ConceptMapTranslateRequest {
        ConceptMapTranslateRequest {
            url: Some(Uri::from("http://example.org/ConceptMap/a")),
            system: Some(Uri::from("http://example.org/source")),
            code: Some(Code::from("S-1")),
            ..ConceptMapTranslateRequest::default()
        }
    }

    #[test]
    fn the_fhir_bound_direction_reads_the_map_forward() {
        let sent = translation(Direction::ToFhir, &request()).expect("a coded request");
        assert_eq!(sent.reverse, None);
        assert_eq!(
            sent.concept_map_url.as_deref(),
            Some("http://example.org/ConceptMap/a")
        );
        assert_eq!(sent.target_system, None);
    }

    #[test]
    fn the_openehr_bound_direction_sets_reverse() {
        let sent = translation(Direction::ToOpenehr, &request()).expect("a coded request");
        assert_eq!(sent.reverse, Some(true));
    }

    #[test]
    fn a_parameter_the_client_cannot_send_is_refused() {
        let mut asked = request();
        asked.target = Some(Uri::from("http://example.org/ValueSet/t"));
        let refused = translation(Direction::ToFhir, &asked);
        assert!(
            matches!(&refused, Err(TerminologyError::Upstream { source }) if source.to_string().contains("target")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_request_with_no_code_is_uncoded() {
        let mut asked = request();
        asked.code = None;
        assert!(matches!(
            translation(Direction::ToFhir, &asked),
            Err(TerminologyError::Uncoded)
        ));
    }

    #[tokio::test]
    async fn no_server_runs_once_over_the_refusing_default() {
        let translator = Translator::default();
        let mut runs = 0;
        let outcome = translator
            .run(Direction::ToFhir, |terminology| {
                runs += 1;
                terminology.translate(&request())
            })
            .await;
        assert_eq!(runs, 1);
        assert!(matches!(outcome, Err(TerminologyError::NotConfigured)));
    }

    #[tokio::test]
    async fn no_server_resolves_nothing_and_the_final_run_refuses() {
        let translator = Translator::default();
        let mut probes = 0;
        let resolved = translator
            .resolve(Direction::ToFhir, |terminology| {
                probes += 1;
                terminology.translate(&request())
            })
            .await;
        assert_eq!(probes, 0, "with no server there is nothing to probe for");
        assert!(matches!(
            resolved.translate(&request()),
            Err(TerminologyError::NotConfigured)
        ));
    }
}
