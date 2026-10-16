// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The invariants of an element definition, evaluated with
//! `ferrobridge-fhirpath` on each occurrence of the element
//! (<https://hl7.org/fhir/R4/conformance-rules.html#constraints>, §2.1.0.6).
//!
//! The focus is the element instance, `%context` with it; `%resource` is the
//! resource holding it and `%rootResource` that resource's container when it
//! is contained, itself otherwise (<https://hl7.org/fhir/R4/fhirpath.html>,
//! §2.9.1.4). An expression is parsed once per validator and checked once per
//! type. An invariant whose expression does not parse, does not fit the
//! element's type, or does not evaluate, including a call to a function the
//! validator cannot answer, is listed as unevaluated with the error.

mod seams;

use std::error::Error;
use std::sync::Arc;
use std::sync::PoisonError;

use ferrobridge_fhirpath::eval::Environment;
use ferrobridge_fhirpath::eval::Focus;
use ferrobridge_fhirpath::eval::convert;
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::named_type;
use ferrobridge_fhirpath::model::path_type;
use ferrobridge_fhirpath::model::r4::R4;
use fhir_types::codec::Value;
use fhir_types::r4::element_definition::ElementDefinition;
use fhir_types::r4::element_definition::ElementDefinitionConstraint;
use fhir_types::r4::extension::ExtensionValue;

use crate::issue::Issue;
use crate::issue::IssueType;
use crate::issue::Rule;
use crate::issue::Severity;
use crate::report::Constraint;
use crate::report::InvariantStage;
use crate::report::Reason;
use crate::validate::BestPractice;
use crate::validate::Validator;
use crate::validate::resolver::container;
use crate::validate::structure::Structure;
use crate::validate::walk::Walk;
use seams::Conformance;
use seams::References;
use seams::Terminology;

/// An invariant expression as the validator holds it: parsed, or the parse
/// error.
pub(crate) type Compiled = Result<Arc<Expression>, String>;

/// The R4 extension that marks a constraint as a best practice
/// (conformance-rules.html §2.1.0.6).
const BEST_PRACTICE: &str =
    "http://hl7.org/fhir/StructureDefinition/elementdefinition-bestpractice";

/// The upstream report of each published invariant whose expression is
/// defective, by the canonical that declares it and its key.
const DEFECTS: [(&str, &str, &str); 3] = [
    (
        "http://hl7.org/fhir/StructureDefinition/ChargeItemDefinition",
        "cid-0",
        "https://github.com/FerroHEALTH/FerroBRIDGE/issues/460",
    ),
    (
        "http://hl7.org/fhir/StructureDefinition/allergyintolerance-substanceExposureRisk",
        "inv-1",
        "https://github.com/FerroHEALTH/FerroBRIDGE/issues/460",
    ),
    (
        "http://hl7.org/fhir/StructureDefinition/DomainResource",
        "dom-3",
        "https://github.com/FerroHEALTH/FerroBRIDGE/issues/444",
    ),
];

/// The element instance an invariant is evaluated on.
pub(crate) struct Subject<'s, 'i> {
    /// The value, absent for a primitive written only as its `_` sibling.
    pub(crate) value: Option<&'i Value>,
    /// The `_` sibling of a primitive.
    pub(crate) element: Option<&'i Value>,
    /// The FHIR type, a type name or the element path of a backbone element.
    pub(crate) type_name: &'s str,
    /// The FHIRPath location.
    pub(crate) location: &'s str,
    /// Whether the instance is the resource the walk is inside.
    pub(crate) resource: bool,
}

/// The canonical that declares `constraint`: its `source`, or the
/// definition it sits in.
fn source<'a>(constraint: &'a ElementDefinitionConstraint, structure: &'a Structure) -> &'a str {
    constraint
        .source
        .as_ref()
        .and_then(|source| source.value.as_deref())
        .unwrap_or_else(|| structure.url())
}

/// The listing entry of `constraint`.
fn listed(constraint: &ElementDefinitionConstraint) -> Constraint {
    Constraint::Invariant {
        key: constraint.key.value.clone().unwrap_or_default(),
        severity: constraint.severity.value.clone().unwrap_or_default(),
    }
}

/// Answers whether `constraint` carries the best-practice extension set to
/// true.
fn best_practice(constraint: &ElementDefinitionConstraint) -> bool {
    constraint.extension.iter().any(|extension| {
        extension.url == BEST_PRACTICE
            && matches!(
                &extension.value,
                Some(ExtensionValue::Boolean(flag)) if flag.value == Some(true)
            )
    })
}

/// The upstream report of the defect in the invariant `key` of `source`.
fn upstream(source: &str, key: &str) -> Option<&'static str> {
    DEFECTS
        .iter()
        .find(|(declared, defect, _)| *declared == source && *defect == key)
        .map(|(_, _, report)| *report)
}

/// The text of `error` followed by each of its causes.
fn chain(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut cause = error.source();
    while let Some(inner) = cause {
        text.push_str(": ");
        text.push_str(&inner.to_string());
        cause = inner.source();
    }
    text
}

impl Validator<'_> {
    /// Returns the expression `text`, parsed on first use.
    fn expression(&self, text: &str) -> Compiled {
        let mut cache = self
            .expressions
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        cache
            .entry(text.to_owned())
            .or_insert_with(|| {
                Expression::parse(text)
                    .map(Arc::new)
                    .map_err(|error| chain(&error))
            })
            .clone()
    }

    /// Checks `expression` against the type `type_name` once, answering the
    /// first member or type name the R4 model refuses.
    fn check(&self, expression: &Expression, type_name: &str) -> Result<(), String> {
        let Some(context) = path_type(&R4, type_name).or_else(|| named_type(&R4, type_name)) else {
            return Ok(());
        };
        let key = (expression.source().to_owned(), type_name.to_owned());
        let mut cache = self.checked.lock().unwrap_or_else(PoisonError::into_inner);
        cache
            .entry(key)
            .or_insert_with(|| {
                expression
                    .check(&R4, context)
                    .map_err(|error| chain(&error))
            })
            .clone()
    }
}

