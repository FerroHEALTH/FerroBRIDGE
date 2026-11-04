// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refresh of an operator's access token on the console's server
//! against the test OpenID Provider (RFC 6749 §6): a session outlives its
//! access token, a rotated refresh token is the one sent next, requests
//! that arrive together refresh once, a refused refresh ends the session,
//! and a provider that does not answer leaves it as it was.

use std::error::Error;

use ferrobridge_console::server::ConsoleState;
use ferrobridge_testkit::oidc::{Provider, TokenAnswer};
use ferrohealth_session::clock::{Clock, ManualClock};
use ferrohealth_ui::view::session::Left;
use http::StatusCode;

use crate::support::{
    Captured, OPERATOR_REFRESH, OPERATOR_TOKEN, ORIGIN, configuration, console_on, get_with,
    held_token, occupancy, send, set_cookies, sign_in,
};

/// The bridge address of a console whose tests never need the bridge.
const NO_BRIDGE: &str = "http://127.0.0.1:9/";

/// The second since the Unix epoch a manual session clock starts at.
const START: i64 = 1_800_000_000;

/// A path every request in these tests asks for: a page the console does not
/// serve, so no bridge is asked and the answer is quick.
const PAGE: &str = "/no-such-page";

/// A provider, the console configured to sign in at it, and a session signed
/// in with an access token already due and [`OPERATOR_REFRESH`].
async fn due_session(
    session: &str,
) -> Result<(Provider, ConsoleState, axum::Router, String), Box<dyn Error>> {
    due_session_on(session, Clock::system()).await
}

/// A session as [`due_session`] signs it in, in a store that counts every
/// lifetime on `clock`.
async fn due_session_on(
    session: &str,
    clock: Clock,
) -> Result<(Provider, ConsoleState, axum::Router, String), Box<dyn Error>> {
    let provider = Provider::start().await?;
    let (state, service) =
        console_on(&configuration(&provider, NO_BRIDGE, "", session), clock).await?;
    let cookie = sign_in(
        &service,
        &provider,
        TokenAnswer {
            expires_in: Some(0),
            refresh_token: Some(String::from(OPERATOR_REFRESH)),
            ..TokenAnswer::bearer(OPERATOR_TOKEN)
        },
    )
    .await?;
    provider.reset().await?;
    Ok((provider, state, service, cookie))
}

/// A refreshed answer carrying `access` and, when given, the rotated
/// refresh token `rotated`.
fn refreshed(access: &str, rotated: Option<&str>) -> TokenAnswer {
    TokenAnswer {
        expires_in: Some(3600),
        refresh_token: rotated.map(str::to_owned),
        ..TokenAnswer::bearer(access)
    }
}

#[tokio::test]
async fn a_session_outlives_its_access_token_by_refreshing_it() -> Result<(), Box<dyn Error>> {
    let (provider, state, service, cookie) = due_session("").await?;
    assert_eq!(None, held_token(&state, &cookie)?);
    provider
        .answer_refresh(OPERATOR_REFRESH, &refreshed("synthetic-access-2", None))
        .await?;
    send(&service, get_with(PAGE, &cookie)?).await?;
    assert_eq!(
        Some(String::from("synthetic-access-2")),
        held_token(&state, &cookie)?
    );
    let requests = provider.token_requests().await.ok_or("recorded")?;
    assert_eq!(1, requests.len());
    Ok(())
}

