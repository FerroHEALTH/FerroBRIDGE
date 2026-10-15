// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outcome of one validation: the issues, and every constraint the
//! validator met and could not evaluate, convertible into an R4
//! `OperationOutcome`.

use std::collections::BTreeMap;
use std::fmt;

use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::operation_outcome::OperationOutcome;
use fhir_types::r4::operation_outcome::OperationOutcomeIssue;
use fhir_types::r4::primitives;

use crate::issue::Issue;
use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Rule;
use crate::issue::Severity;

/// The system of the `details.coding` that carries a message id.
///
/// No specification governs this: our own design. R4 defines no validator
/// messages, so the ids live in a system of their own.
pub const MESSAGE_SYSTEM: &str = "https://ferrobridge.eu/fhir/CodeSystem/validator-message";

/// A constraint of a `StructureDefinition` the validator met on an element of
/// the instance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Constraint {
    /// The slicing an element definition declares
    /// (<https://hl7.org/fhir/R4/profiling.html#slicing>).
    Slicing,
    /// One discriminator of a slicing
    /// (<https://hl7.org/fhir/R4/profiling.html#discriminator>).
    Discriminator {
        /// The discriminator type: `value`, `exists`, `pattern`, `type` or
        /// `profile`.
        kind: String,
        /// The discriminator path.
        path: String,
    },
    /// A `value` or `pattern` discriminator whose slice is told apart by a
    /// required binding.
    DiscriminatorValueSet {
        /// The discriminator path.
        path: String,
        /// The value set the binding names.
        value_set: String,
    },
    /// An invariant (<https://hl7.org/fhir/R4/conformance-rules.html#constraints>).
    Invariant {
        /// The constraint key.
        key: String,
        /// The constraint's own severity, `error` or `warning`.
        severity: String,
    },
    /// A terminology binding (<https://hl7.org/fhir/R4/terminologies.html#strength>).
    Binding {
        /// The binding strength.
        strength: String,
        /// The value set, when the binding names one.
        value_set: Option<String>,
    },
    /// The profiles a type of the element names (`type.profile`).
    TypeProfile {
        /// The profile canonicals.
        profiles: Vec<String>,
    },
    /// The profiles a reference's target SHALL conform to
    /// (`type.targetProfile`).
    TargetProfile {
        /// The profile canonicals.
        profiles: Vec<String>,
    },
    /// A profile the instance claims in `meta.profile`.
    ClaimedProfile {
        /// The profile canonical.
        url: String,
    },
    /// The contexts an extension may be used in
    /// (`StructureDefinition.context`).
    ExtensionContext,
}

/// Why a constraint was not evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// The discriminator path is outside the simple FHIRPath subset
    /// (<https://hl7.org/fhir/R4/fhirpath.html#simple>).
    DiscriminatorPath,
    /// The discriminator type is none of the five R4 defines.
    DiscriminatorType,
    /// The slice sets no value, pattern, required binding, cardinality, type
    /// or profile at the discriminator path to tell it apart by.
    NoDiscriminatorValue,
    /// The value set of the binding cannot be enumerated from the package
    /// set.
    ValueSetNotEnumerated,
    /// A reference the discriminator path resolves names no resource the
    /// reference points into.
    UnresolvedReference,
    /// A reference the discriminator path resolves points outside the
    /// resources the validator searches.
    ReferenceOutOfScope,
    /// The profile is not in the package set.
    ProfileNotFound,
    /// The profile declares a `@default` slice under rules other than
    /// `closed`, which R4 does not allow, so the default slice is not used.
    DefaultSliceNotClosed,
    /// The validator does not evaluate invariants.
    Invariants,
    /// The validator does not check terminology bindings.
    Bindings,
    /// The validator does not validate a reference's target against its
    /// target profiles, or a resource against a profile it claims.
    Profiles,
    /// The validator does not check where an extension is used.
    ExtensionContext,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DiscriminatorPath => {
                "the discriminator path is outside the FHIRPath subset discriminators use"
            }
            Self::DiscriminatorType => "the discriminator type is none R4 defines",
            Self::NoDiscriminatorValue => {
                "the slice sets nothing at the discriminator path to tell it apart by"
            }
            Self::ValueSetNotEnumerated => {
                "the value set cannot be enumerated from the package set"
            }
            Self::UnresolvedReference => "a reference the discriminator resolves does not resolve",
            Self::ReferenceOutOfScope => {
                "a reference the discriminator resolves points outside the resources searched"
            }
            Self::ProfileNotFound => "the profile is not in the package set",
            Self::DefaultSliceNotClosed => {
                "the @default slice is not used, because the slicing rules are not closed"
            }
            Self::Invariants => "invariants are not evaluated",
            Self::Bindings => "terminology bindings are not checked",
            Self::Profiles => "target profiles and claimed profiles are not validated",
            Self::ExtensionContext => "extension contexts are not checked",
        })
    }
}

