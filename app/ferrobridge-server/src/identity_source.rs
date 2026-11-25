// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity source every EHR subject comes from.
//!
//! The bridge keeps no patient identity of its own. It asks the identity
//! source (a `PIXm` Manager such as `FerroPIX`) for the patient's pseudonym in
//! one domain over ITI-83 by `POST`, so the source identifier never sits in a
//! URL (`PIXm` 3.1.0 §2:3.83.4.1.2.2,
//! <https://profiles.ihe.net/ITI/PIXm/3.1.0/ITI-83.html>), and every EHR's
//! `EHR_STATUS.subject.external_ref` names that pseudonym. The inbound engine
//! also feeds it as a Patient Identity Source over ITI-104 (`PIXm` 3.1.0
//! §2:3.104).

use std::time::Duration;

use http::StatusCode;
use ihe_atna::user::OnBehalfOf;
use ihe_iti::pixm::PixmClient;
use ihe_iti::pixm::error::{InvalidInput, PixmError};
use ihe_iti::pixm::identifier::{CrossReference, SourceIdentifier, TargetSystem};
use ihe_iti::pixm::source::PixmSourceClient;
use ihe_iti::pixm::source::error::SourceError as FeedError;
use secrecy::{ExposeSecret, SecretString};

use crate::facade::identity::PersonId;

/// The identity source, resolved from `[identity_source]`.
#[derive(Clone)]
pub struct IdentitySource {
    /// The `PIXm` Consumer bound to the identity source.
    pixm: PixmClient,
    /// The Patient Identity Source bound to the identity source.
    feed: PixmSourceClient,
    /// The domain the pseudonym is asked in.
    pseudonym_domain: TargetSystem,
    /// The identifier systems a source identifier may be in, in the order a
    /// Patient's identifiers are chosen by.
    source_domains: Vec<String>,
    /// How long one exchange may take.
    timeout: Duration,
}

impl std::fmt::Debug for IdentitySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentitySource")
            .field("endpoint", &self.pixm.endpoint().as_str())
            .field("feed", &self.feed.endpoint().as_str())
            .field("pseudonym_domain", &self.pseudonym_domain.as_str())
            .field("source_domains", &self.source_domains)
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// Why a Patient names no identifier the identity source may be asked about.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SourceError {
    /// An identifier carries a value and no system.
    #[error("an identifier carries no system, so the identity source cannot be asked about it")]
    NoSystem,
    /// No identifier is in an accepted domain.
    #[error(
        "the patient carries no identifier in an accepted domain; identity_source.source_domains accepts {}",
        accepted.join(", ")
    )]
    NotAccepted {
        /// The accepted domains, in order.
        accepted: Vec<String>,
    },
    /// The identifier cannot be a source identifier.
    #[error("the identifier cannot be a PIXm source identifier")]
    Invalid(#[source] InvalidInput),
}

