// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIRconnect terminology seam, served by the configured terminology
//! server.
//!
//! A mapping that names a `conceptmap` has its codes "transformed using the
//! conceptmap" (`types-of-mappings/concept-type/manual.adoc`, §`ConceptMaps`),
//! and the engine asks [`Terminology::translate`] for each distinct code. A
//! coding that reaches a `DV_CODED_TEXT` with no display has its display
//! resolved through [`Terminology::lookup`], as `DV_CODED_TEXT.adoc`
//! recommends "since the value is mandatory in openEHR". The seam is
//! synchronous and [`Client`] is async, so [`Translator::run`] answers the
//! engine from questions it resolved before the run: a run that meets a
//! question not yet resolved is discarded, the questions it met are sent
//! (`$translate` and `$lookup`, one call or one `batch`), and the run is
//! repeated. Every run adds at least one question, so the repetition ends.
//! No specification governs this: our own design, which keeps every blocking
//! wait off the runtime.
//!
//! FHIRconnect names no direction for a concept map. A map is read as
//! authored from openEHR to FHIR: the FHIR-bound run translates forward, and
//! the openEHR-bound run sends `reverse=true`, "return all the codes that
//! might be mapped to this code"
//! (<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>).
//!
//! With no `[terminology]` server the engine keeps its default,
//! [`NoTerminology`], which refuses every code: a code is never passed
//! through under a system it was not translated into, and a missing display
//! keeps the code, the page's alternative "if a term server is not an
//! option".

pub mod fetch;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, PoisonError};

use ferrobridge_term::client::Client;
use ferrobridge_term::outcome::Translation;
use fhir_types::r4::operations::code_system_lookup::{
    CodeSystemLookupRequest, CodeSystemLookupResponse,
};
use fhir_types::r4::operations::concept_map_translate::{
    ConceptMapTranslateRequest, ConceptMapTranslateResponse,
};
use fhir_types::r4::primitives;
use fhir_types::r4::primitives::Boolean;
use fhirconnect::engine::terminology::{NoTerminology, Terminology, TerminologyError};

use crate::terminology::fetch::{Answer, Displayed, Lookup, fetch};

/// The direction of one engine run, which decides `reverse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// A composition into FHIR: the map is read forward.
    ToFhir,
    /// FHIR into a composition: the map is read with `reverse=true`.
    ToOpenehr,
}

/// The terminology server the engine runs translate through, when one is
/// configured.
#[derive(Debug, Clone, Default)]
pub struct Translator {
    /// The client of the `[terminology]` server.
    client: Option<Client>,
    /// Whether a code the server does not know takes its code as display.
    display_fallback: bool,
}

impl Translator {
    /// Returns the translator over `client`, `None` when no `[terminology]`
    /// server is configured.
    #[must_use]
    pub const fn new(client: Option<Client>) -> Self {
        Self {
            client,
            display_fallback: false,
        }
    }

    /// Returns this translator with `display_fallback` as the answer to
    /// [`Terminology::display_fallback`]: whether a code the server does not
    /// know takes `coding.code` as its display, which `DV_CODED_TEXT.adoc`
    /// offers "alternatively". The default refuses the unit.
    #[must_use]
    pub const fn with_display_fallback(mut self, display_fallback: bool) -> Self {
        self.display_fallback = display_fallback;
        self
    }

    /// Returns whether a terminology server is configured.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    /// Returns a session that remembers every answer it receives, for the
    /// runs of one request.
    ///
    /// A request that runs a mapping over several entries, such as a search
    /// page and its total, opens one session, so a code repeated across them
    /// is asked once. The session is dropped with the request.
    #[must_use]
    pub fn session(&self) -> Session<'_> {
        Session {
            client: self.client.as_ref(),
            display_fallback: self.display_fallback,
            answers: Mutex::new(BTreeMap::new()),
            displays: Mutex::new(BTreeMap::new()),
        }
    }

    /// Runs `run` with every code it translates or looks up resolved first,
    /// in a session of its own ([`Session::run`]).
    ///
    /// # Errors
    ///
    /// Returns the error `run` returns, as [`Session::run`] does.
    pub async fn run<R, E>(
        &self,
        direction: Direction,
        run: impl FnMut(&dyn Terminology) -> Result<R, E>,
    ) -> Result<R, E> {
        self.session().run(direction, run).await
    }
}

/// The answers the runs of one request share.
///
/// Every `$translate` answer is keyed by the whole question: the concept map,
/// the system, the code, its version and the direction; every `$lookup`
/// answer by the system, the code and the version. Nothing outlives the
/// session.
#[derive(Debug)]
pub struct Session<'t> {
    /// The client of the `[terminology]` server, when one is configured.
    client: Option<&'t Client>,
    /// Whether a code the server does not know takes its code as display.
    display_fallback: bool,
    /// Every `$translate` answer this session received.
    answers: Mutex<BTreeMap<Translation, Answer>>,
    /// Every `$lookup` answer this session received.
    displays: Mutex<BTreeMap<Lookup, Displayed>>,
}