#[tokio::test]
async fn a_rotated_refresh_token_is_the_one_sent_next() -> Result<(), Box<dyn Error>> {
    let (provider, state, service, cookie) = due_session("").await?;
    provider
        .answer_refresh(
            OPERATOR_REFRESH,
            &TokenAnswer {
                expires_in: Some(0),
                ..refreshed("synthetic-access-2", Some("synthetic-refresh-2"))
            },
        )
        .await?;
    provider
        .answer_refresh(
            "synthetic-refresh-2",
            &refreshed("synthetic-access-3", Some("synthetic-refresh-3")),
        )
        .await?;
    send(&service, get_with(PAGE, &cookie)?).await?;
    send(&service, get_with(PAGE, &cookie)?).await?;
    assert_eq!(
        Some(String::from("synthetic-access-3")),
        held_token(&state, &cookie)?
    );
    let requests = provider.token_requests().await.ok_or("recorded")?;
    let sent: Vec<String> = requests
        .iter()
        .filter_map(|request| {
            url::form_urlencoded::parse(&request.body)
                .find(|(name, _)| name == "refresh_token")
                .map(|(_, value)| value.into_owned())
        })
        .collect();
    assert_eq!(vec![OPERATOR_REFRESH, "synthetic-refresh-2"], sent);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn requests_that_arrive_together_refresh_once() -> Result<(), Box<dyn Error>> {
    let (provider, state, service, cookie) = due_session("").await?;
    provider
        .answer_refresh(
            OPERATOR_REFRESH,
            &refreshed("synthetic-access-2", Some("synthetic-refresh-2")),
        )
        .await?;
    let mut requests = Vec::new();
    for _ in 0..8 {
        let service = service.clone();
        let request = get_with(PAGE, &cookie)?;
        requests.push(tokio::spawn(async move {
            send(&service, request)
                .await
                .map(|(response, _)| response.status())
                .map_err(|error| error.to_string())
        }));
    }
    for request in requests {
        assert_eq!(Ok(StatusCode::NOT_FOUND), request.await?);
    }
    let asked = provider.token_requests().await.ok_or("recorded")?;
    assert_eq!(1, asked.len(), "one refresh for the eight requests");
    assert_eq!(
        Some(String::from("synthetic-access-2")),
        held_token(&state, &cookie)?
    );
    Ok(())
}

// RFC 6749 §5.2: `invalid_grant` refuses the refresh token, so the session
// ends and the browser is told to drop its cookie.
#[tokio::test]
async fn a_refused_refresh_ends_the_session_and_drops_its_cookie() -> Result<(), Box<dyn Error>> {
    let (provider, state, service, cookie) = due_session("").await?;
    provider.refuse(400, "invalid_grant").await;
    let (response, _body) = send(&service, get_with(PAGE, &cookie)?).await?;
    let cookies = set_cookies(&response);
    assert!(
        cookies.iter().any(
            |set| set.starts_with("ferrobridge_console_session=;") && set.contains("Max-Age=0")
        ),
        "{cookies:?}"
    );
    assert_eq!(0, occupancy(&state)?.sessions);
    let (page, body) = send(&service, get_with("/", &cookie)?).await?;
    assert_eq!(StatusCode::OK, page.status());
    assert!(body.contains("Sign in"), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_provider_that_fails_on_its_side_leaves_the_session_to_try_again()
-> Result<(), Box<dyn Error>> {
    let (provider, state, service, cookie) = due_session("").await?;
    provider.refuse(503, "temporarily_unavailable").await;
    let (response, _body) = send(&service, get_with(PAGE, &cookie)?).await?;
    assert_eq!(set_cookies(&response), Vec::<String>::new());
    assert_eq!(1, occupancy(&state)?.sessions);
    provider.reset().await?;
    provider
        .answer_refresh(OPERATOR_REFRESH, &refreshed("synthetic-access-2", None))
        .await?;
    send(&service, get_with(PAGE, &cookie)?).await?;
    assert_eq!(
        Some(String::from("synthetic-access-2")),
        held_token(&state, &cookie)?
    );
    Ok(())
}

#[tokio::test]
async fn the_absolute_timeout_ends_a_session_whatever_its_refresh_token()
-> Result<(), Box<dyn Error>> {
    const IDLE: u32 = 3;
    let time = ManualClock::new(START);
    let (provider, state, service, cookie) = due_session_on(
        &format!("idle_timeout_s = {IDLE}\nabsolute_timeout_s = 8"),
        time.clock(),
    )
    .await?;
    let mut extended_for: u32 = 0;
    provider
        .answer_refresh(OPERATOR_REFRESH, &refreshed("synthetic-access-2", None))
        .await?;
    // NOTE: no specification governs this: our own design; the session watch's extension
    // keeps the idle timeout from ending the session and sends no refresh.
    let left = loop {
        // One extension a second stays inside the idle timeout.
        time.advance(1);
        extended_for += 1;
        let extend = http::Request::post("/session")
            .header("cookie", &cookie)
            .header("origin", ORIGIN)
            .body(axum::body::Body::empty())?;
        let (extended, body) = send(&service, extend).await?;
        assert_eq!(
            StatusCode::OK,
            extended.status(),
            "the session lives: {body}"
        );
        let left: Left = serde_json::from_str(&body)?;
        assert_eq!(
            IDLE.min(left.absolute),
            left.idle,
            "the idle timeout starts again"
        );
        if left.absolute <= 2 {
            break left;
        }
    };
    assert!(
        extended_for > IDLE,
        "the extensions kept the session past its idle timeout"
    );
    time.advance(i64::from(left.absolute) + 1);
    send(&service, get_with(PAGE, &cookie)?).await?;
    assert_eq!(None, held_token(&state, &cookie)?);
    assert_eq!(
        Some(0),
        provider.token_requests().await.map(|asked| asked.len())
    );
    Ok(())
}

#[tokio::test]
async fn the_sign_in_and_sign_out_routes_send_no_refresh() -> Result<(), Box<dyn Error>> {
    let (provider, _state, service, cookie) = due_session("").await?;
    provider
        .answer_refresh(OPERATOR_REFRESH, &refreshed("synthetic-access-2", None))
        .await?;
    for path in ["/login", "/login/", "/auth/callback"] {
        send(&service, get_with(path, &cookie)?).await?;
    }
    assert_eq!(
        Some(0),
        provider.token_requests().await.map(|asked| asked.len())
    );
    Ok(())
}

#[tokio::test]
async fn no_refresh_token_reaches_an_answer_a_cookie_or_a_log_line() -> Result<(), Box<dyn Error>> {
    let captured = Captured::default();
    let _guard = captured.logging();
    let (provider, _state, service, cookie) = due_session("").await?;
    provider
        .answer_refresh(
            OPERATOR_REFRESH,
            &refreshed("synthetic-access-2", Some("synthetic-refresh-2")),
        )
        .await?;
    let (response, body) = send(&service, get_with("/", &cookie)?).await?;
    provider.reset().await?;
    provider.refuse(400, "invalid_grant").await;
    let (refused, refused_body) = send(&service, get_with(PAGE, &cookie)?).await?;
    let logged = captured.text();
    let cookies = [set_cookies(&response), set_cookies(&refused)].concat();
    for secret in [
        OPERATOR_REFRESH,
        "synthetic-refresh-2",
        OPERATOR_TOKEN,
        "synthetic-access-2",
    ] {
        assert!(!logged.contains(secret), "a log line carries {secret}");
        assert!(!body.contains(secret), "the page carries {secret}");
        assert!(!refused_body.contains(secret), "the page carries {secret}");
        assert!(
            !cookies.iter().any(|set| set.contains(secret)),
            "a cookie carries {secret}"
        );
    }
    Ok(())
}
