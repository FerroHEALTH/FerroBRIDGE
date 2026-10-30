// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every form post the console takes and every page it reads, held to one
//! rule: it acts only for a live session from the console's own pages, and
//! refuses anything else before it asks the bridge or sets a cookie. Every
//! action is a plain post a page sends, Reload, the editor and the
//! playground to their own page, which draws the answer.

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode};

use crate::stub::{CONTEXT, bridge, signed_in};
use crate::support::{OPERATOR_TOKEN, ORIGIN, get, get_with, header, send};

/// Every form post the console takes, each with a body it accepts: the
/// three answer forms post to the page they are on.
fn posts() -> Vec<(String, String)> {
    vec![
        (String::from("/mappings"), String::new()),
        (
            format!("/mappings/edit/{CONTEXT}"),
            String::from("draft=x&base=&path=&reason=r&decision=check"),
        ),
        (
            String::from("/mappings/new"),
            String::from("draft=x&base=&path=fhirconnect%2Fa.yml&reason=r&decision=check"),
        ),
        (
            String::from("/playground"),
            String::from("input=x&template=&patient="),
        ),
        (
            String::from("/playground/toopenehr"),
            String::from("input=x&template=&format=canonical"),
        ),
        (
            String::from("/playground/validate"),
            String::from("resource_type=Patient&input=x"),
        ),
        (
            String::from("/preference"),
            String::from("back=%2F&theme=dark"),
        ),
        (String::from("/preferences/reset"), String::from("back=%2F")),
        (
            String::from("/sidebar"),
            String::from("sidebar=rail&back=%2F"),
        ),
    ]
}

/// Pages that take no post, beside the ones [`posts`] lists.
const READ_ONLY: [&str; 9] = [
    "/",
    "/search",
    "/mappings/files/fhirconnect/contexts/observation.context.yml",
    "/mappings/templates/synthetic.vital_signs.v1",
    "/facade",
    "/connections",
    "/configuration",
    "/preferences",
    "/about",
];

/// The routes a page or a panel reads the bridge through, each answered
/// with what the bridge holds for a live session alone.
const READS: [&str; 11] = [
    "/",
    "/search?q=observation",
    "/mappings",
    "/mappings/omocl",
    "/mappings/edit/fhirconnect/contexts/observation.context.yml",
    "/playground",
    "/facade/security",
    "/connections/cdr/settings",
    "/configuration",
    "/about",
    "/search/answer?q=observation",
];

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

#[tokio::test]
async fn the_console_takes_posts_at_its_action_routes_alone() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    for (route, form) in posts() {
        let (response, body) = send(
            &service,
            posted(&route, &form, "same-origin", Some(&cookie))?,
        )
        .await?;
        assert_ne!(
            StatusCode::METHOD_NOT_ALLOWED,
            response.status(),
            "{route} takes its post: {body}"
        );
    }
    for route in READ_ONLY {
        let (response, _body) = send(
            &service,
            posted(route, "x=1", "same-origin", Some(&cookie))?,
        )
        .await?;
        assert_eq!(
            StatusCode::METHOD_NOT_ALLOWED,
            response.status(),
            "{route} takes no post"
        );
    }
    Ok(())
}

#[tokio::test]
async fn every_read_refuses_a_caller_without_a_live_session_before_the_bridge()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, _cookie) = signed_in(&bridge).await?;
    let before = bridge.received_requests().await.ok_or("recorded")?.len();
    for route in READS {
        for cookie in [None, Some("ferrobridge_console_session=forged")] {
            let request = match cookie {
                Some(cookie) => get_with(route, cookie)?,
                None => get(route)?,
            };
            let (response, answer) = send(&service, request).await?;
            assert!(
                response.headers().get("set-cookie").is_none(),
                "{route} {cookie:?} set a cookie"
            );
            if route.starts_with("/search/answer") {
                assert_eq!(StatusCode::UNAUTHORIZED, response.status(), "{route}");
            } else {
                assert!(answer.contains("Sign in"), "{route} {cookie:?}: {answer}");
                assert!(
                    !answer.contains("synthetic"),
                    "{route} {cookie:?}: {answer}"
                );
            }
        }
    }
    let after = bridge.received_requests().await.ok_or("recorded")?.len();
    assert_eq!(before, after, "a read without a session asked the bridge");
    Ok(())
}