impl Session<'_> {
    /// Runs `run` with every code it translates or looks up resolved first.
    ///
    /// `run` is called again after each round of calls, so it must be a pure
    /// function of its input; only the result of the run that met no
    /// unresolved question is returned. A question this session already
    /// holds is not asked again. With no server `run` is called once over
    /// [`NoTerminology`].
    ///
    /// # Errors
    ///
    /// Returns the error `run` returns. A question the server refused, or
    /// that the call never reached an answer for, reaches the engine as
    /// [`TerminologyError::Upstream`] carrying the client's error, so the run
    /// refuses with it.
    pub async fn run<R, E>(
        &self,
        direction: Direction,
        mut run: impl FnMut(&dyn Terminology) -> Result<R, E>,
    ) -> Result<R, E> {
        let Some(client) = self.client else {
            return run(&NoTerminology);
        };
        self.settle(client, direction, &mut run).await
    }

    /// Resolves every question `probe` asks, for a final run over the
    /// [`Resolved`] seam this returns.
    ///
    /// This is [`Session::run`] for a run with a side effect of its own, such
    /// as binding the identity of a created resource: `probe` repeats without
    /// it, and only the final run over [`Resolved`] has it.
    pub async fn resolve<R, E>(
        &self,
        direction: Direction,
        mut probe: impl FnMut(&dyn Terminology) -> Result<R, E>,
    ) -> Resolved<'_> {
        if let Some(client) = self.client {
            let _probed: Result<R, E> = self.settle(client, direction, &mut probe).await;
        }
        Resolved {
            session: self,
            direction,
        }
    }

    /// Repeats `run` over the questions resolved so far, resolving the ones
    /// each run meets, and returns the result of the first run that meets
    /// none.
    async fn settle<R, E>(
        &self,
        client: &Client,
        direction: Direction,
        run: &mut impl FnMut(&dyn Terminology) -> Result<R, E>,
    ) -> Result<R, E> {
        loop {
            let (translations, lookups) = {
                let answers = self.answers.lock().unwrap_or_else(PoisonError::into_inner);
                let displays = self.displays.lock().unwrap_or_else(PoisonError::into_inner);
                let prepared = Prepared {
                    direction,
                    display_fallback: self.display_fallback,
                    answers: &answers,
                    displays: &displays,
                    missing: Mutex::new(BTreeSet::new()),
                    missing_lookups: Mutex::new(BTreeSet::new()),
                };
                let result = run(&prepared);
                let missing = prepared.missing();
                if missing.0.is_empty() && missing.1.is_empty() {
                    return result;
                }
                missing
            };
            let (translated, looked_up) = fetch(client, translations, lookups).await;
            self.answers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(translated);
            self.displays
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(looked_up);
        }
    }
}

/// The questions [`Session::resolve`] resolved, as the seam of the final run.
#[derive(Debug)]
pub struct Resolved<'s> {
    /// The session holding the answers.
    session: &'s Session<'s>,
    /// The direction of the run.
    direction: Direction,
}

impl Terminology for Resolved<'_> {
    fn translate(
        &self,
        request: &ConceptMapTranslateRequest,
    ) -> Result<ConceptMapTranslateResponse, TerminologyError> {
        if self.session.client.is_none() {
            return Err(TerminologyError::NotConfigured);
        }
        let translation = translation(self.direction, request)?;
        let answers = self
            .session
            .answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match answers.get(&translation) {
            Some(Ok(response)) => Ok(response.clone()),
            Some(Err(error)) => Err(TerminologyError::Upstream {
                source: Box::new(Arc::clone(error)),
            }),
            None => Err(TerminologyError::Upstream {
                source: Box::new(Unresolved),
            }),
        }
    }

    fn lookup(
        &self,
        request: &CodeSystemLookupRequest,
    ) -> Result<Option<CodeSystemLookupResponse>, TerminologyError> {
        if self.session.client.is_none() {
            return Err(TerminologyError::NotConfigured);
        }
        let lookup = lookup(request)?;
        let displays = self
            .session
            .displays
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match displays.get(&lookup) {
            Some(Ok(response)) => Ok(response.clone()),
            Some(Err(error)) => Err(TerminologyError::Upstream {
                source: Box::new(Arc::clone(error)),
            }),
            None => Err(TerminologyError::Upstream {
                source: Box::new(Unresolved),
            }),
        }
    }

    fn display_fallback(&self) -> bool {
        self.session.display_fallback
    }
}

/// A question the final run met that the probe runs never did.
#[derive(Debug, thiserror::Error)]
#[error("the run asked the terminology server a question its probe runs never met")]
pub struct Unresolved;

/// The seam one run reads: the questions resolved so far, and the questions
/// the run met that are not.
#[derive(Debug)]
struct Prepared<'a> {
    /// The direction of the run.
    direction: Direction,
    /// Whether a code the server does not know takes its code as display.
    display_fallback: bool,
    /// Every `$translate` answer so far.
    answers: &'a BTreeMap<Translation, Answer>,
    /// Every `$lookup` answer so far.
    displays: &'a BTreeMap<Lookup, Displayed>,
    /// The `$translate` questions this run met that are not resolved yet.
    missing: Mutex<BTreeSet<Translation>>,
    /// The `$lookup` questions this run met that are not resolved yet.
    missing_lookups: Mutex<BTreeSet<Lookup>>,
}

