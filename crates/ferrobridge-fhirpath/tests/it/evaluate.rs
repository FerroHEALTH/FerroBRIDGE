// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Evaluation through the public API, one behaviour per case, each citing
//! the clause it encodes.

use ferrobridge_fhirpath::eval::error::EvalError;
use ferrobridge_fhirpath::value::Item;

use super::support::{eval, eval_none, json, truth};

fn patient() -> fhir_types::codec::Value {
    json(
        r#"{
            "resourceType": "Patient",
            "id": "p1",
            "active": true,
            "birthDate": "1974-12-25",
            "_birthDate": {"extension": [{"url": "http://example.org/time", "valueTime": "14:35:45"}]},
            "name": [
                {"use": "official", "family": "Chalmers", "given": ["Peter", "James"]},
                {"use": "usual", "given": ["Jim"]}
            ],
            "deceasedBoolean": false
        }"#,
    )
}

#[test]
fn a_path_selects_every_repeat_in_order() {
    // N1 §3: each step selects the nodes with the label from the step before.
    let patient = patient();
    let given = eval("name.given", &patient).expect("evaluates");
    let texts: Vec<String> = given
        .iter()
        .map(|item| match item {
            Item::Node(node) => node
                .json()
                .and_then(|json| json.as_str())
                .unwrap_or_default()
                .to_owned(),
            other => other.to_string(),
        })
        .collect();
    assert_eq!(texts, ["Peter", "James", "Jim"]);
}

#[test]
fn a_leading_type_name_selects_the_context() {
    // N1 §3: a path may start with the type of the root node.
    let patient = patient();
    assert_eq!(
        truth(&eval("Patient.name.exists()", &patient).expect("evaluates")),
        Some(true)
    );
}

#[test]
fn a_type_name_that_is_not_the_context_is_an_error() {
    // N1 §3: the identifier must resolve to the type of the context.
    let patient = patient();
    assert!(matches!(
        eval("Encounter.status", &patient),
        Err(EvalError::ContextType { .. })
    ));
}

#[test]
fn an_element_the_type_does_not_define_is_an_error() {
    // N1 §3: an identifier that cannot be resolved signals an error.
    let patient = patient();
    assert!(matches!(
        eval("name.given1", &patient),
        Err(EvalError::UnknownElement { .. })
    ));
}

#[test]
fn a_choice_element_answers_to_its_name_without_the_suffix() {
    // FHIR R4 §2.9.1.1: choice elements are labelled without the [x] suffix.
    let patient = patient();
    assert_eq!(
        truth(&eval("deceased is boolean", &patient).expect("evaluates")),
        Some(true)
    );
    assert!(eval("deceasedBoolean", &patient).is_err());
}

#[test]
fn a_primitive_extension_is_reachable_from_the_value() {
    // FHIR R4 §2.9.1.2: a primitive has the children id and extension.
    let patient = patient();
    let found = eval(
        "birthDate.extension('http://example.org/time').exists()",
        &patient,
    )
    .expect("evaluates");
    assert_eq!(truth(&found), Some(true));
}

#[test]
fn an_empty_result_and_an_error_are_distinct() {
    // N1 §4.5: a single-input operator on several items signals an error.
    let patient = patient();
    assert_eq!(
        eval("name.suffix", &patient).expect("evaluates"),
        Vec::new()
    );
    assert!(matches!(
        eval("name.given + 'x'", &patient),
        Err(EvalError::Singleton { .. })
    ));
}

#[test]
fn comparison_across_precisions_is_empty() {
    // N1 §6.2: values of different precision compare as unknown.
    assert_eq!(
        truth(&eval_none("@2018-03 < @2018-03-01").expect("evaluates")),
        None
    );
    assert_eq!(
        truth(&eval_none("@2018-03-01T10:30:00 <= @2018-03-01T10:30:00.0").expect("evaluates")),
        Some(true)
    );
}

#[test]
fn decimals_are_exact() {
    // N1 §4.1.4: implementations should use fixed-precision decimals.
    assert_eq!(
        truth(&eval_none("0.1 + 0.2 = 0.3").expect("evaluates")),
        Some(true)
    );
    assert_eq!(
        truth(&eval_none("1.2 * 1.8 = 2.16").expect("evaluates")),
        Some(true)
    );
}

#[test]
fn integer_overflow_is_empty() {
    // N1 §6.6: arithmetic overflow results in empty.
    assert_eq!(eval_none("2147483647 + 1").expect("evaluates"), Vec::new());
}

#[test]
fn calendar_arithmetic_keeps_the_last_day_of_a_month() {
    // N1 §6.7.1: a day past the end of the resulting month becomes its last.
    let result = eval_none("@2015-01-31 + 1 month").expect("evaluates");
    assert_eq!(
        result.first().map(ToString::to_string),
        Some(String::from("2015-02-28"))
    );
}

#[test]
fn iif_evaluates_only_the_chosen_branch() {
    // N1 §5.5.1: short-circuit behaviour is expected in iif().
    assert_eq!(
        truth(&eval_none("iif(true, true, (1 | 2).single())").expect("evaluates")),
        Some(true)
    );
}

#[test]
fn an_unknown_function_is_an_error() {
    assert!(matches!(
        eval_none("1.frobnicate()"),
        Err(EvalError::UnknownFunction { .. })
    ));
}

#[test]
fn an_undefined_variable_is_an_error() {
    // N1 §9: accessing an undefined environment variable is an error.
    assert!(matches!(
        eval_none("%nothing"),
        Err(EvalError::UnknownVariable { .. })
    ));
}

#[test]
fn fhir_variables_are_defined() {
    // FHIR R4 §2.9.1.7.
    assert_eq!(
        truth(
            &eval_none(
                "%`vs-administrative-gender` = 'http://hl7.org/fhir/ValueSet/administrative-gender'"
            )
            .expect("evaluates")
        ),
        Some(true)
    );
    assert_eq!(
        truth(&eval_none("%ucum = 'http://unitsofmeasure.org'").expect("evaluates")),
        Some(true)
    );
}