/// Why no pseudonym was had.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PseudonymError {
    /// The identity source knows no patient by the source identifier
    /// (§2:3.83.4.2.2.2).
    #[error("the identity source knows no patient by the identifier in {system}")]
    NotFound {
        /// The source identifier's domain.
        system: String,
    },
    /// The identity source knows the patient and holds no single identifier
    /// for them in the pseudonym domain.
    #[error("the identity source holds {count} identifiers for the patient in {domain}, never one")]
    NotOne {
        /// The pseudonym domain.
        domain: String,
        /// How many it holds.
        count: usize,
    },
    /// The exchange did not end in a cross-reference.
    #[error("the identity source did not answer the pseudonym query")]
    Upstream(#[source] Box<PixmError>),
    /// The pseudonym cannot be an EHR subject.
    #[error("the pseudonym cannot be an EHR subject")]
    Subject(#[source] crate::facade::identity::IdError),
}

impl PseudonymError {
    /// Returns whether a repeat may succeed: no answer, or an answer of
    /// `429` or `5xx`.
    #[must_use]
    pub fn transient(&self) -> bool {
        match self {
            Self::Upstream(error) => match error.as_ref() {
                PixmError::Timeout | PixmError::Transport(_) => true,
                PixmError::Rejected { status, .. } => {
                    status.is_server_error() || *status == StatusCode::TOO_MANY_REQUESTS
                }
                _ => false,
            },
            Self::NotFound { .. } | Self::NotOne { .. } | Self::Subject(_) => false,
        }
    }

    /// Returns the status the identity source answered, when it answered one
    /// ITI-83 gives no meaning to.
    #[must_use]
    pub fn upstream_status(&self) -> Option<StatusCode> {
        match self {
            Self::Upstream(error) => match error.as_ref() {
                PixmError::Rejected { status, .. } => Some(*status),
                _ => None,
            },
            _ => None,
        }
    }
}

impl IdentitySource {
    /// Returns the identity source `pixm` and `feed` reach, asked by `POST`
    /// for the pseudonym in `pseudonym_domain` and fed, each within
    /// `timeout`.
    #[must_use]
    pub fn new(
        (pixm, feed): (PixmClient, PixmSourceClient),
        pseudonym_domain: TargetSystem,
        source_domains: Vec<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            pixm: pixm.invoked_by(ihe_iti::pixm::Invocation::Post),
            feed,
            pseudonym_domain,
            source_domains,
            timeout,
        }
    }

    /// Returns this identity source recording every ITI-83 exchange through
    /// `recorder` as the `PIXm` Query Consumer audit profile fixes it, and
    /// every ITI-104 exchange as the Feed Source profile does (`PIXm` 3.1.0
    /// §2:3.83.5.1.1, §2:3.104.5.1.1).
    #[must_use]
    pub fn audited(self, recorder: std::sync::Arc<dyn ihe_atna::balp::AuditRecorder>) -> Self {
        Self {
            pixm: self.pixm.audited(std::sync::Arc::clone(&recorder)),
            feed: self.feed.audited(recorder),
            ..self
        }
    }

    /// Feeds `patient` as the patient `local` names in its source's domain
    /// (ITI-104 Add or Revise Patient, §2:3.104.4.1).
    ///
    /// # Errors
    /// Returns the [`FeedError`] of a message not admitted or not applied.
    pub async fn feed(
        &self,
        local: &SourceIdentifier,
        patient: &fhir_types::r4::patient::Patient,
    ) -> Result<(), FeedError> {
        self.feed
            .feed(local, patient, &OnBehalfOf::System, self.timeout)
            .await
            .map(drop)
    }

    /// Tells the identity source the patient `subsumed` names was merged into
    /// the one `survivor` names, sending `patient`, the subsumed patient
    /// (ITI-104 Resolve Duplicate Patient, §2:3.104.4.2).
    ///
    /// # Errors
    /// Returns the [`FeedError`] of a message not admitted or not applied.
    pub async fn merge(
        &self,
        (subsumed, survivor): (&SourceIdentifier, &SourceIdentifier),
        patient: &fhir_types::r4::patient::Patient,
    ) -> Result<(), FeedError> {
        self.feed
            .resolve_duplicate(
                subsumed,
                survivor,
                patient,
                &OnBehalfOf::System,
                self.timeout,
            )
            .await
            .map(drop)
    }

    /// Returns the domain every EHR subject is a pseudonym in.
    #[must_use]
    pub fn pseudonym_domain(&self) -> &str {
        self.pseudonym_domain.as_str()
    }

    /// Returns the accepted source domains, in order.
    #[must_use]
    pub fn source_domains(&self) -> &[String] {
        &self.source_domains
    }

    /// Returns the source identifier `system` and `value` name, when `system`
    /// is an accepted domain.
    ///
    /// # Errors
    /// Returns [`SourceError::NoSystem`] without a system,
    /// [`SourceError::NotAccepted`] for a system the deployment does not
    /// accept, and [`SourceError::Invalid`] for an empty value.
    pub fn source(
        &self,
        system: Option<&str>,
        value: &str,
    ) -> Result<SourceIdentifier, SourceError> {
        let system = system.ok_or(SourceError::NoSystem)?;
        if !self
            .source_domains
            .iter()
            .any(|accepted| accepted == system)
        {
            return Err(self.not_accepted());
        }
        SourceIdentifier::new(system, SecretString::from(value)).map_err(SourceError::Invalid)
    }

    /// Returns the source identifier among a Patient's `identifiers`, each
    /// its system and value: the identifier in the first accepted domain the
    /// Patient carries one in, in the order of `identity_source.source_domains`.
    ///
    /// # Errors
    /// Returns [`SourceError::NoSystem`] when an identifier carries no
    /// system, and [`SourceError::NotAccepted`] when none is in an accepted
    /// domain.
    // NOTE: PIXm 3.1.0 §2:3.83.4.1.2.1 takes one sourceIdentifier; the order of
    // Patient.identifier carries no meaning, so the deployment's order chooses (our own design).
    pub fn source_of(
        &self,
        identifiers: &[(Option<&str>, &str)],
    ) -> Result<SourceIdentifier, SourceError> {
        if identifiers.iter().any(|(system, _)| system.is_none()) {
            return Err(SourceError::NoSystem);
        }
        for domain in &self.source_domains {
            if let Some((_, value)) = identifiers
                .iter()
                .find(|(system, _)| *system == Some(domain.as_str()))
            {
                return SourceIdentifier::new(domain.as_str(), SecretString::from(*value))
                    .map_err(SourceError::Invalid);
            }
        }
        Err(self.not_accepted())
    }

    /// Returns the refusal that names the accepted domains.
    fn not_accepted(&self) -> SourceError {
        SourceError::NotAccepted {
            accepted: self.source_domains.clone(),
        }
    }

    /// Asks the identity source for the pseudonym of the patient `source`
    /// names, on behalf of `on_behalf`, and returns it as the EHR subject.
    ///
    /// # Errors
    /// Returns [`PseudonymError::NotFound`] when the identity source knows no
    /// such patient, [`PseudonymError::NotOne`] when it holds no single
    /// pseudonym, [`PseudonymError::Upstream`] when the exchange failed, and
    /// [`PseudonymError::Subject`] when the pseudonym cannot be a subject.
    pub async fn pseudonym(
        &self,
        source: &SourceIdentifier,
        on_behalf: &OnBehalfOf,
    ) -> Result<PersonId, PseudonymError> {
        let answer = self
            .pixm
            .cross_reference(
                source,
                std::slice::from_ref(&self.pseudonym_domain),
                on_behalf,
                self.timeout,
            )
            .await
            .map_err(|error| PseudonymError::Upstream(Box::new(error)))?;
        let CrossReference::Matched(found) = answer else {
            return Err(PseudonymError::NotFound {
                system: source.system().to_owned(),
            });
        };
        let domain = self.pseudonym_domain.as_str();
        let mut held = found.in_domain(domain);
        let (Some(pseudonym), None) = (held.next(), held.next()) else {
            return Err(PseudonymError::NotOne {
                domain: domain.to_owned(),
                count: found.in_domain(domain).count(),
            });
        };
        PersonId::new(domain, pseudonym.value().expose_secret()).map_err(PseudonymError::Subject)
    }
}