impl Prepared<'_> {
    /// Returns the questions this run met that are not resolved yet.
    fn missing(self) -> (BTreeSet<Translation>, BTreeSet<Lookup>) {
        (
            self.missing
                .into_inner()
                .unwrap_or_else(PoisonError::into_inner),
            self.missing_lookups
                .into_inner()
                .unwrap_or_else(PoisonError::into_inner),
        )
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

    fn lookup(
        &self,
        request: &CodeSystemLookupRequest,
    ) -> Result<Option<CodeSystemLookupResponse>, TerminologyError> {
        let lookup = lookup(request)?;
        match self.displays.get(&lookup) {
            Some(Ok(response)) => Ok(response.clone()),
            Some(Err(error)) => Err(TerminologyError::Upstream {
                source: Box::new(Arc::clone(error)),
            }),
            None => {
                self.missing_lookups
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(lookup);
                // NOTE: no specification governs this: our own design; this run
                // is discarded, and a found display lets it reach further codes.
                Ok(Some(undisplayed()))
            }
        }
    }

    fn display_fallback(&self) -> bool {
        self.display_fallback
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

/// Returns the `$lookup` question `request` asks.
///
/// # Errors
///
/// Returns [`TerminologyError::Uncoded`] when the request names no system or
/// no code, and [`TerminologyError::Upstream`] when it carries a parameter
/// this client does not send, which is refused rather than dropped.
fn lookup(request: &CodeSystemLookupRequest) -> Result<Lookup, TerminologyError> {
    let parameter = [
        ("coding", request.coding.is_some()),
        ("date", request.date.is_some()),
        ("displayLanguage", request.display_language.is_some()),
        ("property", !request.property.is_empty()),
        ("useSupplement", !request.use_supplement.is_empty()),
    ]
    .into_iter()
    .find_map(|(parameter, set)| set.then_some(parameter));
    if let Some(parameter) = parameter {
        return Err(TerminologyError::Upstream {
            source: Box::new(Uncarried { parameter }),
        });
    }
    let system = request.system.as_ref().and_then(|uri| uri.value.as_deref());
    let code = request.code.as_ref().and_then(|code| code.value.as_deref());
    let (Some(system), Some(code)) = (system, code) else {
        return Err(TerminologyError::Uncoded);
    };
    Ok(Lookup {
        system: String::from(system),
        code: String::from(code),
        version: request.version.as_ref().and_then(|text| text.value.clone()),
    })
}

/// A parameter the engine set that this client does not send.
#[derive(Debug, thiserror::Error)]
#[error("the terminology client does not send the parameter {parameter}")]
pub struct Uncarried {
    /// The parameter, as the operation names it.
    parameter: &'static str,
}

/// Returns whether the terminology server failed anywhere in `error`'s cause
/// chain.
///
/// A call that reached no answer, a refusal, an answer that does not decode
/// and a batch entry answered with another operation's outcome are the
/// server's failure. A request this client could not build or send
/// ([`Uncarried`], a base URL or a parameter the release lacks) and a question
/// the probe runs never met ([`Unresolved`]) are this bridge's own, and are
/// not.
#[must_use]
pub fn failed_upstream(error: &(dyn core::error::Error + 'static)) -> bool {
    let mut cause = Some(error);
    while let Some(current) = cause {
        let node: &(dyn core::error::Error + 'static) =
            match current.downcast_ref::<Arc<dyn core::error::Error + Send + Sync>>() {
                Some(shared) => &**shared,
                None => current,
            };
        if node.is::<fetch::UnexpectedAnswer>() {
            return true;
        }
        if let Some(failure) = node.downcast_ref::<ferrobridge_term::error::Error>() {
            return matches!(
                failure,
                ferrobridge_term::error::Error::Transport { .. }
                    | ferrobridge_term::error::Error::Timeout { .. }
                    | ferrobridge_term::error::Error::ServerFailure { .. }
                    | ferrobridge_term::error::Error::Unauthorized { .. }
                    | ferrobridge_term::error::Error::Token { .. }
                    | ferrobridge_term::error::Error::Renewal { .. }
                    | ferrobridge_term::error::Error::Refused { .. }
                    | ferrobridge_term::error::Error::Body { .. }
                    | ferrobridge_term::error::Error::BatchArity { .. }
            );
        }
        cause = current.source();
    }
    false
}

/// An answer that names no translation.
fn unresolved() -> ConceptMapTranslateResponse {
    ConceptMapTranslateResponse {
        result: Boolean::from(false),
        ..ConceptMapTranslateResponse::default()
    }
}

/// A display that stands in for an unresolved one, in a run that is
/// discarded.
fn undisplayed() -> CodeSystemLookupResponse {
    CodeSystemLookupResponse {
        name: primitives::String::from("unresolved"),
        display: primitives::String::from("unresolved"),
        ..CodeSystemLookupResponse::default()
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
        let session = translator.session();
        let resolved = session
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
