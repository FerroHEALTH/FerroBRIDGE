// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-release reader and writer of the FHIR bodies.
//!
//! Each release has its own generated model, so the same code is generated
//! once per release by `per_release` and the functions below dispatch on
//! [`WireVersion`]. The request and response contracts come from
//! `fhir_types::stu3::operations`, `fhir_types::r4::operations` and
//! `fhir_types::r4b::operations`; this module writes no FHIR by hand.
//!
//! R4 and R4B declare identical `in` and `out` parameter sets for the three
//! operations (the vendored `OperationDefinition-CodeSystem-lookup`,
//! `-ConceptMap-translate` and `-ValueSet-validate-code` of both packages), so
//! they share `r4_contracts`. STU3 differs in three places, which
//! `stu3_contracts` carries (the same three definitions in
//! `hl7.fhir.r3.core` 3.0.2): `$validate-code` names the code system version
//! `version` and has no `valueSetVersion`, `$translate` has no `url`, and
//! `$lookup` types `property.value` as `code`. A parameter the request
//! carries and STU3 does not declare is refused before anything is sent.

use crate::config::WireVersion;
use crate::error::{BodyError, Error, Outcome};
use crate::outcome::{LookupOutcome, Request, TranslateOutcome, ValidateOutcome};
use http::StatusCode;

/// The `HTTPVerb` code a batch entry carries for the `POST` form of an
/// operation (<https://hl7.org/fhir/R4/valueset-http-verb.html>).
const POST: &str = "POST";

/// The `BundleType` code of a request bundle whose entries are independent
/// (<https://hl7.org/fhir/R4/valueset-bundle-type.html>).
const BATCH: &str = "batch";

/// The `BundleType` code the server answers a `batch` with.
const BATCH_RESPONSE: &str = "batch-response";

/// One entry of a `batch-response` `Bundle`, in request order.
#[derive(Debug)]
pub(crate) struct BatchEntry {
    /// The status `entry.response.status` names.
    pub(crate) status: StatusCode,
    /// `entry.resource`, written back as FHIR JSON.
    pub(crate) body: String,
}

/// Why a request could not be written in the configured release.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RequestError {
    /// The `Parameters` or the `Bundle` could not be written as JSON.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// The request carries a parameter the release's operation does not
    /// declare and has no equivalent for.
    #[error("the {operation} request carries `{parameter}`, which the release does not declare")]
    Unsupported {
        /// The operation, as `Resource/$code`.
        operation: &'static str,
        /// The parameter, as the R4 `OperationDefinition` names it.
        parameter: &'static str,
    },
}

impl RequestError {
    /// Returns the client error for this failure of the `operation` request
    /// in `version`.
    pub(crate) fn into_error(self, operation: &'static str, version: WireVersion) -> Error {
        match self {
            Self::Json(source) => Error::RequestBody { operation, source },
            Self::Unsupported {
                operation,
                parameter,
            } => Error::UnsupportedParameter {
                operation,
                parameter,
                wire_version: version,
            },
        }
    }
}

/// The request writers of R4 and R4B, which declare the same parameters.
macro_rules! r4_contracts {
    () => {
        /// Returns the parameter of `request` this release cannot carry.
        ///
        /// R4 and R4B declare every parameter a [`Request`] carries.
        const fn unsupported(_request: &Request) -> Option<&'static str> {
            None
        }

        /// Returns the `Parameters` that carries `request`.
        fn parameters(request: &Request) -> Parameters {
            match request {
                Request::Lookup {
                    system,
                    code,
                    version,
                } => CodeSystemLookupRequest {
                    system: Some(system.as_str().into()),
                    code: Some(code.as_str().into()),
                    version: version.as_deref().map(Into::into),
                    ..Default::default()
                }
                .into_parameters(),
                Request::Translate(translation) => ConceptMapTranslateRequest {
                    url: translation.concept_map_url.as_deref().map(Into::into),
                    system: Some(translation.system.as_str().into()),
                    code: Some(translation.code.as_str().into()),
                    version: translation.version.as_deref().map(Into::into),
                    targetsystem: translation.target_system.as_deref().map(Into::into),
                    reverse: translation.reverse.map(Into::into),
                    ..Default::default()
                }
                .into_parameters(),
                Request::ValidateCode(validation) => validate_code_parameters(validation),
            }
        }

        /// Returns the `Parameters` of one `ValueSet/$validate-code`.
        fn validate_code_parameters(validation: &Validation) -> Parameters {
            let mut request = ValueSetValidateCodeRequest {
                url: Some(validation.value_set_url.as_str().into()),
                value_set_version: validation.value_set_version.as_deref().map(Into::into),
                ..Default::default()
            };
            match &validation.coded {
                Coded::Code {
                    system,
                    code,
                    version,
                    display,
                } => {
                    request.system = Some(system.as_str().into());
                    request.code = Some(code.as_str().into());
                    request.system_version = version.as_deref().map(Into::into);
                    request.display = display.as_deref().map(Into::into);
                }
                Coded::Coding(concept) => request.coding = Some(coding(concept)),
                Coded::CodeableConcept {
                    coding: codings,
                    text,
                } => {
                    request.codeable_concept = Some(codeable_concept(codings, text.as_deref()));
                }
            }
            request.into_parameters()
        }

        /// Returns one `property.value`, which these releases type as any
        /// FHIR type (`OperationDefinition-CodeSystem-lookup`, R4).
        fn lookup_property_value(
            value: ParametersParameterValue,
        ) -> Result<PropertyValue, BodyError> {
            property_value(value)
        }

        /// Returns the `issues` outcome of a false `$validate-code` answer.
        fn invalid_issues(
            answer: &ValueSetValidateCodeResponse,
        ) -> Result<Option<Outcome>, BodyError> {
            answer
                .issues
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?
                .map(|text| outcome(&text))
                .transpose()
        }
    };
}

