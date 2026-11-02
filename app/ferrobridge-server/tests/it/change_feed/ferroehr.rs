// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reference CDR's outbox feed: one case per documented answer of the
//! page and the acknowledgement, and the follower over each failure.

use std::error::Error;

use ferrobridge_server::change_feed::ChangeFeed;
use ferrobridge_server::change_feed::Domain;
use ferrobridge_server::change_feed::FeedError;
use ferrobridge_server::change_feed::PageSize;
use ferrobridge_server::change_feed::Position;
use ferrobridge_server::change_feed::Seq;
use ferrobridge_server::change_feed::Stream;
use ferrobridge_server::change_feed::ferroehr::FerroEhrFeed;
use ferrobridge_server::change_feed::follow::Cursor;
use ferrobridge_server::change_feed::follow::PassError;
use ferrobridge_server::change_feed::follow::pass;
use http::StatusCode;
use serde_json::json;
use wiremock::matchers::{body_json, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Recording;
use super::client;
use super::reader;

/// The extension root the stub serves the feed under.
const ROOT: &str = "/ferroehr/rest/extensions/v1";

/// The feed path of the test reader.
const FEED: &str = "/ferroehr/rest/extensions/v1/admin/outbox/ferrobridge-test";

/// The acknowledgement path of the test reader.
const ACK: &str = "/ferroehr/rest/extensions/v1/admin/outbox/ferrobridge-test/ack";

/// Returns the feed over `server`.
fn feed(server: &MockServer) -> Result<FerroEhrFeed, Box<dyn Error>> {
    Ok(FerroEhrFeed::new(
        client(server)?,
        format!("{}{ROOT}", server.uri()).parse()?,
    ))
}

/// Returns a synthetic envelope at `seq`.
fn envelope(seq: u64) -> serde_json::Value {
    json!({
        "seq": seq,
        "contribution_id": format!("0d4a3c52-5b51-4b4b-9e0e-{seq:012}"),
        "ehr_id": "7d44b88c-4199-4bad-97dc-d78268e01398",
        "committed_at": "2026-11-02T10:00:00Z",
        "versions": [{
            "vo_id": "8849182c-82ad-4088-a07f-48ead4180515",
            "kind": "COMPOSITION",
            "sys_version": 3,
            "change_type": "modification",
            "template_id": "Synthetic vital signs"
        }]
    })
}

/// Returns the `200` page of `events`.
fn page(events: &[serde_json::Value]) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .set_body_json(json!({ "reader": "ferrobridge-test", "events": events }))
}

/// Returns the page size every case asks for.
fn limit() -> PageSize {
    PageSize::new(2).expect("a page size in range")
}

/// Returns the position of `seq` in the clinical outbox.
fn clinical(seq: u64) -> Position {
    Position::Outbox {
        domain: Domain::Clinical,
        seq: Seq::new(seq),
    }
}

/// Mounts an empty party outbox on `server`.
async fn no_party_events(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path(FEED))
        .and(query_param("domain", "party"))
        .respond_with(page(&[]))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_page_reads_both_outboxes_each_in_its_own_stream() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FEED))
        .and(query_param("domain", "clinical"))
        .respond_with(page(&[envelope(41)]))
        .mount(&server)
        .await;
    let mut party = envelope(3);
    party["ehr_id"] = serde_json::Value::Null;
    Mock::given(method("GET"))
        .and(path(FEED))
        .and(query_param("domain", "party"))
        .respond_with(page(&[party]))
        .mount(&server)
        .await;
    for (domain, seq) in [("clinical", 41), ("party", 3)] {
        Mock::given(method("POST"))
            .and(path(ACK))
            .and(query_param("domain", domain))
            .and(body_json(json!({ "last_seq": seq })))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
    }
    let consumer = Recording::default();
    let mut cursor = Cursor::default();
    let done = pass(&feed(&server)?, &reader(), &consumer, limit(), &mut cursor).await?;
    assert_eq!(2, done.read);
    let party_position = Position::Outbox {
        domain: Domain::Party,
        seq: Seq::new(3),
    };
    assert_eq!(
        vec![clinical(41), party_position.clone()],
        done.acknowledged
    );
    assert_eq!(
        Some(&party_position),
        cursor.acknowledged(Stream::Outbox(Domain::Party))
    );
    Ok(())
}

#[tokio::test]
async fn an_erasure_tombstone_is_an_event_of_its_own() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FEED))
        .and(query_param("domain", "clinical"))
        .respond_with(page(&[json!({
            "seq": 57,
            "event": "erase",
            "ehr_id": "7d44b88c-4199-4bad-97dc-d78268e01398",
            "versions": [{ "kind": "EHR", "change_type": "erase" }]
        })]))
        .mount(&server)
        .await;
    no_party_events(&server).await;
    let read = feed(&server)?.page(&reader(), limit()).await?;
    let event = read.events.first().ok_or("the tombstone")?;
    assert!(event.erasure);
    assert_eq!(None, event.contribution_id);
    Ok(())
}

#[tokio::test]
async fn an_empty_feed_is_an_empty_page() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FEED))
        .and(query_param("domain", "clinical"))
        .and(query_param("limit", "2"))
        .and(header_exists("authorization"))
        .respond_with(page(&[]))
        .expect(1)
        .mount(&server)
        .await;
    no_party_events(&server).await;
    let read = feed(&server)?.page(&reader(), limit()).await?;
    assert!(read.events.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_page_reads_each_envelope_in_seq_order() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FEED))
        .and(query_param("domain", "clinical"))
        .respond_with(page(&[envelope(41), envelope(42)]))
        .mount(&server)
        .await;
    no_party_events(&server).await;
    let read = feed(&server)?.page(&reader(), limit()).await?;
    let seqs: Vec<Option<Seq>> = read
        .events
        .iter()
        .map(ferrobridge_server::change_feed::Event::seq)
        .collect();
    assert_eq!(vec![Some(Seq::new(41)), Some(Seq::new(42))], seqs);
    let first = read.events.first().ok_or("an event")?;
    assert_eq!(
        "7d44b88c-4199-4bad-97dc-d78268e01398",
        first.ehr_id.as_ref().ok_or("an EHR")?.as_str()
    );
    assert_eq!(
        "modification",
        first.versions.first().ok_or("a version")?.change_type
    );
    assert_eq!(Some(&json!(41)), first.envelope.get("seq"));
    Ok(())
}

