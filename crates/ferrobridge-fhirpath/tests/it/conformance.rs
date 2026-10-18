// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 FHIRPath test suite for FHIR R4 (`tests-fhir-r4.xml`) as two
//! conformance corpora, one case per `test` element: `fhirpath-r4-stu3` holds
//! every test whose expression calls a function FHIRPath 3.0.0 adds
//! (`eval::STU3_FUNCTIONS`, found by its name), run with
//! `Environment::with_fhirpath_stu3_functions`, and `fhirpath-r4` every other
//! test, run as FHIRPath N1 2.0.0.
//!
//! A case expecting the expression refused (`invalid`) passes when it fails
//! to parse, fails the static check against the input's resource type or
//! the types its `contextType` names, or fails to evaluate; any other case
//! with a `contextType` must pass that check. Any other case passes when the evaluation answers the
//! expected outputs, in order unless the test says `ordered="false"`. A
//! failing case records why, and a case on an input FHIR refuses records the
//! rule the input breaks. A case marked `checkOrderedFunctions="true"`
//! evaluates with `Environment::with_strict_ordering`. No case is
//! special-cased by its name.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use ferrobridge_fhirpath::eval::convert::primitive_value;
use ferrobridge_fhirpath::eval::error::EvalError;
use ferrobridge_fhirpath::eval::{Environment, Focus, STU3_FUNCTIONS};
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::model::{FhirType, Model, named_type, resource_type};
use ferrobridge_fhirpath::seam::{Profiles, SeamError};
use ferrobridge_fhirpath::value::{Collection, Item};
use ferrobridge_testkit::conformance::{Case, Corpus, record};
use fhir_types::codec::Value;
use regex::Regex;
use rust_decimal::Decimal;

use super::suite::{Output, Test, read};
use super::support::{load, suite_dir};

/// The profiles seam of the corpus run: it answers a core base definition
/// (`http://hl7.org/fhir/StructureDefinition/<Resource>`) by the element's
/// resource type alone and refuses every other structure, which is what the
/// suite's `conformsTo()` cases ask.
#[derive(Debug)]
struct BaseDefinitions;

impl Profiles for BaseDefinitions {
    fn conforms_to(
        &self,
        _value: &Value,
        type_name: Option<&str>,
        structure: &str,
    ) -> Result<bool, SeamError> {
        let name = structure
            .strip_prefix("http://hl7.org/fhir/StructureDefinition/")
            .filter(|name| resource_type(&R4, name).is_some())
            .ok_or_else(|| SeamError::from(format!("no structure definition {structure}")))?;
        Ok(type_name == Some(name))
    }
}

/// The suite inputs FHIR refuses, each with the refusal the reader reports
/// and the rule it enforces; every case on such an input fails with that
/// citation.
///
/// `parameters-example-types.xml` declares the XML Schema instance namespace
/// and `xsi:schemaLocation` on its root, and exchanged content "SHALL NOT
/// specify the schema or even contain the schema instance namespace in the
/// resource itself" (<https://hl7.org/fhir/R4/xml.html> §2.6.1.1).
const REFUSED_INPUTS: [(&str, &str, &str); 1] = [(
    "parameters-example-types.xml",
    "Resource.xmlns:xsi: unknown property",
    "R4 xml.html §2.6.1.1 forbids the schema instance namespace in a resource",
)];

/// Why a case on `file` cannot run when the input does not load.
fn refused_input(file: &str, error: &str) -> String {
    match REFUSED_INPUTS
        .iter()
        .find(|(input, refusal, _)| *input == file && *refusal == error)
    {
        Some((_, _, rule)) => format!("the input is refused as FHIR requires ({rule}): {error}"),
        None => format!("the input does not load: {error}"),
    }
}

/// The system type an output's `type` attribute names.
fn system_name(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "boolean" => "Boolean",
        "integer" => "Integer",
        "decimal" => "Decimal",
        "string" | "code" | "id" | "uri" => "String",
        "date" => "Date",
        "dateTime" => "DateTime",
        "time" => "Time",
        "Quantity" => "Quantity",
        _ => return None,
    })
}

