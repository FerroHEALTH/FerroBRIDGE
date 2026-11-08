// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The writers and readers that differ between releases: `stu3_contracts`,
//! `r4_contracts` for R4 and R4B, which declare the same parameters,
//! `r5_contracts` and `r6_contracts`, over `modern_contracts`, which R4 to R6
//! share.

/// Returns the `ValidateOutcome` a `$validate-code` answer states.
macro_rules! validated {
    ($answer:expr, $details:expr, $outcome:expr) => {{
        let message = $answer.message.as_ref().and_then(|text| text.value.clone());
        let display = $answer.display.as_ref().and_then(|text| text.value.clone());
        let outcome = $outcome;
        let details = Box::new($details);
        if super::required_flag($answer.result.value, "result")? {
            Ok(ValidateOutcome::Valid {
                display,
                message,
                outcome,
                details,
            })
        } else {
            Ok(ValidateOutcome::Invalid {
                display,
                message,
                outcome,
                details,
            })
        }
    }};
}

/// Returns the details of a `$validate-code` answer of R4 to R6, whose
/// `ValueSet` and `CodeSystem` forms declare the same `out` parameters.
macro_rules! modern_details {
    ($answer:expr) => {
        crate::outcome::ValidationDetails {
            code: $answer.code.as_ref().and_then(|code| code.value.clone()),
            system: $answer.system.as_ref().and_then(|uri| uri.value.clone()),
            version: $answer.version.as_ref().and_then(|text| text.value.clone()),
            codeable_concept: $answer.codeable_concept.as_ref().map(|echoed| {
                crate::concept::CodeableConcept {
                    coding: echoed.coding.iter().map(concept).collect(),
                    text: echoed.text.as_ref().and_then(|text| text.value.clone()),
                }
            }),
            normalized_code: $answer
                .normalized_code
                .as_ref()
                .and_then(|code| code.value.clone()),
            inactive: $answer.inactive.as_ref().and_then(|flag| flag.value),
            status: $answer.status.as_ref().and_then(|code| code.value.clone()),
            caused_by_unknown_systems: $answer
                .x_caused_by_unknown_system
                .iter()
                .filter_map(|uri| uri.value.clone())
                .collect(),
            unknown_systems: $answer
                .x_unknown_system
                .iter()
                .filter_map(|uri| uri.value.clone())
                .collect(),
        }
    };
}

/// Returns the `issues` outcome of a `$validate-code` answer of R4 to R6.
macro_rules! modern_issues {
    ($answer:expr) => {
        $answer
            .issues
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?
            .map(|text| outcome(&text))
            .transpose()
    };
}

/// The writers and readers R4 to R6 share: every parameter a [`Request`]
/// carries outside `$translate` is declared under the same name and type,
/// with the terminology ecosystem's additions
/// (<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>).
macro_rules! modern_contracts {
    ($release:ident) => {
        use fhir_types::$release::operations::code_system_validate_code::CodeSystemValidateCodeRequest;
        use fhir_types::$release::operations::code_system_validate_code::CodeSystemValidateCodeResponse;

        /// Returns whether this release defines no such operation; R4 to R6
        /// define all six.
        const fn undefined(_request: &Request) -> bool {
            false
        }

        /// Returns whether `request` carries a code this release requires a
        /// system for and names none.
        ///
        /// R4 to R6 take a system or a context, which [`Coded`] carries by
        /// construction.
        const fn system_required(_request: &Request) -> bool {
            false
        }

        /// Sets `systemVersion`, the version of the code's system.
        fn set_system_version(request: &mut ValueSetValidateCodeRequest, version: Option<&str>) {
            request.system_version = version.map(Into::into);
        }

        /// Sets `valueSetVersion`.
        fn set_value_set_version(request: &mut ValueSetExpandRequest, version: Option<&str>) {
            request.value_set_version = version.map(Into::into);
        }

        /// Returns the `Parameters` of one `CodeSystem/$validate-code`; an
        /// instance-level run sends no `url`.
        fn code_system_validate_parameters(
            validation: &crate::request::CodeSystemValidation,
        ) -> Result<Parameters, RequestError> {
            Ok(CodeSystemValidateCodeRequest {
                url: validation.code_system.canonical().map(Into::into),
                version: validation.version.as_deref().map(Into::into),
                code: Some(validation.code.as_str().into()),
                display: validation.display.as_deref().map(Into::into),
                display_language: validation.display_language.as_deref().map(Into::into),
                date: validation.date.as_deref().map(Into::into),
                ..Default::default()
            }
            .into_parameters())
        }

        /// Returns one `property.value`, which these releases type as any
        /// FHIR type (`OperationDefinition-CodeSystem-lookup`).
        fn lookup_property_value(
            value: ParametersParameterValue,
        ) -> Result<PropertyValue, BodyError> {
            property_value(value)
        }

        /// Returns every `used-conceptmap` of a `$translate` answer.
        fn used_maps(answer: &ConceptMapTranslateResponse) -> Vec<String> {
            answer
                .used_conceptmap
                .iter()
                .filter_map(|uri| uri.value.clone())
                .collect()
        }

        /// Returns the details of a `ValueSet/$validate-code` answer.
        fn validation_details(
            answer: &ValueSetValidateCodeResponse,
        ) -> crate::outcome::ValidationDetails {
            modern_details!(answer)
        }

        /// Returns the `issues` outcome of a `ValueSet/$validate-code` answer.
        fn validation_issues(
            answer: &ValueSetValidateCodeResponse,
        ) -> Result<Option<Outcome>, BodyError> {
            modern_issues!(answer)
        }

        /// Returns the `CodeSystem/$validate-code` answer `body` carries.
        pub(crate) fn code_system_validate_code(body: &str) -> Result<ValidateOutcome, BodyError> {
            let parameters = serde_json::from_str::<Parameters>(body)?;
            let answer = CodeSystemValidateCodeResponse::from_parameters(&parameters)?;
            validated!(answer, modern_details!(answer), modern_issues!(answer)?)
        }
    };
}

