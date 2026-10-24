// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The calls a session makes for the questions a run met, and the R4 answers
//! the engine reads from what the client decoded.

use core::error::Error;
use std::collections::BTreeSet;
use std::sync::Arc;

use ferrobridge_term::client::Client;
use ferrobridge_term::concept::{Concept, Match};
use ferrobridge_term::outcome::{
    BatchOutcome, LookupOutcome, Request, TranslateOutcome, Translation,
};
use fhir_types::r4::coding::Coding;
use fhir_types::r4::operations::code_system_lookup::CodeSystemLookupResponse;
use fhir_types::r4::operations::concept_map_translate::{
    ConceptMapTranslateResponse, ConceptMapTranslateResponseMatch,
};
use fhir_types::r4::primitives;
use fhir_types::r4::primitives::{Boolean, Canonical, Code, Uri};

/// What the server answered for one `$translate`, shared across the repeated
/// runs.
pub(super) type Answer = Result<ConceptMapTranslateResponse, Arc<dyn Error + Send + Sync>>;

/// What the server answered for one `$lookup`: the details, `None` for a
/// code it does not know.
pub(super) type Displayed = Result<Option<CodeSystemLookupResponse>, Arc<dyn Error + Send + Sync>>;

/// One `$lookup` question: the system, the code and the version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Lookup {
    /// The code system.
    pub(super) system: String,
    /// The code.
    pub(super) code: String,
    /// The code system version, when the coding names one.
    pub(super) version: Option<String>,
}

impl Lookup {
    /// Returns the client request this question sends.
    fn request(&self) -> Request {
        Request::Lookup {
            system: self.system.clone(),
            code: self.code.clone(),
            version: self.version.clone(),
            date: None,
        }
    }
}

/// The answers to every question a round of runs met.
pub(super) type Fetched = (Vec<(Translation, Answer)>, Vec<(Lookup, Displayed)>);

/// Returns the answer to every question of `translations` and `lookups`.
///
/// One question is one call; several are one `batch`, answered by position.
/// A batch that failed as a whole is the answer of every question it carried.
pub(super) async fn fetch(
    client: &Client,
    translations: BTreeSet<Translation>,
    lookups: BTreeSet<Lookup>,
) -> Fetched {
    let translations: Vec<Translation> = translations.into_iter().collect();
    let lookups: Vec<Lookup> = lookups.into_iter().collect();
    match (translations.as_slice(), lookups.as_slice()) {
        ([one], []) => {
            let answer = client
                .translate(one)
                .await
                .map_err(shared)
                .and_then(|outcome| response(&outcome));
            return (vec![(one.clone(), answer)], Vec::new());
        }
        ([], [one]) => {
            let answer = client
                .lookup(&one.system, &one.code, one.version.as_deref())
                .await
                .map_err(shared)
                .and_then(|outcome| displayed(&outcome));
            return (Vec::new(), vec![(one.clone(), answer)]);
        }
        _ => {}
    }
    let requests: Vec<Request> = translations
        .iter()
        .cloned()
        .map(Request::Translate)
        .chain(lookups.iter().map(Lookup::request))
        .collect();
    match client.batch(&requests).await {
        Ok(entries) => {
            let mut entries = entries.into_iter();
            let translated = translations
                .into_iter()
                .zip(entries.by_ref())
                .map(|(translation, entry)| {
                    let answer = match entry {
                        Ok(BatchOutcome::Translate(outcome)) => response(&outcome),
                        Ok(_) => Err(shared(UnexpectedAnswer)),
                        Err(error) => Err(shared(error)),
                    };
                    (translation, answer)
                })
                .collect();
            let looked_up = lookups
                .into_iter()
                .zip(entries)
                .map(|(lookup, entry)| {
                    let answer = match entry {
                        Ok(BatchOutcome::Lookup(outcome)) => displayed(&outcome),
                        Ok(_) => Err(shared(UnexpectedAnswer)),
                        Err(error) => Err(shared(error)),
                    };
                    (lookup, answer)
                })
                .collect();
            (translated, looked_up)
        }
        Err(error) => {
            let error = shared(error);
            (
                translations
                    .into_iter()
                    .map(|translation| (translation, Err(Arc::clone(&error))))
                    .collect(),
                lookups
                    .into_iter()
                    .map(|lookup| (lookup, Err(Arc::clone(&error))))
                    .collect(),
            )
        }
    }
}

/// Returns `error` in the form every run that meets its question shares.
pub(super) fn shared(error: impl Error + Send + Sync + 'static) -> Arc<dyn Error + Send + Sync> {
    Arc::new(error)
}

/// A batch entry answered with an outcome of another operation.
#[derive(Debug, thiserror::Error)]
#[error("the terminology server answered a batch entry with another operation's outcome")]
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
            message: message.as_deref().map(primitives::String::from),
            r#match: matches.iter().map(matched).collect(),
            ..ConceptMapTranslateResponse::default()
        }),
        TranslateOutcome::NoMatch { message } => Ok(ConceptMapTranslateResponse {
            result: Boolean::from(false),
            message: message.as_deref().map(primitives::String::from),
            ..ConceptMapTranslateResponse::default()
        }),
        _ => Err(shared(UnexpectedAnswer)),
    }
}

/// Returns the R4 `$lookup` answer `outcome` carries, `None` for a code the
/// server does not know.
///
/// The engine reads the display alone, so the designations and properties
/// the client decoded stay out of the answer.
fn displayed(outcome: &LookupOutcome) -> Displayed {
    match outcome {
        LookupOutcome::Found {
            name,
            version,
            display,
            ..
        } => Ok(Some(CodeSystemLookupResponse {
            name: primitives::String::from(name.as_str()),
            version: version.as_deref().map(primitives::String::from),
            display: primitives::String::from(display.as_str()),
            ..CodeSystemLookupResponse::default()
        })),
        LookupOutcome::NotFound { .. } => Ok(None),
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
        version: concept.version.as_deref().map(primitives::String::from),
        code: concept.code.as_deref().map(Code::from),
        display: concept.display.as_deref().map(primitives::String::from),
        ..Coding::default()
    }
}
