// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server half of the tools: the session and plain-post checks, the
//! reading of what the operator entered, the bridge call, and the answer
//! drawn as HTML.
//!
//! Nothing here keeps what the operator entered or what the bridge
//! answered, and no log line carries either: a refusal is logged with its
//! status alone.

use fhir_types::r4::parameters::{Parameters, ParametersParameter, ParametersParameterValue};
use fhir_types::r4::primitives;
use fhir_types::r4::reference::Reference;
use leptos::context::use_context;

use crate::model::ViewError;
use crate::tools::render;

/// The `$toopenehr` formats the REST API chapter defines.
const FORMATS: [&str; 2] = ["canonical", "flat"];

/// The console's state and the operator's token, for a request with a live
/// session that a page of the console sent through its bundle.
///
/// A plain form post, which a page sends before its bundle has loaded,
/// asks for HTML; the tools answer only the bundle's call, so it is refused
/// before the bridge is asked anything.
pub(crate) fn admitted()
-> Result<(crate::server::ViewerState, crate::bridge::AccessToken), ViewError> {
    let signed_in = crate::session::request::signed_in()?;
    let plain = use_context::<http::request::Parts>().is_some_and(|parts| {
        parts
            .headers
            .get(http::header::ACCEPT)
            .and_then(|accept| accept.to_str().ok())
            .is_some_and(|accept| accept.contains("text/html"))
    });
    if plain {
        return Err(ViewError::PlainPost);
    }
    Ok(signed_in)
}

/// A parameter named `name` carrying `value`.
fn parameter(name: &str, value: ParametersParameterValue) -> ParametersParameter {
    ParametersParameter {
        name: primitives::String::from(name),
        value: Some(value),
        ..ParametersParameter::default()
    }
}

/// Runs `$tofhir` over what the operator entered.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session and
/// [`ViewError::PlainPost`] for a plain form post.
pub async fn to_fhir(
    composition: &str,
    template_id: &str,
    patient: &str,
) -> Result<String, ViewError> {
    let (state, token) = admitted()?;
    let composition = composition.trim();
    if composition.is_empty() {
        return Ok(render::invalid(
            "$tofhir",
            "The composition is empty: enter a canonical or FLAT composition.",
        ));
    }
    let mut parameters = vec![parameter(
        "composition",
        ParametersParameterValue::String(primitives::String::from(composition)),
    )];
    let template_id = template_id.trim();
    if !template_id.is_empty() {
        parameters.push(parameter(
            "templateId",
            ParametersParameterValue::String(primitives::String::from(template_id)),
        ));
    }
    let patient = patient.trim();
    if !patient.is_empty() {
        // NOTE: FHIRconnect REST API chapter, `ToFhir.fsh`: the patient
        // travels as `context.patient`, a `Reference`, in the request body.
        let reference = Reference {
            reference: Some(primitives::String::from(patient)),
            ..Reference::default()
        };
        parameters.push(ParametersParameter {
            name: primitives::String::from("context"),
            part: vec![parameter(
                "patient",
                ParametersParameterValue::Reference(Box::new(reference)),
            )],
            ..ParametersParameter::default()
        });
    }
    let request = Parameters {
        parameter: parameters,
        ..Parameters::default()
    };
    let answer = state.bridge().to_fhir(&request, &token).await;
    Ok(render::bundle(&answer))
}

/// Runs `$toopenehr` over what the operator entered.
///
/// # Errors
/// As [`to_fhir`].
pub async fn to_openehr(
    bundle: &str,
    template_id: &str,
    format: &str,
) -> Result<String, ViewError> {
    let (state, token) = admitted()?;
    if !FORMATS.contains(&format) {
        return Ok(render::invalid(
            "$toopenehr",
            "The format is not canonical or flat.",
        ));
    }
    // NOTE: no specification governs this: our own design; text that is no
    // JSON is the operator's input to correct, never sent.
    let Ok(bundle) = serde_json::from_str::<serde_json::Value>(bundle) else {
        return Ok(render::invalid("$toopenehr", "The Bundle is not JSON."));
    };
    if bundle
        .get("resourceType")
        .and_then(serde_json::Value::as_str)
        != Some("Bundle")
    {
        return Ok(render::invalid(
            "$toopenehr",
            "The Bundle is not a resource whose resourceType is Bundle.",
        ));
    }
    let mut parameter = vec![serde_json::json!({ "name": "bundle", "resource": bundle })];
    let template_id = template_id.trim();
    if !template_id.is_empty() {
        parameter.push(serde_json::json!({ "name": "templateId", "valueString": template_id }));
    }
    parameter.push(serde_json::json!({ "name": "format", "valueCode": format }));
    // NOTE: FHIRconnect `ToOpenEhr.fsh` declares `bundle` a resource part; the
    // console's model holds no clinical resource, so the Bundle passes as JSON.
    let envelope = serde_json::json!({ "resourceType": "Parameters", "parameter": parameter });
    let answer = state.bridge().to_openehr(&envelope, &token).await;
    Ok(render::composition(&answer))
}

/// Runs `$validate` on `resource_type` over the resource the operator
/// entered.
///
/// # Errors
/// As [`to_fhir`].
pub async fn validate(resource_type: &str, resource: String) -> Result<String, ViewError> {
    let (state, token) = admitted()?;
    // NOTE: no specification governs this: our own design; text that is no
    // JSON is the operator's input to correct, never sent.
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&resource) else {
        return Ok(render::invalid("$validate", "The resource is not JSON."));
    };
    if parsed
        .get("resourceType")
        .and_then(serde_json::Value::as_str)
        != Some(resource_type)
    {
        return Ok(render::invalid(
            "$validate",
            "The resource's resourceType is not the type chosen.",
        ));
    }
    let answer = state
        .bridge()
        .validate(resource_type, resource, &token)
        .await;
    Ok(render::outcome(&answer))
}

/// The resource types the bridge's `CapabilityStatement` declares
/// `$validate` on, in its order.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session.
pub async fn validate_types() -> Result<crate::model::Outcome<Vec<String>>, ViewError> {
    let (state, token) = crate::session::request::signed_in()?;
    match state.bridge().capability_statement(&token).await {
        Ok(statement) => Ok(crate::model::Outcome::Shown(
            statement
                .rest
                .iter()
                .flat_map(|rest| rest.resource.iter())
                .filter(|resource| {
                    resource
                        .operation
                        .iter()
                        .any(|operation| operation.name.value.as_deref() == Some("validate"))
                })
                .filter_map(|resource| resource.r#type.value.clone())
                .collect(),
        )),
        Err(error) => crate::model::server::refusal(&error).map(crate::model::Outcome::Refused),
    }
}
