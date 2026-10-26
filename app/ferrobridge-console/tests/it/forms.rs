// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every form post and every server function the console takes, held to
//! one rule: it acts only for a live session from the console's own pages,
//! and refuses anything else before it asks the bridge or sets a cookie.
//! Every action is a post a page sends with no WebAssembly; once the page
//! has hydrated, the same action goes through its server function, and
//! every read through one of the reads' server functions.

use std::collections::BTreeSet;
use std::error::Error;

use axum::body::Body;
use http::{Method, Request, StatusCode};

use crate::stub::{CONTEXT, bridge, signed_in};
use crate::support::{OPERATOR_TOKEN, ORIGIN, send};

/// Every form post the console takes, each with a body it accepts.
fn posts() -> Vec<(String, &'static str)> {
    vec![
        (String::from("/mappings"), ""),
        (
            format!("/mappings/edit/{CONTEXT}"),
            "draft=x&base=&path=&reason=r&decision=check",
        ),
        (
            String::from("/mappings/new"),
            "draft=x&base=&path=fhirconnect%2Fa.yml&reason=r&decision=check",
        ),
        (String::from("/playground"), "input=x&template=&patient="),
        (
            String::from("/playground/toopenehr"),
            "input=x&template=&format=canonical",
        ),
        (
            String::from("/playground/validate"),
            "resource_type=Patient&input=x",
        ),
        (String::from("/preference"), "back=%2F&theme=dark"),
        (String::from("/preferences/reset"), "back=%2F"),
        (String::from("/sidebar"), "sidebar=rail&back=%2F"),
    ]
}

/// A post of `form` to `route` with `site` as its fetch metadata, carrying
/// `cookie` when given.
fn posted(
    route: &str,
    form: &str,
    site: &str,
    cookie: Option<&str>,
) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post(route)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("sec-fetch-site", site)
        .header("origin", ORIGIN);
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    Ok(request.body(Body::from(form.to_owned()))?)
}

/// Every server function the console registers, each with its content
/// type and a body it accepts.
fn server_functions() -> Vec<(&'static str, &'static str, String)> {
    let form = "application/x-www-form-urlencoded";
    let json = "application/json";
    vec![
        ("/api/capability", form, String::new()),
        ("/api/configuration", form, String::new()),
        (
            "/api/edit",
            json,
            format!(
                r#"{{"address":"/mappings/edit/{CONTEXT}","post":{{"draft":"x","decision":"check"}}}}"#
            ),
        ),
        ("/api/file", form, format!("path={CONTEXT}")),
        ("/api/info", form, String::new()),
        ("/api/listing", form, String::new()),
        ("/api/readiness", form, String::new()),
        ("/api/reload", form, String::new()),
        (
            "/api/run",
            json,
            String::from(r#"{"address":"/playground","post":{"input":"x"}}"#),
        ),
        ("/api/smart", form, String::new()),
        ("/api/state", form, String::new()),
        ("/api/visit", form, String::new()),
    ]
}

/// A call of the server function at `path` with `body` of `kind`, with
/// `site` as its fetch metadata, carrying `cookie` when given.
fn called(
    path: &str,
    kind: &str,
    body: &str,
    site: &str,
    cookie: Option<&str>,
) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post(path)
        .header("content-type", kind)
        .header("accept", "application/json")
        .header("sec-fetch-site", site)
        .header("origin", ORIGIN);
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    Ok(request.body(Body::from(body.to_owned()))?)
}

#[test]
fn the_console_registers_its_server_functions_alone() {
    let registered: BTreeSet<String> = leptos::server_fn::axum::server_fn_paths()
        .map(|(path, method)| {
            assert_eq!(Method::POST, method, "{path} is called by POST");
            path.to_owned()
        })
        .collect();
    let expected: BTreeSet<String> = server_functions()
        .into_iter()
        .map(|(path, _, _)| path.to_owned())
        .collect();
    assert_eq!(expected, registered);
}

