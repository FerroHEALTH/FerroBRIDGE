// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Whether a reference names a version as `ElementDefinition.type.versioning`
//! asks: "If type.versioning is present in the definition, the 'reference'
//! element SHALL be present and have a value and the reference SHALL be
//! populated as the versioning constraint dictates"
//! (<https://hl7.org/fhir/R4/elementdefinition.html>, §2.30.0.4.4), with the
//! codes of the R4 code system `reference-version-rules`.
//!
//! A literal reference to a FHIR REST server is version specific when it
//! carries `_history/[vid]` (<https://hl7.org/fhir/R4/references.html>,
//! §2.3.0.1), and a `canonical` when it carries `|[version]` (§2.3.0.5).

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;

use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::report::Constraint;
use crate::report::Reason;
use crate::validate::reference::literal;
use crate::validate::structure::Structure;
use crate::validate::walk::Occurrence;
use crate::validate::walk::Walk;

/// What a reference says about the version of its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    /// It names a version.
    Specific,
    /// It names none.
    Independent,
    /// It is no literal reference to a FHIR REST server, which
    /// references.html says "should be assumed to be version specific".
    Assumed,
}

/// The form of the literal `reference`.
fn reference_form(reference: &str) -> Form {
    if reference.starts_with('#') || reference.contains(':') && !reference.contains("://") {
        return Form::Assumed;
    }
    let path = reference.split(['?', '#']).next().unwrap_or(reference);
    let segments: Vec<&str> = path.split('/').collect();
    let resource = |at: usize| {
        segments
            .get(at)
            .is_some_and(|name| SCHEMAS.is_resource(name))
    };
    let count = segments.len();
    let history = count >= 4 && segments.get(count - 2) == Some(&"_history") && resource(count - 4);
    if history {
        return Form::Specific;
    }
    if count >= 2 && resource(count - 2) {
        return Form::Independent;
    }
    Form::Assumed
}

/// The form of the `canonical` `value`.
fn canonical_form(value: &str) -> Form {
    if value.contains('|') {
        Form::Specific
    } else {
        Form::Independent
    }
}

impl<'i> Walk<'_, '_, 'i> {
    /// Holds the `Reference` or `canonical` `occurrence` holds to the
    /// versioning rule `rule` of element `id` of `structure`.
    pub(crate) fn versioning(
        &mut self,
        structure: &Structure,
        id: &str,
        rule: &str,
        code: Option<&str>,
        occurrence: &Occurrence<'i>,
    ) {
        let location = &occurrence.location;
        let form = match code {
            Some("Reference") => {
                let Some(object) = occurrence.value.filter(|value| value.as_object().is_some())
                else {
                    return;
                };
                let Some(reference) = literal(object) else {
                    self.version_issue(
                        location,
                        format!("{id} sets the versioning {rule}, and the reference carries no literal reference"),
                    );
                    return;
                };
                reference_form(reference)
            }
            Some("canonical") => match occurrence.value.and_then(Value::as_str) {
                Some(value) => canonical_form(value),
                None => return,
            },
            _ => return,
        };
        let fault = match (rule, form) {
            ("specific", Form::Independent) => "names no version",
            ("independent", Form::Specific) => "names a version",
            // NOTE: references.html §2.3.0.1 Reference.reference: a reference not to a FHIR
            // REST server "should be assumed to be version specific", so independent is undecided.
            ("independent", Form::Assumed) => {
                self.out().unevaluated(
                    structure.url(),
                    id,
                    Constraint::Versioning {
                        rule: rule.to_owned(),
                    },
                    location,
                    Reason::NotRestfulReference,
                );
                return;
            }
            ("specific" | "independent" | "either", _) => return,
            _ => {
                self.out().unevaluated(
                    structure.url(),
                    id,
                    Constraint::Versioning {
                        rule: rule.to_owned(),
                    },
                    location,
                    Reason::VersioningRule(rule.to_owned()),
                );
                return;
            }
        };
        self.version_issue(
            location,
            format!("{id} sets the versioning {rule}, and the reference {fault}"),
        );
    }

    /// Reports a reference that breaks its versioning rule.
    fn version_issue(&mut self, location: &str, message: String) {
        self.out().issue(
            Severity::Error,
            IssueType::Structure,
            MessageId::VersioningMismatch,
            location,
            message,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::Form;
    use super::canonical_form;
    use super::reference_form;

    #[test]
    fn a_literal_reference_is_read_for_its_version() {
        assert_eq!(reference_form("Patient/1"), Form::Independent);
        assert_eq!(reference_form("Patient/1/_history/2"), Form::Specific);
        assert_eq!(
            reference_form("http://example.org/fhir/Patient/1/_history/2"),
            Form::Specific
        );
        assert_eq!(
            reference_form("http://example.org/fhir/Patient/1"),
            Form::Independent
        );
        assert_eq!(reference_form("#p1"), Form::Assumed);
        assert_eq!(
            reference_form("urn:uuid:00000000-0000-0000-0000-000000000001"),
            Form::Assumed
        );
        assert_eq!(
            reference_form("http://example.org/document.pdf"),
            Form::Assumed
        );
    }

    #[test]
    fn a_canonical_names_a_version_after_a_bar() {
        assert_eq!(canonical_form("http://example.org/vs|1.0"), Form::Specific);
        assert_eq!(canonical_form("http://example.org/vs"), Form::Independent);
    }
}