impl<'i> Walk<'_, '_, 'i> {
    /// Evaluates every invariant of `element` on `subject`, once per
    /// location, key and declaring canonical.
    pub(crate) fn invariants(
        &mut self,
        structure: &Structure,
        element: &ElementDefinition,
        subject: &Subject<'_, 'i>,
    ) {
        for constraint in &element.constraint {
            self.invariant(structure, element, constraint, subject);
        }
    }

    /// Lists every invariant of `element` as unevaluated: the walk does not
    /// know the type of the occurrence at `location`.
    pub(crate) fn untyped_invariants(
        &mut self,
        structure: &Structure,
        id: &str,
        element: &ElementDefinition,
        location: &str,
    ) {
        for constraint in &element.constraint {
            let reason = Reason::InvariantRefused {
                stage: InvariantStage::Check,
                error: "the type of the element is not known".to_owned(),
                upstream: None,
            };
            self.out()
                .unevaluated(structure.url(), id, listed(constraint), location, reason);
        }
    }

    /// Evaluates one invariant on `subject`.
    fn invariant(
        &mut self,
        structure: &Structure,
        element: &ElementDefinition,
        constraint: &ElementDefinitionConstraint,
        subject: &Subject<'_, 'i>,
    ) {
        let key = constraint.key.value.as_deref().unwrap_or_default();
        let source = source(constraint, structure);
        if !self.first_invariant(subject.location, key, source) {
            return;
        }
        let id = element
            .id
            .as_deref()
            .or(element.path.value.as_deref())
            .unwrap_or_default();
        let refuse = |stage, error| Reason::InvariantRefused {
            stage,
            error,
            upstream: upstream(source, key),
        };
        let Some(text) = constraint
            .expression
            .as_ref()
            .and_then(|expression| expression.value.as_deref())
        else {
            self.out().unevaluated(
                structure.url(),
                id,
                listed(constraint),
                subject.location,
                Reason::NoExpression,
            );
            return;
        };
        let expression = match self.validator().expression(text) {
            Ok(expression) => expression,
            Err(error) => {
                let reason = refuse(InvariantStage::Parse, error);
                self.out().unevaluated(
                    structure.url(),
                    id,
                    listed(constraint),
                    subject.location,
                    reason,
                );
                return;
            }
        };
        if let Err(error) = self.validator().check(&expression, subject.type_name) {
            let reason = refuse(InvariantStage::Check, error);
            self.out().unevaluated(
                structure.url(),
                id,
                listed(constraint),
                subject.location,
                reason,
            );
            return;
        }
        match self.evaluate(&expression, structure, subject) {
            Ok(Some(true)) => {}
            Ok(answer) => {
                let issue = self.failure(constraint, source, subject.location, answer.is_none());
                self.out().push(issue);
            }
            Err(error) => {
                let reason = refuse(InvariantStage::Evaluate, error);
                self.out().unevaluated(
                    structure.url(),
                    id,
                    listed(constraint),
                    subject.location,
                    reason,
                );
            }
        }
    }

    /// Evaluates `expression` on `subject` as a Boolean (N1 §4.5): `None`
    /// for an empty result, or the error with its causes.
    fn evaluate(
        &self,
        expression: &Expression,
        structure: &Structure,
        subject: &Subject<'_, 'i>,
    ) -> Result<Option<bool>, String> {
        let scope = self.scope();
        let resource = scope.last().copied();
        let root = container(scope);
        let Some(anchor) = subject.value.or(subject.element) else {
            return Err("the occurrence carries nothing to evaluate on".to_owned());
        };
        let focus = if subject.resource {
            Focus::resource(anchor)
        } else {
            Focus::primitive(subject.value, subject.element, subject.type_name, anchor)
        }
        .within(resource, root);
        let references = References::new(self.validator(), scope);
        let terminology = Terminology::new(self.validator(), structure.origin());
        let conformance = Conformance::new(self.validator(), self.depth());
        let environment = Environment::new(&R4)
            .with_resolver(&references)
            .with_terminology(&terminology)
            .with_profiles(&conformance);
        expression
            .evaluate(&environment, focus)
            .and_then(|result| convert::boolean(&R4, &result, "an invariant"))
            .map_err(|error| chain(&error))
    }

    /// The issue a failed invariant raises at `location`.
    fn failure(
        &self,
        constraint: &ElementDefinitionConstraint,
        source: &str,
        location: &str,
        empty: bool,
    ) -> Issue {
        let key = constraint.key.value.clone().unwrap_or_default();
        let severity = if best_practice(constraint) {
            match self.validator().best_practice {
                BestPractice::Warning => Severity::Warning,
                BestPractice::Error => Severity::Error,
            }
        } else if constraint.severity.value.as_deref() == Some("warning") {
            Severity::Warning
        } else {
            Severity::Error
        };
        let human = constraint.human.value.as_deref().unwrap_or_default();
        // NOTE: conformance-rules.html §2.1.0.6: the expression "must evaluate to true", so an
        // empty result fails it as false does, the reading the HL7 FHIR Validator also takes.
        let message = if empty {
            format!("{key}: {human} (the expression answered empty)")
        } else {
            format!("{key}: {human}")
        };
        Issue {
            severity,
            code: IssueType::Invariant,
            expression: location.to_owned(),
            rule: Rule::Constraint {
                key,
                source: source.to_owned(),
            },
            message,
        }
    }
}
