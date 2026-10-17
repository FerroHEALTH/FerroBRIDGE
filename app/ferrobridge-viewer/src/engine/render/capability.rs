// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The capability screen, drawn on the server.
//!
//! It shows the bridge's R4 `CapabilityStatement` (FHIR R4
//! `capabilitystatement.html`) as cards for
//! its resources with their interactions, search parameters and operations,
//! the system-level interactions and operations, and the security, the
//! statement's JSON behind a disclosure, and the SMART discovery document
//! (SMART App Launch 2.2.0, conformance).

use fhir_types::r4::capability_statement::{
    CapabilityStatement, CapabilityStatementRest, CapabilityStatementRestResource,
};
use leptos::prelude::*;

use crate::bridge::BridgeError;
use crate::engine::render::parts::{self, Shape};

/// The capability screen for the two answers.
#[must_use]
pub fn render(
    statement: &Result<(CapabilityStatement, serde_json::Value), BridgeError>,
    smart: &Result<serde_json::Value, BridgeError>,
) -> String {
    let capability = match statement {
        Ok((statement, raw)) => cards(statement, raw),
        Err(error) => parts::failed("CapabilityStatement", error),
    };
    let discovery = match smart {
        Ok(document) => {
            discovery(document).unwrap_or_else(|shape| parts::unreadable("SMART discovery", &shape))
        }
        Err(error) => parts::failed("SMART discovery", error),
    };
    view! { <div class="sections">{capability} {discovery}</div> }.to_html()
}

/// The text a primitive holds, or an empty string.
fn value(primitive: Option<&String>) -> String {
    primitive.cloned().unwrap_or_default()
}