#[tokio::test]
async fn the_page_of_a_caller_without_a_session_says_signed_out() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, _cookie) = signed_in(&bridge).await?;
    let before = bridge.received_requests().await.ok_or("recorded")?.len();
    let (response, answer) = send(&service, get("/")?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert!(answer.contains("Sign in to the console"), "{answer}");
    assert!(!answer.contains(crate::support::OPERATOR_NAME), "{answer}");
    assert!(
        !answer.contains(&bridge.uri()),
        "no setting reaches a visitor: {answer}"
    );
    let after = bridge.received_requests().await.ok_or("recorded")?.len();
    assert_eq!(before, after, "the page asked the bridge");
    Ok(())
}

#[tokio::test]
async fn a_page_carries_no_session_cookie_and_no_token() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let session = cookie.split_once('=').map_or("", |(_, value)| value);
    assert_ne!("", session, "a session cookie");
    for route in READS
        .iter()
        .filter(|route| !route.starts_with("/search/answer"))
    {
        let (response, answer) = send(&service, get_with(route, &cookie)?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{route}");
        assert_eq!(
            "no-store",
            header(&response, "cache-control"),
            "no page is kept: {route}"
        );
        assert!(answer.contains("Synthetic Operator"), "{route}");
        assert!(!answer.contains(session), "{route}: {answer}");
        assert!(!answer.contains(OPERATOR_TOKEN), "{route}: {answer}");
    }
    Ok(())
}

#[tokio::test]
async fn every_page_refuses_a_post_from_another_origin() -> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, cookie) = signed_in(&bridge).await?;
    let before = bridge.received_requests().await.ok_or("recorded")?.len();
    for route in READ_ONLY {
        let (response, answer) =
            send(&service, posted(route, "x=1", "cross-site", Some(&cookie))?).await?;
        assert_eq!(StatusCode::FORBIDDEN, response.status(), "{route}");
        assert!(
            answer.contains("the console takes this request only from its own pages"),
            "{route}: {answer}"
        );
    }
    let after = bridge.received_requests().await.ok_or("recorded")?.len();
    assert_eq!(before, after, "a foreign post asked the bridge");
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
                send(&service, posted(&route, &form, "same-origin", cookie)?).await?;
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
        let (response, body) = send(
            &service,
            posted(&route, &form, "cross-site", Some(&cookie))?,
        )
        .await?;
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

#[tokio::test]
async fn a_plain_answer_post_without_a_live_session_draws_the_sign_in_and_asks_nothing()
-> Result<(), Box<dyn Error>> {
    let bridge = bridge().await;
    let (service, _cookie) = signed_in(&bridge).await?;
    let before = bridge.received_requests().await.ok_or("recorded")?.len();
    for (route, form) in posts()
        .into_iter()
        .filter(|(route, _)| route.starts_with("/mappings") || route.starts_with("/playground"))
    {
        let request = crate::stub::post(&route, &form, "ferrobridge_console_session=forged")?;
        let (response, body) = send(&service, request).await?;
        assert!(
            response.headers().get("set-cookie").is_none(),
            "{route} set a cookie"
        );
        assert!(body.contains("Sign in"), "{route}: {body}");
        assert!(
            !body.contains("synthetic"),
            "{route} drew something the bridge holds: {body}"
        );
    }
    let after = bridge.received_requests().await.ok_or("recorded")?.len();
    assert_eq!(before, after, "a post without a session asked the bridge");
    Ok(())
}
