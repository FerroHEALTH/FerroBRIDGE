// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology server seam: the binding checks the package set cannot
//! decide, sent to a server through `ferrobridge-term` after the walk.
//!
//! The walk is synchronous and reads only the package set. A binding it
//! cannot decide locally (a value set or code system the set does not carry,
//! a filter it does not evaluate) becomes a [`PendingBinding`] in the
//! [`Report`], listed as unevaluated with the local reason.
//! [`Report::resolve_bindings`] sends every pending check to a
//! [`Terminology`] in one `ValueSet/$validate-code` call or one batch, turns
//! each answer into an issue, and lifts the listing of every binding it
//! answered. A check the server refused or failed stays listed with the
//! failure class and the upstream status, never passed.
//!
//! ```no_run
//! use ferrobridge_term::client::Client;
//! use ferrobridge_term::config::{Config, WireVersion};
//! use ferrobridge_validator::set::PackageSet;
//! use ferrobridge_validator::validate::Validator;
//! use fhir_types::codec::Value;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let set = PackageSet::load(["vendor/hl7.fhir.r4.core"])?;
//! let patient: Value = serde_json::from_str(r#"{"resourceType": "Patient", "gender": "female"}"#)?;
//! let mut report = Validator::new(&set).validate(&patient)?;
//! let client = Client::new(Config::new("https://tx.example.org/r4".parse()?, WireVersion::R4))?;
//! # let _call = async move {
//! report.resolve_bindings(&client).await;
//! assert!(report.pending_bindings().is_empty());
//! # };
//! # Ok(())
//! # }
//! ```

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::future::Future;

use ferrobridge_term::client::Client;
use ferrobridge_term::error::Error;
use ferrobridge_term::outcome::Coded;
use ferrobridge_term::outcome::ValidateOutcome;
use ferrobridge_term::outcome::Validation;

use crate::issue::Issue;
use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Rule;
use crate::issue::Severity;
use crate::report::Key;
use crate::report::Reason;
use crate::report::Report;

/// A terminology server that answers `ValueSet/$validate-code`
/// (<https://hl7.org/fhir/R4/valueset-operation-validate-code.html>).
pub trait Terminology {
    /// Validates every one of `validations`, answering them by position.
    ///
    /// The outer error is a failure of the whole call; an inner one is the
    /// failure of that validation alone.
    fn validate_all(
        &self,
        validations: &[Validation],
    ) -> impl Future<Output = Result<Vec<Result<ValidateOutcome, Error>>, Error>> + Send;
}

impl Terminology for Client {
    fn validate_all(
        &self,
        validations: &[Validation],
    ) -> impl Future<Output = Result<Vec<Result<ValidateOutcome, Error>>, Error>> + Send {
        Self::validate_all(self, validations)
    }
}

/// How binding strength decides the severity of a code outside the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Strength {
    /// `required` (terminologies.html §4.1.5.1).
    Required,
    /// `extensible` (§4.1.5.2).
    Extensible,
    /// `preferred` (§4.1.5.3).
    Preferred,
    /// The maximum value set of the `elementdefinition-maxValueSet`
    /// extension, held as a required binding.
    Maximum,
}

/// A binding check only a terminology server can decide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingBinding {
    /// The unevaluated listing it lifts once answered.
    pub(crate) key: Key,
    /// The FHIRPath location of the value.
    pub(crate) location: String,
    /// The strength the answer is held to.
    pub(crate) strength: Strength,
    /// The value set as the binding names it.
    pub(crate) value_set: String,
    /// The question sent.
    pub(crate) validation: Validation,
    /// The display of a lone coding whose code system the set does not
    /// carry, compared with the display the server answers.
    pub(crate) display: Option<String>,
}

impl PendingBinding {
    /// Returns the FHIRPath location of the value.
    #[must_use]
    pub fn location(&self) -> &str {
        &self.location
    }

    /// Returns the value set as the binding names it.
    #[must_use]
    pub fn value_set(&self) -> &str {
        &self.value_set
    }

    /// Returns the `ValueSet/$validate-code` question the check sends.
    #[must_use]
    pub const fn validation(&self) -> &Validation {
        &self.validation
    }
}

