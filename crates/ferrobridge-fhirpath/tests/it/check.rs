// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The static check of an expression against its context type (N1 §3,
//! §11).

use ferrobridge_fhirpath::check::SemanticError;
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::model::{path_type, resource_type};

fn check(expression: &str, context: &str) -> Result<(), SemanticError> {
    let context = path_type(&R4, context)
        .or_else(|| resource_type(&R4, context))
        .expect("the context should be a type");
    Expression::parse(expression)
        .expect("parses")
        .check(&R4, context)
}

#[test]
fn a_path_through_defined_elements_passes() {
    assert_eq!(check("name.given.first() = 'x'", "Patient"), Ok(()));
    assert_eq!(check("Observation.value.unit", "Observation"), Ok(()));
    assert_eq!(check("birthDate.extension.value", "Patient"), Ok(()));
}

#[test]
fn an_undefined_member_is_refused() {
    assert!(matches!(
        check("name.given1", "Patient"),
        Err(SemanticError::UnknownElement { .. })
    ));
}

#[test]
fn a_choice_type_suffix_is_refused() {
    // FHIR R4 §2.9.1.1: a choice element is named without its type.
    assert!(matches!(
        check("Observation.valueQuantity.unit", "Observation"),
        Err(SemanticError::UnknownElement { .. })
    ));
}

#[test]
fn a_cast_narrows_what_follows() {
    // N1 §6.3.3: `as` gives the value as the named type.
    assert!(matches!(
        check("(Observation.value as Period).unit", "Observation"),
        Err(SemanticError::UnknownElement { .. })
    ));
    assert_eq!(
        check("(Observation.value as Quantity).unit", "Observation"),
        Ok(())
    );
}

#[test]
fn a_leading_type_that_is_not_the_context_is_refused() {
    assert!(matches!(
        check("Encounter.status", "Patient"),
        Err(SemanticError::ContextType { .. })
    ));
}

#[test]
fn an_unknown_type_specifier_is_refused() {
    assert!(matches!(
        check("gender.ofType(string1)", "Patient"),
        Err(SemanticError::UnknownType { .. })
    ));
}

#[test]
fn a_literal_criterion_of_iif_must_be_boolean() {
    // N1 §5.5.1: the criterion is expected to evaluate to a Boolean.
    assert!(matches!(
        check("iif('x', 1, 2)", "Patient"),
        Err(SemanticError::Criterion { .. })
    ));
    assert_eq!(check("iif(true, 1, 2)", "Patient"), Ok(()));
}

#[test]
fn untypable_paths_pass() {
    // N1 §11: children(), descendants() and resolve() cannot be typed.
    assert_eq!(check("descendants().anything", "Patient"), Ok(()));
    assert_eq!(check("contained.anything", "Patient"), Ok(()));
}

#[test]
fn a_cast_of_a_choice_narrows_to_the_named_type() {
    // N1 §6.3.3: `as` gives the value as the named type, here one of the
    // two types RiskAssessment.prediction.probability[x] admits.
    assert_eq!(
        check("(probability as Range).low", "RiskAssessment.prediction"),
        Ok(())
    );
    assert_eq!(
        check("probability.as(Range).high", "RiskAssessment.prediction"),
        Ok(())
    );
    assert!(matches!(
        check("(probability as decimal).low", "RiskAssessment.prediction"),
        Err(SemanticError::UnknownElement { .. })
    ));
}

#[test]
fn of_type_narrows_what_follows() {
    // N1 §5.2.4: ofType() keeps the items of the named type, so what follows
    // navigates that type alone.
    assert_eq!(
        check("prediction.probability.ofType(Range).low", "RiskAssessment"),
        Ok(())
    );
    assert!(matches!(
        check(
            "prediction.probability.ofType(decimal).low",
            "RiskAssessment"
        ),
        Err(SemanticError::UnknownElement { .. })
    ));
    assert!(matches!(
        check("value.ofType(Period).unit", "Observation"),
        Err(SemanticError::UnknownElement { .. })
    ));
}