/// The statement's summary, its `rest` blocks and its JSON.
fn cards(statement: &CapabilityStatement, raw: &serde_json::Value) -> AnyView {
    let software = statement.software.as_ref();
    let name = value(software.and_then(|software| software.name.value.as_ref()));
    let version = value(
        software
            .and_then(|software| software.version.as_ref())
            .and_then(|version| version.value.as_ref()),
    );
    let formats = statement
        .format
        .iter()
        .filter_map(|format| format.value.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let rests = statement.rest.iter().map(rest).collect_view();
    view! {
        <section class="card">
            <h2>"CapabilityStatement"</h2>
            <dl>
                <dt>"Software"</dt>
                <dd>{format!("{name} {version}")}</dd>
                <dt>"FHIR version"</dt>
                <dd>{value(statement.fhir_version.value.as_ref())}</dd>
                <dt>"Kind"</dt>
                <dd>{value(statement.kind.value.as_ref())}</dd>
                <dt>"Status"</dt>
                <dd>{value(statement.status.value.as_ref())}</dd>
                <dt>"Formats"</dt>
                <dd>{formats}</dd>
            </dl>
        </section>
        {rests}
        {parts::raw_json("The CapabilityStatement as JSON", raw)}
    }
    .into_any()
}

/// One `rest` block: its security, its system-level interactions and
/// operations, and a card per resource.
fn rest(rest: &CapabilityStatementRest) -> AnyView {
    let mode = value(rest.mode.value.as_ref());
    let security = match &rest.security {
        Some(security) => {
            let services = security
                .service
                .iter()
                .flat_map(|service| {
                    service.coding.iter().filter_map(|coding| {
                        coding
                            .display
                            .as_ref()
                            .and_then(|display| display.value.clone())
                            .or_else(|| coding.code.as_ref().and_then(|code| code.value.clone()))
                    })
                })
                .collect::<Vec<_>>()
                .join(", ");
            let cors = security
                .cors
                .as_ref()
                .and_then(|cors| cors.value)
                .map_or("not stated", |cors| if cors { "yes" } else { "no" });
            let description = value(
                security
                    .description
                    .as_ref()
                    .and_then(|description| description.value.as_ref()),
            );
            view! {
                <dl>
                    <dt>"Services"</dt>
                    <dd>{services}</dd>
                    <dt>"CORS"</dt>
                    <dd>{cors}</dd>
                    <dt>"Description"</dt>
                    <dd>{description}</dd>
                </dl>
            }
            .into_any()
        }
        None => view! { <p>"The statement declares no security."</p> }.into_any(),
    };
    let interactions = rest
        .interaction
        .iter()
        .filter_map(|interaction| interaction.code.value.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let operations = rest
        .operation
        .iter()
        .map(|operation| {
            view! {
                <li>
                    <code>{format!("${}", value(operation.name.value.as_ref()))}</code>
                    " "
                    {value(operation.definition.value.as_ref())}
                </li>
            }
        })
        .collect_view();
    let resources = rest.resource.iter().map(resource).collect_view();
    view! {
        <section class="card">
            <h2>{format!("REST, {mode} mode: security")}</h2>
            {security}
        </section>
        <section class="card">
            <h2>{format!("REST, {mode} mode: system")}</h2>
            <dl>
                <dt>"Interactions"</dt>
                <dd>{interactions}</dd>
            </dl>
            <ul>{operations}</ul>
        </section>
        {resources}
    }
    .into_any()
}

/// The card of one resource type.
fn resource(resource: &CapabilityStatementRestResource) -> AnyView {
    let kind = value(resource.r#type.value.as_ref());
    let profile = value(
        resource
            .profile
            .as_ref()
            .and_then(|profile| profile.value.as_ref()),
    );
    let interactions = resource
        .interaction
        .iter()
        .filter_map(|interaction| interaction.code.value.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let search = resource
        .search_param
        .iter()
        .map(|parameter| {
            view! {
                <tr>
                    <th scope="row">
                        <code>{value(parameter.name.value.as_ref())}</code>
                    </th>
                    <td>{value(parameter.r#type.value.as_ref())}</td>
                </tr>
            }
        })
        .collect_view();
    let operations = resource
        .operation
        .iter()
        .map(|operation| {
            view! {
                <li>
                    <code>{format!("${}", value(operation.name.value.as_ref()))}</code>
                    " "
                    {value(operation.definition.value.as_ref())}
                </li>
            }
        })
        .collect_view();
    let searches = (!resource.search_param.is_empty()).then(|| {
        view! {
            <table>
                <thead>
                    <tr>
                        <th scope="col">"Search parameter"</th>
                        <th scope="col">"Type"</th>
                    </tr>
                </thead>
                <tbody>{search}</tbody>
            </table>
        }
    });
    view! {
        <section class="card">
            <h2>{kind}</h2>
            <dl>
                <dt>"Profile"</dt>
                <dd>{profile}</dd>
                <dt>"Interactions"</dt>
                <dd>{interactions}</dd>
            </dl>
            {searches}
            <ul>{operations}</ul>
        </section>
    }
    .into_any()
}

/// The SMART discovery document: its endpoints, capabilities and scopes,
/// and its JSON.
fn discovery(document: &serde_json::Value) -> Result<AnyView, Shape> {
    let issuer = parts::optional_text(document, "issuer")?
        .unwrap_or("not stated")
        .to_owned();
    let authorize = parts::text(document, "authorization_endpoint")?.to_owned();
    let token = parts::text(document, "token_endpoint")?.to_owned();
    let words = |key: &'static str| -> Result<String, Shape> {
        match document.get(key) {
            None => Ok(String::from("not stated")),
            Some(serde_json::Value::Array(items)) => items
                .iter()
                .map(|item| item.as_str().ok_or(Shape { path: key }))
                .collect::<Result<Vec<_>, Shape>>()
                .map(|items| items.join(", ")),
            Some(_) => Err(Shape { path: key }),
        }
    };
    let capabilities = words("capabilities")?;
    let scopes = words("scopes_supported")?;
    Ok(view! {
        <section class="card">
            <h2>"SMART discovery"</h2>
            <dl>
                <dt>"Issuer"</dt>
                <dd>
                    <code>{issuer}</code>
                </dd>
                <dt>"Authorization endpoint"</dt>
                <dd>
                    <code>{authorize}</code>
                </dd>
                <dt>"Token endpoint"</dt>
                <dd>
                    <code>{token}</code>
                </dd>
                <dt>"Capabilities"</dt>
                <dd>{capabilities}</dd>
                <dt>"Scopes"</dt>
                <dd>{scopes}</dd>
            </dl>
            {parts::raw_json("The discovery document as JSON", document)}
        </section>
    }
    .into_any())
}
