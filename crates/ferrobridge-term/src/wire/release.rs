// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `per_release` macro, which generates the reader and writer of one
//! release over its own generated model.

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
                Coded, ExpandOutcome, Expanded, Expansion, ExpansionContains, LookupOutcome, Request,
                SubsumesOutcome, Subsumption, TranslateOutcome, ValidateOutcome, Validation,
            };
            use fhir_types::$release::bundle::{Bundle, BundleEntry, BundleEntryRequest};
            use fhir_types::$release::codeable_concept::CodeableConcept;
            use fhir_types::$release::coding::Coding;
            use fhir_types::$release::operation_outcome::OperationOutcome;
            use fhir_types::$release::operations::code_system_lookup::CodeSystemLookupRequest;
            use fhir_types::$release::operations::code_system_lookup::CodeSystemLookupResponse;
            use fhir_types::$release::operations::code_system_subsumes::CodeSystemSubsumesRequest;
            use fhir_types::$release::operations::code_system_subsumes::CodeSystemSubsumesResponse;
            use fhir_types::$release::operations::concept_map_translate::ConceptMapTranslateRequest;
            use fhir_types::$release::operations::concept_map_translate::ConceptMapTranslateResponse;
            use fhir_types::$release::operations::value_set_expand::ValueSetExpandRequest;
            use fhir_types::$release::operations::value_set_validate_code::ValueSetValidateCodeRequest;
            use fhir_types::$release::operations::value_set_validate_code::ValueSetValidateCodeResponse;
            use fhir_types::$release::parameters::{
                Parameters, ParametersParameter, ParametersParameterValue,
            };
            use fhir_types::$release::resource::Resource;
            use fhir_types::$release::value_set::{ValueSet, ValueSetExpansionContains};
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
                    None => parameters(request),
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

            /// Returns the `Parameters` of one `CodeSystem/$subsumes`, which
            /// every release declares with the same inputs.
            fn subsumes_parameters(subsumption: &Subsumption) -> Parameters {
                CodeSystemSubsumesRequest {
                    system: Some(subsumption.system.as_str().into()),
                    code_a: Some(subsumption.code_a.as_str().into()),
                    code_b: Some(subsumption.code_b.as_str().into()),
                    version: subsumption.version.as_deref().map(Into::into),
                    ..Default::default()
                }
                .into_parameters()
            }

            /// Returns the `ValueSet/$expand` answer `body` carries.
            ///
            /// The operation's one `out` parameter is the `ValueSet` named
            /// `return`, so "the response is simply the resource itself"
            /// (<https://hl7.org/fhir/R4/operations.html#response>).
            pub(crate) fn expanded(body: &str) -> Result<ExpandOutcome, BodyError> {
                let value_set = serde_json::from_str::<ValueSet>(body)?;
                let expansion = value_set
                    .expansion
                    .as_ref()
                    .ok_or(BodyError::MissingValue {
                        parameter: "return.expansion",
                    })?;
                Ok(ExpandOutcome::Expanded(Expanded {
                    url: value_set.url.as_ref().and_then(|uri| uri.value.clone()),
                    version: value_set.version.as_ref().and_then(|text| text.value.clone()),
                    total: expansion.total.as_ref().and_then(|number| number.value),
                    offset: expansion.offset.as_ref().and_then(|number| number.value),
                    contains: expansion
                        .contains
                        .iter()
                        .map(contains)
                        .collect::<Result<Vec<_>, BodyError>>()?,
                }))
            }

            /// Returns one `expansion.contains` entry with the entries
            /// nested under it.
            fn contains(entry: &ValueSetExpansionContains) -> Result<ExpansionContains, BodyError> {
                let designations = entry
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
                                "contains.designation.value",
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, BodyError>>()?;
                Ok(ExpansionContains {
                    system: entry.system.as_ref().and_then(|uri| uri.value.clone()),
                    version: entry.version.as_ref().and_then(|text| text.value.clone()),
                    code: entry.code.as_ref().and_then(|code| code.value.clone()),
                    display: entry.display.as_ref().and_then(|text| text.value.clone()),
                    r#abstract: entry.r#abstract.as_ref().and_then(|flag| flag.value),
                    inactive: entry.inactive.as_ref().and_then(|flag| flag.value),
                    designations,
                    contains: entry
                        .contains
                        .iter()
                        .map(contains)
                        .collect::<Result<Vec<_>, BodyError>>()?,
                })
            }

            /// Returns the `CodeSystem/$subsumes` answer `body` carries.
            pub(crate) fn subsumes(body: &str) -> Result<SubsumesOutcome, BodyError> {
                let parameters = serde_json::from_str::<Parameters>(body)?;
                let answer = CodeSystemSubsumesResponse::from_parameters(&parameters)?;
                let code = super::required(answer.outcome.value.clone(), "outcome")?;
                SubsumesOutcome::from_code(&code).ok_or(BodyError::UnknownCode {
                    parameter: "outcome",
                    code,
                })
            }

            /// Returns the `Bundle.total` of the `searchset` `body` carries.
            pub(crate) fn search_total(body: &str) -> Result<u32, BodyError> {
                let bundle = serde_json::from_str::<Bundle>(body)?;
                let kind = bundle.r#type.value.clone().unwrap_or_default();
                if kind != super::SEARCHSET {
                    return Err(BodyError::SearchType { found: kind });
                }
                bundle
                    .total
                    .as_ref()
                    .and_then(|total| total.value)
                    .ok_or(BodyError::MissingValue { parameter: "total" })
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
