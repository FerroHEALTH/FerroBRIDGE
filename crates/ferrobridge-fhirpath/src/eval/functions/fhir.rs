// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The functions FHIR R4 adds (<https://hl7.org/fhir/R4/fhirpath.html#2.9.1.5>).
//!
//! `resolve()`, `memberOf()`, `subsumes()`, `subsumedBy()`, `conformsTo()`,
//! `elementDefinition()` and `slice()` call the caller's seams; without one
//! the call is an error naming the function.

use std::sync::Arc;

use fhir_types::codec::Value;

use super::Call;
use crate::eval::convert::{describe, primitive_value, single, string};
use crate::eval::error::EvalError;
use crate::model::{FhirType, named_type};
use crate::seam::{Coded, Coding, ResolveContext, SeamError};
use crate::value::node::{Doc, Node};
use crate::value::{Collection, Item};

/// The error for a seam failure of `function`.
fn seam(function: &'static str) -> impl FnOnce(SeamError) -> EvalError {
    move |source| EvalError::Seam { function, source }
}

/// The JSON of the first node of a collection, the resource a `%resource`
/// or `%rootResource` stands for.
fn first_json<'c>(items: &'c Collection<'_>) -> Option<&'c Value> {
    items.first().and_then(Item::as_node).and_then(Node::json)
}

/// A one-item Boolean collection.
fn flag<'a>(value: bool) -> Collection<'a> {
    vec![Item::Boolean(value)]
}

/// The `url` of an extension item.
fn url<'i>(item: &'i Item<'_>) -> Option<&'i str> {
    item.as_node()
        .and_then(Node::json)
        .and_then(|json| json.get("url"))
        .and_then(Value::as_str)
}

/// Calls the function when this module defines it.
pub(super) fn call<'a>(call: &Call<'_, '_, 'a>) -> Result<Option<Collection<'a>>, EvalError> {
    let result = match call.name {
        "extension" => extension(call),
        "hasValue" => has_value(call),
        "getValue" => get_value(call),
        "resolve" => resolve(call),
        "elementDefinition" => element_definition(call),
        "slice" => slice(call),
        "checkModifiers" => check_modifiers(call),
        "conformsTo" => conforms_to(call),
        "memberOf" => member_of(call),
        "subsumes" => subsumption(call, "subsumes"),
        "subsumedBy" => subsumption(call, "subsumedBy"),
        "htmlChecks" => html_checks(call),
        _ => return Ok(None),
    };
    result.map(Some)
}

/// `extension(url)`: the extensions with the url.
fn extension<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    call.arity(1, 1, "1")?;
    let model = call.model();
    let Some(wanted) = string(model, &call.arg_or_empty(0)?, "extension")? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in &call.input {
        let Item::Node(node) = item else { continue };
        for extension in node.member(model, "extension")?.unwrap_or_default() {
            if url(&extension) == Some(wanted.as_str()) {
                out.push(extension);
            }
        }
    }
    Ok(out)
}

/// `hasValue()`: a single primitive with a value.
fn has_value<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    call.arity(0, 0, "no")?;
    Ok(flag(match call.input.as_slice() {
        [Item::Node(node)] => {
            node.fhir_type().is_some_and(FhirType::is_primitive) && node.json().is_some()
        }
        _ => false,
    }))
}

/// `getValue()`: the system value of a single primitive.
fn get_value<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    call.arity(0, 0, "no")?;
    match call.input.as_slice() {
        [Item::Node(node)] if node.fhir_type().is_some_and(FhirType::is_primitive) => {
            Ok(primitive_value(node)?.into_iter().collect())
        }
        _ => Ok(Vec::new()),
    }
}

/// `resolve()`: the resources the references name, through the seam.
fn resolve<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    call.arity(0, 0, "no")?;
    let model = call.model();
    let resolver = call.engine.env().resolver.ok_or(EvalError::NoSeam {
        function: "resolve",
    })?;
    let context = ResolveContext {
        resource: first_json(call.engine.resource()),
        root_resource: first_json(call.engine.root()),
    };
    let mut out = Vec::new();
    for item in &call.input {
        let Some(reference) = reference(item)? else {
            continue;
        };
        if let Some(target) = resolver
            .resolve(&reference, context)
            .map_err(seam("resolve"))?
        {
            out.push(Item::Node(Node::resource(
                model,
                Doc::Owned(Arc::new(target)),
            )));
        }
    }
    Ok(out)
}