/// The request writers and readers of R4 and R4B, after
/// `OperationDefinition-ConceptMap-translate` of `hl7.fhir.r4.core` 4.0.1 and
/// `hl7.fhir.r4b.core` 4.3.0.
macro_rules! r4_contracts {
    ($release:ident) => {
        modern_contracts!($release);

        /// Returns the parameter of `request` this release cannot carry.
        ///
        /// R4 and R4B declare every parameter a [`Request`] carries.
        const fn unsupported(_request: &Request) -> Option<&'static str> {
            None
        }

        /// Sets the `$validate-code` inputs beside the coded value.
        fn validate_code_extras(request: &mut ValueSetValidateCodeRequest, validation: &Validation) {
            request.value_set_version = validation.value_set_version.as_deref().map(Into::into);
            request.display_language = validation.display_language.as_deref().map(Into::into);
            request.active_only = validation.active_only.map(Into::into);
        }

        /// Returns `value` as `compose.include.filter.value`, a `string`.
        fn filter_value(value: &str) -> fhir_types::$release::primitives::String {
            value.into()
        }

        /// Returns no `definition`, which R4 and R4B do not declare.
        const fn lookup_definition(_found: &CodeSystemLookupResponse) -> Option<String> {
            None
        }

        /// Returns the `Parameters` of one `$translate`: `code`, `system`,
        /// `version` and `targetsystem`.
        fn translate_parameters(translation: &crate::request::Translation) -> Parameters {
            ConceptMapTranslateRequest {
                url: translation.concept_map_url.as_deref().map(Into::into),
                concept_map_version: translation.concept_map_version.as_deref().map(Into::into),
                code: Some(translation.code.as_str().into()),
                system: Some(translation.system.as_str().into()),
                version: translation.version.as_deref().map(Into::into),
                targetsystem: translation.target_system.as_deref().map(Into::into),
                reverse: translation.reverse.map(Into::into),
                ..Default::default()
            }
            .into_parameters()
        }

        /// Returns one match, with its `equivalence` and the terminology
        /// ecosystem's additions.
        fn translate_match(
            found: &fhir_types::$release::operations::concept_map_translate::ConceptMapTranslateResponseMatch,
        ) -> Result<crate::concept::Match, BodyError> {
            Ok(crate::concept::Match {
                equivalence: found
                    .equivalence
                    .as_ref()
                    .and_then(|code| code.value.as_deref())
                    .map(crate::concept::Equivalence::new),
                concept: found.concept.as_ref().map(concept),
                source: found.source.as_ref().and_then(|url| url.value.clone()),
                origin_map: found.origin_map.as_ref().and_then(|url| url.value.clone()),
                no_map: found.no_map.as_ref().and_then(|flag| flag.value),
                source_concept: found.source_concept.as_ref().map(concept),
                source_comment: found.source_comment.as_ref().and_then(|text| text.value.clone()),
                target_comment: found.target_comment.as_ref().and_then(|text| text.value.clone()),
                products: found
                    .product
                    .iter()
                    .map(|product| {
                        Ok(crate::concept::MatchValue {
                            name: product.element.as_ref().and_then(|uri| uri.value.clone()),
                            value: product
                                .concept
                                .clone()
                                .map(|coding| property_value(ParametersParameterValue::Coding(Box::new(coding))))
                                .transpose()?,
                        })
                    })
                    .collect::<Result<Vec<_>, BodyError>>()?,
                ..Default::default()
            })
        }
    };
}

