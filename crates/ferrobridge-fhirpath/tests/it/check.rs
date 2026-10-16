// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The static check of an expression against its context type (N1 §3,
//! §11).

use ferrobridge_fhirpath::check::SemanticError;
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::model::{named_type, path_type, resource_type};

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

/// Checks `expression` with a context that may hold any of `types`.
fn check_types(expression: &str, types: &[&str]) -> Result<(), SemanticError> {
    let contexts: Vec<_> = types
        .iter()
        .map(|name| named_type(&R4, name).expect("the context should be a type"))
        .collect();
    Expression::parse(expression)
        .expect("parses")
        .check_any(&R4, &contexts)
}

/// Whether the check refuses `expression` for a function's input.
fn refused_input(expression: &str, types: &[&str]) -> bool {
    matches!(
        check_types(expression, types),
        Err(SemanticError::Input { .. })
    )
}

#[test]
fn to_string_of_a_type_it_does_not_take_is_refused() {
    // N1 §5.5.8 and §11: toString() takes a primitive or a Quantity.
    assert!(refused_input("$this.toString()", &["Period"]));
    assert!(refused_input("toString()", &["Period", "Timing"]));
    assert_eq!(
        check_types("$this.toString()", &["Period", "dateTime"]),
        Ok(())
    );
    assert_eq!(check_types("$this.toString()", &["Age"]), Ok(()));
}

#[test]
fn implies_checks_its_right_operand_under_the_guard() {
    let both = ["Period", "Timing"];
    assert_eq!(
        check_types("$this is dateTime implies $this.toString() = ''", &both),
        Ok(())
    );
    assert!(refused_input(
        "$this is Period implies $this.toString() = ''",
        &both
    ));
    assert_eq!(
        check_types(
            "$this.is(dateTime) implies $this.toString() = ''",
            &["Period"]
        ),
        Ok(())
    );
    assert!(refused_input(
        "$this.is(Period) implies $this.toString() = ''",
        &both
    ));
}

#[test]
fn and_checks_its_right_operand_under_the_guard() {
    assert_eq!(
        check_types("$this is dateTime and $this.toString() = ''", &["Period"]),
        Ok(())
    );
    assert!(refused_input(
        "$this is Period and $this.toString() = ''",
        &["Period", "Timing"]
    ));
}

#[test]
fn or_checks_its_right_operand_with_the_guard_false() {
    assert_eq!(
        check_types("$this is Period or $this.toString() = ''", &["Period"]),
        Ok(())
    );
    assert!(refused_input(
        "$this is Period or $this.toString() = ''",
        &["Period", "Timing"]
    ));
    assert!(refused_input(
        "($this is dateTime or %resource.id = 'x') implies $this.toString() = ''",
        &["Period"]
    ));
}

#[test]
fn not_inverts_the_guard() {
    assert_eq!(
        check_types(
            "($this is Period).not() implies $this.toString() = ''",
            &["Period"]
        ),
        Ok(())
    );
    assert!(refused_input(
        "($this is Period).not() implies $this.toString() = ''",
        &["Period", "Timing"]
    ));
}

#[test]
fn iif_checks_each_branch_under_the_criterion() {
    assert_eq!(
        check_types(
            "iif($this is dateTime, $this.toString() = '', true)",
            &["Period"]
        ),
        Ok(())
    );
    assert_eq!(
        check_types(
            "iif($this is Period, true, $this.toString() = '')",
            &["Period"]
        ),
        Ok(())
    );
    assert!(refused_input(
        "iif($this is Period, $this.toString() = '', true)",
        &["Period"]
    ));
}

#[test]
fn where_answers_the_types_its_criterion_keeps() {
    assert_eq!(
        check("value.where($this is Quantity).unit", "Observation"),
        Ok(())
    );
    assert!(matches!(
        check("value.where($this is Period).unit", "Observation"),
        Err(SemanticError::UnknownElement { .. })
    ));
}

#[test]
fn a_member_under_the_guard_is_checked_against_the_narrowed_types() {
    let both = ["Period", "Timing"];
    assert_eq!(
        check_types("$this is Period implies $this.start.exists()", &both),
        Ok(())
    );
    assert!(matches!(
        check_types("$this is Timing implies $this.start.exists()", &both),
        Err(SemanticError::UnknownElement { .. })
    ));
    assert_eq!(
        check_types("$this is Quantity implies $this.unknown.exists()", &both),
        Ok(())
    );
}

#[test]
fn a_supertype_guard_keeps_its_subtypes() {
    // N1 §6.3.1: `is` is true for the type named or a subclass of it.
    assert!(refused_input(
        "$this is Element implies $this.toString() = ''",
        &["Period"]
    ));
}

#[test]
fn a_guard_on_another_expression_does_not_narrow_the_focus() {
    assert!(refused_input(
        "$this.start is dateTime implies $this.toString() = ''",
        &["Period"]
    ));
    assert!(refused_input(
        "$this is dateTime xor $this.toString() = ''",
        &["Period"]
    ));
}