/// The request writers of STU3, after `OperationDefinition-CodeSystem-lookup`,
/// `-ConceptMap-translate` and `-ValueSet-validate-code` of
/// `hl7.fhir.r3.core` 3.0.2.
macro_rules! stu3_contracts {
    () => {
        /// Returns the parameter of `request` STU3 declares no equivalent for.
        ///
        /// STU3 `$translate` has no `url` and names no concept map
        /// (<https://hl7.org/fhir/STU3/conceptmap-operations.html#translate>),
        /// and STU3 `$validate-code` has no `valueSetVersion`
        /// (<https://hl7.org/fhir/STU3/valueset-operations.html#validate-code>):
        /// its `version` is the code system's.
        const fn unsupported(request: &Request) -> Option<&'static str> {
            match request {
                Request::Translate(crate::outcome::Translation {
                    concept_map_url: Some(_),
                    ..
                }) => Some("url"),
                Request::ValidateCode(Validation {
                    value_set_version: Some(_),
                    ..
                }) => Some("valueSetVersion"),
                _ => None,
            }
        }

        /// Returns the `Parameters` that carries `request`, which
        /// `unsupported` has passed.
        fn parameters(request: &Request) -> Parameters {
            match request {
                Request::Lookup {
                    system,
                    code,
                    version,
                } => CodeSystemLookupRequest {
                    system: Some(system.as_str().into()),
                    code: Some(code.as_str().into()),
                    version: version.as_deref().map(Into::into),
                    ..Default::default()
                }
                .into_parameters(),
                Request::Translate(translation) => ConceptMapTranslateRequest {
                    system: Some(translation.system.as_str().into()),
                    code: Some(translation.code.as_str().into()),
                    version: translation.version.as_deref().map(Into::into),
                    targetsystem: translation.target_system.as_deref().map(Into::into),
                    reverse: translation.reverse.map(Into::into),
                    ..Default::default()
                }
                .into_parameters(),
                Request::ValidateCode(validation) => validate_code_parameters(validation),
            }
        }

        /// Returns the `Parameters` of one STU3 `ValueSet/$validate-code`.
        fn validate_code_parameters(validation: &Validation) -> Parameters {
            let mut request = ValueSetValidateCodeRequest {
                url: Some(validation.value_set_url.as_str().into()),
                ..Default::default()
            };
            match &validation.coded {
                Coded::Code {
                    system,
                    code,
                    version,
                    display,
                } => {
                    request.system = Some(system.as_str().into());
                    request.code = Some(code.as_str().into());
                    // NOTE: STU3 `version` is "the version of the system", R4's `systemVersion`
                    // (`OperationDefinition-ValueSet-validate-code`, 3.0.2 and 4.0.1).
                    request.version = version.as_deref().map(Into::into);
                    request.display = display.as_deref().map(Into::into);
                }
                Coded::Coding(concept) => request.coding = Some(coding(concept)),
                Coded::CodeableConcept {
                    coding: codings,
                    text,
                } => {
                    request.codeable_concept = Some(codeable_concept(codings, text.as_deref()));
                }
            }
            request.into_parameters()
        }

        /// Returns one `property.value`, which STU3 types as `code`
        /// (`OperationDefinition-CodeSystem-lookup`, 3.0.2).
        fn lookup_property_value(
            value: fhir_types::stu3::primitives::Code,
        ) -> Result<PropertyValue, BodyError> {
            property_value(ParametersParameterValue::Code(value))
        }

        /// Returns the `issues` outcome of a false `$validate-code` answer,
        /// a parameter STU3 does not declare.
        const fn invalid_issues(
            _answer: &ValueSetValidateCodeResponse,
        ) -> Result<Option<Outcome>, BodyError> {
            Ok(None)
        }
    };
}