/// The `$translate` reader of R5 and R6, whose matches state a
/// `relationship` and carry `product`, `dependsOn` and `property` parts
/// (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
macro_rules! relationship_match {
    ($release:ident) => {
        /// Returns one match, with its `relationship`.
        fn translate_match(
            found: &fhir_types::$release::operations::concept_map_translate::ConceptMapTranslateResponseMatch,
        ) -> Result<crate::concept::Match, BodyError> {
            let part = |name: &fhir_types::$release::primitives::Uri,
                        value: &ParametersParameterValue| {
                Ok(crate::concept::MatchValue {
                    name: name.value.clone(),
                    value: Some(property_value(value.clone())?),
                })
            };
            Ok(crate::concept::Match {
                relationship: found
                    .relationship
                    .as_ref()
                    .and_then(|code| code.value.as_deref())
                    .map(crate::concept::Relationship::new),
                concept: found.concept.as_ref().map(concept),
                origin_map: found.origin_map.as_ref().and_then(|url| url.value.clone()),
                no_map: found.no_map.as_ref().and_then(|flag| flag.value),
                source_concept: found.source_concept.as_ref().map(concept),
                source_comment: found.source_comment.as_ref().and_then(|text| text.value.clone()),
                target_comment: found.target_comment.as_ref().and_then(|text| text.value.clone()),
                products: found
                    .product
                    .iter()
                    .map(|product| part(&product.attribute, &product.value))
                    .collect::<Result<Vec<_>, BodyError>>()?,
                depends_on: found
                    .depends_on
                    .iter()
                    .map(|depends| part(&depends.attribute, &depends.value))
                    .collect::<Result<Vec<_>, BodyError>>()?,
                properties: found
                    .property
                    .iter()
                    .map(|property| part(&property.uri, &property.value))
                    .collect::<Result<Vec<_>, BodyError>>()?,
                ..Default::default()
            })
        }

        /// Returns `definition`, which R5 and R6 declare.
        fn lookup_definition(found: &CodeSystemLookupResponse) -> Option<String> {
            found.definition.as_ref().and_then(|text| text.value.clone())
        }

        /// Returns the parameter of `request` this release cannot carry.
        ///
        /// R5 replaced `reverse` with the `target*` inputs
        /// (<https://hl7.org/fhir/R5/conceptmap-operation-translate.html>).
        const fn unsupported(request: &Request) -> Option<&'static str> {
            match request {
                Request::Translate(crate::request::Translation {
                    reverse: Some(_), ..
                }) => Some("reverse"),
                _ => None,
            }
        }
    };
}

/// The request writers of R5, after `OperationDefinition-ConceptMap-translate`
/// of `hl7.fhir.r5.core` 5.0.0, which renamed the code `sourceCode` and the
/// target system `targetSystem`.
macro_rules! r5_contracts {
    ($release:ident) => {
        modern_contracts!($release);
        relationship_match!($release);

        /// Sets the `$validate-code` inputs beside the coded value.
        fn validate_code_extras(
            request: &mut ValueSetValidateCodeRequest,
            validation: &Validation,
        ) {
            request.value_set_version = validation.value_set_version.as_deref().map(Into::into);
            request.display_language = validation.display_language.as_deref().map(Into::into);
            request.active_only = validation.active_only.map(Into::into);
        }

        /// Returns `value` as `compose.include.filter.value`, a `string`.
        fn filter_value(value: &str) -> fhir_types::$release::primitives::String {
            value.into()
        }

        /// Returns the `Parameters` of one `$translate`: `sourceCode`,
        /// `system`, `version` and `targetSystem`.
        fn translate_parameters(translation: &crate::request::Translation) -> Parameters {
            ConceptMapTranslateRequest {
                url: translation.concept_map_url.as_deref().map(Into::into),
                concept_map_version: translation.concept_map_version.as_deref().map(Into::into),
                source_code: Some(translation.code.as_str().into()),
                system: Some(translation.system.as_str().into()),
                version: translation.version.as_deref().map(Into::into),
                target_system: translation.target_system.as_deref().map(Into::into),
                ..Default::default()
            }
            .into_parameters()
        }
    };
}

