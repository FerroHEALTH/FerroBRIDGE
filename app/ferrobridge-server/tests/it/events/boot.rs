// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The events lane at boot: a CDR that serves no outbox feed for the reader
//! refuses the start, and a lane whose broker is down starts degraded while
//! the process stays ready.

use std::collections::BTreeMap;
use std::error::Error;
use std::time::Duration;

use ferrobridge_server::boot::{Backoff, serve_settings};
use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::Settings;
use serde_json::json;
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A short backoff for the not-ready wait.
const BACKOFF: Backoff = Backoff {
    initial: Duration::from_millis(20),
    max: Duration::from_millis(50),
};

/// Returns settings whose events lane follows the reference CDR's feed at
/// `cdr` and publishes to a broker on `broker_port`, where nothing listens,
/// parking poisoned events in `store`.
fn settings(
    cdr: &MockServer,
    broker_port: u16,
    store: &tempfile::TempDir,
) -> Result<Settings, Box<dyn Error>> {
    let text = format!(
        "[usage_report]\nenabled = false\n\n\
         [cdr]\nbase_url = \"{uri}/v1\"\n\n\
         [change_feed]\nenabled = true\nsource = \"ferroehr\"\nurl = \"{uri}/ext\"\npoll_interval = \"10ms\"\nbackoff = \"10ms\"\nmax_backoff = \"20ms\"\ndead_letters = \"{dead_letters}\"\n\n\
         [events]\nenabled = true\nreader = \"ferrobridge-test\"\nurl = \"amqp://guest:guest@127.0.0.1:{broker_port}/%2f\"\nexchange = \"ferrobridge.test.events\"\nconfirm_timeout = \"500ms\"\n",
        uri = cdr.uri(),
        dead_letters = store.path().join("dead-letters.redb").display(),
    );
    Ok(Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?)
}

/// Returns a local port nothing listens on.
fn closed_port() -> Result<u16, Box<dyn Error>> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

#[tokio::test]
async fn a_cdr_that_serves_no_feed_for_the_reader_refuses_the_start() -> Result<(), Box<dyn Error>>
{
    let cdr = MockServer::start().await;
    let store = tempfile::tempdir()?;
    Mock::given(method("GET"))
        .and(path("/ext/admin/outbox/ferrobridge-test"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&cdr)
        .await;
    let outcome = serve_settings(
        &settings(&cdr, closed_port()?, &store)?,
        || async { Ok(TcpListener::bind("127.0.0.1:0").await?) },
        BACKOFF,
        std::future::pending::<()>(),
    )
    .await;
    let error = outcome.err().ok_or("the start is refused")?;
    assert!(
        format!("{error:#}").contains("serves no outbox feed"),
        "{error:#}"
    );
    Ok(())
}

#[tokio::test]
async fn a_broker_that_is_down_degrades_the_lane_and_the_process_stays_ready()
-> Result<(), Box<dyn Error>> {
    let cdr = MockServer::start().await;
    let store = tempfile::tempdir()?;
    Mock::given(method("GET"))
        .and(path("/ext/admin/outbox/ferrobridge-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "reader": "ferrobridge-test",
            "events": [{
                "seq": 1,
                "contribution_id": "0d4a3c52-5b51-4b4b-9e0e-000000000001",
                "ehr_id": "7d44b88c-4199-4bad-97dc-d78268e01398",
                "committed_at": "2026-11-02T10:00:00Z",
                "versions": [{
                    "vo_id": "8849182c-82ad-4088-a07f-48ead4180515",
                    "kind": "COMPOSITION",
                    "sys_version": 1,
                    "change_type": "249",
                    "template_id": null
                }]
            }]
        })))
        .mount(&cdr)
        .await;
    Mock::given(method("POST"))
        .and(path("/ext/admin/outbox/ferrobridge-test/ack"))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&cdr)
        .await;
    let settings = settings(&cdr, closed_port()?, &store)?;
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
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let body = loop {
            let answered = client
                .get(format!("http://{address}/health/readiness"))
                .send()
                .await;
            if let Ok(response) = answered
                && response.status().as_u16() == 200
            {
                let body: serde_json::Value = response.json().await?;
                let degraded = body["indicators"].as_array().is_some_and(|indicators| {
                    indicators.iter().any(|indicator| {
                        indicator["name"] == "events" && indicator["state"] == "degraded"
                    })
                });
                if degraded {
                    break body;
                }
            }
            if tokio::time::Instant::now() > deadline {
                return Err::<serde_json::Value, Box<dyn Error>>(
                    "readiness never reported the events lane degraded".into(),
                );
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let _sent = stop.send(());
        Ok(body)
    };
    let (served, probed) = tokio::join!(serving, probing);
    probed?;
    served.map_err(|error| format!("{error:#}"))?;
    Ok(())
}
