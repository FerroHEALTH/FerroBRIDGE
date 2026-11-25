// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The logical model of a source's JSON, generated from the schema its
//! `OpenAPI` document publishes.
//!
//! One `OpenAPI` Schema Object (`OpenAPI` 3.0.3 §4.7.24) becomes one
//! `StructureDefinition` of `kind = logical`, its properties the elements:
//! `allOf` members are merged in order, a `$ref` to a local schema is
//! followed, `string` maps to `string`, `date` or `dateTime` by its `format`,
//! `integer` to `integer`, `number` to `decimal`, `boolean` to `boolean`, an
//! `array` repeats its item type, and an `object` or a referenced object
//! schema becomes a backbone element with its own children. No
//! specification governs the mapping of an `OpenAPI` schema to a logical
//! model: our own design.

use fhir_types::codec::{Object, Value};

/// Why a schema cannot become a logical model.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OpenApiError {
    /// The document has no schema of the name.
    #[error("the OpenAPI document has no schema {0}")]
    NoSchema(String),
    /// A `$ref` points outside the document's schemas.
    #[error("the reference {0} names no schema of the document")]
    Reference(String),
    /// The schemas refer to each other in a cycle.
    #[error("the schema {0} contains itself")]
    Cycle(String),
}

/// One element of the model being built.
struct Element {
    path: String,
    max: &'static str,
    code: String,
}

/// Returns the logical model of the schema `name` of `document`, with the
/// canonical `url`.
///
/// # Errors
/// Returns [`OpenApiError`] for an unknown schema, a reference outside the
/// document's schemas, or a schema that contains itself.
pub fn logical_model(document: &Value, name: &str, url: &str) -> Result<Value, OpenApiError> {
    let schema = schema(document, name)?;
    let mut elements = vec![Element {
        path: name.to_owned(),
        max: "1",
        code: String::from("Base"),
    }];
    let mut trail = vec![name.to_owned()];
    properties(document, schema, name, &mut elements, &mut trail)?;
    let elements = elements
        .into_iter()
        .map(|element| {
            object([
                ("id", string(&element.path)),
                ("path", string(&element.path)),
                ("min", Value::from(0)),
                ("max", string(element.max)),
                (
                    "type",
                    Value::Array(vec![object([("code", string(&element.code))])]),
                ),
            ])
        })
        .collect();
    Ok(object([
        ("resourceType", string("StructureDefinition")),
        ("url", string(url)),
        ("name", string(name)),
        ("status", string("draft")),
        ("kind", string("logical")),
        ("abstract", Value::Bool(false)),
        ("type", string(name)),
        ("snapshot", object([("element", Value::Array(elements))])),
    ]))
}

/// Returns the schema `name` of `document`.
fn schema<'d>(document: &'d Value, name: &str) -> Result<&'d Value, OpenApiError> {
    document
        .get("components")
        .and_then(|components| components.get("schemas"))
        .and_then(|schemas| schemas.get(name))
        .ok_or_else(|| OpenApiError::NoSchema(name.to_owned()))
}

/// Returns the schema a `$ref` of `node` names, or `node` itself.
fn resolved<'d>(
    document: &'d Value,
    node: &'d Value,
) -> Result<(&'d Value, Option<String>), OpenApiError> {
    match node.get("$ref").and_then(Value::as_str) {
        Some(reference) => {
            let name = reference
                .strip_prefix("#/components/schemas/")
                .ok_or_else(|| OpenApiError::Reference(reference.to_owned()))?;
            Ok((schema(document, name)?, Some(name.to_owned())))
        }
        None => Ok((node, None)),
    }
}

/// Adds the elements of the properties of `node`, merged with its `allOf`
/// members, under `path`.
fn properties(
    document: &Value,
    node: &Value,
    path: &str,
    elements: &mut Vec<Element>,
    trail: &mut Vec<String>,
) -> Result<(), OpenApiError> {
    if let Some(members) = node.get("allOf").and_then(Value::as_array) {
        for member in members {
            let (member, name) = resolved(document, member)?;
            enter(trail, name.as_deref())?;
            properties(document, member, path, elements, trail)?;
            leave(trail, name.as_deref());
        }
    }
    let Some(members) = node.get("properties").and_then(Value::as_object) else {
        return Ok(());
    };
    for (key, property) in members {
        let child = format!("{path}.{key}");
        let (property, name) = resolved(document, property)?;
        let (item, max) = match property.get("type").and_then(Value::as_str) {
            Some("array") => {
                let items = property
                    .get("items")
                    .ok_or_else(|| OpenApiError::Reference(child.clone()))?;
                (resolved(document, items)?, "*")
            }
            _ => ((property, name), "1"),
        };
        let ((item, item_name), max) = (item, max);
        let code = code(item);
        let complex = code == "BackboneElement";
        elements.push(Element {
            path: child.clone(),
            max,
            code: String::from(code),
        });
        if complex {
            enter(trail, item_name.as_deref())?;
            properties(document, item, &child, elements, trail)?;
            leave(trail, item_name.as_deref());
        }
    }
    Ok(())
}

/// Records that the schema `name` is being expanded, refusing a cycle.
fn enter(trail: &mut Vec<String>, name: Option<&str>) -> Result<(), OpenApiError> {
    if let Some(name) = name {
        if trail.iter().any(|seen| seen == name) {
            return Err(OpenApiError::Cycle(name.to_owned()));
        }
        trail.push(name.to_owned());
    }
    Ok(())
}

/// Records that the schema `name` is expanded.
fn leave(trail: &mut Vec<String>, name: Option<&str>) {
    if name.is_some() {
        trail.pop();
    }
}

/// Returns the FHIR type code of the schema `node`.
fn code(node: &Value) -> &'static str {
    let format = node.get("format").and_then(Value::as_str);
    match (node.get("type").and_then(Value::as_str), format) {
        (Some("string"), Some("date")) => "date",
        (Some("string"), Some("date-time")) => "dateTime",
        (Some("integer"), _) => "integer",
        (Some("number"), _) => "decimal",
        (Some("boolean"), _) => "boolean",
        (Some("object") | None, _)
            if node.get("properties").is_some() || node.get("allOf").is_some() =>
        {
            "BackboneElement"
        }
        _ => "string",
    }
}

/// Returns a JSON string.
fn string(text: &str) -> Value {
    Value::String(text.to_owned())
}

/// Returns an object of `members`.
fn object<const N: usize>(members: [(&str, Value); N]) -> Value {
    Value::Object(
        members
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect::<Object>(),
    )
}
