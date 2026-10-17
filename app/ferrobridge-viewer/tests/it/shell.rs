// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shell around every page: the theme the server draws from its cookie,
//! the top bar with the bridge's status and version, the grouped
//! navigation and its drawer, the theme switch, the operator's menu, the
//! About page naming the maker, and the page for a path the console does not
//! serve.

use std::error::Error;

use axum::body::Body;
use ferrobridge_testkit::oidc::{Provider, TokenAnswer};
use ferrobridge_viewer::model::{BridgeView, Outcome, Refusal, ViewError};
use ferrobridge_viewer::shell::{NAVIGATION, bridge_status};
use ferrobridge_viewer::styles::Tone;
use http::{Request, StatusCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{
    OPERATOR_NAME, OPERATOR_TOKEN, configuration, console, get, get_with, header, send,
    set_cookies, sign_in,
};

/// A stub bridge that answers its `CapabilityStatement` at version 0.0.8.
async fn bridge() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/metadata"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"resourceType":"CapabilityStatement","status":"active","date":"2026-10-17","kind":"instance","software":{"name":"FerroBRIDGE","version":"0.0.8"},"fhirVersion":"4.0.1","format":["json"]}"#,
            "application/fhir+json",
        ))
        .mount(&server)
        .await;
    server
}

/// The console over `bridge` and a signed-in session's cookie.
async fn signed_in(bridge: &MockServer) -> Result<(axum::Router, String), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let base = format!("{}/", bridge.uri());
    let (_state, service) = console(&configuration(&provider, &base, "", ""))?;
    let cookie = sign_in(&service, &provider, TokenAnswer::bearer(OPERATOR_TOKEN)).await?;
    Ok((service, cookie))
}

/// The `data-theme` the page's `<html>` carries.
fn data_theme(page: &str) -> &str {
    page.split_once("data-theme=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(theme, _)| theme)
        .unwrap_or_default()
}

#[tokio::test]
async fn a_page_without_a_theme_cookie_follows_the_system() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (_response, page) = send(&service, get("/")?).await?;
    assert_eq!("system", data_theme(&page));
    Ok(())
}

#[tokio::test]
async fn the_server_draws_the_theme_its_cookie_names_and_ignores_another_word()
-> Result<(), Box<dyn Error>> {
    let (_state, service) = console("[session]\nsecure_cookie = false\n")?;
    for (cookie, theme) in [
        ("ferrobridge_viewer_theme=dark", "dark"),
        ("ferrobridge_viewer_theme=light", "light"),
        ("ferrobridge_viewer_theme=sepia", "system"),
    ] {
        let (_response, page) = send(&service, get_with("/", cookie)?).await?;
        assert_eq!(theme, data_theme(&page), "{cookie}");
    }
    Ok(())
}

