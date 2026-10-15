// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: the vendored corpora's locations, document loading, and
//! the evaluation shorthand the topic tests use.

use std::path::{Path, PathBuf};

use ferrobridge_fhirpath::eval::error::EvalError;
use ferrobridge_fhirpath::eval::{Environment, Focus};
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::value::{Collection, Item};
use fhir_types::codec::Value;

/// The workspace root.
pub(crate) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The vendored FHIRPath test suite directory.
pub(crate) fn suite_dir() -> PathBuf {
    root().join("docs/specs/fhirpath-tests/r4")
}

/// Reads a FHIR resource in XML or JSON into the lexical document model.
pub(crate) fn load(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{error}"))?;
    if path.extension().is_some_and(|extension| extension == "xml") {
        fhir_types::xml::from_xml(&fhir_types::r4::schema::SCHEMAS, &text)
            .map(Value::Object)
            .map_err(|error| format!("{error}"))
    } else {
        serde_json::from_str(&text).map_err(|error| format!("{error}"))
    }
}

/// A JSON document from text.
pub(crate) fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("the fixture should be JSON")
}

/// Evaluates `expression` on `resource` in a plain R4 environment.
pub(crate) fn eval<'a>(expression: &str, resource: &'a Value) -> Result<Collection<'a>, EvalError> {
    let parsed = Expression::parse(expression).expect("the expression should parse");
    parsed.evaluate(&Environment::new(&R4), Focus::resource(resource))
}

/// Evaluates `expression` with no context.
pub(crate) fn eval_none(expression: &str) -> Result<Collection<'static>, EvalError> {
    let parsed = Expression::parse(expression).expect("the expression should parse");
    parsed.evaluate(&Environment::new(&R4), Focus::none())
}

/// The single Boolean an evaluation answers, `None` for an empty result.
pub(crate) fn truth(result: &Collection<'_>) -> Option<bool> {
    match result.as_slice() {
        [] => None,
        [Item::Boolean(value)] => Some(*value),
        other => panic!("expected one Boolean, got {other:?}"),
    }
}
