// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every form post the console takes, held to one rule: it acts only for a
//! live session from the console's own pages, and refuses anything else
//! before it asks the bridge or sets a cookie. The console registers no
//! server function: every action is one of these posts, which a page sends
//! with no WebAssembly.

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode};

use crate::stub::{CONTEXT, bridge, signed_in};
use crate::support::{ORIGIN, send};

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

#[test]
fn the_console_registers_no_server_function() {
    assert_eq!(0, leptos::server_fn::axum::server_fn_paths().count());
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