/// A constraint the validator met and did not evaluate, never a silent pass.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Unevaluated {
    /// The constraint.
    pub constraint: Constraint,
    /// The canonical of the `StructureDefinition` that declares it.
    pub structure: String,
    /// The element id it is declared on.
    pub element: String,
    /// The FHIRPath location of the first occurrence it applied to.
    pub location: String,
    /// Why it was not evaluated.
    pub reason: Reason,
}

/// The outcome of one validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    issues: Vec<Issue>,
    unevaluated: Vec<Unevaluated>,
}

impl Report {
    /// Returns the issues in the order they were found.
    #[must_use]
    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }

    /// Returns every constraint not evaluated, once per declaring
    /// `StructureDefinition`, element and constraint, in that order.
    #[must_use]
    pub fn unevaluated(&self) -> &[Unevaluated] {
        &self.unevaluated
    }

    /// Answers whether no issue is an error or fatal.
    ///
    /// An unevaluated constraint does not make the instance invalid; it makes
    /// the answer incomplete, which [`Report::unevaluated`] states.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self
            .issues
            .iter()
            .any(|issue| matches!(issue.severity, Severity::Fatal | Severity::Error))
    }

    /// Returns the issues carrying `id`.
    pub fn with_message(&self, id: MessageId) -> impl Iterator<Item = &Issue> {
        self.issues
            .iter()
            .filter(move |issue| issue.rule == Rule::Message(id))
    }
}

/// Collects issues and unevaluated constraints during one validation.
#[derive(Debug, Default)]
pub(crate) struct Collector {
    issues: Vec<Issue>,
    unevaluated: BTreeMap<(String, String, Constraint), (String, Reason)>,
}

impl Collector {
    /// Records an issue against one of our message ids.
    pub(crate) fn issue(
        &mut self,
        severity: Severity,
        code: IssueType,
        id: MessageId,
        expression: &str,
        message: String,
    ) {
        self.issues.push(Issue {
            severity,
            code,
            expression: expression.to_owned(),
            rule: Rule::Message(id),
            message,
        });
    }

    /// Records a constraint not evaluated, keeping its first location.
    pub(crate) fn unevaluated(
        &mut self,
        structure: &str,
        element: &str,
        constraint: Constraint,
        location: &str,
        reason: Reason,
    ) {
        self.unevaluated
            .entry((structure.to_owned(), element.to_owned(), constraint))
            .or_insert_with(|| (location.to_owned(), reason));
    }

    /// Answers whether no issue collected is an error or fatal.
    pub(crate) fn is_valid(&self) -> bool {
        self.first_error().is_none()
    }

    /// Returns the first issue collected that is an error or fatal.
    pub(crate) fn first_error(&self) -> Option<&Issue> {
        self.issues
            .iter()
            .find(|issue| matches!(issue.severity, Severity::Fatal | Severity::Error))
    }

    /// Adds the issues and unevaluated constraints `other` collected.
    pub(crate) fn absorb(&mut self, other: Self) {
        self.issues.extend(other.issues);
        for (key, value) in other.unevaluated {
            self.unevaluated.entry(key).or_insert(value);
        }
    }

    /// Finishes the report.
    pub(crate) fn finish(self) -> Report {
        Report {
            issues: self.issues,
            unevaluated: self
                .unevaluated
                .into_iter()
                .map(
                    |((structure, element, constraint), (location, reason))| Unevaluated {
                        constraint,
                        structure,
                        element,
                        location,
                        reason,
                    },
                )
                .collect(),
        }
    }
}