/// `elementDefinition()`: the definitions of the elements, through the seam.
fn element_definition<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    call.arity(0, 0, "no")?;
    let definitions = call.engine.env().definitions.ok_or(EvalError::NoSeam {
        function: "elementDefinition",
    })?;
    let ty = named_type(call.model(), "ElementDefinition").ok_or(EvalError::UnknownType {
        name: String::from("ElementDefinition"),
    })?;
    let mut out = Vec::new();
    for item in &call.input {
        let Some(path) = item.as_node().and_then(Node::definition) else {
            continue;
        };
        if let Some(definition) = definitions
            .element_definition(path)
            .map_err(seam("elementDefinition"))?
        {
            out.push(Item::Node(Node::typed(
                ty,
                Doc::Owned(Arc::new(definition)),
                None,
            )));
        }
    }
    Ok(out)
}

/// `slice(structure, name)`: the elements in the slice, through the seam.
fn slice<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    call.arity(2, 2, "2")?;
    let model = call.model();
    let definitions = call
        .engine
        .env()
        .definitions
        .ok_or(EvalError::NoSeam { function: "slice" })?;
    let (Some(structure), Some(name)) = (
        string(model, &call.arg_or_empty(0)?, "slice")?,
        string(model, &call.arg_or_empty(1)?, "slice")?,
    ) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in &call.input {
        let Some(node) = item.as_node() else { continue };
        let Some(json) = node.json() else { continue };
        let type_name = node.fhir_type().map(|ty| ty.name);
        if definitions
            .in_slice(&structure, &name, json, type_name)
            .map_err(seam("slice"))?
        {
            out.push(item.clone());
        }
    }
    Ok(out)
}

/// `checkModifiers([modifier])`: the input, when no modifier extension
/// other than the admitted ones is present.
fn check_modifiers<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    let model = call.model();
    let mut admitted = Vec::new();
    for index in 0..call.args.len() {
        for item in call.arg_or_empty(index)? {
            match item {
                Item::String(text) => admitted.push(text),
                other => return Err(EvalError::type_error("checkModifiers", describe(&other))),
            }
        }
    }
    for item in &call.input {
        let Item::Node(node) = item else { continue };
        for extension in node.member(model, "modifierExtension")?.unwrap_or_default() {
            let found = url(&extension).unwrap_or_default();
            if !admitted.iter().any(|admitted| admitted == found) {
                return Err(EvalError::Modifier {
                    url: String::from(found),
                });
            }
        }
    }
    Ok(call.input.clone())
}

/// `conformsTo(structure)`, through the seam.
fn conforms_to<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    call.arity(1, 1, "1")?;
    let profiles = call.engine.env().profiles.ok_or(EvalError::NoSeam {
        function: "conformsTo",
    })?;
    let Some(structure) = string(call.model(), &call.arg_or_empty(0)?, "conformsTo")? else {
        return Ok(Vec::new());
    };
    let Some(item) = single(&call.input, "conformsTo")? else {
        return Ok(Vec::new());
    };
    let Some((json, type_name)) = item.as_node().and_then(|node| {
        node.json()
            .map(|json| (json, node.fhir_type().map(|ty| ty.name)))
    }) else {
        return Err(EvalError::type_error("conformsTo", describe(item)));
    };
    let conforms = profiles
        .conforms_to(json, type_name, &structure)
        .map_err(seam("conformsTo"))?;
    Ok(flag(conforms))
}

/// `memberOf(valueset)`, through the seam.
fn member_of<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    call.arity(1, 1, "1")?;
    let terminology = call.engine.env().terminology.ok_or(EvalError::NoSeam {
        function: "memberOf",
    })?;
    let Some(value_set) = string(call.model(), &call.arg_or_empty(0)?, "memberOf")? else {
        return Ok(Vec::new());
    };
    let Some(item) = single(&call.input, "memberOf")? else {
        return Ok(Vec::new());
    };
    let member = terminology
        .member_of(&coded(item, "memberOf")?, &value_set)
        .map_err(seam("memberOf"))?;
    Ok(flag(member))
}

