// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The AMQP broker itself: an unreachable broker refuses the publish, and a
//! real `RabbitMQ` (the container lane, behind `FERROBRIDGE_E2E`) receives
//! every confirmed message in order in the queue `[events]` bound.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_server::change_feed::PageSize;
use ferrobridge_server::change_feed::ReaderName;
use ferrobridge_server::change_feed::ferroehr::FerroEhrFeed;
use ferrobridge_server::change_feed::follow::Cursor;
use ferrobridge_server::change_feed::follow::pass;
use ferrobridge_server::events::Publish;
use ferrobridge_server::events::PublishError;
use ferrobridge_server::events::Publisher;
use ferrobridge_server::events::Queue;
use ferrobridge_server::events::Settings;
use ferrobridge_server::events::amqp::AmqpBroker;
use ferrobridge_testkit::containers;
use lapin::options::BasicGetOptions;
use secrecy::SecretString;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::change_feed::client;
use crate::change_feed::reader;

/// The queue the lane binds in these cases.
const QUEUE: &str = "ferrobridge.test.compositions";

/// Returns the lane settings for the broker at `url`.
fn settings(url: &str) -> Settings {
    Settings {
        reader: ReaderName::new("ferrobridge-test").expect("a valid reader name"),
        url: SecretString::from(url.to_owned()),
        exchange: String::from("ferrobridge.test.events"),
        queues: vec![Queue {
            name: String::from(QUEUE),
            binding: String::from("COMPOSITION.*.*"),
        }],
        confirm_timeout: Duration::from_secs(10),
    }
}

#[tokio::test]
async fn an_unreachable_broker_refuses_the_publish() -> Result<(), Box<dyn Error>> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let broker = AmqpBroker::new(settings(&format!(
        "amqp://guest:guest@127.0.0.1:{port}/%2f"
    )));
    let refused = broker.publish("COMPOSITION.249.-", b"{}").await;
    assert!(
        matches!(
            refused,
            Err(PublishError::Amqp(_) | PublishError::Timeout(_))
        ),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_real_broker_holds_every_confirmed_message_in_order() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let rabbit = containers::rabbitmq().await?;
    let server = MockServer::start().await;
    let events: Vec<serde_json::Value> = (1..=3)
        .map(|seq| {
            json!({
                "seq": seq,
                "contribution_id": format!("0d4a3c52-5b51-4b4b-9e0e-{seq:012}"),
                "ehr_id": "7d44b88c-4199-4bad-97dc-d78268e01398",
                "committed_at": "2026-11-02T10:00:00Z",
                "versions": [{
                    "vo_id": "8849182c-82ad-4088-a07f-48ead4180515",
                    "kind": "COMPOSITION",
                    "sys_version": seq,
                    "version_tree_id": seq.to_string(),
                    "change_type": "251",
                    "template_id": "vital_signs.v1"
                }]
            })
        })
        .collect();
    for (domain, served) in [("clinical", events.as_slice()), ("party", &[][..])] {
        Mock::given(method("GET"))
            .and(path("/ext/admin/outbox/ferrobridge-test"))
            .and(query_param("domain", domain))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "reader": "ferrobridge-test", "events": served })),
            )
            .mount(&server)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/ext/admin/outbox/ferrobridge-test/ack"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let feed = FerroEhrFeed::new(client(&server)?, format!("{}/ext", server.uri()).parse()?);
    let publisher = Publisher::new(Arc::new(AmqpBroker::new(settings(rabbit.url()))));
    let mut cursor = Cursor::default();
    pass(
        &feed,
        &reader(),
        &publisher,
        PageSize::new(10).ok_or("a page size")?,
        &mut cursor,
    )
    .await?;

    let connection =
        lapin::Connection::connect(rabbit.url(), lapin::ConnectionProperties::default()).await?;
    let channel = connection.create_channel().await?;
    let mut seqs = Vec::new();
    while let Some(message) = channel
        .basic_get(QUEUE.into(), BasicGetOptions { no_ack: true })
        .await?
    {
        let body: serde_json::Value = serde_json::from_slice(&message.delivery.data)?;
        assert_eq!(
            "COMPOSITION.251.vital_signs_v1",
            message.delivery.routing_key.as_str()
        );
        seqs.push(body["seq"].clone());
    }
    assert_eq!(vec![json!(1), json!(2), json!(3)], seqs);
    Ok(())
}