#[tokio::test]
async fn the_signed_in_shell_carries_the_bar_the_navigation_and_the_footer()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, page) = send(&service, get_with("/", &cookie)?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(page.contains(r##"<a href="#main""##), "a skip link: {page}");
    assert!(page.contains(r#"<main id="main""#), "{page}");
    assert!(page.contains("Bridge UP · v0.0.8"), "{page}");
    assert!(page.contains(OPERATOR_NAME), "{page}");
    assert!(page.contains(r#"popovertarget="sidebar""#), "{page}");
    assert!(
        page.contains(r#"<nav id="sidebar" popover="auto""#),
        "{page}"
    );
    assert!(page.contains(r#"aria-label="Theme""#), "{page}");
    assert!(page.contains(r#"action="/api/theme""#), "{page}");
    assert!(page.contains(r#"aria-pressed="true""#), "{page}");
    assert!(page.contains("Cadasto B.V."), "the footer names the maker");
    assert!(page.contains(r#"href="/about""#), "{page}");
    assert!(!page.contains(OPERATOR_TOKEN), "{page}");
    Ok(())
}

#[tokio::test]
async fn the_navigation_draws_its_groups_in_order_and_leaves_an_empty_one_out()
-> Result<(), Box<dyn Error>> {
    let titles: Vec<&str> = NAVIGATION.iter().map(|group| group.title).collect();
    assert_eq!(vec!["Overview", "Mappings", "Tools", "Bridge"], titles);
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/", &cookie)?).await?;
    let mut at = 0;
    for group in NAVIGATION {
        let heading = format!(r#"id="{}""#, group.id);
        match page.find(&heading) {
            Some(found) if !group.entries.is_empty() => {
                assert!(found > at, "{} out of order", group.title);
                at = found;
                assert!(
                    page.contains(&format!(r#"aria-labelledby="{}""#, group.id)),
                    "the list names its heading"
                );
            }
            None if group.entries.is_empty() => {}
            found => panic!("{}: {found:?}", group.title),
        }
    }
    assert!(
        page.contains(r#"aria-current="page""#),
        "the dashboard is the page"
    );
    Ok(())
}

#[tokio::test]
async fn a_visitor_reads_no_navigation_no_bar_and_the_footer() -> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (_response, page) = send(&service, get("/")?).await?;
    assert!(!page.contains("<nav"), "{page}");
    assert!(!page.contains("Sign out"), "{page}");
    assert!(page.contains("Cadasto B.V."), "{page}");
    Ok(())
}

#[tokio::test]
async fn about_names_the_maker_and_shows_the_bridge_only_to_an_operator()
-> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (response, page) = send(&service, get("/about")?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(page.contains("Made by"), "{page}");
    assert!(page.contains("Cadasto B.V."), "{page}");
    assert!(page.contains(env!("CARGO_PKG_VERSION")), "{page}");
    assert!(!page.contains("<code>http"), "{page}");
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (_response, page) = send(&service, get_with("/about", &cookie)?).await?;
    assert!(
        page.contains(&format!("<code>{}/</code>", bridge.uri())),
        "{page}"
    );
    Ok(())
}

#[tokio::test]
async fn a_path_the_console_does_not_serve_is_a_404_page_with_the_way_back()
-> Result<(), Box<dyn Error>> {
    let (_state, service) = console("")?;
    let (response, page) = send(&service, get("/no/such/page")?).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    assert!(page.contains("404: the console has no page at this address."));
    assert!(page.contains("Back to the console"));
    Ok(())
}

/// A `POST` of the theme server function with `form` and `accept`.
fn theme_post(form: &str, accept: &str, cookie: &str) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post("/api/theme")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", accept)
        .header("sec-fetch-site", "same-origin")
        .header("cookie", cookie)
        .body(Body::from(form.to_owned()))?)
}

#[tokio::test]
async fn the_theme_switch_keeps_the_choice_in_its_cookie_and_the_next_page_wears_it()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, body) = send(
        &service,
        theme_post("theme=dark", "application/json", &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    let kept = set_cookies(&response)
        .into_iter()
        .find(|set| set.starts_with("ferrobridge_viewer_theme=dark"))
        .ok_or("the theme cookie")?;
    assert!(
        kept.contains("HttpOnly") && kept.contains("Max-Age=34560000"),
        "{kept}"
    );
    let both = format!("{cookie}; ferrobridge_viewer_theme=dark");
    let (_response, page) = send(&service, get_with("/", &both)?).await?;
    assert_eq!("dark", data_theme(&page));
    Ok(())
}

#[tokio::test]
async fn a_plain_theme_post_returns_to_the_consoles_own_page_alone() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, _body) = send(
        &service,
        theme_post("theme=light&back=%2Fabout", "text/html", &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/about", header(&response, "location"));
    let (response, _body) = send(
        &service,
        theme_post(
            "theme=light&back=https%3A%2F%2Fattacker.example.net%2F",
            "text/html",
            &cookie,
        )?,
    )
    .await?;
    assert_ne!(
        "https://attacker.example.net/",
        header(&response, "location")
    );
    assert!(!header(&response, "location").contains("attacker"));
    Ok(())
}

#[tokio::test]
async fn a_word_that_names_no_theme_is_refused_and_sets_no_cookie() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let (response, body) = send(
        &service,
        theme_post("theme=sepia", "application/json", &cookie)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(body.contains("Invalid"), "{body}");
    assert!(
        !set_cookies(&response)
            .iter()
            .any(|set| set.starts_with("ferrobridge_viewer_theme")),
        "no cookie for a refused word"
    );
    Ok(())
}

// WCAG 2.2 SC 1.4.1: the status is a word as well as a tint.
#[test]
fn the_bridge_status_is_a_word_for_every_answer() {
    let shown = Ok(Outcome::Shown(BridgeView {
        software: Some(String::from("FerroBRIDGE")),
        version: Some(String::from("0.0.8")),
        fhir_version: Some(String::from("4.0.1")),
    }));
    assert_eq!(
        (Tone::Ok, String::from("Bridge UP · v0.0.8")),
        bridge_status(&shown)
    );
    let refused = Ok(Outcome::Refused(Refusal::Bridge {
        status: 403,
        diagnostics: Vec::new(),
    }));
    assert_eq!(
        (Tone::Danger, String::from("Bridge refused 403")),
        bridge_status(&refused)
    );
    assert_eq!(
        (Tone::Danger, String::from("Bridge DOWN")),
        bridge_status(&Err(ViewError::Unreachable))
    );
    assert_eq!(
        (Tone::Warn, String::from("Bridge UNKNOWN")),
        bridge_status(&Err(ViewError::Unavailable))
    );
}
