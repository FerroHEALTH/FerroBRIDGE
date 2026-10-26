// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The engine's declared losses as `OperationOutcome` issues.
//!
//! "Where an engine returns a partial result rather than failing outright ...
//! those gaps MUST be made explicit through the `OperationOutcome` rather than
//! silently dropping data" (`engine/rest-api.adoc`
//! §Reporting partial results with `OperationOutcome`). FerroBRIDGE never
//! returns a partial result, so what reaches an outcome here is the closed set
//! of losses [`crate::engine::outcome::Loss`] declares, and nothing else.
//!
//! A composition field the engine filled is `information`, because the run
//! produced a value the source did not carry. Everything else is `warning`,
//! because content the source carried did not reach the output. The chapter
//! codes its own worked example `incomplete`, which is the type used here.
//!
//! Each issue says where its loss happened. The FHIRPath of the FHIR element
//! is the issue's `expression`, and the archetype path of the openEHR node is
//! a `details.coding` in [`OPENEHR_PATH_SYSTEM`], so a loss with no FHIR
//! element (a composition field the engine defaulted) is still located. The
//! `details.text` stays the loss's own sentence.

use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::operation_outcome::OperationOutcome;
use fhir_types::r4::operation_outcome::OperationOutcomeIssue;
use fhir_types::r4::primitives;
use fhir_types::r4::primitives::Code;

use crate::engine::outcome::Loss;
use crate::engine::outcome::Warning;

/// The `error` severity of `IssueSeverity`.
pub const ERROR: &str = "error";

/// The `warning` severity of `IssueSeverity`.
pub const WARNING: &str = "warning";

/// The `information` severity of `IssueSeverity`.
pub const INFORMATION: &str = "information";

/// The system of the `details.coding` that carries the openEHR path a loss
/// happened at.
///
/// The code is the archetype path of the RM node
/// ([`crate::engine::outcome::Location::openehr`]), never a value. No
/// specification governs this: our own design. R4 names no code system for
/// openEHR paths, and `OperationOutcome.issue.expression` takes FHIRPath
/// only, so the openEHR side travels as a coding of its own.
pub const OPENEHR_PATH_SYSTEM: &str = "https://ferrobridge.eu/fhir/CodeSystem/openehr-path";

/// Returns one issue with `severity`, `code` and `text`.
///
/// The severities are the `IssueSeverity` codes
/// (<https://hl7.org/fhir/R4/valueset-issue-severity.html>) and the codes the
/// `IssueType` ones (<https://hl7.org/fhir/R4/valueset-issue-type.html>).
#[must_use]
pub fn issue(severity: &str, code: &str, text: &str) -> OperationOutcomeIssue {
    OperationOutcomeIssue {
        severity: Code::from(severity),
        code: Code::from(code),
        details: Some(CodeableConcept {
            text: Some(primitives::String::from(text)),
            ..CodeableConcept::default()
        }),
        ..OperationOutcomeIssue::default()
    }
}

/// Returns the issue one declared loss renders as.
///
/// The issue's `expression` is the FHIRPath of the FHIR element the loss
/// concerns, when it has one, and its `details` carry the openEHR path in
/// [`OPENEHR_PATH_SYSTEM`], when it has one.
#[must_use]
pub fn of_warning(warning: &Warning) -> OperationOutcomeIssue {
    let severity = match *warning.loss() {
        Loss::Defaulted { .. } | Loss::InapplicableOccurrences { .. } => INFORMATION,
        Loss::Skipped { .. }
        | Loss::LastOfMany { .. }
        | Loss::OneWayFallback { .. }
        | Loss::UnmappedOccurrences { .. } => WARNING,
    };
    let code = match *warning.loss() {
        Loss::Defaulted { .. } | Loss::InapplicableOccurrences { .. } => "informational",
        Loss::Skipped { .. }
        | Loss::LastOfMany { .. }
        | Loss::OneWayFallback { .. }
        | Loss::UnmappedOccurrences { .. } => "incomplete",
    };
    let mut rendered = issue(severity, code, &warning.to_string());
    let location = warning.location();
    // NOTE: R4 OperationOutcome.issue.expression, "a simple subset of FHIRPath limited to
    // element names, repetition indicators and the default child accessor", the subset
    // `tree::locate` writes; an openEHR path is no FHIRPath, so it never goes here.
    if let Some(path) = location.fhir() {
        rendered.expression = vec![primitives::String::from(path)];
    }
    if let (Some(path), Some(details)) = (location.openehr(), rendered.details.as_mut()) {
        details.coding.push(Coding {
            system: Some(primitives::Uri::from(OPENEHR_PATH_SYSTEM)),
            code: Some(Code::from(path)),
            ..Coding::default()
        });
    }
    rendered
}

