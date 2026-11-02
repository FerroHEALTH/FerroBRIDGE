// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vendor-neutral source over ITS-REST 1.1.0 `/query/aql`: the query it
//! sends past the watermark, the events it reads from a result set, the
//! watermark it keeps across a restart, the versions that share an instant
//! with the watermark, and the refusals that leave the watermark alone.

use std::collections::BTreeSet;
use std::error::Error;
use std::sync::Arc;

use ferrobridge_server::cdr::ids::ContributionUid;
use ferrobridge_server::change_feed::ChangeFeed;
use ferrobridge_server::change_feed::Committed;
use ferrobridge_server::change_feed::FeedError;
use ferrobridge_server::change_feed::PageSize;
use ferrobridge_server::change_feed::Position;
use ferrobridge_server::change_feed::Stream;
use ferrobridge_server::change_feed::aql::AqlFeed;
use ferrobridge_server::change_feed::aql::Start;
use ferrobridge_server::change_feed::follow::Cursor;
use ferrobridge_server::change_feed::follow::PassError;
use ferrobridge_server::change_feed::follow::pass;
use ferrobridge_server::change_feed::watermark::WatermarkStore;
use ferrobridge_testkit::stubs::its_rest;
use serde_json::json;
use wiremock::matchers::{body_partial_json, body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Recording;
use super::client;
use super::reader;

/// The query route.
const QUERY: &str = "/v1/query/aql";

/// The instant the readers of these cases start at.
const START: &str = "2026-11-02T09:00:00Z";

/// The columns of the source's projection.
const COLUMNS: [(&str, &str); 6] = [
    ("#0", "/ehr_id/value"),
    ("#1", "/uid/value"),
    ("#2", "/contribution/id/value"),
    ("#3", "/commit_audit/time_committed/value"),
    ("#4", "/commit_audit/change_type/defining_code/code_string"),
    ("#5", "/archetype_details/template_id/value"),
];

/// Returns a synthetic row of the version `object` at trunk version
/// `version`, committed at `at`.
fn row(object: u32, version: u32, at: &str) -> Vec<serde_json::Value> {
    vec![
        json!("7d44b88c-4199-4bad-97dc-d78268e01398"),
        json!(format!(
            "8849182c-82ad-4088-a07f-{object:012}::synthetic.cdr::{version}"
        )),
        json!(format!("0d4a3c52-5b51-4b4b-9e0e-{object:012}")),
        json!(at),
        json!(if version == 1 { "249" } else { "251" }),
        json!("Synthetic vital signs"),
    ]
}

/// Returns the result set of `rows`.
fn answer(rows: &[Vec<serde_json::Value>]) -> ResponseTemplate {
    its_rest::result_set("SELECT synthetic", &COLUMNS, rows)
}

/// Returns a source over `server` keeping its watermarks in `store`.
fn feed(
    server: &MockServer,
    store: Arc<WatermarkStore>,
    start: Start,
) -> Result<AqlFeed, Box<dyn Error>> {
    Ok(AqlFeed::new(client(server)?, store, start))
}

/// Returns a store in a fresh directory, with the directory.
fn store() -> Result<(tempfile::TempDir, Arc<WatermarkStore>), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let store = WatermarkStore::open(&directory.path().join("watermarks.redb"))?;
    Ok((directory, Arc::new(store)))
}

/// Returns the page size every case asks for.
fn limit() -> PageSize {
    PageSize::new(3).expect("a page size in range")
}

#[tokio::test]
async fn a_first_read_binds_the_start_and_records_it_as_the_watermark() -> Result<(), Box<dyn Error>>
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(QUERY))
        .and(body_string_contains(
            "FROM EHR e CONTAINS VERSION v[ALL_VERSIONS] CONTAINS COMPOSITION c",
        ))
        .and(body_string_contains(
            "WHERE v/commit_audit/time_committed/value >= $since",
        ))
        .and(body_string_contains(
            "ORDER BY v/commit_audit/time_committed/value ASC, v/uid/value ASC",
        ))
        .and(body_partial_json(
            json!({ "query_parameters": { "since": START }, "fetch": 3, "offset": 0 }),
        ))
        .respond_with(answer(&[]))
        .expect(1)
        .mount(&server)
        .await;
    let (_directory, store) = store()?;
    let read = feed(&server, Arc::clone(&store), Start::At(String::from(START)))?
        .page(&reader(), limit())
        .await?;
    assert!(read.events.is_empty());
    let watermark = store.get(&reader())?.ok_or("the start is recorded")?;
    assert_eq!(START, watermark.text());
    assert!(watermark.versions().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_reader_that_starts_at_the_beginning_sends_no_window() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(QUERY))
        .respond_with(answer(&[]))
        .mount(&server)
        .await;
    let (_directory, store) = store()?;
    feed(&server, Arc::clone(&store), Start::Beginning)?
        .page(&reader(), limit())
        .await?;
    let requests = server.received_requests().await.ok_or("recorded")?;
    let body: serde_json::Value =
        serde_json::from_slice(&requests.first().ok_or("one query")?.body)?;
    let query = body["q"].as_str().ok_or("a query")?;
    assert!(!query.contains("WHERE"), "{query}");
    assert!(body.get("query_parameters").is_none(), "{body}");
    assert_eq!(None, store.get(&reader())?);
    Ok(())
}

