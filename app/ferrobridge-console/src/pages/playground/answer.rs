// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the bridge answers a run of the playground.
//!
//! The input is checked, then `$tofhir`, `$toopenehr` or `$validate` is
//! sent. The server runs it for the playground's post and its server
//! function alike.

use fhir_types::r4::operation_outcome::OperationOutcome;
use fhir_types::r4::parameters::{Parameters, ParametersParameter, ParametersParameterValue};
use fhir_types::r4::primitives;
use fhir_types::r4::reference::Reference;

use crate::actions::{PlaygroundAnswer, PlaygroundPost, RunAnswer};
use crate::app::PLAYGROUND;
use crate::bridge::AccessToken;
use crate::locale::messages::Msg;
use crate::model::loaded;
use crate::server::ConsoleState;

/// The `$toopenehr` formats the REST API chapter defines.
const FORMATS: [&str; 2] = ["canonical", "flat"];

/// The tab an address names, `tofhir` for the playground itself.
fn tab_of(address: &str) -> &str {
    address
        .strip_prefix(PLAYGROUND)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or("tofhir")
}

/// A parameter named `name` carrying `value`.
fn parameter(name: &str, value: ParametersParameterValue) -> ParametersParameter {
    ParametersParameter {
        name: primitives::String::from(name),
        value: Some(value),
        ..ParametersParameter::default()
    }
}

/// What the bridge answers the post `post` of the playground at `address`,
/// as the operator whose access token is `token`.
pub async fn answer(
    state: &ConsoleState,
    token: &AccessToken,
    address: &str,
    post: &PlaygroundPost,
) -> PlaygroundAnswer {
    let input = post.input.trim();
    match tab_of(address) {
        "toopenehr" => to_openehr_answer(state, token, input, post).await,
        "validate" => validate_answer(state, token, input, post).await,
        _ => to_fhir_answer(state, token, input, post).await,
    }
}

/// Runs `$tofhir` over the composition `input`.
async fn to_fhir_answer(
    state: &ConsoleState,
    token: &AccessToken,
    input: &str,
    post: &PlaygroundPost,
) -> PlaygroundAnswer {
    if input.is_empty() {
        return PlaygroundAnswer::Invalid(Msg::CompositionEmpty);
    }
    let mut parameters = vec![parameter(
        "composition",
        ParametersParameterValue::String(primitives::String::from(input)),
    )];
    let template = post.template.trim();
    if !template.is_empty() {
        parameters.push(parameter(
            "templateId",
            ParametersParameterValue::String(primitives::String::from(template)),
        ));
    }
    let patient = post.patient.trim();
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
    PlaygroundAnswer::Answered(
        loaded(state.bridge().to_fhir(&request, token).await)
            .map(|outcome| outcome.map(RunAnswer::from)),
    )
}

/// Runs `$toopenehr` over the Bundle `input`.
async fn to_openehr_answer(
    state: &ConsoleState,
    token: &AccessToken,
    input: &str,
    post: &PlaygroundPost,
) -> PlaygroundAnswer {
    let format = if post.format.is_empty() {
        "canonical"
    } else {
        post.format.as_str()
    };
    if !FORMATS.contains(&format) {
        return PlaygroundAnswer::Invalid(Msg::FormatNotKnown);
    }
    // NOTE: no specification governs this: our own design; text that is no
    // JSON is the operator's input to correct, never sent.
    let Ok(bundle) = serde_json::from_str::<serde_json::Value>(input) else {
        return PlaygroundAnswer::Invalid(Msg::BundleNotJson);
    };
    if bundle
        .get("resourceType")
        .and_then(serde_json::Value::as_str)
        != Some("Bundle")
    {
        return PlaygroundAnswer::Invalid(Msg::BundleNotBundle);
    }
    let mut parameter = vec![serde_json::json!({ "name": "bundle", "resource": bundle })];
    let template = post.template.trim();
    if !template.is_empty() {
        parameter.push(serde_json::json!({ "name": "templateId", "valueString": template }));
    }
    parameter.push(serde_json::json!({ "name": "format", "valueCode": format }));
    // NOTE: FHIRconnect `ToOpenEhr.fsh` declares `bundle` a resource part; the
    // console's model holds no clinical resource, so the Bundle passes as JSON.
    let envelope = serde_json::json!({ "resourceType": "Parameters", "parameter": parameter });
    let answer = loaded(state.bridge().to_openehr(&envelope, token).await)
        .map(|outcome| outcome.map(|(parameters, raw)| converted(&parameters, raw)));
    PlaygroundAnswer::Answered(answer)
}

/// Runs `$validate` on the resource `input`.
async fn validate_answer(
    state: &ConsoleState,
    token: &AccessToken,
    input: &str,
    post: &PlaygroundPost,
) -> PlaygroundAnswer {
    // NOTE: no specification governs this: our own design; text that is no
    // JSON is the operator's input to correct, never sent.
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(input) else {
        return PlaygroundAnswer::Invalid(Msg::ResourceNotJson);
    };
    if parsed
        .get("resourceType")
        .and_then(serde_json::Value::as_str)
        != Some(post.resource_type.as_str())
    {
        return PlaygroundAnswer::Invalid(Msg::ResourceTypeMismatch);
    }
    let answer = loaded(
        state
            .bridge()
            .validate(&post.resource_type, input.to_owned(), token)
            .await,
    )
    .map(|outcome| outcome.map(|(_outcome, raw)| RunAnswer::from(raw)));
    PlaygroundAnswer::Answered(answer)
}

/// What a `$toopenehr` answer shows: the composition its `composition`
/// parameter carries, with the issues of its `outcome` beside it, or the
/// whole answer when it carries no composition (FHIRconnect
/// `ToOpenEhr.fsh`).
fn converted(parameters: &Parameters, raw: serde_json::Value) -> RunAnswer {
    let named = |name: &str| {
        parameters
            .parameter
            .iter()
            .find(|parameter| parameter.name.value.as_deref() == Some(name))
    };
    let composition = named("composition").and_then(|parameter| match &parameter.value {
        Some(ParametersParameterValue::String(text)) => text.value.as_deref(),
        _ => None,
    });
    let issues = named("outcome")
        .and_then(|parameter| match &parameter.resource {
            Some(fhir_types::r4::resource::Resource::OperationOutcome(outcome)) => {
                Some(issue_words(outcome))
            }
            _ => None,
        })
        .unwrap_or_default();
    let Some(text) = composition else {
        return RunAnswer::from(raw);
    };
    // NOTE: FHIRconnect `ToOpenEhr.fsh`: the composition is serialized as a string;
    // one that is no JSON is shown as the text it is.
    let document =
        serde_json::from_str(text).unwrap_or_else(|_| serde_json::Value::String(text.to_owned()));
    RunAnswer { document, issues }
}

/// The words of each issue of `outcome`: its `details.text` and its
/// `diagnostics`, each when present (FHIR R4 `OperationOutcome.issue`).
fn issue_words(outcome: &OperationOutcome) -> Vec<String> {
    outcome
        .issue
        .iter()
        .flat_map(|issue| {
            let details = issue
                .details
                .as_ref()
                .and_then(|details| details.text.as_ref())
                .and_then(|text| text.value.clone());
            let diagnostics = issue
                .diagnostics
                .as_ref()
                .and_then(|text| text.value.clone());
            details.into_iter().chain(diagnostics)
        })
        .collect()
}
