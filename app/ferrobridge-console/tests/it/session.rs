// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The session route the page's session watch asks, on the wire of FerroUI
//! `docs/consoles.md`, "The session's end": a `GET` reads what the session
//! has left and never extends it, a `POST` extends it, and `401` says it is
//! gone. A signed-in page carries the watch, the signed-out page the mark
//! that ends every other tab, and the sign-in the watch leads to keeps the
//! page's address as `return` and comes back to it.

use std::error::Error;
use std::time::Duration;

use axum::body::Body;
use ferrobridge_testkit::oidc::{IdTokenClaims, Provider, TokenAnswer};
use ferrohealth_ui::view::session::{Left, WARNING_ID};
use http::{Request, StatusCode};

use crate::support::{
    CLIENT_ID, OPERATOR_TOKEN, ORIGIN, begin_at, complete, configuration, console, get, get_with,
    header, send, sign_in,
};

/// The bridge address of a console whose tests never reach the bridge.
const NO_BRIDGE: &str = "http://127.0.0.1:9/";

/// The session route.
const SESSION: &str = ferrohealth_session::paths::SESSION;

/// The idle timeout of these tests' sessions, in seconds.
const IDLE: u32 = 120;

/// A provider, a console whose sessions idle out after [`IDLE`] seconds,
/// and the cookie of a session signed in at it.
async fn signed_in() -> Result<(Provider, axum::Router, String), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (_state, service) = console(&configuration(
        &provider,
        NO_BRIDGE,
        "",
        &format!("idle_timeout = \"{IDLE}s\"\nabsolute_timeout = \"1h\""),
    ))
    .await?;
    let cookie = sign_in(&service, &provider, TokenAnswer::bearer(OPERATOR_TOKEN)).await?;
    Ok((provider, service, cookie))
}

/// A `POST` of the session route from the console's own origin, carrying
/// `cookie` when given.
fn extend(cookie: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post(SESSION).header("origin", ORIGIN);
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    Ok(request.body(Body::empty())?)
}

/// What a `200` answer of the session route says is left.
fn left(status: StatusCode, body: &str) -> Result<Left, Box<dyn Error>> {
    assert_eq!(StatusCode::OK, status, "{body}");
    Ok(serde_json::from_str(body)?)
}

#[tokio::test]
async fn a_read_of_the_session_route_answers_what_is_left_and_never_extends_it()
-> Result<(), Box<dyn Error>> {
    let (_provider, service, cookie) = signed_in().await?;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let (response, body) = send(&service, get_with(SESSION, &cookie)?).await?;
    assert_eq!("application/json", header(&response, "content-type"));
    assert_eq!("no-store", header(&response, "cache-control"));
    let first = left(response.status(), &body)?;
    assert!(first.idle < IDLE, "a second has passed: {first:?}");
    assert!(first.absolute < 3600, "{first:?}");
    let (response, body) = send(&service, get_with(SESSION, &cookie)?).await?;
    let second = left(response.status(), &body)?;
    assert!(
        second.idle <= first.idle,
        "a read never restarts the idle timeout: {first:?} then {second:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_post_to_the_session_route_extends_the_idle_timeout() -> Result<(), Box<dyn Error>> {
    let (_provider, service, cookie) = signed_in().await?;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let (response, body) = send(&service, extend(Some(&cookie))?).await?;
    assert_eq!("no-store", header(&response, "cache-control"));
    let extended = left(response.status(), &body)?;
    assert_eq!(IDLE, extended.idle, "the idle timeout starts again");
    assert!(extended.absolute < 3600, "the absolute one never moves");
    Ok(())
}

#[tokio::test]
async fn the_session_route_answers_401_with_no_session_or_an_ended_one()
-> Result<(), Box<dyn Error>> {
    let (_provider, service, _cookie) = signed_in().await?;
    for request in [
        get(SESSION)?,
        extend(None)?,
        get_with(SESSION, "ferrobridge_console_session=not-a-session")?,
        extend(Some("ferrobridge_console_session=not-a-session"))?,
    ] {
        let method = request.method().clone();
        let (response, body) = send(&service, request).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, response.status(), "{method}");
        assert_eq!("no-store", header(&response, "cache-control"));
        // NOTE: RFC 9457 §3: the 401 is a problem document, its detail in the console's words.
        assert_eq!(
            "application/problem+json",
            header(&response, "content-type")
        );
        let problem: serde_json::Value = serde_json::from_str(&body)?;
        assert_eq!(
            Some("Your session ended. Sign in again to go on."),
            problem.get("detail").and_then(serde_json::Value::as_str),
            "{method}: {body}"
        );
    }
    Ok(())
}