/// `subsumes(code)` and `subsumedBy(code)`: an equivalent coding answers
/// without a call, every other pair goes through the seam.
fn subsumption<'a>(
    call: &Call<'_, '_, 'a>,
    function: &'static str,
) -> Result<Collection<'a>, EvalError> {
    call.arity(1, 1, "1")?;
    let terminology = call
        .engine
        .env()
        .terminology
        .ok_or(EvalError::NoSeam { function })?;
    let other = call.arg_or_empty(0)?;
    let (Some(source), Some(given)) = (single(&call.input, function)?, single(&other, function)?)
    else {
        return Ok(Vec::new());
    };
    let (source, given) = (codings(source, function)?, codings(given, function)?);
    for a in &source {
        for b in &given {
            if a.system == b.system && a.code == b.code {
                return Ok(flag(true));
            }
            let (ancestor, descendant) = if function == "subsumes" {
                (a, b)
            } else {
                (b, a)
            };
            if terminology
                .subsumes(ancestor, descendant)
                .map_err(seam(function))?
            {
                return Ok(flag(true));
            }
        }
    }
    Ok(flag(false))
}

/// `htmlChecks()`: the narrative rules on an xhtml element; undefined, so
/// empty, on any other.
fn html_checks<'a>(call: &Call<'_, '_, 'a>) -> Result<Collection<'a>, EvalError> {
    call.arity(0, 0, "no")?;
    Ok(match call.input.as_slice() {
        [Item::Node(node)] if node.fhir_type().is_some_and(|ty| ty.name == "xhtml") => {
            let text = node.json().and_then(Value::as_str).unwrap_or_default();
            flag(crate::html::check(text))
        }
        _ => Vec::new(),
    })
}

/// The reference an item names: a string or a URI-typed primitive, or a
/// `Reference`'s `reference` (FHIR R4 §2.9.1.5).
fn reference(item: &Item<'_>) -> Result<Option<String>, EvalError> {
    match item {
        Item::String(text) => Ok(Some(text.clone())),
        Item::Node(node) => match node.fhir_type() {
            Some(ty) if ty.name == "Reference" => Ok(node
                .json()
                .and_then(|json| json.get("reference"))
                .and_then(Value::as_str)
                .map(String::from)),
            Some(ty) if ty.is_primitive() => match primitive_value(node)? {
                Some(Item::String(text)) => Ok(Some(text)),
                _ => Ok(None),
            },
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}

/// The coding a `Coding` node holds.
fn coding(json: &Value) -> Coding<'_> {
    let text = |key: &str| json.get(key).and_then(Value::as_str);
    Coding {
        system: text("system"),
        version: text("version"),
        code: text("code"),
    }
}

/// What `memberOf()` asks about an item (FHIR R4 §2.9.1.5).
fn coded<'c>(item: &'c Item<'_>, function: &str) -> Result<Coded<'c>, EvalError> {
    match item {
        Item::String(text) => Ok(Coded::Code(text)),
        Item::Node(node) => match (node.fhir_type().map(|ty| ty.name), node.json()) {
            (Some("Coding"), Some(json)) => Ok(Coded::Coding(coding(json))),
            (Some("CodeableConcept"), Some(json)) => Ok(Coded::Concept(
                json.get("coding")
                    .and_then(Value::as_array)
                    .unwrap_or_default()
                    .iter()
                    .map(coding)
                    .collect(),
            )),
            (Some(_), Some(Value::String(text))) => Ok(Coded::Code(text)),
            _ => Err(EvalError::type_error(function, describe(item))),
        },
        other => Err(EvalError::type_error(function, describe(other))),
    }
}

/// The codings of a `Coding` or `CodeableConcept` item.
fn codings<'c>(item: &'c Item<'_>, function: &str) -> Result<Vec<Coding<'c>>, EvalError> {
    match coded(item, function)? {
        Coded::Coding(one) => Ok(vec![one]),
        Coded::Concept(all) => Ok(all),
        Coded::Code(_) => Err(EvalError::type_error(function, describe(item))),
    }
}
