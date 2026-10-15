// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The functions FHIR R4 defines over the world outside the document
//! (<https://hl7.org/fhir/R4/fhirpath.html#2.9.1.5>): each calls a seam the
//! caller supplies, a missing seam is an error naming the function, and a
//! seam's failure carries its source.

use std::error::Error;

use ferrobridge_fhirpath::eval::error::EvalError;
use ferrobridge_fhirpath::eval::{Environment, Focus};
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::seam::{
    Coded, Coding, Definitions, Profiles, ResolveContext, Resolver, SeamError, Terminology,
};
use ferrobridge_fhirpath::value::Collection;
use fhir_types::codec::Value;

use super::support::{json, truth};

fn observation() -> Value {
    json(
        r#"{
            "resourceType": "Observation",
            "status": "final",
            "code": {"coding": [{"system": "http://loinc.org", "code": "8867-4"}]},
            "subject": {"reference": "Patient/p1"},
            "performer": [{"reference": "Practitioner/x"}],
            "modifierExtension": [{"url": "http://example.org/m", "valueBoolean": true}]
        }"#,
    )
}

/// Resolves `Patient/p1` and nothing else.
#[derive(Debug)]
struct OnePatient;

impl Resolver for OnePatient {
    fn resolve(
        &self,
        reference: &str,
        context: ResolveContext<'_>,
    ) -> Result<Option<Value>, SeamError> {
        assert!(
            context.resource.is_some(),
            "the context resource travels to the seam"
        );
        Ok((reference == "Patient/p1")
            .then(|| json(r#"{"resourceType": "Patient", "id": "p1", "active": true}"#)))
    }
}

/// A seam that always fails.
#[derive(Debug)]
struct Broken;

impl Resolver for Broken {
    fn resolve(
        &self,
        _reference: &str,
        _context: ResolveContext<'_>,
    ) -> Result<Option<Value>, SeamError> {
        Err(SeamError::from("the server refused the read"))
    }
}

/// Knows LOINC 8867-4 in one value set and one subsumption.
#[derive(Debug)]
struct SmallTerminology;

impl Terminology for SmallTerminology {
    fn member_of(&self, coded: &Coded<'_>, value_set: &str) -> Result<bool, SeamError> {
        let heart_rate = |coding: &Coding<'_>| {
            coding.system == Some("http://loinc.org") && coding.code == Some("8867-4")
        };
        Ok(value_set == "http://example.org/vs/vitals"
            && match coded {
                Coded::Coding(coding) => heart_rate(coding),
                Coded::Concept(codings) => codings.iter().any(heart_rate),
                Coded::Code(code) => *code == "8867-4",
            })
    }

    fn subsumes(&self, ancestor: &Coding<'_>, descendant: &Coding<'_>) -> Result<bool, SeamError> {
        Ok(ancestor.code == Some("vital") && descendant.code == Some("8867-4"))
    }
}

/// Answers every profile with the resource's type.
#[derive(Debug)]
struct TypeProfiles;

impl Profiles for TypeProfiles {
    fn conforms_to(
        &self,
        _value: &Value,
        type_name: Option<&str>,
        structure: &str,
    ) -> Result<bool, SeamError> {
        Ok(structure.ends_with(type_name.unwrap_or("?")))
    }
}

/// Defines `Observation.status` and a slice holding final observations.
#[derive(Debug)]
struct StatusDefinitions;

impl Definitions for StatusDefinitions {
    fn element_definition(&self, path: &str) -> Result<Option<Value>, SeamError> {
        Ok((path == "Observation.status")
            .then(|| json(r#"{"path": "Observation.status", "min": 1}"#)))
    }

    fn in_slice(
        &self,
        structure: &str,
        slice: &str,
        value: &Value,
        _type_name: Option<&str>,
    ) -> Result<bool, SeamError> {
        if structure != "http://example.org/sd" {
            return Err(SeamError::from(format!("no structure {structure}")));
        }
        Ok(slice == "final" && value.get("status").and_then(Value::as_str) == Some("final"))
    }
}

fn run<'a>(
    expression: &str,
    env: &Environment<'a>,
    resource: &'a Value,
) -> Result<Collection<'a>, EvalError> {
    Expression::parse(expression)
        .expect("parses")
        .evaluate(env, Focus::resource(resource))
}

#[test]
fn every_seam_function_without_its_seam_names_itself() {
    let observation = observation();
    let env = Environment::new(&R4);
    for (expression, function) in [
        ("subject.resolve()", "resolve"),
        ("code.memberOf('http://example.org/vs/vitals')", "memberOf"),
        (
            "code.coding.first().subsumes(code.coding.first())",
            "subsumes",
        ),
        (
            "code.coding.first().subsumedBy(code.coding.first())",
            "subsumedBy",
        ),
        (
            "conformsTo('http://hl7.org/fhir/StructureDefinition/Observation')",
            "conformsTo",
        ),
        ("status.elementDefinition()", "elementDefinition"),
        (
            "Observation.slice('http://example.org/sd', 'final')",
            "slice",
        ),
    ] {
        match run(expression, &env, &observation) {
            Err(EvalError::NoSeam { function: named }) => {
                assert_eq!(named, function, "{expression}");
            }
            other => panic!("{expression}: expected NoSeam, got {other:?}"),
        }
    }
}

#[test]
fn resolve_answers_the_target_and_drops_what_does_not_resolve() {
    let observation = observation();
    let resolver = OnePatient;
    let env = Environment::new(&R4).with_resolver(&resolver);
    assert_eq!(
        truth(&run("subject.resolve().active = true", &env, &observation).expect("evaluates")),
        Some(true)
    );
    assert_eq!(
        truth(&run("subject.resolve() is Patient", &env, &observation).expect("evaluates")),
        Some(true)
    );
    assert_eq!(
        run("performer.resolve()", &env, &observation).expect("evaluates"),
        Vec::new()
    );
}

#[test]
fn a_seam_failure_ends_the_evaluation_with_its_source() {
    let observation = observation();
    let broken = Broken;
    let env = Environment::new(&R4).with_resolver(&broken);
    let error = run("subject.resolve()", &env, &observation).expect_err("the seam fails");
    assert!(matches!(
        error,
        EvalError::Seam {
            function: "resolve",
            ..
        }
    ));
    assert_eq!(
        error.source().map(ToString::to_string),
        Some(String::from("the server refused the read"))
    );
}

#[test]
fn member_of_asks_the_terminology_about_the_concept() {
    let observation = observation();
    let terminology = SmallTerminology;
    let env = Environment::new(&R4).with_terminology(&terminology);
    assert_eq!(
        truth(
            &run(
                "code.memberOf('http://example.org/vs/vitals')",
                &env,
                &observation
            )
            .expect("evaluates")
        ),
        Some(true)
    );
    assert_eq!(
        truth(
            &run(
                "code.memberOf('http://example.org/vs/other')",
                &env,
                &observation
            )
            .expect("evaluates")
        ),
        Some(false)
    );
}

#[test]
fn subsumption_answers_an_equivalent_coding_without_a_call() {
    let observation = observation();
    let terminology = SmallTerminology;
    let env = Environment::new(&R4).with_terminology(&terminology);
    let same = "code.coding.first().subsumes(code.coding.first())";
    assert_eq!(
        truth(&run(same, &env, &observation).expect("evaluates")),
        Some(true)
    );
}

#[test]
fn conforms_to_asks_the_profiles() {
    let observation = observation();
    let profiles = TypeProfiles;
    let env = Environment::new(&R4).with_profiles(&profiles);
    let base = "conformsTo('http://hl7.org/fhir/StructureDefinition/Observation')";
    assert_eq!(
        truth(&run(base, &env, &observation).expect("evaluates")),
        Some(true)
    );
}

#[test]
fn element_definition_and_slice_read_the_definitions() {
    let observation = observation();
    let definitions = StatusDefinitions;
    let env = Environment::new(&R4).with_definitions(&definitions);
    assert_eq!(
        truth(&run("status.elementDefinition().min = 1", &env, &observation).expect("evaluates")),
        Some(true)
    );
    assert_eq!(
        truth(
            &run(
                "Observation.slice('http://example.org/sd', 'final').exists()",
                &env,
                &observation
            )
            .expect("evaluates")
        ),
        Some(true)
    );
    assert!(matches!(
        run(
            "Observation.slice('http://example.org/other', 'final')",
            &env,
            &observation
        ),
        Err(EvalError::Seam {
            function: "slice",
            ..
        })
    ));
}

#[test]
fn check_modifiers_refuses_an_unknown_modifier_extension() {
    // FHIR R4 §2.9.1.5: if the check fails, an error is thrown.
    let observation = observation();
    let env = Environment::new(&R4);
    assert!(matches!(
        run("Observation.checkModifiers()", &env, &observation),
        Err(EvalError::Modifier { .. })
    ));
    let admitted = run(
        "Observation.checkModifiers('http://example.org/m').count()",
        &env,
        &observation,
    )
    .expect("evaluates");
    assert_eq!(admitted.len(), 1);
}

#[test]
fn trace_returns_its_input() {
    // N1 §5.9.1: trace does not change the input.
    let observation = observation();
    let env = Environment::new(&R4);
    assert_eq!(
        truth(&run("status.trace('s') = 'final'", &env, &observation).expect("evaluates")),
        Some(true)
    );
}