/// Generates the reader and writer of one FHIR release, with the request
/// writers `$contracts` expands to.
macro_rules! per_release {
    ($module:ident, $release:ident, $contracts:ident) => {
        /// The bodies of one FHIR release, over its own generated model.
        pub(crate) mod $module {
            use super::{BatchEntry, RequestError, BATCH, BATCH_RESPONSE, POST};
            use crate::concept::{
                Concept, Designation, Equivalence, Match, Property, PropertyValue,
            };
            use crate::error::{BodyError, Issue, Outcome, TxIssue};
            use crate::outcome::{
                Coded, LookupOutcome, Request, TranslateOutcome, ValidateOutcome, Validation,
            };
            use fhir_types::$release::bundle::{Bundle, BundleEntry, BundleEntryRequest};
            use fhir_types::$release::codeable_concept::CodeableConcept;
            use fhir_types::$release::coding::Coding;
            use fhir_types::$release::operation_outcome::OperationOutcome;
            use fhir_types::$release::operations::code_system_lookup::CodeSystemLookupRequest;
            use fhir_types::$release::operations::code_system_lookup::CodeSystemLookupResponse;
            use fhir_types::$release::operations::concept_map_translate::ConceptMapTranslateRequest;
            use fhir_types::$release::operations::concept_map_translate::ConceptMapTranslateResponse;
            use fhir_types::$release::operations::value_set_validate_code::ValueSetValidateCodeRequest;
            use fhir_types::$release::operations::value_set_validate_code::ValueSetValidateCodeResponse;
            use fhir_types::$release::parameters::{
                Parameters, ParametersParameter, ParametersParameterValue,
            };
            use fhir_types::$release::resource::Resource;
            use fhir_types::codec::Json;

            $contracts!();

            /// Returns the `Parameters` that carries `request`, or the
            /// parameter this release cannot carry.
            fn encode(request: &Request) -> Result<Parameters, RequestError> {
                match unsupported(request) {
                    Some(parameter) => Err(RequestError::Unsupported {
                        operation: request.operation(),
                        parameter,
                    }),
                    None => Ok(parameters(request)),
                }
            }

            /// Returns `concept` as a `Coding` of this release.
            fn coding(concept: &Concept) -> Coding {
                Coding {
                    system: concept.system.as_deref().map(Into::into),
                    version: concept.version.as_deref().map(Into::into),
                    code: concept.code.as_deref().map(Into::into),
                    display: concept.display.as_deref().map(Into::into),
                    ..Default::default()
                }
            }

            /// Returns `codings` and `text` as a `CodeableConcept` of this
            /// release.
            fn codeable_concept(codings: &[Concept], text: Option<&str>) -> CodeableConcept {
                CodeableConcept {
                    coding: codings.iter().map(coding).collect(),
                    text: text.map(Into::into),
                    ..Default::default()
                }
            }

            /// Returns the parameter of `request` this release cannot carry.
            pub(crate) const fn unsupported_parameter(request: &Request) -> Option<&'static str> {
                unsupported(request)
            }

            /// Returns the FHIR JSON body that carries `request`.
            pub(crate) fn request_body(request: &Request) -> Result<String, RequestError> {
                Ok(serde_json::to_string(&encode(request)?)?)
            }

            /// Returns the FHIR JSON `batch` Bundle that carries `requests`.
            ///
            /// A request this release cannot carry refuses the whole batch,
            /// so nothing is sent.
            pub(crate) fn batch_body(requests: &[Request]) -> Result<String, RequestError> {
                let entry = requests
                    .iter()
                    .map(|request| {
                        Ok(BundleEntry {
                            request: Some(BundleEntryRequest {
                                method: POST.into(),
                                url: super::path_of(request).into(),
                                ..Default::default()
                            }),
                            resource: Some(Resource::Parameters(Box::new(encode(request)?))),
                            ..Default::default()
                        })
                    })
                    .collect::<Result<Vec<_>, RequestError>>()?;
                let bundle = Bundle {
                    r#type: BATCH.into(),
                    entry,
                    ..Default::default()
                };
                Ok(serde_json::to_string(&bundle)?)
            }

            /// Returns the entries of the `batch-response` `body` carries.
            pub(crate) fn batch_entries(body: &str) -> Result<Vec<BatchEntry>, BodyError> {
                let bundle = serde_json::from_str::<Bundle>(body)?;
                let kind = bundle.r#type.value.clone().unwrap_or_default();
                if kind != BATCH_RESPONSE {
                    return Err(BodyError::BatchType { found: kind });
                }
                bundle.entry.into_iter().map(entry).collect()
            }

            /// Returns one `batch-response` entry as its status and body.
            fn entry(entry: BundleEntry) -> Result<BatchEntry, BodyError> {
                let status = entry
                    .response
                    .as_ref()
                    .and_then(|response| response.status.value.as_deref())
                    .and_then(super::status_of)
                    .ok_or(BodyError::EntryStatus)?;
                let resource = entry.resource.ok_or(BodyError::EntryResource)?;
                Ok(BatchEntry {
                    status,
                    body: serde_json::to_string(&resource)?,
                })
            }

            /// Returns the `OperationOutcome` that `body` carries.
            pub(crate) fn outcome(body: &str) -> Result<Outcome, BodyError> {
                let decoded = serde_json::from_str::<OperationOutcome>(body)?;
                let issues = decoded
                    .issue
                    .iter()
                    .map(|issue| {
                        let details = issue.details.as_ref();
                        Ok(Issue {
                            severity: super::required(
                                issue.severity.value.clone(),
                                "issue.severity",
                            )?,
                            code: super::required(issue.code.value.clone(), "issue.code")?,
                            details: details
                                .and_then(|held| held.text.as_ref())
                                .and_then(|text| text.value.clone()),
                            diagnostics: issue
                                .diagnostics
                                .as_ref()
                                .and_then(|text| text.value.clone()),
                            tx_issue_type: details
                                .and_then(|held| held.coding.iter().find_map(tx_issue)),
                        })
                    })
                    .collect::<Result<Vec<_>, BodyError>>()?;
                Ok(Outcome { issues })
            }

            /// Returns the `tx-issue-type` code `coding` carries, if it is one.
            fn tx_issue(coding: &Coding) -> Option<TxIssue> {
                let system = coding.system.as_ref().and_then(|uri| uri.value.as_deref());
                if system != Some(crate::error::TX_ISSUE_TYPE) {
                    return None;
                }
                Some(TxIssue {
                    code: coding.code.as_ref().and_then(|code| code.value.clone())?,
                    display: coding
                        .display
                        .as_ref()
                        .and_then(|text| text.value.clone()),
                })
            }

            /// Returns the `CodeSystem/$lookup` answer `body` carries.
            pub(crate) fn lookup_found(body: &str) -> Result<LookupOutcome, BodyError> {
                let parameters = serde_json::from_str::<Parameters>(body)?;
                let found = CodeSystemLookupResponse::from_parameters(&parameters)?;
                let designations = found
                    .designation
                    .iter()
                    .map(|designation| {
                        Ok(Designation {
                            language: designation
                                .language
                                .as_ref()
                                .and_then(|code| code.value.clone()),
                            usage: designation.r#use.as_ref().map(concept),
                            value: super::required(
                                designation.value.value.clone(),
                                "designation.value",
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, BodyError>>()?;
                let properties = found
                    .property
                    .iter()
                    .map(|property| {
                        Ok(Property {
                            code: super::required(
                                property.code.value.clone(),
                                "property.code",
                            )?,
                            value: property
                                .value
                                .clone()
                                .map(lookup_property_value)
                                .transpose()?,
                            description: property
                                .description
                                .as_ref()
                                .and_then(|text| text.value.clone()),
                        })
                    })
                    .collect::<Result<Vec<_>, BodyError>>()?;
                Ok(LookupOutcome::Found {
                    name: super::required(found.name.value.clone(), "name")?,
                    version: found.version.as_ref().and_then(|text| text.value.clone()),
                    display: super::required(found.display.value.clone(), "display")?,
                    designations,
                    properties,
                })
            }

            /// Returns the `ConceptMap/$translate` answer `body` carries.
            pub(crate) fn translate(body: &str) -> Result<TranslateOutcome, BodyError> {
                let parameters = serde_json::from_str::<Parameters>(body)?;
                let answer = ConceptMapTranslateResponse::from_parameters(&parameters)?;
                let message = answer.message.as_ref().and_then(|text| text.value.clone());
                if !super::required_flag(answer.result.value, "result")? {
                    return Ok(TranslateOutcome::NoMatch { message });
                }
                let matches = answer
                    .r#match
                    .iter()
                    .map(|found| Match {
                        equivalence: found
                            .equivalence
                            .as_ref()
                            .and_then(|code| code.value.as_deref())
                            .map(Equivalence::new),
                        concept: found.concept.as_ref().map(concept),
                        source: found.source.as_ref().and_then(|url| url.value.clone()),
                    })
                    .collect();
                Ok(TranslateOutcome::Translated { matches, message })
            }

            /// Returns the `ValueSet/$validate-code` answer `body` carries.
            pub(crate) fn validate_code(body: &str) -> Result<ValidateOutcome, BodyError> {
                let parameters = serde_json::from_str::<Parameters>(body)?;
                let answer = ValueSetValidateCodeResponse::from_parameters(&parameters)?;
                let message = answer.message.as_ref().and_then(|text| text.value.clone());
                if super::required_flag(answer.result.value, "result")? {
                    return Ok(ValidateOutcome::Valid {
                        display: answer.display.as_ref().and_then(|text| text.value.clone()),
                        message,
                    });
                }
                Ok(ValidateOutcome::Invalid {
                    message,
                    outcome: invalid_issues(&answer)?,
                })
            }

            /// Returns `coding` in the release-neutral shape.
            fn concept(coding: &Coding) -> Concept {
                Concept {
                    system: coding.system.as_ref().and_then(|uri| uri.value.clone()),
                    version: coding.version.as_ref().and_then(|text| text.value.clone()),
                    code: coding.code.as_ref().and_then(|code| code.value.clone()),
                    display: coding.display.as_ref().and_then(|text| text.value.clone()),
                }
            }

            /// Returns one property value as the choice member that carried it
            /// and that member's lexical JSON.
            fn property_value(value: ParametersParameterValue) -> Result<PropertyValue, BodyError> {
                let parameter = ParametersParameter {
                    name: "value".into(),
                    value: Some(value),
                    ..Default::default()
                };
                parameter
                    .to_json()?
                    .into_iter()
                    .find(|(key, _)| key.starts_with("value"))
                    .map(|(member, value)| PropertyValue { member, value })
                    .ok_or(BodyError::MissingValue {
                        parameter: "property.value",
                    })
            }
        }
    };
}

per_release!(stu3, stu3, stu3_contracts);
per_release!(r4, r4, r4_contracts);
per_release!(r4b, r4b, r4_contracts);

/// Returns the relative URL of `request`, the form a batch entry carries
/// (<https://hl7.org/fhir/R4/http.html#transaction>).
fn path_of(request: &Request) -> &'static str {
    match request {
        Request::Lookup { .. } => crate::LOOKUP,
        Request::Translate(_) => crate::TRANSLATE,
        Request::ValidateCode(_) => crate::VALIDATE_CODE,
    }
}

/// Returns the status an HTTP status line names, `None` when it names none.
///
/// `Bundle.entry.response.status` is "the status code returned by processing
/// this entry", written as the status line's code and optional reason phrase
/// (<https://hl7.org/fhir/R4/bundle-definitions.html#Bundle.entry.response.status>).
fn status_of(line: &str) -> Option<StatusCode> {
    let code = line.split_whitespace().next()?;
    StatusCode::from_bytes(code.as_bytes()).ok()
}

/// Returns the value of a primitive the operation declares as mandatory.
///
/// A FHIR primitive can be present with an extension and no value at all
/// (<https://hl7.org/fhir/R4/json.html#primitive>), so an answer whose
/// `display` states nothing is a defective answer, never an empty string.
fn required(value: Option<String>, parameter: &'static str) -> Result<String, BodyError> {
    value.ok_or(BodyError::MissingValue { parameter })
}

/// Returns the value of a mandatory `boolean` out parameter.
fn required_flag(value: Option<bool>, parameter: &'static str) -> Result<bool, BodyError> {
    value.ok_or(BodyError::MissingValue { parameter })
}

/// Returns the `OperationOutcome` that `body` carries, read in `version`.
pub(crate) fn outcome(version: WireVersion, body: &str) -> Result<Outcome, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::outcome(body),
        WireVersion::R4 => r4::outcome(body),
        WireVersion::R4B => r4b::outcome(body),
    }
}

/// Returns the parameter of `request` that `version` cannot carry.
pub(crate) const fn unsupported(version: WireVersion, request: &Request) -> Option<&'static str> {
    match version {
        WireVersion::Stu3 => stu3::unsupported_parameter(request),
        WireVersion::R4 => r4::unsupported_parameter(request),
        WireVersion::R4B => r4b::unsupported_parameter(request),
    }
}