impl From<&Report> for OperationOutcome {
    /// Builds the R4 `OperationOutcome`: one issue per [`Issue`], then one
    /// `warning` of type `not-supported` per [`Unevaluated`].
    ///
    /// Each issue carries its location in `expression`, its message in
    /// `details.text`, and its message id as a `details.coding` in
    /// [`MESSAGE_SYSTEM`]; a constraint key is a coding whose system is the
    /// declaring `StructureDefinition`'s canonical. A report with neither
    /// issues nor unevaluated constraints becomes one `information` issue,
    /// because `OperationOutcome.issue` is `1..*`
    /// (<https://hl7.org/fhir/R4/operationoutcome.html>).
    fn from(report: &Report) -> Self {
        let mut issues: Vec<OperationOutcomeIssue> =
            report.issues.iter().map(outcome_issue).collect();
        issues.extend(report.unevaluated.iter().map(|unevaluated| {
            outcome_issue(&Issue {
                severity: Severity::Warning,
                code: IssueType::NotSupported,
                expression: unevaluated.location.clone(),
                rule: Rule::Message(MessageId::ConstraintUnevaluated),
                message: describe(unevaluated),
            })
        }));
        if issues.is_empty() {
            issues.push(outcome_issue(&Issue {
                severity: Severity::Information,
                code: IssueType::Informational,
                expression: String::new(),
                rule: Rule::Message(MessageId::AllOk),
                message: "no issues, and every constraint met was evaluated".to_owned(),
            }));
        }
        Self {
            issue: issues,
            ..Self::default()
        }
    }
}

/// The message of an unevaluated constraint.
fn describe(unevaluated: &Unevaluated) -> String {
    let what = match &unevaluated.constraint {
        Constraint::Slicing => "the slicing".to_owned(),
        Constraint::Discriminator { kind, path } => {
            format!("the {kind} discriminator {path}")
        }
        Constraint::DiscriminatorValueSet { path, value_set } => {
            format!("the discriminator {path} over the value set {value_set}")
        }
        Constraint::Invariant { key, .. } => format!("the invariant {key}"),
        Constraint::Binding {
            strength,
            value_set,
        } => match value_set {
            Some(value_set) => format!("the {strength} binding to {value_set}"),
            None => format!("the {strength} binding"),
        },
        Constraint::TypeProfile { profiles } => {
            format!("the type profiles {}", profiles.join(", "))
        }
        Constraint::TargetProfile { profiles } => {
            format!("the target profiles {}", profiles.join(", "))
        }
        Constraint::ClaimedProfile { url } => format!("the claimed profile {url}"),
        Constraint::ExtensionContext => "the extension context".to_owned(),
    };
    format!(
        "{what} on {} of {} was not evaluated: {}",
        unevaluated.element, unevaluated.structure, unevaluated.reason
    )
}

/// One `OperationOutcome.issue`.
fn outcome_issue(issue: &Issue) -> OperationOutcomeIssue {
    let coding = match &issue.rule {
        Rule::Message(id) => Coding {
            system: Some(primitives::Uri::from(MESSAGE_SYSTEM.to_owned())),
            code: Some(primitives::Code::from(id.code().to_owned())),
            ..Coding::default()
        },
        Rule::Constraint { key, source } => Coding {
            system: Some(primitives::Uri::from(source.clone())),
            code: Some(primitives::Code::from(key.clone())),
            ..Coding::default()
        },
    };
    OperationOutcomeIssue {
        severity: primitives::Code::from(issue.severity.code().to_owned()),
        code: primitives::Code::from(issue.code.code().to_owned()),
        details: Some(CodeableConcept {
            coding: vec![coding],
            text: Some(primitives::String::from(issue.message.clone())),
            ..CodeableConcept::default()
        }),
        expression: if issue.expression.is_empty() {
            Vec::new()
        } else {
            vec![primitives::String::from(issue.expression.clone())]
        },
        ..OperationOutcomeIssue::default()
    }
}