#[tokio::test]
async fn each_row_is_one_composition_event_with_the_versions_at_its_instant()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(QUERY))
        .respond_with(answer(&[
            row(1, 1, "2026-11-02T10:00:00Z"),
            row(2, 3, "2026-11-02T10:00:00Z"),
            row(3, 1, "2026-11-02T10:00:01Z"),
        ]))
        .mount(&server)
        .await;
    let (_directory, store) = store()?;
    let read = feed(&server, store, Start::At(String::from(START)))?
        .page(&reader(), limit())
        .await?;
    assert_eq!(3, read.events.len());
    let second = read.events.get(1).ok_or("a second event")?;
    let version = second.versions.first().ok_or("a version")?;
    assert_eq!("COMPOSITION", version.kind);
    assert_eq!(Some(3), version.sys_version);
    assert_eq!(Some("3"), version.version_tree_id.as_deref());
    assert_eq!("251", version.change_type);
    assert_eq!(
        Some("Synthetic vital signs"),
        version.template_id.as_deref()
    );
    assert_eq!(None, second.seq());
    match &second.position {
        Position::Committed(at) => {
            assert_eq!("2026-11-02T10:00:00Z", at.text());
            assert_eq!(
                2,
                at.versions().len(),
                "both versions of the instant are finished here"
            );
        }
        Position::Outbox { .. } => return Err("an AQL event has no seq".into()),
    }
    match &read.events.get(2).ok_or("a third event")?.position {
        Position::Committed(at) => assert_eq!(1, at.versions().len()),
        Position::Outbox { .. } => return Err("an AQL event has no seq".into()),
    }
    Ok(())
}

#[tokio::test]
async fn the_watermark_survives_a_restart_and_a_version_sharing_its_instant_is_still_read()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    let (directory, store) = store()?;
    let boundary = "2026-11-02T10:00:00Z";
    let finished = row(1, 1, boundary);
    Mock::given(method("POST"))
        .and(path(QUERY))
        .respond_with(answer(std::slice::from_ref(&finished)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let consumer = Recording::default();
    let mut cursor = Cursor::default();
    pass(
        &feed(&server, Arc::clone(&store), Start::At(String::from(START)))?,
        &reader(),
        &consumer,
        limit(),
        &mut cursor,
    )
    .await?;
    drop(store);

    // A version committed at the same instant arrives after the pass.
    let late = row(0, 1, boundary);
    Mock::given(method("POST"))
        .and(path(QUERY))
        .and(body_partial_json(
            json!({ "query_parameters": { "since": boundary }, "fetch": 4 }),
        ))
        .respond_with(answer(&[late.clone(), finished]))
        .expect(1)
        .mount(&server)
        .await;
    let reopened = Arc::new(WatermarkStore::open(
        &directory.path().join("watermarks.redb"),
    )?);
    let read = feed(&server, reopened, Start::At(String::from(START)))?
        .page(&reader(), limit())
        .await?;
    assert_eq!(
        1,
        read.events.len(),
        "the finished version is not offered again"
    );
    let event = read.events.first().ok_or("the late version")?;
    assert_eq!(
        late.get(2).and_then(serde_json::Value::as_str),
        event.contribution_id.as_ref().map(ContributionUid::as_str)
    );
    Ok(())
}

#[tokio::test]
async fn an_acknowledgement_never_moves_the_watermark_backwards() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    let (_directory, store) = store()?;
    let source = feed(&server, Arc::clone(&store), Start::At(String::from(START)))?;
    let later = Committed::new(
        "2026-11-02T11:00:00Z".parse()?,
        String::from("2026-11-02T11:00:00Z"),
        BTreeSet::from([String::from("a::synthetic.cdr::1")]),
    );
    source
        .acknowledge(&reader(), &Position::Committed(later.clone()))
        .await?;
    let earlier = Committed::new(
        "2026-11-02T10:00:00Z".parse()?,
        String::from("2026-11-02T10:00:00Z"),
        BTreeSet::new(),
    );
    let refused = source
        .acknowledge(&reader(), &Position::Committed(earlier))
        .await;
    assert!(
        matches!(refused, Err(FeedError::Behind { .. })),
        "{refused:?}"
    );
    assert_eq!(Some(later), store.get(&reader())?);
    Ok(())
}

#[tokio::test]
async fn each_refusal_of_the_query_leaves_the_watermark_where_it_was() -> Result<(), Box<dyn Error>>
{
    for status in [400_u16, 401, 403, 408, 500, 503] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(QUERY))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let (_directory, store) = store()?;
        let consumer = Recording::default();
        let mut cursor = Cursor::default();
        let failed = pass(
            &feed(&server, Arc::clone(&store), Start::At(String::from(START)))?,
            &reader(),
            &consumer,
            limit(),
            &mut cursor,
        )
        .await;
        let Err(PassError::Feed(error)) = failed else {
            return Err(format!("{status}: expected a feed refusal, got {failed:?}").into());
        };
        match status {
            400 | 408 => assert!(
                matches!(error, FeedError::Query { .. }),
                "{status}: {error:?}"
            ),
            401 | 403 => assert!(
                matches!(error, FeedError::Refused { .. }),
                "{status}: {error:?}"
            ),
            _ => assert!(
                matches!(error, FeedError::Unavailable { .. }),
                "{status}: {error:?}"
            ),
        }
        let watermark = store.get(&reader())?.ok_or("the start is recorded")?;
        assert_eq!(START, watermark.text(), "{status}");
        assert_eq!(None, cursor.acknowledged(Stream::Committed), "{status}");
    }
    Ok(())
}

#[tokio::test]
async fn a_row_before_the_watermark_is_a_malformed_answer() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(QUERY))
        .respond_with(answer(&[row(1, 1, "2026-11-02T08:00:00Z")]))
        .mount(&server)
        .await;
    let (_directory, store) = store()?;
    let refused = feed(&server, store, Start::At(String::from(START)))?
        .page(&reader(), limit())
        .await;
    assert!(
        matches!(refused, Err(FeedError::Malformed { .. })),
        "{refused:?}"
    );
    Ok(())
}
