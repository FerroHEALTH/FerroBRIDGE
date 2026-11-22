// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The committal metadata a `POST` or `PUT` of a COMPOSITION carries.
//!
//! ITS-REST 1.1.0 §Requests and responses/HTTP headers/openehr-version and
//! openehr-audit-details defines these headers in prose only: the three
//! `OpenAPI` documents declare none of them. The header names are the 1.1.0
//! ones; the same section's deprecation table maps the 1.0.3 spellings
//! (`openEHR-VERSION`, `openEHR-AUDIT_DETAILS`, `openEHR-TEMPLATE_ID`, which
//! carried the attribute path in the header NAME) onto them, and the bridge
//! sends only the 1.1.0 form. The generated parameters of the commit
//! operations carry the three as fields, and this module renders their values.
//!
//! The audit header states no `committer`: the CDR writes who committed from
//! the caller it authenticated, and a CDR that does so refuses a stated
//! committer that says more than its own. The header leaves it out, which the
//! same section allows ("None of these headers are mandatory").

use http::HeaderValue;
use openehr_base::v1_3::base_types::identification::template_id::TemplateId;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
use openehr_rm::v1_2::data_types::text::dv_text::DvText;

/// The `openehr-version` header name.
pub const VERSION_HEADER: &str = "openehr-version";
/// The `openehr-audit-details` header name.
pub const AUDIT_DETAILS_HEADER: &str = "openehr-audit-details";
/// The `openehr-template-id` header name.
pub const TEMPLATE_ID_HEADER: &str = "openehr-template-id";

/// A commit header that could not be rendered.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HeaderError {
    /// A free-text part of a commit header carries a quote or a control
    /// character, which the quoted `key="value"` form cannot hold without
    /// changing the attribute list the service reads.
    #[error("an attribute of the {header} header carries text the quoted value form cannot hold")]
    Attribute {
        /// The header that was being built.
        header: &'static str,
        /// Which attribute, and which character.
        #[source]
        source: CodeError,
    },
    /// A value cannot travel in an HTTP header.
    #[error("the value for the {header} header is not a legal header value")]
    Value {
        /// The header that was being built.
        header: &'static str,
        /// What the HTTP stack reported.
        #[source]
        source: http::header::InvalidHeaderValue,
    },
}

/// A header attribute that would not survive the quoted value grammar.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CodeError {
    /// The attribute was empty.
    #[error("a {kind} must not be empty")]
    Empty {
        /// The attribute that was being rendered.
        kind: &'static str,
    },
    /// The attribute carried a character the quoted header value cannot hold.
    #[error("a {kind} must not contain {character:?}")]
    ForbiddenCharacter {
        /// The attribute that was being rendered.
        kind: &'static str,
        /// The offending character.
        character: char,
    },
}

/// Returns `text` as a quotable header value part of `kind`.
fn quotable(kind: &'static str, text: &str) -> Result<String, CodeError> {
    if text.is_empty() {
        return Err(CodeError::Empty { kind });
    }
    if let Some(character) = text.chars().find(|c| *c == '"' || c.is_control()) {
        return Err(CodeError::ForbiddenCharacter { kind, character });
    }
    Ok(text.to_owned())
}

/// The committal metadata headers one commit renders into, in the shape the
/// generated parameters of the commit operations carry them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitHeaders {
    /// The `openehr-version` value.
    pub version: Option<String>,
    /// The `openehr-audit-details` values, one field line each.
    pub audit_details: Option<Vec<String>>,
    /// The `openehr-template-id` value.
    pub template_id: Option<String>,
}

/// The `openehr-audit-details` members a header commit states: the
/// `UpdateAudit` members of ITS-REST 1.1.0 apart from `committer`, which the
/// CDR writes from the caller it authenticated.
#[derive(Debug, Clone)]
pub struct HeaderAudit {
    /// `AUDIT_DETAILS.change_type`.
    pub change_type: DvCodedText,
    /// `AUDIT_DETAILS.description`.
    pub description: Option<DvText>,
    /// `AUDIT_DETAILS.system_id`.
    pub system_id: Option<String>,
}

impl HeaderAudit {
    /// Returns an audit stating `change_type` alone.
    #[must_use]
    pub const fn new(change_type: DvCodedText) -> Self {
        Self {
            change_type,
            description: None,
            system_id: None,
        }
    }
}