/// Returns the outcome `warnings` render as, or `None` when there are none.
///
/// A run that declared no loss answers no outcome at all: the chapter's
/// `outcome` parameter is "Present when mapping issues occurred".
#[must_use]
pub fn of_warnings(warnings: &[Warning]) -> Option<OperationOutcome> {
    if warnings.is_empty() {
        return None;
    }
    Some(OperationOutcome {
        issue: warnings.iter().map(of_warning).collect(),
        ..OperationOutcome::default()
    })
}

#[cfg(test)]
mod tests {
    use super::OPENEHR_PATH_SYSTEM;
    use super::of_warning;
    use super::of_warnings;
    use crate::engine::outcome::Location;
    use crate::engine::outcome::Loss;
    use crate::engine::outcome::SkipReason;
    use crate::engine::outcome::Warning;

    #[test]
    fn no_loss_renders_no_outcome() {
        assert!(of_warnings(&[]).is_none());
    }

    #[test]
    fn a_defaulted_field_is_information() {
        let issue = of_warning(&Warning::new(
            Loss::Defaulted {
                field: String::from("/context/start_time"),
            },
            Location::default().with_openehr("/context/start_time"),
        ));
        assert_eq!(issue.severity.value.as_deref(), Some("information"));
        assert_eq!(issue.code.value.as_deref(), Some("informational"));
    }

    #[test]
    fn a_skipped_mapping_is_an_incomplete_warning() {
        let issue = of_warning(&Warning::new(
            Loss::Skipped {
                mapping: String::from("bodySite"),
                reason: SkipReason::Unidirectional,
            },
            Location::default(),
        ));
        assert_eq!(issue.severity.value.as_deref(), Some("warning"));
        assert_eq!(issue.code.value.as_deref(), Some("incomplete"));
    }

    #[test]
    fn a_fhir_location_is_the_expression_and_an_openehr_one_a_coding() {
        let issue = of_warning(&Warning::new(
            Loss::Skipped {
                mapping: String::from("bodySite"),
                reason: SkipReason::Unidirectional,
            },
            Location::default()
                .with_fhir("Condition.bodySite[0]")
                .with_openehr("/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0012]"),
        ));
        let expressions: Vec<Option<&str>> = issue
            .expression
            .iter()
            .map(|expression| expression.value.as_deref())
            .collect();
        assert_eq!(expressions, [Some("Condition.bodySite[0]")]);
        let details = issue.details.expect("the issue carries details");
        let codings: Vec<(Option<&str>, Option<&str>)> = details
            .coding
            .iter()
            .map(|coding| {
                (
                    coding
                        .system
                        .as_ref()
                        .and_then(|system| system.value.as_deref()),
                    coding.code.as_ref().and_then(|code| code.value.as_deref()),
                )
            })
            .collect();
        assert_eq!(
            codings,
            [(
                Some(OPENEHR_PATH_SYSTEM),
                Some(
                    "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0012]"
                )
            )]
        );
        assert_eq!(
            details.text.and_then(|text| text.value),
            Some(String::from(
                "the mapping bodySite did not run: its unidirectional marker names the other direction"
            ))
        );
    }

    #[test]
    fn a_loss_with_no_fhir_element_carries_no_expression() {
        let issue = of_warning(&Warning::new(
            Loss::Defaulted {
                field: String::from("/composer"),
            },
            Location::default().with_openehr("/composer"),
        ));
        assert!(issue.expression.is_empty(), "{:?}", issue.expression);
        let codes: Vec<Option<String>> = issue
            .details
            .map(|details| details.coding)
            .unwrap_or_default()
            .into_iter()
            .map(|coding| coding.code.and_then(|code| code.value))
            .collect();
        assert_eq!(codes, [Some(String::from("/composer"))]);
    }
}