/// The request writers of the R6 ballot, after
/// `OperationDefinition-ConceptMap-translate` of `hl7.fhir.r6.core`
/// 6.0.0-ballot5, which also renamed `system` and `version` to
/// `sourceSystem` and `sourceVersion`.
macro_rules! r6_contracts {
    ($release:ident) => {
        modern_contracts!($release);
        relationship_match!($release);

        /// Sets the `$validate-code` inputs beside the coded value;
        /// `displayLanguage` repeats in R6
        /// (<https://hl7.org/fhir/6.0.0-ballot5/valueset-operation-validate-code.html>).
        fn validate_code_extras(
            request: &mut ValueSetValidateCodeRequest,
            validation: &Validation,
        ) {
            request.value_set_version = validation.value_set_version.as_deref().map(Into::into);
            request.display_language = validation
                .display_language
                .as_deref()
                .map(Into::into)
                .into_iter()
                .collect();
            request.active_only = validation.active_only.map(Into::into);
        }

        /// Returns `value` as `compose.include.filter.value`, which R6 makes
        /// optional.
        fn filter_value(value: &str) -> Option<fhir_types::$release::primitives::String> {
            Some(value.into())
        }

        /// Returns the `Parameters` of one `$translate`: `sourceCode`,
        /// `sourceSystem`, `sourceVersion` and `targetSystem`.
        fn translate_parameters(translation: &crate::request::Translation) -> Parameters {
            ConceptMapTranslateRequest {
                url: translation.concept_map_url.as_deref().map(Into::into),
                concept_map_version: translation.concept_map_version.as_deref().map(Into::into),
                source_code: Some(translation.code.as_str().into()),
                source_system: Some(translation.system.as_str().into()),
                source_version: translation.version.as_deref().map(Into::into),
                target_system: translation.target_system.as_deref().map(Into::into),
                ..Default::default()
            }
            .into_parameters()
        }
    };
}

