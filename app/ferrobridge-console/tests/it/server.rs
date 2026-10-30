// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP surface driven in-process: the health route, the
//! pages, FerroUI's assets, and the security headers on each.

use axum::body::Body;
use http::header::{
    ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_SECURITY_POLICY, CONTENT_TYPE,
    REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use http::{Request, Response, StatusCode};
use tower::ServiceExt as _;

use ferrobridge_console::server::{ConsoleState, router};

use crate::support::settings;

/// Sends `request` to a console whose bridge is out of reach.
async fn send(request: Request<Body>) -> Response<Body> {
    let state = ConsoleState::new(settings("http://127.0.0.1:9/")).expect("the state builds");
    router(state)
        .oneshot(request)
        .await
        .expect("the router answers")
}

/// A `GET` of `path` with `Accept-Encoding: br, gzip`.
fn get(path: &str) -> Request<Body> {
    Request::get(path)
        .header(ACCEPT_ENCODING, "br, gzip")
        .body(Body::empty())
        .expect("a request")
}

/// The text of `response`'s header `name`.
fn header<'a>(response: &'a Response<Body>, name: &http::HeaderName) -> &'a str {
    response
        .headers()
        .get(name)
        .unwrap_or_else(|| panic!("a {name} header"))
        .to_str()
        .expect("a text header")
}

/// The body of `response` as text.
async fn text(response: Response<Body>) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body reads");
    String::from_utf8(bytes.to_vec()).expect("a UTF-8 body")
}

/// Asserts the headers every answer carries.
fn assert_hardened(response: &Response<Body>) {
    assert_eq!(
        "nosniff",
        header(response, &X_CONTENT_TYPE_OPTIONS),
        "no media-type sniffing"
    );
    assert_eq!("DENY", header(response, &X_FRAME_OPTIONS), "never framed");
    assert_eq!(
        "no-referrer",
        header(response, &REFERRER_POLICY),
        "no referrer leaves the console"
    );
    let policy = header(response, &CONTENT_SECURITY_POLICY);
    assert!(!policy.contains("'unsafe-inline'"), "{policy}");
    assert!(!policy.contains("'unsafe-eval'"), "{policy}");
    assert!(!policy.contains("'wasm-unsafe-eval'"), "{policy}");
    assert!(!policy.contains("'nonce-"), "{policy}");
    assert!(policy.contains("script-src 'self';"), "{policy}");
    assert!(policy.contains("style-src 'self';"), "{policy}");
    assert!(policy.contains("frame-ancestors 'none'"), "{policy}");
}

#[tokio::test]
async fn the_health_route_answers_up_with_the_headers() {
    let response = send(get("/health")).await;
    assert_eq!(StatusCode::OK, response.status());
    assert_hardened(&response);
    assert_eq!("no-store", header(&response, &CACHE_CONTROL));
    assert_eq!(r#"{"state":"up"}"#, text(response).await);
    let readiness = send(get("/health/readiness")).await;
    assert_eq!(StatusCode::OK, readiness.status());
    assert_hardened(&readiness);
}

#[tokio::test]
async fn a_page_loads_the_librarys_assets_under_the_strict_policy_and_is_never_stored_or_compressed()
 {
    let first = send(get("/")).await;
    assert_eq!(StatusCode::OK, first.status());
    assert_hardened(&first);
    assert_eq!("no-store", header(&first, &CACHE_CONTROL));
    assert!(
        first.headers().get(CONTENT_ENCODING).is_none(),
        "a document is never compressed"
    );
    let page = text(first).await;
    let base = format!("/ferrohealth-ui/{}", ferrohealth_ui::serve::VERSION);
    for asset in [
        format!("{base}/ferrohealth-ui.css"),
        format!("{base}/themes/ferrobridge.css"),
        format!("{base}/scripts/fui.js"),
    ] {
        assert!(page.contains(&asset), "the page loads {asset}: {page}");
    }
    assert!(!page.contains("nonce="), "no script needs a nonce: {page}");
    assert!(
        !page.contains(".wasm"),
        "the page loads no WebAssembly: {page}"
    );
    assert!(!page.contains(" style="), "no inline style: {page}");
}

#[tokio::test]
async fn an_unknown_path_is_a_404_page_with_the_headers() {
    let response = send(get("/no-such-page")).await;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    assert_hardened(&response);
    assert_eq!("no-store", header(&response, &CACHE_CONTROL));
}

#[tokio::test]
async fn the_librarys_assets_are_served_for_a_year_under_their_version() {
    let base = format!("/ferrohealth-ui/{}", ferrohealth_ui::serve::VERSION);
    for (asset, kind) in [
        (format!("{base}/scripts/fui.js"), "text/javascript"),
        (format!("{base}/ferrohealth-ui.css"), "text/css"),
        (format!("{base}/themes/ferrobridge.css"), "text/css"),
    ] {
        let response = send(get(&asset)).await;
        assert_eq!(StatusCode::OK, response.status(), "{asset}");
        assert_hardened(&response);
        assert!(
            header(&response, &CONTENT_TYPE).starts_with(kind),
            "{asset}"
        );
        assert_eq!(
            "public, max-age=31536000, immutable",
            header(&response, &CACHE_CONTROL),
            "{asset}"
        );
        assert!(
            response.headers().get(CONTENT_ENCODING).is_none(),
            "{asset} is served as it stands"
        );
    }
    let stale = send(get("/ferrohealth-ui/0.0.0/scripts/fui.js")).await;
    assert_eq!(
        StatusCode::NOT_FOUND,
        stale.status(),
        "another version's path holds nothing"
    );
}

#[tokio::test]
async fn the_favicon_the_console_ships_is_served_whole() {
    let shipped = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/public/favicon.ico"))
        .expect("the shipped favicon");
    let response = send(get("/favicon.ico")).await;
    assert_eq!(StatusCode::OK, response.status());
    assert_hardened(&response);
    assert!(
        response.headers().get(CONTENT_ENCODING).is_none(),
        "the favicon is served as it stands"
    );
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the body reads");
    assert_eq!(shipped, bytes.to_vec(), "the favicon arrives byte for byte");
}