#[tokio::test]
async fn every_server_function_refuses_a_caller_without_a_live_session_before_the_bridge()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, _cookie) = signed_in(&bridge).await?;
    let before = bridge.received_requests().await.ok_or("recorded")?.len();
    for (path, kind, body) in server_functions() {
        if path == "/api/visit" {
            continue;
        }
        for cookie in [None, Some("ferrobridge_console_session=forged")] {
            let (response, answer) =
                send(&service, called(path, kind, &body, "same-origin", cookie)?).await?;
            assert_ne!(StatusCode::OK, response.status(), "{path} {cookie:?}");
            assert!(answer.contains("SignedOut"), "{path} {cookie:?}: {answer}");
            assert!(
                response.headers().get("set-cookie").is_none(),
                "{path} {cookie:?} set a cookie"
            );
        }
    }
    let after = bridge.received_requests().await.ok_or("recorded")?.len();
    assert_eq!(before, after, "a call without a session asked the bridge");
    Ok(())
}

#[tokio::test]
async fn the_visit_of_a_caller_without_a_session_says_signed_out() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, _cookie) = signed_in(&bridge).await?;
    let before = bridge.received_requests().await.ok_or("recorded")?.len();
    let form = "application/x-www-form-urlencoded";
    let (response, answer) = send(
        &service,
        called("/api/visit", form, "", "same-origin", None)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(answer.contains(r#""session":"SignedOut""#), "{answer}");
    assert!(answer.contains(r#""operator":null"#), "{answer}");
    assert!(answer.contains(r#""settings":null"#), "{answer}");
    let after = bridge.received_requests().await.ok_or("recorded")?.len();
    assert_eq!(before, after, "the visit asked the bridge");
    Ok(())
}

#[tokio::test]
async fn the_visit_carries_no_session_cookie_and_no_token() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let form = "application/x-www-form-urlencoded";
    let (response, answer) = send(
        &service,
        called("/api/visit", form, "", "same-origin", Some(&cookie))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(answer.contains(r#""session":"SignedIn""#), "{answer}");
    let session = cookie.split_once('=').map_or("", |(_, value)| value);
    assert!(!session.is_empty() && !answer.contains(session), "{answer}");
    assert!(!answer.contains(OPERATOR_TOKEN), "{answer}");
    Ok(())
}

#[tokio::test]
async fn every_server_function_refuses_a_request_from_another_origin() -> Result<(), Box<dyn Error>>
{
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let before = bridge.received_requests().await.ok_or("recorded")?.len();
    for (path, kind, body) in server_functions() {
        let (response, answer) = send(
            &service,
            called(path, kind, &body, "cross-site", Some(&cookie))?,
        )
        .await?;
        assert_eq!(StatusCode::FORBIDDEN, response.status(), "{path}");
        assert!(
            answer.contains("the console takes this request only from its own pages"),
            "{path}: {answer}"
        );
    }
    let after = bridge.received_requests().await.ok_or("recorded")?.len();
    assert_eq!(before, after, "a foreign call asked the bridge");
    Ok(())
}

#[tokio::test]
async fn every_post_refuses_a_caller_without_a_live_session_before_the_bridge()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, _cookie) = signed_in(&bridge).await?;
    let before = bridge.received_requests().await.ok_or("recorded")?.len();
    for (route, form) in posts() {
        for cookie in [None, Some("ferrobridge_console_session=forged")] {
            let (response, _body) =
                send(&service, posted(&route, form, "same-origin", cookie)?).await?;
            assert_ne!(StatusCode::OK, response.status(), "{route} {cookie:?}");
            assert!(
                response.headers().get("set-cookie").is_none(),
                "{route} {cookie:?} set a cookie"
            );
        }
    }
    let after = bridge.received_requests().await.ok_or("recorded")?.len();
    assert_eq!(before, after, "a post without a session asked the bridge");
    Ok(())
}

#[tokio::test]
async fn every_post_refuses_a_request_from_another_origin() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let before = bridge.received_requests().await.ok_or("recorded")?.len();
    for (route, form) in posts() {
        let (response, body) =
            send(&service, posted(&route, form, "cross-site", Some(&cookie))?).await?;
        assert_eq!(StatusCode::FORBIDDEN, response.status(), "{route}");
        assert!(
            body.contains("the console takes this request only from its own pages"),
            "{route}: {body}"
        );
    }
    let after = bridge.received_requests().await.ok_or("recorded")?.len();
    assert_eq!(before, after, "a foreign post asked the bridge");
    Ok(())
}
