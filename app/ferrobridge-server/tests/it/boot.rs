// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A CDR that is not ready at boot: the process stays up, readiness answers
//! `503` with the `startup` indicator, and the lanes start once the CDR
//! serves the templates.
//!
//! The template fetch is `GET /definition/template/adl1.4/{template_id}`
//! (openEHR ITS-REST 1.1.0, `definition-codegen.openapi.yaml`), which
//! documents no `5xx`; a `503` is the not-ready answer. No specification
//! governs the wait: our own design.

use ferrobridge_server::boot::{Backoff, serve_settings};
use ferrobridge_server::config::{Config, Settings};
use http::StatusCode;
use std::collections::BTreeMap;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use wiremock::matchers;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The mapping set the operations lane compiles: one context over the
/// testkit template.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures");

/// The ADL 1.4 route of the template the fixture context names.
const ADL14_ROUTE: &str = "/definition/template/adl1.4/ferrobridge.diagnose.v1";

/// A wait short enough for a test.
const BACKOFF: Backoff = Backoff {
    initial: Duration::from_millis(20),
    max: Duration::from_millis(50),
};

/// Returns settings with a CDR at `cdr`, one attempt per call, and the
/// fixture mapping set.
fn settings(cdr: &MockServer) -> Result<Settings, Box<dyn Error>> {
    let text = format!(
        "[cdr]\nbase_url = \"{}/\"\ntimeout_ms = 2000\n\n[cdr.retry]\nmax_attempts = 1\n\n[auth]\nenabled = false\n\n[mappings]\ndirectory = \"{FIXTURES}\"\n\n[usage_report]\nenabled = false\n",
        cdr.uri()
    );
    Ok(Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?)
}

/// Mounts `response` on the template route of `cdr`.
async fn template(cdr: &MockServer, response: ResponseTemplate) {
    Mock::given(matchers::method("GET"))
        .and(matchers::path(ADL14_ROUTE))
        .respond_with(response)
        .mount(cdr)
        .await;
}

/// Returns the status and the JSON body of `GET {path}` on `address`.
async fn get(
    client: &reqwest::Client,
    address: SocketAddr,
    path: &str,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    let response = client.get(format!("http://{address}{path}")).send().await?;
    let status = StatusCode::from_u16(response.status().as_u16())?;
    let text = response.text().await?;
    let body = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    Ok((status, body))
}

/// Asks readiness on `address` until it answers `wanted`, for at most five
/// seconds, and returns the last body.
async fn readiness_until(
    client: &reqwest::Client,
    address: SocketAddr,
    wanted: StatusCode,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        // The handover from the probe routes to the lanes closes the idle
        // connections, so a request that meets a closing one is asked again.
        let last = match get(client, address, "/health/readiness").await {
            Ok((status, body)) if status == wanted => return Ok(body),
            Ok((status, body)) => format!("{status}: {body}"),
            Err(error) => error.to_string(),
        };
        if tokio::time::Instant::now() > deadline {
            return Err(format!("readiness never answered {wanted}, last {last}").into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_cdr_that_answers_late_keeps_the_process_up_until_the_lanes_start()
-> Result<(), Box<dyn Error>> {
    let cdr = MockServer::start().await;
    template(&cdr, ResponseTemplate::new(503)).await;
    let settings = settings(&cdr)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = serve_settings(
        &settings,
        move || async move { Ok(listener) },
        BACKOFF,
        async move {
            let _closed = stopped.await.is_err();
        },
    );

    let probing = async {
        let client = reqwest::Client::new();
        let waiting = readiness_until(&client, address, StatusCode::SERVICE_UNAVAILABLE).await?;
        assert_eq!(
            Some("down"),
            indicator(&waiting, "startup").and_then(|startup| startup["state"].as_str()),
            "{waiting}"
        );
        assert!(
            indicator(&waiting, "startup").is_some_and(|startup| startup.get("detail").is_none()),
            "the open route names the indicator and never its detail: {waiting}"
        );
        let (liveness, _) = get(&client, address, "/health/liveness").await?;
        assert_eq!(StatusCode::OK, liveness, "the process stays up");

        cdr.reset().await;
        template(
            &cdr,
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/xml")
                .set_body_string(ferrobridge_testkit::fixtures::DIAGNOSE_OPT),
        )
        .await;
        let ready = readiness_until(&client, address, StatusCode::OK).await?;
        assert!(
            indicator(&ready, "startup").is_none(),
            "the started lanes report their own indicators: {ready}"
        );
        assert_eq!(
            Some("up"),
            indicator(&ready, "cdr").and_then(|cdr| cdr["state"].as_str()),
            "{ready}"
        );
        stop.send(()).map_err(|()| "the server is gone")?;
        Ok::<(), Box<dyn Error>>(())
    };

    let (served, probed) = tokio::join!(serving, probing);
    probed?;
    served?;
    Ok(())
}

#[tokio::test]
async fn a_template_the_cdr_refuses_still_fails_the_start() -> Result<(), Box<dyn Error>> {
    let cdr = MockServer::start().await;
    template(&cdr, ResponseTemplate::new(400)).await;
    let settings = settings(&cdr)?;

    let outcome = serve_settings(
        &settings,
        || async { Ok(TcpListener::bind("127.0.0.1:0").await?) },
        BACKOFF,
        std::future::pending::<()>(),
    )
    .await;
    let error = outcome
        .err()
        .ok_or("a documented refusal fails the start")?;
    assert!(
        format!("{error:#}").contains("ferrobridge.diagnose.v1"),
        "{error:#}"
    );
    Ok(())
}

#[tokio::test]
async fn a_stop_while_the_cdr_is_not_ready_ends_the_wait() -> Result<(), Box<dyn Error>> {
    let cdr = MockServer::start().await;
    template(&cdr, ResponseTemplate::new(503)).await;
    let settings = settings(&cdr)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();

    let serving = serve_settings(
        &settings,
        move || async move { Ok(listener) },
        BACKOFF,
        async move {
            let _closed = stopped.await.is_err();
        },
    );
    let probing = async {
        let client = reqwest::Client::new();
        readiness_until(&client, address, StatusCode::SERVICE_UNAVAILABLE).await?;
        stop.send(()).map_err(|()| "the server is gone")?;
        Ok::<(), Box<dyn Error>>(())
    };
    let (served, probed) = tokio::join!(serving, probing);
    probed?;
    served?;
    Ok(())
}

/// Returns the indicator called `name` in the readiness document `document`.
fn indicator<'d>(document: &'d serde_json::Value, name: &str) -> Option<&'d serde_json::Value> {
    document["indicators"]
        .as_array()?
        .iter()
        .find(|indicator| indicator["name"] == name)
}
