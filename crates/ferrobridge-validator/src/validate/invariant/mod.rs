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
use ferrobridge_fhirpath::model::choice_element;
use ferrobridge_fhirpath::model::named_type;
use ferrobridge_fhirpath::model::path_types;
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

    /// Checks `expression` once against `context`, a type name, the element
    /// path of a backbone element or of a choice element, answering the first
    /// member or type name the R4 model refuses.
    fn check(&self, expression: &Expression, context: &str) -> Result<(), String> {
        let Some(contexts) =
            path_types(&R4, context).or_else(|| named_type(&R4, context).map(|ty| vec![ty]))
        else {
            return Ok(());
        };
        let key = (expression.source().to_owned(), context.to_owned());
        let mut cache = self.checked.lock().unwrap_or_else(PoisonError::into_inner);
        cache
            .entry(key)
            .or_insert_with(|| {
                expression
                    .check_any(&R4, &contexts)
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
        // NOTE: no specification governs this: our own design; a constraint a snapshot repeats
        // from its base, or two definitions in play share, is evaluated once per occurrence.
        if !self.first_invariant(subject.location, key, source) {
            return;
        }
        match self.answer(element, constraint, structure, subject) {
            Ok(true) => {}
            Ok(false) => {
                let issue = self.failure(constraint, source, subject.location);
                self.out().push(issue);
            }
            Err(reason) => {
                let id = element
                    .id
                    .as_deref()
                    .or(element.path.value.as_deref())
                    .unwrap_or_default();
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

    /// Answers `constraint`, a constraint of `element`, on `subject`: the
    /// Boolean its expression evaluates to, or why it was not evaluated.
    fn answer(
        &self,
        element: &ElementDefinition,
        constraint: &ElementDefinitionConstraint,
        structure: &Structure,
        subject: &Subject<'_, 'i>,
    ) -> Result<bool, Reason> {
        let key = constraint.key.value.as_deref().unwrap_or_default();
        let refuse = |stage, error| Reason::InvariantRefused {
            stage,
            error,
            upstream: upstream(source(constraint, structure), key),
        };
        let text = constraint
            .expression
            .as_ref()
            .and_then(|expression| expression.value.as_deref())
            .ok_or(Reason::NoExpression)?;
        let expression = self
            .validator()
            .expression(text)
            .map_err(|error| refuse(InvariantStage::Parse, error))?;
        // NOTE: fhirpath.html §2.9.1.1 and N1 §11: an invariant on a choice element is written
        // against the choice, so it is checked against every type the choice admits.
        let choice = element
            .path
            .value
            .as_deref()
            .filter(|path| choice_element(&R4, path).is_some());
        self.validator()
            .check(&expression, choice.unwrap_or(subject.type_name))
            .map_err(|error| refuse(InvariantStage::Check, error))?;
        // NOTE: conformance-rules.html §2.1.0.6 asks for "true" and N1 §6.2 and §6.5 carry empty
        // as the unknown of three-valued logic, so an empty answer neither meets nor breaks it.
        self.evaluate(&expression, structure, subject, choice)
            .map_err(|error| refuse(InvariantStage::Evaluate, error))?
            .ok_or(Reason::InvariantEmpty)
    }

    /// Evaluates `expression` on `subject`, an occurrence of the choice
    /// element at `choice` when one is named, as a Boolean (N1 §4.5): `None`
    /// for an empty result, or the error with its causes.
    fn evaluate(
        &self,
        expression: &Expression,
        structure: &Structure,
        subject: &Subject<'_, 'i>,
        choice: Option<&str>,
    ) -> Result<Option<bool>, String> {
        let scope = self.scope();
        let resource = scope.last().copied();
        // NOTE: fhirpath.html §2.9.1.4: %rootResource is the container of a contained %resource
        // and %resource itself otherwise, so a Bundle entry is its own root.
        let root = container(scope);
        let Some(anchor) = subject.value.or(subject.element) else {
            return Err("the occurrence carries nothing to evaluate on".to_owned());
        };
        let mut focus = if subject.resource {
            Focus::resource(anchor)
        } else {
            Focus::primitive(subject.value, subject.element, subject.type_name, anchor)
        }
        .within(resource, root);
        if let Some(path) = choice {
            focus = focus.defined_by(path);
        }
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
        let message = format!("{key}: {human}");
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