// The W3C Fetch Metadata Request Headers and RFC 6454 §7: an extension from
// another site is refused before the session is read.
#[tokio::test]
async fn a_cross_site_extension_is_refused() -> Result<(), Box<dyn Error>> {
    let (_provider, service, cookie) = signed_in().await?;
    let request = Request::post(SESSION)
        .header("cookie", &cookie)
        .header("sec-fetch-site", "cross-site")
        .body(Body::empty())?;
    let (response, _body) = send(&service, request).await?;
    assert_eq!(StatusCode::FORBIDDEN, response.status());
    Ok(())
}

/// Signs in at `service` with a `GET` of `login` and answers where the
/// completed sign-in leads.
async fn led_to(
    service: &axum::Router,
    provider: &Provider,
    login: &str,
) -> Result<String, Box<dyn Error>> {
    let begun = begin_at(service, login).await?;
    let claims = IdTokenClaims::new(&provider.issuer(), CLIENT_ID, begun.pair("nonce"));
    let answer = TokenAnswer {
        id_token: Some(provider.id_token(&claims)?),
        ..TokenAnswer::bearer(OPERATOR_TOKEN)
    };
    provider.reset().await?;
    provider
        .answer_grant("grant_type=authorization_code", &answer)
        .await?;
    let (response, body) = complete(service, &begun).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status(), "{body}");
    Ok(header(&response, "location").to_owned())
}

#[tokio::test]
async fn a_sign_in_begun_with_a_return_leads_back_to_that_page() -> Result<(), Box<dyn Error>> {
    let (provider, service, _cookie) = signed_in().await?;
    assert_eq!(
        "/mappings/files?page=2",
        led_to(
            &service,
            &provider,
            "/login?return=/mappings/files%3Fpage%3D2"
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn a_return_that_leaves_the_console_leads_to_the_dashboard() -> Result<(), Box<dyn Error>> {
    let (provider, service, _cookie) = signed_in().await?;
    for foreign in [
        "https://attacker.example.net/",
        "//attacker.example.net/",
        "/\\attacker.example.net/",
        "/signed-out",
        "/logout",
        "mappings",
    ] {
        let login = format!(
            "/login?return={}",
            url::form_urlencoded::byte_serialize(foreign.as_bytes()).collect::<String>()
        );
        assert_eq!("/", led_to(&service, &provider, &login).await?, "{foreign}");
    }
    Ok(())
}

#[tokio::test]
async fn the_sign_in_page_carries_the_page_asked_for_to_the_providers_button()
-> Result<(), Box<dyn Error>> {
    let (_provider, service, _cookie) = signed_in().await?;
    let (_response, body) = send(&service, get("/mappings?tab=files")?).await?;
    assert!(
        body.contains(r#"href="/login?return=/mappings%3Ftab%3Dfiles""#),
        "{body}"
    );
    let (_response, body) = send(&service, get("/?session=ended&return=%2Fevents")?).await?;
    assert!(body.contains(r#"href="/login?return=/events""#), "{body}");
    assert!(body.contains("Your session"), "the ended notice: {body}");
    let (_response, body) = send(
        &service,
        get("/?session=ended&return=%2F%2Fattacker.example.net")?,
    )
    .await?;
    assert!(
        !body.contains("/login?return="),
        "a foreign return never rides the button: {body}"
    );
    assert!(body.contains(r#"href="/login""#), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_signed_in_page_draws_the_session_watch_and_a_sign_in_page_does_not()
-> Result<(), Box<dyn Error>> {
    let (_provider, service, cookie) = signed_in().await?;
    let (response, body) = send(&service, get_with("/preferences", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains(&format!(r#"id="{WARNING_ID}""#)), "{body}");
    assert!(
        body.contains(r#"data-fui-status="/session""#),
        "the watch asks the session route: {body}"
    );
    let (_response, body) = send(&service, get("/preferences")?).await?;
    assert!(!body.contains(&format!(r#"id="{WARNING_ID}""#)), "{body}");
    Ok(())
}

#[tokio::test]
async fn the_signed_out_page_ends_every_other_tab() -> Result<(), Box<dyn Error>> {
    let (_provider, service, _cookie) = signed_in().await?;
    let (_response, body) = send(&service, get("/signed-out")?).await?;
    assert!(body.contains("data-fui-ended"), "{body}");
    Ok(())
}
