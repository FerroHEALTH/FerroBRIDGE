// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The landing page in each state of the request's session: the sign-in for
//! a visitor, and for a signed-in operator their name, what the bridge says
//! of itself read with their own token, and the sign-out.

use std::error::Error;

use ferrobridge_testkit::oidc::{Provider, TokenAnswer};
use http::StatusCode;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{
    OPERATOR_NAME, OPERATOR_TOKEN, configuration, console, get, get_with, send, sign_in,
};

#[tokio::test]
async fn a_visitor_reads_the_sign_in_as_a_link_the_client_router_leaves_alone()
-> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (response, body) = send(&service, get("/")?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(
        body.contains(r#"<a href="/login" rel="external""#) && body.contains(">Sign in</a>"),
        "{body}"
    );
    assert!(!body.contains("Sign out"), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_signed_in_operator_reads_their_name_the_bridge_and_no_token()
-> Result<(), Box<dyn Error>> {
    let bridge = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/metadata"))
        .and(header("authorization", format!("Bearer {OPERATOR_TOKEN}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"resourceType":"CapabilityStatement","status":"active","date":"2026-10-17","kind":"instance","software":{"name":"FerroBRIDGE","version":"0.0.8"},"fhirVersion":"4.0.1","format":["json"]}"#,
            "application/fhir+json",
        ))
        .mount(&bridge)
        .await;
    let provider = Provider::start().await?;
    let base = format!("{}/", bridge.uri());
    let (_state, service) = console(&configuration(&provider, &base, "", ""))?;
    let cookie = sign_in(&service, &provider, TokenAnswer::bearer(OPERATOR_TOKEN)).await?;
    let (response, body) = send(&service, get_with("/", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(body.contains(OPERATOR_NAME), "{body}");
    assert!(body.contains("Bridge UP · v0.0.8"), "{body}");
    assert!(body.contains(">Dashboard</h1>"), "{body}");
    assert!(!body.contains(OPERATOR_TOKEN), "{body}");
    assert!(!body.contains(r#"href="/login""#), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_bridge_out_of_reach_is_said_on_the_page_never_left_blank() -> Result<(), Box<dyn Error>>
{
    let provider = Provider::start().await?;
    let (_state, service) = console(&configuration(&provider, "http://127.0.0.1:9/", "", ""))?;
    let cookie = sign_in(&service, &provider, TokenAnswer::bearer(OPERATOR_TOKEN)).await?;
    let (response, body) = send(&service, get_with("/", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(body.contains("the bridge could not be reached"), "{body}");
    Ok(())
}