impl Report {
    /// Sends every pending binding check to `terminology` and folds the
    /// answers into the report.
    ///
    /// The checks go as one call, or as one batch when there are several,
    /// each distinct question once. A code the server finds outside the set
    /// is an issue of the binding's severity, and a valid one lifts the
    /// binding's listing. A check the server refused or failed, or a batch
    /// that failed whole, stays listed with
    /// [`Reason::TerminologyFailed`], carrying the failure class and the
    /// upstream status. A listing for a reason no server can lift stays.
    pub async fn resolve_bindings<T: Terminology + ?Sized>(&mut self, terminology: &T) {
        if self.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        let mut questions: Vec<Validation> = Vec::new();
        let mut positions: BTreeMap<&Validation, usize> = BTreeMap::new();
        let mut asked = Vec::with_capacity(pending.len());
        for check in &pending {
            let position = *positions.entry(&check.validation).or_insert_with(|| {
                questions.push(check.validation.clone());
                questions.len().saturating_sub(1)
            });
            asked.push(position);
        }
        let answers: Vec<Result<ValidateOutcome, Reason>> =
            match terminology.validate_all(&questions).await {
                Ok(answers) if answers.len() == questions.len() => answers
                    .into_iter()
                    .map(|answer| answer.map_err(|error| failure(&error)))
                    .collect(),
                Ok(_) => questions.iter().map(|_| Err(batch_arity())).collect(),
                Err(error) => questions.iter().map(|_| Err(failure(&error))).collect(),
            };
        let mut failed: BTreeMap<Key, (String, Reason)> = BTreeMap::new();
        for (check, position) in pending.iter().zip(asked) {
            let reason = match answers.get(position) {
                Some(Ok(ValidateOutcome::Valid { display, .. })) => {
                    if let (Some(held), Some(answered)) = (&check.display, display)
                        && held != answered
                    {
                        self.issues.push(display_issue(
                            &check.location,
                            check.validation_system(),
                            Some(answered),
                        ));
                    }
                    continue;
                }
                Some(Ok(ValidateOutcome::Invalid { .. })) => {
                    self.issues.push(binding_issue(
                        check.strength,
                        &check.key.1,
                        &check.value_set,
                        &check.location,
                    ));
                    continue;
                }
                Some(Ok(_)) => Reason::TerminologyFailed {
                    kind: "unknown-outcome",
                    status: None,
                },
                Some(Err(reason)) => reason.clone(),
                None => batch_arity(),
            };
            failed
                .entry(check.key.clone())
                .or_insert_with(|| (check.location.clone(), reason));
        }
        let keys: BTreeSet<&Key> = pending.iter().map(|check| &check.key).collect();
        let held = &self.held;
        self.unevaluated.retain_mut(|unevaluated| {
            let key = (
                unevaluated.structure.clone(),
                unevaluated.element.clone(),
                unevaluated.constraint.clone(),
            );
            if !keys.contains(&key) || held.contains(&key) {
                return true;
            }
            match failed.get(&key) {
                Some((location, reason)) => {
                    unevaluated.location.clone_from(location);
                    unevaluated.reason = reason.clone();
                    true
                }
                None => false,
            }
        });
    }
}

impl PendingBinding {
    /// Returns the system of the lone coding the check asks about.
    fn validation_system(&self) -> &str {
        match &self.validation.coded {
            Coded::Coding(concept) => concept.system.as_deref().unwrap_or_default(),
            _ => "",
        }
    }
}

/// The reason a failed call leaves a binding unevaluated.
fn failure(error: &Error) -> Reason {
    Reason::TerminologyFailed {
        kind: error.kind(),
        status: error.upstream_status(),
    }
}

/// The reason for a batch answered with the wrong number of entries.
const fn batch_arity() -> Reason {
    Reason::TerminologyFailed {
        kind: "batch-arity",
        status: None,
    }
}

/// The issue for a value with no code from the value set of a binding.
pub(crate) fn binding_issue(
    strength: Strength,
    element: &str,
    value_set: &str,
    location: &str,
) -> Issue {
    // NOTE: terminologies.html §4.1.5.2 and §4.1.5.3: extensible is decided "based on human
    // review" and preferred binds nothing; no specification governs these severities: our own.
    let (severity, id, what) = match strength {
        Strength::Required => (Severity::Error, MessageId::NotInValueSet, "required"),
        Strength::Maximum => (
            Severity::Error,
            MessageId::NotInValueSet,
            "maximum value set",
        ),
        Strength::Extensible => (
            Severity::Warning,
            MessageId::NotInExtensibleValueSet,
            "extensible",
        ),
        Strength::Preferred => (
            Severity::Information,
            MessageId::NotInPreferredValueSet,
            "preferred",
        ),
    };
    Issue {
        severity,
        code: IssueType::CodeInvalid,
        expression: location.to_owned(),
        rule: Rule::Message(id),
        message: format!(
            "{element} is bound ({what}) to {value_set}, and the value holds no code of it"
        ),
    }
}

/// The issue for a coding whose display is none its code system gives the
/// code.
pub(crate) fn display_issue(location: &str, system: &str, expected: Option<&str>) -> Issue {
    let expected = expected.map_or_else(
        || "the code system gives the code no display".to_owned(),
        |display| format!("the code system displays the code as \"{display}\""),
    );
    // NOTE: no specification governs this severity: our own design; datatypes.html#Coding makes
    // the display a representation the system defines, and a mismatch does not change the code.
    Issue {
        severity: Severity::Warning,
        code: IssueType::Value,
        expression: location.to_owned(),
        rule: Rule::Message(MessageId::DisplayMismatch),
        message: format!("the display is none {system} defines for the code; {expected}"),
    }
}