/// The committal metadata one commit carries.
///
/// Every member is optional: "None of these headers are mandatory, but
/// whatever is provided it MUST be merged with the default VERSION and
/// `VERSION.audit_details` attributes on commit runtime" (ITS-REST 1.1.0
/// §Requests and responses/HTTP headers/openehr-version and
/// openehr-audit-details). `time_committed` is always the server's, and so
/// is `committer`.
#[derive(Debug, Clone, Default)]
pub struct CommitContext {
    /// `VERSION.lifecycle_state`, sent as `openehr-version`.
    pub lifecycle_state: Option<DvCodedText>,
    /// `change_type`, `description` and `system_id`, sent as
    /// `openehr-audit-details`.
    pub audit: Option<HeaderAudit>,
    /// The template the composition is built from, sent as
    /// `openehr-template-id`.
    pub template_id: Option<TemplateId>,
}

impl CommitContext {
    /// Returns the header values this context renders into, in wire order.
    ///
    /// # Errors
    /// Returns [`HeaderError::Attribute`] when a member carries text the
    /// quoted attribute form cannot hold, and [`HeaderError::Value`] when a
    /// member carries text no HTTP header value can hold.
    pub fn headers(&self) -> Result<CommitHeaders, HeaderError> {
        let mut headers = CommitHeaders::default();
        if let Some(state) = self.lifecycle_state.as_ref() {
            let code = quoted(
                VERSION_HEADER,
                "lifecycle_state",
                &state.defining_code.code_string,
            )?;
            headers.version = Some(header_value(
                VERSION_HEADER,
                format!("lifecycle_state.code_string=\"{code}\""),
            )?);
        }
        if let Some(audit) = self.audit.as_ref() {
            let mut lines = Vec::new();
            let code = quoted(
                AUDIT_DETAILS_HEADER,
                "change_type",
                &audit.change_type.defining_code.code_string,
            )?;
            lines.push(format!("change_type.code_string=\"{code}\""));
            if let Some(description) = audit.description.as_ref() {
                let value = match description {
                    DvText::DvText(text) => &text.value,
                    DvText::DvCodedText(text) => &text.value,
                };
                let description = quoted(AUDIT_DETAILS_HEADER, "description", value)?;
                lines.push(format!("description.value=\"{description}\""));
            }
            if let Some(system_id) = audit.system_id.as_ref() {
                let system_id = quoted(AUDIT_DETAILS_HEADER, "system_id", system_id)?;
                lines.push(format!("system_id=\"{system_id}\""));
            }
            headers.audit_details = Some(
                lines
                    .into_iter()
                    .map(|line| header_value(AUDIT_DETAILS_HEADER, line))
                    .collect::<Result<_, _>>()?,
            );
        }
        if let Some(template_id) = self.template_id.as_ref() {
            if template_id.value.is_empty() {
                return Err(HeaderError::Attribute {
                    header: TEMPLATE_ID_HEADER,
                    source: CodeError::Empty {
                        kind: "template_id",
                    },
                });
            }
            headers.template_id =
                Some(header_value(TEMPLATE_ID_HEADER, template_id.value.clone())?);
        }
        Ok(headers)
    }
}

/// Returns `value` when it can travel as a value of `header`.
fn header_value(header: &'static str, value: String) -> Result<String, HeaderError> {
    HeaderValue::from_str(&value).map_err(|source| HeaderError::Value { header, source })?;
    Ok(value)
}

/// Returns `text` as a quoted attribute value of `header`, refusing a quote or
/// a control character that would rewrite the attribute list.
fn quoted(header: &'static str, kind: &'static str, text: &str) -> Result<String, HeaderError> {
    quotable(kind, text).map_err(|source| HeaderError::Attribute { header, source })
}

#[cfg(test)]
mod tests {
    use super::{
        AUDIT_DETAILS_HEADER, CommitContext, HeaderAudit, HeaderError, TEMPLATE_ID_HEADER,
        VERSION_HEADER,
    };
    use openehr_base::v1_3::base_types::identification::template_id::TemplateId;
    use openehr_base::v1_3::base_types::identification::terminology_id::TerminologyId;
    use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
    use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
    use openehr_rm::v1_2::data_types::text::dv_text::{DvText, DvTextData};

