// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `per_release` macro, which generates the reader and writer of one
//! release over its own generated model.

/// Returns the `designation` parts `$designations` carries, `$label` naming
/// the mandatory value in an error; `$lookup` and `$expand` declare them alike.
macro_rules! designations {
    ($designations:expr, $label:literal) => {
        $designations
            .iter()
            .map(|designation| {
                Ok(Designation {
                    language: designation
                        .language
                        .as_ref()
                        .and_then(|code| code.value.clone()),
                    usage: designation.r#use.as_ref().map(concept),
                    value: super::required(designation.value.value.clone(), $label)?,
                })
            })
            .collect::<Result<Vec<_>, BodyError>>()?
    };
}

/// Generates the reader and writer of one FHIR release, with the
/// release-specific writers and readers `$contracts` expands to.
macro_rules! per_release {
    ($module:ident, $release:ident, $contracts:ident) => {
        /// The bodies of one FHIR release, over its own generated model.
        pub(crate) mod $module {
            use super::RequestError;
            #[cfg(feature = "client")]
            use super::{BatchEntry, BATCH, BATCH_RESPONSE, POST};
            use crate::concept::{Concept, Designation, Property, PropertyValue};
            use crate::error::{BodyError, Issue, Outcome, TxIssue};
            use crate::outcome::{
                ExpandOutcome, Expanded, ExpansionContains, ExpansionParameter, LookupOutcome,
                SubsumesOutcome, TranslateOutcome, Unclosed, ValidateOutcome,
            };
            use crate::request::{
                Coded, Expansion, InlineValueSet, Request, Subsumption, Validation, ValueSetSource,
            };
            #[cfg(feature = "client")]
            use fhir_types::$release::bundle::{Bundle, BundleEntry, BundleEntryRequest};
            use fhir_types::$release::codeable_concept::CodeableConcept;
            use fhir_types::$release::coding::Coding;
            use fhir_types::$release::extension::ExtensionValue;
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
            #[cfg(feature = "client")]
            use fhir_types::$release::resource::Resource;
            use fhir_types::$release::value_set::{
                ValueSet, ValueSetCompose, ValueSetComposeInclude, ValueSetComposeIncludeFilter,
                ValueSetExpansion, ValueSetExpansionContains,
            };
            use fhir_types::codec::Json;

            $contracts!($release);

            /// Returns the `Parameters` that carries `request`, or the
            /// parameter this release cannot carry.
            fn encode(request: &Request) -> Result<Parameters, RequestError> {
                if undefined(request) {
                    return Err(RequestError::UndefinedOperation {
                        operation: request.operation(),
                    });
                }
                if let Some(parameter) = unsupported(request) {
                    return Err(RequestError::Unsupported {
                        operation: request.operation(),
                        parameter,
                    });
                }
                if system_required(request) {
                    return Err(RequestError::SystemRequired {
                        operation: request.operation(),
                    });
                }
                parameters(request)
            }

            /// Returns the `Parameters` that carries `request`, which
            /// `encode` has checked.
            fn parameters(request: &Request) -> Result<Parameters, RequestError> {
                Ok(match request {
                    Request::Lookup {
                        system,
                        code,
                        version,
                        date,
                        display_language,
                    } => CodeSystemLookupRequest {
                        system: Some(system.as_str().into()),
                        code: Some(code.as_str().into()),
                        version: version.as_deref().map(Into::into),
                        date: date.as_deref().map(Into::into),
                        display_language: display_language.as_deref().map(Into::into),
                        ..Default::default()
                    }
                    .into_parameters(),
                    Request::Translate(translation) => translate_parameters(translation),
                    Request::ValidateCode(validation) => validate_code_parameters(validation),
                    Request::CodeSystemValidateCode(validation) => {
                        code_system_validate_parameters(validation)?
                    }
                    Request::Expand(expansion) => expand_parameters(expansion)?,
                    Request::Subsumes(subsumption) => subsumes_parameters(subsumption),
                })
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

            /// Returns the `Parameters` of one `ValueSet/$validate-code`; an
            /// instance-level run sends no `url`, since the address names the
            /// value set.
            fn validate_code_parameters(validation: &Validation) -> Parameters {
                let mut request = ValueSetValidateCodeRequest {
                    url: validation.value_set.canonical().map(Into::into),
                    date: validation.date.as_deref().map(Into::into),
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
                        set_system_version(&mut request, version.as_deref());
                        request.display = display.as_deref().map(Into::into);
                    }
                    Coded::InContext {
                        context,
                        code,
                        display,
                    } => {
                        request.context = Some(context.as_str().into());
                        request.code = Some(code.as_str().into());
                        request.display = display.as_deref().map(Into::into);
                    }
                    Coded::Coding(concept) => request.coding = Some(coding(concept)),
                    Coded::CodeableConcept {
                        coding: codings,
                        text,
                    } => {
                        request.codeable_concept =
                            Some(codeable_concept(codings, text.as_deref()));
                    }
                }
                validate_code_extras(&mut request, validation);
                request.into_parameters()
            }

            /// Returns the `Parameters` of one `ValueSet/$expand`.
            fn expand_parameters(expansion: &Expansion) -> Result<Parameters, RequestError> {
                let (url, value_set) = match &expansion.value_set {
                    ValueSetSource::Stored(target) => (target.canonical().map(Into::into), None),
                    ValueSetSource::Inline(inline) => (None, Some(inline_value_set(inline))),
                    ValueSetSource::Authored(authored) => (
                        None,
                        Some(
                            serde_json::from_str::<ValueSet>(authored.json())
                                .map_err(RequestError::AuthoredValueSet)?,
                        ),
                    ),
                };
                let mut request = ValueSetExpandRequest {
                    url,
                    value_set,
                    date: expansion.date.as_deref().map(Into::into),
                    filter: expansion.filter.as_deref().map(Into::into),
                    offset: super::integer(expansion.offset, "offset")?.map(Into::into),
                    count: super::integer(expansion.count, "count")?.map(Into::into),
                    include_designations: expansion.include_designations.map(Into::into),
                    active_only: expansion.active_only.map(Into::into),
                    exclude_nested: expansion.exclude_nested.map(Into::into),
                    display_language: expansion.display_language.as_deref().map(Into::into),
                    ..Default::default()
                };
                set_value_set_version(&mut request, expansion.value_set_version.as_deref());
                Ok(request.into_parameters())
            }

            /// Returns `inline` as this release's `ValueSet`, with the one
            /// status a request-scoped value set can carry
            /// (<https://hl7.org/fhir/R4/valueset.html>, `status` `1..1`).
            fn inline_value_set(inline: &InlineValueSet) -> ValueSet {
                let include = inline
                    .include
                    .iter()
                    .map(|include| ValueSetComposeInclude {
                        system: Some(include.system.as_str().into()),
                        version: include.version.as_deref().map(Into::into),
                        filter: include
                            .filters
                            .iter()
                            .map(|filter| ValueSetComposeIncludeFilter {
                                property: filter.property.as_str().into(),
                                op: filter.op.as_str().into(),
                                value: filter_value(&filter.value),
                                ..Default::default()
                            })
                            .collect(),
                        ..Default::default()
                    })
                    .collect();
                ValueSet {
                    status: "active".into(),
                    compose: Some(ValueSetCompose {
                        include,
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            }

            /// Returns the `Parameters` of one `CodeSystem/$subsumes`, which
            /// every release declares with the same inputs.
            fn subsumes_parameters(subsumption: &Subsumption) -> Parameters {
                CodeSystemSubsumesRequest {
                    system: subsumption.code_system.canonical().map(Into::into),
                    code_a: Some(subsumption.code_a.as_str().into()),
                    code_b: Some(subsumption.code_b.as_str().into()),
                    version: subsumption.version.as_deref().map(Into::into),
                    ..Default::default()
                }
                .into_parameters()
            }

            /// Returns whether this release defines no such operation.
            #[cfg(feature = "client")]
            pub(crate) const fn undefined_operation(request: &Request) -> bool {
                undefined(request)
            }

            /// Returns the parameter of `request` this release cannot carry.
            #[cfg(feature = "client")]
            pub(crate) const fn unsupported_parameter(request: &Request) -> Option<&'static str> {
                unsupported(request)
            }

            /// Returns whether `request` names no system where this release
            /// requires one.
            #[cfg(feature = "client")]
            pub(crate) const fn requires_system(request: &Request) -> bool {
                system_required(request)
            }

            /// Returns the FHIR JSON body that carries `request`.
            pub(crate) fn request_body(request: &Request) -> Result<String, RequestError> {
                Ok(serde_json::to_string(&encode(request)?)?)
            }

            /// Returns the query that carries `request` in the `GET` form,
            /// `None` when an input is not primitive.
            ///
            /// The `GET` form is open only when "there are no parameters with
            /// complex types (including resources)", each parameter then
            /// sent as a query parameter of its own name
            /// (<https://hl7.org/fhir/R4/operations.html#request>).
            pub(crate) fn request_query(
                request: &Request,
            ) -> Result<Option<Vec<(String, String)>>, RequestError> {
                let mut query = Vec::new();
                for parameter in encode(request)?.parameter {
                    let Some(name) = parameter.name.value.clone() else {
                        return Ok(None);
                    };
                    if parameter.resource.is_some() || !parameter.part.is_empty() {
                        return Ok(None);
                    }
                    let written = parameter.to_json()?;
                    let value = written
                        .into_iter()
                        .find(|(key, _)| key.starts_with("value"))
                        .map(|(_, value)| value);
                    let text = match value {
                        Some(fhir_types::codec::Value::String(text)) => text,
                        Some(fhir_types::codec::Value::Bool(flag)) => flag.to_string(),
                        Some(fhir_types::codec::Value::Number(number)) => number.to_string(),
                        _ => return Ok(None),
                    };
                    query.push((name, text));
                }
                Ok(Some(query))
            }

            /// Returns the FHIR JSON `batch` Bundle that carries `requests`.
            ///
            /// A request this release cannot carry refuses the whole batch,
            /// so nothing is sent.
            #[cfg(feature = "client")]
            pub(crate) fn batch_body(requests: &[Request]) -> Result<String, RequestError> {
                let entry = requests
                    .iter()
                    .map(|request| {
                        Ok(BundleEntry {
                            request: Some(BundleEntryRequest {
                                method: POST.into(),
                                url: super::path_of(request).as_str().into(),
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
            #[cfg(feature = "client")]
            pub(crate) fn batch_entries(body: &str) -> Result<Vec<BatchEntry>, BodyError> {
                let bundle = serde_json::from_str::<Bundle>(body)?;
                let kind = bundle.r#type.value.clone().unwrap_or_default();
                if kind != BATCH_RESPONSE {
                    return Err(BodyError::BatchType { found: kind });
                }
                bundle.entry.into_iter().map(entry).collect()
            }

            /// Returns one `batch-response` entry as its status and body.
            #[cfg(feature = "client")]
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
                            codings: details
                                .map(|held| held.coding.iter().map(concept).collect())
                                .unwrap_or_default(),
                            expression: issue
                                .expression
                                .iter()
                                .filter_map(|path| path.value.clone())
                                .collect(),
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
                let designations = designations!(found.designation, "designation.value");
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
                    definition: lookup_definition(&found),
                    designations,
                    properties,
                })
            }

            /// Returns the `ConceptMap/$translate` answer `body` carries.
            pub(crate) fn translate(body: &str) -> Result<TranslateOutcome, BodyError> {
                let parameters = serde_json::from_str::<Parameters>(body)?;
                let answer = ConceptMapTranslateResponse::from_parameters(&parameters)?;
                let message = answer.message.as_ref().and_then(|text| text.value.clone());
                let matches = answer.r#match.iter().map(translate_match).collect::<Result<Vec<_>, BodyError>>()?;
                let used_maps = used_maps(&answer);
                if super::required_flag(answer.result.value, "result")? {
                    return Ok(TranslateOutcome::Translated {
                        matches,
                        message,
                        used_maps,
                    });
                }
                Ok(TranslateOutcome::NoMatch {
                    message,
                    matches,
                    used_maps,
                })
            }

            /// Returns the `ValueSet/$validate-code` answer `body` carries.
            pub(crate) fn validate_code(body: &str) -> Result<ValidateOutcome, BodyError> {
                let parameters = serde_json::from_str::<Parameters>(body)?;
                let answer = ValueSetValidateCodeResponse::from_parameters(&parameters)?;
                validated!(answer, validation_details(&answer), validation_issues(&answer)?)
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
                    parameters: expansion
                        .parameter
                        .iter()
                        .map(|parameter| {
                            Ok(ExpansionParameter {
                                name: super::required(
                                    parameter.name.value.clone(),
                                    "expansion.parameter.name",
                                )?,
                                value: parameter
                                    .to_json()?
                                    .into_iter()
                                    .find(|(key, _)| key.starts_with("value"))
                                    .map(|(member, value)| PropertyValue { member, value }),
                            })
                        })
                        .collect::<Result<Vec<_>, BodyError>>()?,
                    unclosed: unclosed(expansion),
                    contains: expansion
                        .contains
                        .iter()
                        .map(contains)
                        .collect::<Result<Vec<_>, BodyError>>()?,
                }))
            }

            /// Returns the `valueset-unclosed` mark of `expansion` with its
            /// stated reasons, `None` unless the mark is set `true`.
            fn unclosed(expansion: &ValueSetExpansion) -> Option<Unclosed> {
                let marked = expansion.extension.iter().any(|extension| {
                    extension.url == super::UNCLOSED
                        && matches!(
                            &extension.value,
                            Some(ExtensionValue::Boolean(flag)) if flag.value == Some(true)
                        )
                });
                marked.then(|| Unclosed {
                    reasons: expansion
                        .extension
                        .iter()
                        .filter(|extension| extension.url == super::UNCLOSED_REASON)
                        .filter_map(|extension| match &extension.value {
                            Some(ExtensionValue::String(text)) => text.value.clone(),
                            _ => None,
                        })
                        .collect(),
                })
            }

            /// Returns one `expansion.contains` entry with the entries
            /// nested under it.
            fn contains(entry: &ValueSetExpansionContains) -> Result<ExpansionContains, BodyError> {
                let designations = designations!(entry.designation, "contains.designation.value");
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
            #[cfg(feature = "client")]
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

            /// Returns one parameter value as the choice member that carried it
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