/// Whether `item` is the expected output.
fn matches(item: &Item<'_>, expected: &Output) -> Result<bool, String> {
    let (own_type, value) = match item {
        Item::Node(node) => {
            let ty = node.fhir_type().map(|ty| ty.name);
            let value = primitive_value(node).map_err(|error| error.to_string())?;
            match value {
                Some(value) => (ty, value),
                None => return Ok(false),
            }
        }
        other => (None, other.clone()),
    };
    if let Some(kind) = expected.kind.as_deref() {
        let type_matches = own_type == Some(kind)
            || (system_name(kind).is_some() && value.system_type() == system_name(kind))
            || (kind == "date" && matches!(value, Item::DateTime(_)) && own_type == Some("date"));
        if !type_matches {
            return Ok(false);
        }
    }
    let text = expected.text.strip_prefix('@').unwrap_or(&expected.text);
    if let (Some(a), Ok(b)) = (number(&value), text.parse::<Decimal>()) {
        return Ok(a == b);
    }
    Ok(value.to_string() == text)
}

/// An Integer or Decimal as a decimal.
fn number(item: &Item<'_>) -> Option<Decimal> {
    match item {
        Item::Integer(value) => Some(Decimal::from(*value)),
        Item::Decimal(value) => Some(*value),
        _ => None,
    }
}

