// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[terminology]` server as the terminology seam of `eehrxf`.
//!
//! Each call is the R4 operation the seam names, `ConceptMap/$translate`,
//! `CodeSystem/$lookup` or `ValueSet/$validate-code`
//! (<https://hl7.org/fhir/R4/terminology-service.html>). A refusal, a
//! time-out, a transport failure and an answer that does not read are each a
//! [`TerminologyError`] naming the operation, so no failed call reads as an
//! answer.

use async_trait::async_trait;
use eehrxf::terminology::Candidate;
use eehrxf::terminology::Match;
use eehrxf::terminology::Operation;
use eehrxf::terminology::TerminologyError;
use eehrxf::terminology::TerminologyServer;
use ferrobridge_term::client::Client;
use ferrobridge_term::concept::Concept;
use ferrobridge_term::error::Error as TermError;
use ferrobridge_term::outcome::LookupOutcome;
use ferrobridge_term::outcome::TranslateOutcome;
use ferrobridge_term::outcome::ValidateOutcome;
use ferrobridge_term::request::Coded;
use ferrobridge_term::request::Translation;
use ferrobridge_term::request::Validation;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::String as FhirString;
use fhir_types::r4::primitives::Uri;

/// The `[terminology]` server, reached through its client.
#[derive(Debug, Clone)]
pub struct Server {
    /// The client of the `[terminology]` server.
    client: Client,
}

impl Server {
    /// Returns the seam over `client`.
    #[must_use]
    pub const fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl TerminologyServer for Server {
    async fn translate(
        &self,
        system: &str,
        code: &str,
        target: &str,
    ) -> Result<Vec<Match>, TerminologyError> {
        let translation = Translation::new(system, code).with_target_system(target);
        let outcome = self
            .client
            .translate(&translation)
            .await
            .map_err(|error| refusal(Operation::Translate, error))?;
        match outcome {
            TranslateOutcome::Translated { matches, .. } => Ok(matches
                .into_iter()
                .map(|found| Match {
                    equivalence: found
                        .equivalence
                        .as_ref()
                        .map(|equivalence| equivalence.as_str().to_owned()),
                    concept: found.concept.as_ref().map(coding),
                })
                .collect()),
            // NOTE: FHIR R4 `$translate`: a `result` of false states that no match exists.
            TranslateOutcome::NoMatch { .. } => Ok(Vec::new()),
            _ => Err(unread(Operation::Translate)),
        }
    }

    async fn lookup(&self, system: &str, code: &str) -> Result<String, TerminologyError> {
        let outcome = self
            .client
            .lookup(system, code, None)
            .await
            .map_err(|error| refusal(Operation::Lookup, error))?;
        match outcome {
            LookupOutcome::Found { display, .. } => Ok(display),
            LookupOutcome::NotFound { .. } => Err(TerminologyError::NotFound {
                operation: Operation::Lookup,
            }),
            _ => Err(unread(Operation::Lookup)),
        }
    }

    async fn validate_code(
        &self,
        value_set: &str,
        concept: &Candidate,
    ) -> Result<bool, TerminologyError> {
        let coded = match concept {
            Candidate::Code { code, context } => Coded::InContext {
                context: context.clone(),
                code: code.clone(),
                display: None,
            },
            Candidate::Coding(coding) => Coded::Coding(concept_of(coding)),
            Candidate::Concept(concept) => Coded::CodeableConcept {
                coding: concept.coding.iter().map(concept_of).collect(),
                text: concept.text.as_ref().and_then(|text| text.value.clone()),
            },
            _ => return Err(unread(Operation::ValidateCode)),
        };
        let outcome = self
            .client
            .validate(&Validation::new(value_set, coded))
            .await
            .map_err(|error| refusal(Operation::ValidateCode, error))?;
        match outcome {
            ValidateOutcome::Valid { .. } => Ok(true),
            ValidateOutcome::Invalid { .. } => Ok(false),
            _ => Err(unread(Operation::ValidateCode)),
        }
    }
}

/// Returns the refusal of `operation` that `error` describes.
fn refusal(operation: Operation, error: TermError) -> TerminologyError {
    if matches!(error, TermError::Timeout { .. }) {
        return TerminologyError::Timeout { operation };
    }
    if matches!(error, TermError::Body { .. }) {
        return TerminologyError::Answer {
            operation,
            source: Box::new(error),
        };
    }
    match error.upstream_status() {
        Some(status) => TerminologyError::Status { operation, status },
        None => TerminologyError::Transport {
            operation,
            source: Box::new(error),
        },
    }
}

/// Returns the R4 `Coding` of a concept the server answered.
fn coding(concept: &Concept) -> Coding {
    Coding {
        system: concept.system.as_deref().map(Uri::from),
        version: concept.version.as_deref().map(FhirString::from),
        code: concept.code.as_deref().map(Code::from),
        display: concept.display.as_deref().map(FhirString::from),
        ..Coding::default()
    }
}

/// Returns the concept an R4 `Coding` names.
fn concept_of(coding: &Coding) -> Concept {
    Concept {
        system: coding
            .system
            .as_ref()
            .and_then(|system| system.value.clone()),
        version: coding
            .version
            .as_ref()
            .and_then(|version| version.value.clone()),
        code: coding.code.as_ref().and_then(|code| code.value.clone()),
        display: coding
            .display
            .as_ref()
            .and_then(|display| display.value.clone()),
    }
}

/// Returns the refusal of `operation` for a form this seam does not read.
fn unread(operation: Operation) -> TerminologyError {
    TerminologyError::Answer {
        operation,
        source: Box::new(UnreadForm),
    }
}

/// A candidate or an outcome of a form this seam does not read.
#[derive(Debug, thiserror::Error)]
#[error("a terminology form this seam does not read")]
struct UnreadForm;