/// Returns the FHIR JSON body that carries `request`, written in `version`.
pub(crate) fn request_body(
    version: WireVersion,
    request: &Request,
) -> Result<String, RequestError> {
    match version {
        WireVersion::Stu3 => stu3::request_body(request),
        WireVersion::R4 => r4::request_body(request),
        WireVersion::R4B => r4b::request_body(request),
    }
}

/// Returns the FHIR JSON `batch` Bundle that carries `requests`.
pub(crate) fn batch_body(
    version: WireVersion,
    requests: &[Request],
) -> Result<String, RequestError> {
    match version {
        WireVersion::Stu3 => stu3::batch_body(requests),
        WireVersion::R4 => r4::batch_body(requests),
        WireVersion::R4B => r4b::batch_body(requests),
    }
}

/// Returns the entries of the `batch-response` `body` carries.
pub(crate) fn batch_entries(
    version: WireVersion,
    body: &str,
) -> Result<Vec<BatchEntry>, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::batch_entries(body),
        WireVersion::R4 => r4::batch_entries(body),
        WireVersion::R4B => r4b::batch_entries(body),
    }
}

/// Returns the `CodeSystem/$lookup` answer `body` carries.
pub(crate) fn lookup_found(version: WireVersion, body: &str) -> Result<LookupOutcome, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::lookup_found(body),
        WireVersion::R4 => r4::lookup_found(body),
        WireVersion::R4B => r4b::lookup_found(body),
    }
}

