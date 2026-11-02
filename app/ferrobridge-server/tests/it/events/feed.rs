// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The publisher as a change-feed consumer, on either source: every version
//! of an event is one message in order, under the reference CDR's routing
//! keys, and the cursor moves only over events the broker confirmed whole.

use std::error::Error;
use std::sync::Arc;

use ferrobridge_server::change_feed::Domain;
use ferrobridge_server::change_feed::PageSize;
use ferrobridge_server::change_feed::Position;
use ferrobridge_server::change_feed::Seq;
use ferrobridge_server::change_feed::Stream;
use ferrobridge_server::change_feed::aql::AqlFeed;
use ferrobridge_server::change_feed::aql::Start;
use ferrobridge_server::change_feed::ferroehr::FerroEhrFeed;
use ferrobridge_server::change_feed::follow::Cursor;
use ferrobridge_server::change_feed::follow::PassError;
use ferrobridge_server::change_feed::follow::pass;
use ferrobridge_server::change_feed::watermark::WatermarkStore;
use ferrobridge_server::events::Publisher;
use ferrobridge_testkit::stubs::its_rest;
use serde_json::json;
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Recording;
use crate::change_feed::client;
use crate::change_feed::reader;

/// The feed path of the test reader.
const FEED: &str = "/ext/admin/outbox/ferrobridge-test";

/// The acknowledgement path of the test reader.
const ACK: &str = "/ext/admin/outbox/ferrobridge-test/ack";

/// Returns a synthetic envelope at `seq` with two changed versions.
fn envelope(seq: u64) -> serde_json::Value {
    json!({
        "seq": seq,
        "contribution_id": format!("0d4a3c52-5b51-4b4b-9e0e-{seq:012}"),
        "ehr_id": "7d44b88c-4199-4bad-97dc-d78268e01398",
        "committed_at": "2026-11-02T10:00:00Z",
        "versions": [
            {
                "vo_id": "8849182c-82ad-4088-a07f-48ead4180515",
                "kind": "COMPOSITION",
                "sys_version": 3,
                "version_tree_id": "3",
                "change_type": "251",
                "template_id": "vital_signs.v1"
            },
            {
                "vo_id": "8849182c-82ad-4088-a07f-48ead4180516",
                "kind": "EHR_STATUS",
                "sys_version": 2,
                "version_tree_id": "2",
                "change_type": "251",
                "template_id": null
            }
        ]
    })
}

/// Mounts the clinical outbox serving `events` and an empty party outbox.
async fn outbox(server: &MockServer, events: &[serde_json::Value]) {
    for (domain, served) in [("clinical", events), ("party", &[][..])] {
        Mock::given(method("GET"))
            .and(path(FEED))
            .and(query_param("domain", domain))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "reader": "ferrobridge-test", "events": served })),
            )
            .mount(server)
            .await;
    }
}

/// Returns the reference CDR's feed over `server`.
fn ferroehr(server: &MockServer) -> Result<FerroEhrFeed, Box<dyn Error>> {
    Ok(FerroEhrFeed::new(
        client(server)?,
        format!("{}/ext", server.uri()).parse()?,
    ))
}

/// Returns a publisher over `broker`.
fn publisher(broker: &Arc<Recording>) -> Publisher {
    let cloned = Arc::clone(broker);
    let shared: Arc<dyn ferrobridge_server::events::Publish> = cloned;
    Publisher::new(shared)
}

/// Returns the page size every case asks for.
fn limit() -> PageSize {
    PageSize::new(10).expect("a page size in range")
}

#[tokio::test]
async fn each_version_is_one_message_in_seq_order_under_its_routing_key()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    outbox(&server, &[envelope(41), envelope(42)]).await;
    Mock::given(method("POST"))
        .and(path(ACK))
        .and(query_param("domain", "clinical"))
        .and(body_json(json!({ "last_seq": 42 })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let broker = Arc::new(Recording::default());
    let publisher = publisher(&broker);
    let mut cursor = Cursor::default();
    pass(
        &ferroehr(&server)?,
        &reader(),
        &publisher,
        limit(),
        &mut cursor,
    )
    .await?;
    let confirmed = broker.confirmed();
    let keys: Vec<&str> = confirmed.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        vec![
            "COMPOSITION.251.vital_signs_v1",
            "EHR_STATUS.251.-",
            "COMPOSITION.251.vital_signs_v1",
            "EHR_STATUS.251.-"
        ],
        keys
    );
    let (_, second) = confirmed.get(1).ok_or("a second message")?;
    assert_eq!(json!(41), second["seq"]);
    assert_eq!(json!(1), second["version_index"]);
    assert_eq!(
        json!("0d4a3c52-5b51-4b4b-9e0e-000000000041"),
        second["contribution_id"]
    );
    Ok(())
}

