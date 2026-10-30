// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every page of the console in each state of the request's session: the
//! sign-in for a visitor, and for a signed-in operator the page drawn whole
//! on the server in the shell, its title in the document, never a token,
//! and the script modules that add behaviour to what the server drew.

use std::error::Error;

use http::StatusCode;

use crate::stub::{CONTEXT, bridge, signed_in};
use crate::support::{OPERATOR_NAME, OPERATOR_TOKEN, console, drawn, get, get_with, send};

/// Every route of the console, each with words its page draws.
const PAGES: [(&str, &str); 29] = [
    ("/", ">Dashboard</h1>"),
    ("/mappings", ">Mapping set</h1>"),
    ("/mappings/omocl", "measurement.yml"),
    ("/mappings/templates", "synthetic.vital_signs.v1"),
    ("/mappings/files", "Path or file name"),
    ("/mappings/diagnostics", "missing-key"),
    (
        "/mappings/files/fhirconnect/contexts/observation.context.yml",
        "synthetic-observation",
    ),
    (
        "/mappings/templates/synthetic.vital_signs.v1",
        "Contexts that name it",
    ),
    (
        "/mappings/edit/fhirconnect/contexts/observation.context.yml",
        ">Edit</h1>",
    ),
    ("/mappings/new", ">New mapping file</h1>"),
    ("/playground", "Convert to FHIR"),
    ("/playground/toopenehr", "Convert to openEHR"),
    ("/playground/validate", "Validate a resource"),
    ("/facade", ">FHIR facade</h1>"),
    ("/facade/capability", "From the CapabilityStatement"),
    ("/facade/security", "permission-v2"),
    ("/facade/settings", "facade.base_url"),
    ("/hl7v2", "0.0.0.0:2575"),
    ("/hl7v2/settings", "hl7v2.listen"),
    ("/omop", "ferrobridge etl run"),
    ("/omop/settings", "etl.page_size"),
    ("/connections", ">Connections</h1>"),
    ("/connections/terminology", "answers 503"),
    ("/connections/cdr/settings", "cdr.base_url"),
    ("/configuration", ">Configuration</h1>"),
    ("/configuration/mappings", "mappings.directory"),
    ("/configuration/core", "server.listen"),
    ("/preferences", ">Your preferences</h1>"),
    ("/about", "0123456789ab"),
];

#[tokio::test]
async fn a_visitor_reads_the_sign_in_on_every_page() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    for (route, _) in PAGES {
        let (response, body) = send(&service, get(route)?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{route}");
        assert!(body.contains("Sign in to the console"), "{route}: {body}");
        assert!(!body.contains(">Dashboard</h1>"), "{route}");
    }
    Ok(())
}

#[tokio::test]
async fn every_page_is_drawn_whole_for_a_signed_in_operator() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for (route, words) in PAGES {
        let (response, body) = send(&service, get_with(route, &cookie)?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{route}: {body}");
        assert!(body.contains(words), "{route} draws {words}: {body}");
        assert!(body.contains(OPERATOR_NAME), "{route}");
        assert!(body.contains("Bridge up"), "{route}");
        assert!(!body.contains(OPERATOR_TOKEN), "{route}");
        assert!(body.contains("<title>"), "{route}");
    }
    Ok(())
}

#[tokio::test]
async fn the_page_is_drawn_whole_and_its_behaviour_loads_as_modules() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, body) = send(
        &service,
        get_with(&format!("/mappings/edit/{CONTEXT}"), &cookie)?,
    )
    .await?;
    assert!(
        body.contains(r#"<script type="module" src="/ferrohealth-ui/"#),
        "the one loader of the library's modules: {body}"
    );
    assert_eq!(
        1,
        body.matches("<script").count(),
        "no other script: {body}"
    );
    assert!(!body.contains("hydrate"), "nothing hydrates: {body}");
    assert!(!body.contains("<leptos-island"), "no island: {body}");
    assert!(
        drawn(&body).contains(r#"name="draft""#),
        "the editor is drawn in the page: {body}"
    );
    assert!(
        drawn(&body).contains(r#"data-fui="editor""#),
        "the editor's module is named on what the server drew: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_bridge_out_of_reach_is_said_on_the_page_never_left_blank() -> Result<(), Box<dyn Error>>
{
    let provider = ferrobridge_testkit::oidc::Provider::start().await?;
    let (_state, service) = console(&crate::support::configuration(
        &provider,
        "http://127.0.0.1:9/",
        "",
        "",
    ))?;
    let cookie = crate::support::sign_in(
        &service,
        &provider,
        ferrobridge_testkit::oidc::TokenAnswer::bearer(OPERATOR_TOKEN),
    )
    .await?;
    let (response, body) = send(&service, get_with("/", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        body.contains("The bridge could not be reached, so this page cannot be shown."),
        "{body}"
    );
    assert!(body.contains("Bridge unreachable"), "{body}");
    Ok(())
}