/// Whether the result answers every expected output.
fn compare(result: &Collection<'_>, outputs: &[Output], ordered: bool) -> Result<bool, String> {
    if result.len() != outputs.len() {
        return Ok(false);
    }
    if ordered {
        for (item, output) in result.iter().zip(outputs) {
            if !matches(item, output)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    let mut used = vec![false; outputs.len()];
    for item in result {
        let mut found = false;
        for (index, output) in outputs.iter().enumerate() {
            if !used[index] && matches(item, output)? {
                used[index] = true;
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A short rendering of a result for a failure reason.
fn render(result: &Collection<'_>) -> String {
    let items: Vec<String> = result
        .iter()
        .map(|item| match item {
            Item::Node(node) => match primitive_value(node) {
                Ok(Some(value)) => format!("{node}:{value}"),
                _ => node.to_string(),
            },
            other => other.to_string(),
        })
        .collect();
    format!("[{}]", items.join(", "))
}

/// The types a case's `contextType` names (`Period|Timing`), the types the
/// context may hold when the expression is checked.
fn context_types(model: &dyn Model, test: &Test) -> Result<Option<Vec<FhirType>>, String> {
    let Some(names) = test.attribute("contextType") else {
        return Ok(None);
    };
    names
        .split('|')
        .map(|name| named_type(model, name).ok_or_else(|| format!("no context type {name}")))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// The verdict on one test.
fn verdict(
    test: &Test,
    inputs: &BTreeMap<String, Result<Value, String>>,
    stu3: bool,
) -> Result<(), String> {
    let model: &dyn Model = &R4;
    let invalid = test.invalid.is_some();
    let expression = match Expression::parse(&test.expression) {
        Ok(expression) => expression,
        Err(_) if invalid => return Ok(()),
        Err(error) => return Err(format!("does not parse: {error}")),
    };
    let input = match test.attribute("inputfile") {
        Some(file) => Some(
            inputs
                .get(file)
                .ok_or_else(|| format!("no input {file}"))?
                .as_ref()
                .map_err(|error| refused_input(file, error))?,
        ),
        None => None,
    };
    let root_type = input
        .and_then(|value| value.get("resourceType"))
        .and_then(Value::as_str)
        .and_then(|name| resource_type(model, name));
    if invalid
        && test.attribute("context").is_none()
        && let Some(root_type) = root_type
        && expression.check(model, root_type).is_err()
    {
        return Ok(());
    }
    if let Some(contexts) = context_types(model, test)? {
        match expression.check_any(model, &contexts) {
            Err(_) if invalid => return Ok(()),
            Err(error) => return Err(format!("refused by the static check: {error}")),
            Ok(()) => {}
        }
    }
    let profiles = BaseDefinitions;
    let environment = Environment::new(model).with_profiles(&profiles);
    let environment = if test.attribute("checkOrderedFunctions") == Some("true") {
        environment.with_strict_ordering()
    } else {
        environment
    };
    let environment = if stu3 {
        environment.with_fhirpath_stu3_functions()
    } else {
        environment
    };
    let focus = match (input, test.attribute("context")) {
        (Some(resource), Some(context)) => {
            let context = Expression::parse(context)
                .map_err(|error| format!("the context does not parse: {error}"))?;
            let items = context
                .evaluate(&environment, Focus::resource(resource))
                .map_err(|error| format!("the context does not evaluate: {error}"))?;
            Focus::items(items, Some(resource))
        }
        (Some(resource), None) => Focus::resource(resource),
        (None, _) => Focus::none(),
    };
    let result = match expression.evaluate(&environment, focus) {
        Ok(result) => result,
        Err(_) if invalid => return Ok(()),
        // N1 §6.3.1 and §6.3.3: an identifier that cannot be resolved to a valid type
        // identifier makes the evaluator throw an error, so a case expecting a result fails.
        Err(error @ EvalError::UnknownType { .. }) => {
            return Err(format!(
                "refused as N1 §6.3.1 requires for an unresolvable type: {error}"
            ));
        }
        Err(error) => return Err(format!("does not evaluate: {error}")),
    };
    if let Some(kind) = &test.invalid {
        return Err(format!(
            "expected the expression refused ({kind}), it answers {}",
            render(&result)
        ));
    }
    let result = if test.attribute("predicate") == Some("true") {
        vec![Item::Boolean(!result.is_empty())]
    } else {
        result
    };
    let ordered = test.attribute("ordered") != Some("false");
    if compare(&result, &test.outputs, ordered)? {
        Ok(())
    } else {
        let expected: Vec<String> = test
            .outputs
            .iter()
            .map(|output| format!("{}:{}", output.kind.as_deref().unwrap_or("?"), output.text))
            .collect();
        Err(format!(
            "expected [{}], got {}",
            expected.join(", "),
            render(&result)
        ))
    }
}

/// Whether `expression` calls one of the functions FHIRPath 3.0.0 adds
/// (`STU3_FUNCTIONS`), found by the function name followed by `(`, so an
/// expression that does not parse as N1 is routed too.
fn calls_stu3_function(expression: &str) -> bool {
    static CALL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(&format!(r"\b(?:{})\s*\(", STU3_FUNCTIONS.join("|")))
            .expect("the pattern should compile")
    });
    CALL.is_match(expression)
}

#[test]
fn conformance_fhirpath_r4() {
    run(Corpus::FhirpathR4);
}

#[test]
fn conformance_fhirpath_r4_stu3() {
    run(Corpus::FhirpathR4Stu3);
}

/// Runs the suite's share of `corpus`: the tests that call a FHIRPath 3.0.0
/// function for `FhirpathR4Stu3`, with the opt-in on, and every other test
/// for `FhirpathR4`, as N1 2.0.0.
fn run(corpus: Corpus) {
    let stu3 = corpus == Corpus::FhirpathR4Stu3;
    let directory = suite_dir();
    let text = std::fs::read_to_string(directory.join("fhirpath/tests-fhir-r4.xml"))
        .expect("the suite should be vendored");
    let tests = read(&text).expect("the suite should read");
    let mut inputs = BTreeMap::new();
    for test in &tests {
        if let Some(file) = test.attribute("inputfile") {
            inputs
                .entry(String::from(file))
                .or_insert_with(|| load(&directory.join(file)));
        }
    }
    for (file, refusal, _) in REFUSED_INPUTS {
        let loaded = inputs
            .get(file)
            .unwrap_or_else(|| panic!("the suite reads the refused input {file}"));
        assert_eq!(
            loaded.as_ref().err().map(String::as_str),
            Some(refusal),
            "{file}: the listed refusal"
        );
    }
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut cases = Vec::with_capacity(tests.len());
    for test in &tests {
        let base = format!("{}/{}", test.group, test.name).replace(char::is_whitespace, "_");
        let count = seen.entry(base.clone()).or_insert(0);
        *count += 1;
        let id = if *count == 1 {
            base
        } else {
            format!("{base}#{count}")
        };
        if calls_stu3_function(&test.expression) != stu3 {
            continue;
        }
        let case = match verdict(test, &inputs, stu3) {
            Ok(()) => Case::pass(id),
            Err(reason) => match test.attribute("version").filter(|_| !stu3) {
                Some(version) => Case::fail(
                    id,
                    format!("targets FHIRPath {version}, beyond N1 2.0.0: {reason}"),
                ),
                None => Case::fail(id, reason),
            },
        };
        cases.push(case);
    }
    let outcome = record(corpus, &cases).expect("the corpus should record");
    assert!(
        outcome.regressed.is_empty(),
        "cases the pass list records fail now: {:#?}",
        outcome.regressed
    );
}