#[tokio::test]
async fn a_broker_refusal_leaves_the_cursor_after_the_last_event_it_confirmed_whole()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    outbox(&server, &[envelope(41), envelope(42)]).await;
    Mock::given(method("POST"))
        .and(path(ACK))
        .and(body_json(json!({ "last_seq": 41 })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    // The broker confirms the first event and one version of the second.
    let broker = Arc::new(Recording::accepting(3));
    let publisher = publisher(&broker);
    let mut cursor = Cursor::default();
    let failed = pass(
        &ferroehr(&server)?,
        &reader(),
        &publisher,
        limit(),
        &mut cursor,
    )
    .await;
    assert!(
        matches!(failed, Err(PassError::Consumer { .. })),
        "{failed:?}"
    );
    assert_eq!(
        Some(&Position::Outbox {
            domain: Domain::Clinical,
            seq: Seq::new(41)
        }),
        cursor.acknowledged(Stream::Outbox(Domain::Clinical))
    );
    Ok(())
}

#[tokio::test]
async fn a_broker_that_confirms_nothing_moves_no_cursor() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    outbox(&server, &[envelope(41)]).await;
    Mock::given(method("POST"))
        .and(path(ACK))
        .respond_with(ResponseTemplate::new(204))
        .expect(0)
        .mount(&server)
        .await;
    let publisher = Publisher::new(Arc::new(Recording::accepting(0)));
    let mut cursor = Cursor::default();
    let failed = pass(
        &ferroehr(&server)?,
        &reader(),
        &publisher,
        limit(),
        &mut cursor,
    )
    .await;
    assert!(
        matches!(failed, Err(PassError::Consumer { .. })),
        "{failed:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_cdr_refusal_publishes_nothing() -> Result<(), Box<dyn Error>> {
    for status in [401_u16, 404, 503] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(FEED))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(ACK))
            .respond_with(ResponseTemplate::new(204))
            .expect(0)
            .mount(&server)
            .await;
        let broker = Arc::new(Recording::default());
        let publisher = publisher(&broker);
        let mut cursor = Cursor::default();
        let failed = pass(
            &ferroehr(&server)?,
            &reader(),
            &publisher,
            limit(),
            &mut cursor,
        )
        .await;
        assert!(
            matches!(failed, Err(PassError::Feed(_))),
            "{status}: {failed:?}"
        );
        assert_eq!(0, broker.confirmed().len(), "{status}");
    }
    Ok(())
}

#[tokio::test]
async fn the_aql_source_publishes_under_the_same_grammar() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(its_rest::result_set(
            "SELECT synthetic",
            &[
                ("#0", "/ehr_id/value"),
                ("#1", "/uid/value"),
                ("#2", "/contribution/id/value"),
                ("#3", "/commit_audit/time_committed/value"),
                ("#4", "/commit_audit/change_type/defining_code/code_string"),
                ("#5", "/archetype_details/template_id/value"),
            ],
            &[vec![
                json!("7d44b88c-4199-4bad-97dc-d78268e01398"),
                json!("8849182c-82ad-4088-a07f-48ead4180515::synthetic.cdr::1"),
                json!("0d4a3c52-5b51-4b4b-9e0e-000000000001"),
                json!("2026-11-02T10:00:00Z"),
                json!("249"),
                json!("openEHR-EHR-COMPOSITION.encounter.v1"),
            ]],
        ))
        .mount(&server)
        .await;
    let directory = tempfile::tempdir()?;
    let store = Arc::new(WatermarkStore::open(
        &directory.path().join("watermarks.redb"),
    )?);
    let feed = AqlFeed::new(
        client(&server)?,
        Arc::clone(&store),
        Start::At(String::from("2026-11-02T09:00:00Z")),
    );
    let broker = Arc::new(Recording::default());
    let publisher = publisher(&broker);
    let mut cursor = Cursor::default();
    pass(&feed, &reader(), &publisher, limit(), &mut cursor).await?;
    let confirmed = broker.confirmed();
    let (key, body) = confirmed.first().ok_or("one message")?;
    assert_eq!("COMPOSITION.249.openEHR-EHR-COMPOSITION_encounter_v1", key);
    assert_eq!(json!(0), body["version_index"]);
    let watermark = store.get(&reader())?.ok_or("the watermark moved")?;
    assert_eq!("2026-11-02T10:00:00Z", watermark.text());
    Ok(())
}