/// Returns the `ConceptMap/$translate` answer `body` carries.
pub(crate) fn translate(version: WireVersion, body: &str) -> Result<TranslateOutcome, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::translate(body),
        WireVersion::R4 => r4::translate(body),
        WireVersion::R4B => r4b::translate(body),
    }
}

/// Returns the `ValueSet/$validate-code` answer `body` carries.
pub(crate) fn validate_code(
    version: WireVersion,
    body: &str,
) -> Result<ValidateOutcome, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::validate_code(body),
        WireVersion::R4 => r4::validate_code(body),
        WireVersion::R4B => r4b::validate_code(body),
    }
}

#[cfg(test)]
mod tests {
    use super::{batch_body, request_body, status_of};
    use crate::config::WireVersion;
    use crate::outcome::Request;
    use http::StatusCode;

    fn lookup() -> Request {
        Request::Lookup {
            system: "http://example.org/cs".to_owned(),
            code: "a".to_owned(),
            version: Some("1.0.0".to_owned()),
        }
    }

    #[test]
    fn a_status_line_reads_its_code() {
        assert_eq!(Some(StatusCode::OK), status_of("200 OK"));
        assert_eq!(Some(StatusCode::BAD_REQUEST), status_of("400"));
        assert_eq!(None, status_of(""));
        assert_eq!(None, status_of("not a status"));
    }

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn both_releases_write_the_same_lookup_parameters() -> Result<(), super::RequestError> {
        let r4 = request_body(WireVersion::R4, &lookup())?;
        let r4b = request_body(WireVersion::R4B, &lookup())?;
        assert_eq!(r4, r4b);
        assert!(r4.contains(r#""name":"system""#), "{r4}");
        assert!(r4.contains(r#""name":"code""#), "{r4}");
        assert!(r4.contains(r#""name":"version""#), "{r4}");
        Ok(())
    }

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn a_batch_bundle_carries_one_post_entry_per_request() -> Result<(), super::RequestError> {
        let body = batch_body(WireVersion::R4, &[lookup(), lookup()])?;
        assert!(body.contains(r#""type":"batch""#), "{body}");
        assert_eq!(2, body.matches(r#""method":"POST""#).count(), "{body}");
        assert_eq!(
            2,
            body.matches(r#""url":"CodeSystem/$lookup""#).count(),
            "{body}"
        );
        Ok(())
    }
}