    fn rendered(context: &CommitContext) -> Vec<(&'static str, String)> {
        let headers = context.headers().expect("the context renders");
        let version = headers
            .version
            .into_iter()
            .map(|value| (VERSION_HEADER, value));
        let audit = headers
            .audit_details
            .into_iter()
            .flatten()
            .map(|value| (AUDIT_DETAILS_HEADER, value));
        let template = headers
            .template_id
            .into_iter()
            .map(|value| (TEMPLATE_ID_HEADER, value));
        version.chain(audit).chain(template).collect()
    }

    fn coded(code: &str) -> DvCodedText {
        DvCodedText {
            value: String::from("synthetic"),
            hyperlink: None,
            formatting: None,
            mappings: None,
            language: None,
            encoding: None,
            defining_code: CodePhrase {
                terminology_id: TerminologyId {
                    value: String::from("openehr"),
                },
                code_string: String::from(code),
                preferred_term: None,
            },
        }
    }

    fn text(value: &str) -> DvText {
        DvText::DvText(DvTextData {
            value: String::from(value),
            hyperlink: None,
            formatting: None,
            mappings: None,
            language: None,
            encoding: None,
        })
    }

    fn audit(change_type: &str) -> HeaderAudit {
        HeaderAudit::new(coded(change_type))
    }

    #[test]
    fn the_headers_carry_the_1_1_0_value_form_with_no_committer() {
        let context = CommitContext {
            lifecycle_state: Some(coded("532")),
            audit: Some(HeaderAudit {
                description: Some(text("An updated composition")),
                system_id: Some(String::from("example.openehr.systemid")),
                ..audit("251")
            }),
            template_id: Some(TemplateId {
                value: String::from("Vital Signs"),
            }),
        };
        assert_eq!(
            vec![
                (
                    VERSION_HEADER,
                    "lifecycle_state.code_string=\"532\"".to_owned()
                ),
                (
                    AUDIT_DETAILS_HEADER,
                    "change_type.code_string=\"251\"".to_owned()
                ),
                (
                    AUDIT_DETAILS_HEADER,
                    "description.value=\"An updated composition\"".to_owned()
                ),
                (
                    AUDIT_DETAILS_HEADER,
                    "system_id=\"example.openehr.systemid\"".to_owned()
                ),
                (TEMPLATE_ID_HEADER, "Vital Signs".to_owned()),
            ],
            rendered(&context)
        );
    }

    #[test]
    fn an_empty_context_renders_no_header() {
        assert_eq!(
            rendered(&CommitContext::default()),
            Vec::<(&str, String)>::new()
        );
    }

    #[test]
    fn a_code_refuses_a_quote() {
        let context = CommitContext {
            audit: Some(audit("2\"51")),
            ..CommitContext::default()
        };
        assert!(matches!(
            context.headers(),
            Err(HeaderError::Attribute { header, .. }) if header == AUDIT_DETAILS_HEADER
        ));
        let context = CommitContext {
            lifecycle_state: Some(coded("")),
            ..CommitContext::default()
        };
        assert!(matches!(
            context.headers(),
            Err(HeaderError::Attribute { header, .. }) if header == VERSION_HEADER
        ));
    }

    #[test]
    fn a_quote_in_a_free_text_part_is_refused_before_the_header_is_built() {
        let context = CommitContext {
            audit: Some(HeaderAudit {
                description: Some(text("first\", change_type.code_string=\"249")),
                ..audit("251")
            }),
            ..CommitContext::default()
        };
        let error = context
            .headers()
            .expect_err("a quote rewrites the attribute list");
        assert!(
            matches!(error, HeaderError::Attribute { header, .. } if header == AUDIT_DETAILS_HEADER),
            "expected a header attribute refusal, got {error:?}"
        );
        let context = CommitContext {
            audit: Some(HeaderAudit {
                system_id: Some(String::from("sys\"tem")),
                ..audit("251")
            }),
            ..CommitContext::default()
        };
        assert!(matches!(
            context.headers(),
            Err(HeaderError::Attribute { .. })
        ));
    }

    #[test]
    fn a_description_that_is_not_a_header_value_is_a_typed_error() {
        let context = CommitContext {
            audit: Some(HeaderAudit {
                description: Some(text("first\nsecond")),
                ..audit("251")
            }),
            ..CommitContext::default()
        };
        let error = context.headers().expect_err("a newline is refused");
        assert!(
            matches!(error, HeaderError::Attribute { header, .. } if header == AUDIT_DETAILS_HEADER)
        );
    }
}