/// The request writers and readers of STU3, after
/// `OperationDefinition-CodeSystem-lookup`, `-ConceptMap-translate`,
/// `-ValueSet-validate-code` and `-ValueSet-expand` of `hl7.fhir.r3.core`
/// 3.0.2.
macro_rules! stu3_contracts {
    ($release:ident) => {
        /// Returns whether STU3 defines no such operation: it has no
        /// `CodeSystem/$validate-code`
        /// (<https://hl7.org/fhir/STU3/codesystem-operations.html>).
        const fn undefined(request: &Request) -> bool {
            matches!(request, Request::CodeSystemValidateCode(_))
        }

        /// Returns the parameter of `request` STU3 declares no equivalent for.
        ///
        /// STU3 `$translate` has no `url` and no `conceptMapVersion`
        /// (<https://hl7.org/fhir/STU3/conceptmap-operations.html#translate>),
        /// and STU3 `$validate-code` has no `valueSetVersion` and no
        /// `activeOnly`
        /// (<https://hl7.org/fhir/STU3/valueset-operations.html#validate-code>):
        /// its `version` is the code system's. STU3 `$expand` has no
        /// `valueSetVersion` either
        /// (<https://hl7.org/fhir/STU3/valueset-operations.html#expand>).
        const fn unsupported(request: &Request) -> Option<&'static str> {
            match request {
                Request::Translate(crate::request::Translation {
                    concept_map_url: Some(_),
                    ..
                }) => Some("url"),
                Request::Translate(crate::request::Translation {
                    concept_map_version: Some(_),
                    ..
                }) => Some("conceptMapVersion"),
                Request::ValidateCode(Validation {
                    value_set_version: Some(_),
                    ..
                })
                | Request::Expand(Expansion {
                    value_set_version: Some(_),
                    ..
                }) => Some("valueSetVersion"),
                Request::ValidateCode(Validation {
                    active_only: Some(_),
                    ..
                }) => Some("activeOnly"),
                _ => None,
            }
        }

        /// Returns whether `request` carries a code with no system.
        ///
        /// STU3 `$validate-code` reads "If a code is provided, a system must
        /// be provided" (`OperationDefinition-ValueSet-validate-code`, `code`,
        /// 3.0.2), with no context alternative.
        const fn system_required(request: &Request) -> bool {
            matches!(
                request,
                Request::ValidateCode(Validation {
                    coded: Coded::InContext { .. },
                    ..
                })
            )
        }

        /// Sets the code system version, which STU3 names `version`.
        fn set_system_version(request: &mut ValueSetValidateCodeRequest, version: Option<&str>) {
            // NOTE: STU3 `version` is "the version of the system", R4's `systemVersion`
            // (`OperationDefinition-ValueSet-validate-code`, 3.0.2 and 4.0.1).
            request.version = version.map(Into::into);
        }

        /// Sets the `$validate-code` inputs beside the coded value that STU3
        /// declares, which `unsupported` has passed.
        fn validate_code_extras(request: &mut ValueSetValidateCodeRequest, validation: &Validation) {
            request.display_language = validation.display_language.as_deref().map(Into::into);
        }

        /// Leaves `valueSetVersion` unset, which STU3 does not declare and
        /// `unsupported` has refused.
        const fn set_value_set_version(_request: &mut ValueSetExpandRequest, _version: Option<&str>) {}

        /// Refuses `CodeSystem/$validate-code`, which `undefined` has already
        /// refused before this is reached.
        const fn code_system_validate_parameters(
            _validation: &crate::request::CodeSystemValidation,
        ) -> Result<Parameters, RequestError> {
            Err(RequestError::UndefinedOperation {
                operation: crate::CODE_SYSTEM_VALIDATE_CODE,
            })
        }

        /// Returns `value` as `compose.include.filter.value`, a `code` in
        /// STU3.
        fn filter_value(value: &str) -> fhir_types::$release::primitives::Code {
            value.into()
        }

        /// Returns one `property.value`, which STU3 types as `code`
        /// (`OperationDefinition-CodeSystem-lookup`, 3.0.2).
        fn lookup_property_value(
            value: fhir_types::$release::primitives::Code,
        ) -> Result<PropertyValue, BodyError> {
            property_value(ParametersParameterValue::Code(value))
        }

        /// Returns no `definition`, which STU3 does not declare.
        const fn lookup_definition(_found: &CodeSystemLookupResponse) -> Option<String> {
            None
        }

        /// Returns the `Parameters` of one STU3 `$translate`, which
        /// `unsupported` has passed.
        fn translate_parameters(translation: &crate::request::Translation) -> Parameters {
            ConceptMapTranslateRequest {
                code: Some(translation.code.as_str().into()),
                system: Some(translation.system.as_str().into()),
                version: translation.version.as_deref().map(Into::into),
                targetsystem: translation.target_system.as_deref().map(Into::into),
                reverse: translation.reverse.map(Into::into),
                ..Default::default()
            }
            .into_parameters()
        }

        /// Returns one STU3 match: `equivalence`, `concept`, `product` and
        /// `source`.
        fn translate_match(
            found: &fhir_types::$release::operations::concept_map_translate::ConceptMapTranslateResponseMatch,
        ) -> Result<crate::concept::Match, BodyError> {
            Ok(crate::concept::Match {
                equivalence: found
                    .equivalence
                    .as_ref()
                    .and_then(|code| code.value.as_deref())
                    .map(crate::concept::Equivalence::new),
                concept: found.concept.as_ref().map(concept),
                source: found.source.as_ref().and_then(|url| url.value.clone()),
                products: found
                    .product
                    .iter()
                    .map(|product| {
                        Ok(crate::concept::MatchValue {
                            name: product.element.as_ref().and_then(|uri| uri.value.clone()),
                            value: product
                                .concept
                                .clone()
                                .map(|coding| property_value(ParametersParameterValue::Coding(Box::new(coding))))
                                .transpose()?,
                        })
                    })
                    .collect::<Result<Vec<_>, BodyError>>()?,
                ..Default::default()
            })
        }

        /// Returns no `used-conceptmap`, which STU3 does not declare.
        const fn used_maps(_answer: &ConceptMapTranslateResponse) -> Vec<String> {
            Vec::new()
        }

        /// Returns no details, since STU3 `$validate-code` declares only
        /// `result`, `message` and `display`.
        fn validation_details(
            _answer: &ValueSetValidateCodeResponse,
        ) -> crate::outcome::ValidationDetails {
            crate::outcome::ValidationDetails::default()
        }

        /// Returns no `issues`, a parameter STU3 does not declare.
        const fn validation_issues(
            _answer: &ValueSetValidateCodeResponse,
        ) -> Result<Option<Outcome>, BodyError> {
            Ok(None)
        }
    };
}