#[tokio::test]
async fn an_acknowledgement_posts_the_last_seq_and_reads_the_204() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ACK))
        .and(query_param("domain", "clinical"))
        .and(body_json(json!({ "last_seq": 42 })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    feed(&server)?.acknowledge(&reader(), &clinical(42)).await?;
    Ok(())
}

#[tokio::test]
async fn an_unknown_reader_is_its_own_refusal() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FEED))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let refused = feed(&server)?.page(&reader(), limit()).await;
    assert!(
        matches!(refused, Err(FeedError::UnknownReader { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn an_acknowledgement_outside_the_cursor_range_is_a_conflict() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ACK))
        .respond_with(ResponseTemplate::new(409))
        .mount(&server)
        .await;
    let refused = feed(&server)?.acknowledge(&reader(), &clinical(7)).await;
    match refused {
        Err(FeedError::Conflict { last, .. }) => assert_eq!(clinical(7), *last),
        other => return Err(format!("expected a conflict, got {other:?}").into()),
    }
    Ok(())
}

#[tokio::test]
async fn a_refused_credential_and_a_server_failure_keep_their_status() -> Result<(), Box<dyn Error>>
{
    for (status, refused) in [(401_u16, true), (403, true), (500, false), (503, false)] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(FEED))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let error = feed(&server)?
            .page(&reader(), limit())
            .await
            .expect_err("the feed refuses");
        assert_eq!(
            Some(StatusCode::from_u16(status)?),
            error.status(),
            "{error:?}"
        );
        if refused {
            assert!(matches!(error, FeedError::Refused { .. }), "{error:?}");
        } else {
            assert!(matches!(error, FeedError::Unavailable { .. }), "{error:?}");
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_position_of_the_aql_source_is_refused_before_any_call() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    let foreign = Position::Committed(ferrobridge_server::change_feed::Committed::new(
        "2026-11-02T10:00:00Z".parse()?,
        String::from("2026-11-02T10:00:00Z"),
        std::collections::BTreeSet::new(),
    ));
    let refused = feed(&server)?.acknowledge(&reader(), &foreign).await;
    assert!(
        matches!(refused, Err(FeedError::Foreign { .. })),
        "{refused:?}"
    );
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn the_cursor_never_moves_on_a_failed_read() -> Result<(), Box<dyn Error>> {
    for status in [401_u16, 404, 500, 503] {
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
        let consumer = Recording::default();
        let mut cursor = Cursor::default();
        let failed = pass(&feed(&server)?, &reader(), &consumer, limit(), &mut cursor).await;
        assert!(
            matches!(failed, Err(PassError::Feed(_))),
            "{status}: {failed:?}"
        );
        assert_eq!(
            None,
            cursor.acknowledged(Stream::Outbox(Domain::Clinical)),
            "{status}"
        );
        assert_eq!(Vec::<Position>::new(), consumer.finished(), "{status}");
    }
    Ok(())
}

#[tokio::test]
async fn a_pass_acknowledges_only_the_prefix_the_consumer_finished() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FEED))
        .and(query_param("domain", "clinical"))
        .respond_with(page(&[envelope(41), envelope(42)]))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(ACK))
        .and(query_param("domain", "clinical"))
        .and(body_json(json!({ "last_seq": 41 })))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let consumer = Recording::failing_at(1);
    let mut cursor = Cursor::default();
    no_party_events(&server).await;
    let failed = pass(&feed(&server)?, &reader(), &consumer, limit(), &mut cursor).await;
    assert!(
        matches!(failed, Err(PassError::Consumer { .. })),
        "{failed:?}"
    );
    assert_eq!(
        Some(&clinical(41)),
        cursor.acknowledged(Stream::Outbox(Domain::Clinical))
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_acknowledgement_leaves_the_cursor_where_it_was() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FEED))
        .and(query_param("domain", "clinical"))
        .respond_with(page(&[envelope(41)]))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(ACK))
        .respond_with(ResponseTemplate::new(409))
        .mount(&server)
        .await;
    let consumer = Recording::default();
    let mut cursor = Cursor::default();
    no_party_events(&server).await;
    let failed = pass(&feed(&server)?, &reader(), &consumer, limit(), &mut cursor).await;
    assert!(
        matches!(failed, Err(PassError::Feed(FeedError::Conflict { .. }))),
        "{failed:?}"
    );
    assert_eq!(None, cursor.acknowledged(Stream::Outbox(Domain::Clinical)));
    Ok(())
}

#[tokio::test]
async fn a_page_out_of_seq_order_is_refused_before_any_event_is_offered()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(FEED))
        .and(query_param("domain", "clinical"))
        .respond_with(page(&[envelope(42), envelope(41)]))
        .mount(&server)
        .await;
    let consumer = Recording::default();
    let mut cursor = Cursor::default();
    no_party_events(&server).await;
    let failed = pass(&feed(&server)?, &reader(), &consumer, limit(), &mut cursor).await;
    assert!(
        matches!(failed, Err(PassError::Unordered { .. })),
        "{failed:?}"
    );
    assert_eq!(Vec::<Position>::new(), consumer.finished());
    Ok(())
}
