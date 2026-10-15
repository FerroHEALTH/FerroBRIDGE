// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The STU3 terminology contracts: exactly what 3.0.2 declares, with no
//! terminology ecosystem overlay.

use fhir_types::operation::{Parameter, ParameterSource, ParameterUse};

use crate::STU3;
use crate::operations::{contracts, find};

#[test]
fn every_stu3_terminology_operation_gets_a_contract() {
    let contracts = contracts(&STU3, "stu3");
    let modules: Vec<&str> = contracts.iter().map(|c| c.module.as_str()).collect();
    // The STU3 operations on CodeSystem, ValueSet and ConceptMap
    // (https://hl7.org/fhir/STU3/operationslist.html): STU3 defines
    // CodeSystem/$compose and neither $find-matches nor CodeSystem/$validate-code.
    assert_eq!(
        modules,
        vec![
            "code_system_compose",
            "code_system_lookup",
            "code_system_subsumes",
            "concept_map_closure",
            "concept_map_translate",
            "value_set_expand",
            "value_set_validate_code",
        ]
    );
}

#[test]
fn the_stu3_expand_request_is_exactly_what_3_0_2_declares() {
    // https://hl7.org/fhir/STU3/valueset-operations.html#expand: `profile` is a
    // uri naming an ExpansionProfile, and there is no version parameter.
    let contracts = contracts(&STU3, "stu3");
    let expand = find(&contracts, "ValueSet", "expand");
    let inputs: Vec<(&str, Option<&str>)> = expand
        .inputs
        .iter()
        .map(|f| (f.fhir_name.as_str(), f.type_code.as_deref()))
        .collect();
    assert_eq!(
        inputs,
        vec![
            ("url", Some("uri")),
            ("valueSet", Some("ValueSet")),
            ("context", Some("uri")),
            ("filter", Some("string")),
            ("profile", Some("uri")),
            ("date", Some("dateTime")),
            ("offset", Some("integer")),
            ("count", Some("integer")),
            ("includeDesignations", Some("boolean")),
            ("includeDefinition", Some("boolean")),
            ("activeOnly", Some("boolean")),
            ("excludeNested", Some("boolean")),
            ("excludeNotForUI", Some("boolean")),
            ("excludePostCoordinated", Some("boolean")),
            ("displayLanguage", Some("code")),
            ("limitedExpansion", Some("boolean")),
        ]
    );
}

#[test]
fn the_stu3_validate_code_takes_one_version() {
    // https://hl7.org/fhir/STU3/valueset-operations.html#validate-code.
    let contracts = contracts(&STU3, "stu3");
    let validate = find(&contracts, "ValueSet", "validate-code");
    let inputs: Vec<&str> = validate
        .inputs
        .iter()
        .map(|f| f.fhir_name.as_str())
        .collect();
    assert!(inputs.contains(&"version"));
    assert!(!inputs.contains(&"valueSetVersion"));
    assert!(!inputs.contains(&"systemVersion"));
    let outputs: Vec<&str> = validate
        .outputs
        .iter()
        .map(|f| f.fhir_name.as_str())
        .collect();
    assert_eq!(outputs, vec!["result", "message", "display"]);
}

#[test]
fn the_stu3_lookup_property_value_is_a_code() {
    // https://hl7.org/fhir/STU3/codesystem-operations.html#lookup.
    let contracts = contracts(&STU3, "stu3");
    let lookup = find(&contracts, "CodeSystem", "lookup");
    let property = lookup
        .outputs
        .iter()
        .find(|f| f.fhir_name == "property")
        .expect("the property output");
    let value = property
        .parts
        .iter()
        .find(|part| part.fhir_name == "value")
        .expect("property.value");
    assert_eq!(value.type_code.as_deref(), Some("code"));
}

/// Every parameter and part of `parameters`, depth first.
fn flattened(parameters: &'static [Parameter]) -> Vec<&'static Parameter> {
    let mut out = Vec::new();
    for parameter in parameters {
        out.push(parameter);
        out.extend(flattened(parameter.parts));
    }
    out
}

#[test]
fn the_emitted_stu3_descriptors_carry_no_overlay() {
    for operation in fhir_types::stu3::operations::OPERATIONS {
        for parameter in flattened(operation.parameters) {
            assert_eq!(
                parameter.source,
                ParameterSource::Version,
                "{} {}: STU3 takes no terminology ecosystem parameter",
                operation.code,
                parameter.name
            );
        }
    }
    // The R4 descriptors carry the overlay, so the check above can fail.
    let r4 = fhir_types::r4::operations::OPERATIONS
        .iter()
        .flat_map(|operation| flattened(operation.parameters))
        .filter(|parameter| parameter.source != ParameterSource::Version)
        .count();
    assert!(r4 > 0, "the R4 descriptors carry the overlay");
    let translate = fhir_types::stu3::operations::OPERATIONS
        .iter()
        .find(|operation| operation.code == "translate")
        .expect("STU3 declares $translate");
    assert!(translate.parameter(ParameterUse::In, "url").is_none());
    assert!(
        translate
            .parameter(ParameterUse::In, "conceptMap")
            .is_none()
    );
}
