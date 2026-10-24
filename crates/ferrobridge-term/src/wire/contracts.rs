// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The request writers of each release: `r4_contracts` for R4 and R4B, which
//! declare the same parameters, and `stu3_contracts` for STU3.

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
        fn parameters(request: &Request) -> Result<Parameters, RequestError> {
            Ok(match request {
                Request::Lookup {
                    system,
                    code,
                    version,
                    date,
                } => CodeSystemLookupRequest {
                    system: Some(system.as_str().into()),
                    code: Some(code.as_str().into()),
                    version: version.as_deref().map(Into::into),
                    date: date.as_deref().map(Into::into),
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
                Request::Expand(expansion) => expand_parameters(expansion)?,
                Request::Subsumes(subsumption) => subsumes_parameters(subsumption),
            })
        }

        /// Returns the `Parameters` of one `ValueSet/$validate-code`.
        fn validate_code_parameters(validation: &Validation) -> Parameters {
            let mut request = ValueSetValidateCodeRequest {
                url: Some(validation.value_set_url.as_str().into()),
                value_set_version: validation.value_set_version.as_deref().map(Into::into),
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

        /// Returns the `Parameters` of one `ValueSet/$expand`.
        fn expand_parameters(expansion: &Expansion) -> Result<Parameters, RequestError> {
            Ok(ValueSetExpandRequest {
                url: Some(expansion.value_set_url.as_str().into()),
                value_set_version: expansion.value_set_version.as_deref().map(Into::into),
                date: expansion.date.as_deref().map(Into::into),
                filter: expansion.filter.as_deref().map(Into::into),
                offset: super::integer(expansion.offset, "offset")?.map(Into::into),
                count: super::integer(expansion.count, "count")?.map(Into::into),
                include_designations: expansion.include_designations.map(Into::into),
                ..Default::default()
            }
            .into_parameters())
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
/// `-ConceptMap-translate`, `-ValueSet-validate-code` and `-ValueSet-expand`
/// of `hl7.fhir.r3.core` 3.0.2.
macro_rules! stu3_contracts {
    () => {
        /// Returns the parameter of `request` STU3 declares no equivalent for.
        ///
        /// STU3 `$translate` has no `url` and names no concept map
        /// (<https://hl7.org/fhir/STU3/conceptmap-operations.html#translate>),
        /// and STU3 `$validate-code` has no `valueSetVersion`
        /// (<https://hl7.org/fhir/STU3/valueset-operations.html#validate-code>):
        /// its `version` is the code system's. STU3 `$expand` has no
        /// `valueSetVersion` either
        /// (<https://hl7.org/fhir/STU3/valueset-operations.html#expand>).
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
                Request::Expand(Expansion {
                    value_set_version: Some(_),
                    ..
                }) => Some("valueSetVersion"),
                _ => None,
            }
        }

        /// Returns the `Parameters` that carries `request`, which
        /// `unsupported` has passed.
        fn parameters(request: &Request) -> Result<Parameters, RequestError> {
            Ok(match request {
                Request::Lookup {
                    system,
                    code,
                    version,
                    date,
                } => CodeSystemLookupRequest {
                    system: Some(system.as_str().into()),
                    code: Some(code.as_str().into()),
                    version: version.as_deref().map(Into::into),
                    date: date.as_deref().map(Into::into),
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
                Request::Expand(expansion) => expand_parameters(expansion)?,
                Request::Subsumes(subsumption) => subsumes_parameters(subsumption),
            })
        }

        /// Returns the `Parameters` of one STU3 `ValueSet/$validate-code`.
        fn validate_code_parameters(validation: &Validation) -> Parameters {
            let mut request = ValueSetValidateCodeRequest {
                url: Some(validation.value_set_url.as_str().into()),
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

        /// Returns the `Parameters` of one STU3 `ValueSet/$expand`, which
        /// `unsupported` has passed.
        fn expand_parameters(expansion: &Expansion) -> Result<Parameters, RequestError> {
            Ok(ValueSetExpandRequest {
                url: Some(expansion.value_set_url.as_str().into()),
                date: expansion.date.as_deref().map(Into::into),
                filter: expansion.filter.as_deref().map(Into::into),
                offset: super::integer(expansion.offset, "offset")?.map(Into::into),
                count: super::integer(expansion.count, "count")?.map(Into::into),
                include_designations: expansion.include_designations.map(Into::into),
                ..Default::default()
            }
            .into_parameters())
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
