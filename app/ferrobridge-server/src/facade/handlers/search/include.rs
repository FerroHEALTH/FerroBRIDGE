// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `_include`: the resources a match references through a declared reference
//! parameter (<https://hl7.org/fhir/R4/search.html#include>).
//!
//! The facade stores nothing, so the resources it can include are those the
//! match's own outbound run creates beside it; a reference to anything else
//! is followed by the client. No specification governs where an included
//! resource is found: our own design.

use ferrobridge_fhirpath::eval::Environment;
use ferrobridge_fhirpath::eval::Focus;
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::value::Item;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use http::StatusCode;

use crate::facade::handlers::render::Rendered;
use crate::facade::identity::FhirResourceId;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;
use crate::facade::search::bundle::Inclusion;
use crate::facade::search::criteria::Included;

/// Returns the resources of `created` that `matched` references through each
/// of `includes`, each once, in the order `created` holds them.
///
/// # Errors
///
/// The `500` of an expression the evaluator refuses on the rendered match.
pub(super) fn follow(
    includes: &[Included],
    matched: &Object,
    created: &[(String, FhirResourceId, Rendered)],
) -> Result<Vec<Inclusion>, Refusal> {
    if includes.is_empty() || created.is_empty() {
        return Ok(Vec::new());
    }
    let resource = Value::Object(matched.clone());
    let mut referenced: Vec<(Option<&str>, String)> = Vec::new();
    for include in includes {
        for reference in references(&include.expression, &resource)? {
            referenced.push((include.target.as_deref(), reference));
        }
    }
    Ok(created
        .iter()
        .filter(|(resource_type, id, _)| {
            let literal = format!("{resource_type}/{}", id.as_str());
            referenced.iter().any(|(target, reference)| {
                target.is_none_or(|target| target == resource_type) && *reference == literal
            })
        })
        .map(|(resource_type, id, rendered)| Inclusion {
            resource_type: resource_type.clone(),
            id: id.clone(),
            resource: rendered.body.clone(),
        })
        .collect())
}

/// Returns the `Reference.reference` of every reference `expression` selects
/// on `resource`.
fn references(expression: &str, resource: &Value) -> Result<Vec<String>, Refusal> {
    let refuse = |detail: String| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(detail),
        )
    };
    let parsed = Expression::parse(expression).map_err(|error| {
        refuse(format!(
            "the include expression {expression} does not parse: {error}"
        ))
    })?;
    let environment = Environment::new(&R4);
    let selected = parsed
        .evaluate(&environment, Focus::resource(resource))
        .map_err(|error| {
            refuse(format!(
                "the include expression {expression} failed: {error}"
            ))
        })?;
    Ok(selected
        .iter()
        .filter_map(|item| match item {
            Item::Node(node) => node.json(),
            _ => None,
        })
        .filter_map(|reference| match reference {
            Value::Object(object) => object.get("reference").and_then(Value::as_str),
            _ => None,
        })
        .map(str::to_owned)
        .collect())
}
