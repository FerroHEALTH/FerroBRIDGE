// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reference CDR's outbox feed: `GET {root}/admin/outbox/{reader}` and
//! `POST {root}/admin/outbox/{reader}/ack`, reached with the `[cdr]`
//! credentials.
//!
//! The feed is the reference CDR's own extension outside ITS-REST, so no specification
//! governs it: our own design, written against the contract the reference CDR
//! publishes for it. A `200` page lists the events after the reader's cursor
//! in `seq` order; a `204` acknowledgement moves the cursor; a `404` names an
//! unknown or inactive reader and a `409` an acknowledgement below the cursor
//! or above the last event.

use http::Method;
use http::StatusCode;
use openehr_its::rest::client::Request;
use openehr_its::rest::client::path_segment;
use serde::Deserialize;

use crate::cdr::CdrClient;
use crate::cdr::error::Upstream;
use crate::cdr::ids::ContributionUid;
use crate::cdr::ids::EhrId;
use crate::change_feed::ChangeFeed;
use crate::change_feed::ChangedVersion;
use crate::change_feed::Event;
use crate::change_feed::FeedError;
use crate::change_feed::Page;
use crate::change_feed::PageSize;
use crate::change_feed::Position;
use crate::change_feed::ReaderName;
use crate::change_feed::Seq;

/// The outbox feed of one deployment of the reference CDR.
#[derive(Debug, Clone)]
pub struct FerroEhrFeed {
    /// The client whose engine and credentials every call goes through.
    cdr: CdrClient,
    /// The root of the reference CDR's extensions, the path `/admin/outbox` lives
    /// under.
    root: url::Url,
}

impl FerroEhrFeed {
    /// Returns the feed served under `root`, reached through `cdr`.
    #[must_use]
    pub const fn new(cdr: CdrClient, root: url::Url) -> Self {
        Self { cdr, root }
    }

    /// Returns the root the feed is served under.
    #[must_use]
    pub const fn root(&self) -> &url::Url {
        &self.root
    }

    /// Sends `request` and returns the answer.
    async fn send(&self, request: Request) -> Result<Upstream, FeedError> {
        self.cdr
            .extension(&self.root, request)
            .await
            .map_err(|source| FeedError::Cdr { source })
    }
}

/// Returns the path of the feed of `reader`.
fn feed_path(reader: &ReaderName) -> String {
    format!("/admin/outbox/{}", path_segment(&reader.as_str()))
}

/// Returns the refusal a status every feed call shares means, or the answer
/// back for a status the call reads itself.
fn shared_refusal(upstream: Upstream) -> Result<Upstream, FeedError> {
    let status = upstream.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(FeedError::Refused {
            upstream: Box::new(upstream),
        });
    }
    if status.is_server_error() {
        return Err(FeedError::Unavailable {
            upstream: Box::new(upstream),
        });
    }
    Ok(upstream)
}

#[async_trait::async_trait]
impl ChangeFeed for FerroEhrFeed {
    async fn page(&self, reader: &ReaderName, limit: PageSize) -> Result<Page, FeedError> {
        let mut request = Request::new(Method::GET, feed_path(reader));
        request.query("limit", limit.get());
        let upstream = shared_refusal(self.send(request).await?)?;
        let status = upstream.status();
        if status == StatusCode::NOT_FOUND {
            return Err(FeedError::UnknownReader {
                reader: reader.clone(),
                upstream: Box::new(upstream),
            });
        }
        if status != StatusCode::OK {
            return Err(FeedError::Undocumented {
                status,
                upstream: Box::new(upstream),
            });
        }
        read_page(reader, upstream.body())
    }

    async fn acknowledge(&self, reader: &ReaderName, last: &Position) -> Result<(), FeedError> {
        let Position::Seq(last) = *last else {
            return Err(FeedError::Foreign {
                position: last.clone(),
            });
        };
        let mut request = Request::new(Method::POST, format!("{}/ack", feed_path(reader)));
        request
            .json_body(&serde_json::json!({ "last_seq": last.get() }), None)
            .map_err(|source| FeedError::Cdr {
                source: crate::cdr::error::CdrError::client(source, None),
            })?;
        let upstream = shared_refusal(self.send(request).await?)?;
        let status = upstream.status();
        if status == StatusCode::NO_CONTENT {
            return Ok(());
        }
        if status == StatusCode::NOT_FOUND {
            return Err(FeedError::UnknownReader {
                reader: reader.clone(),
                upstream: Box::new(upstream),
            });
        }
        if status == StatusCode::CONFLICT {
            return Err(FeedError::Conflict {
                reader: reader.clone(),
                last,
                upstream: Box::new(upstream),
            });
        }
        Err(FeedError::Undocumented {
            status,
            upstream: Box::new(upstream),
        })
    }
}

/// The page body.
#[derive(Debug, Deserialize)]
struct WirePage {
    /// The reader the page was read for.
    reader: String,
    /// The events, each read on its own so its raw members stay.
    events: Vec<serde_json::Map<String, serde_json::Value>>,
}

/// The members of one event the adapter reads.
#[derive(Debug, Deserialize)]
struct WireEvent {
    /// The delivery sequence number.
    seq: u64,
    /// The contribution.
    contribution_id: String,
    /// The EHR, absent or `null` for a demographic contribution.
    #[serde(default)]
    ehr_id: Option<String>,
    /// The commit instant, RFC 3339.
    committed_at: String,
    /// The changed versions.
    versions: Vec<WireVersion>,
}

/// One changed version on the wire.
#[derive(Debug, Deserialize)]
struct WireVersion {
    /// The versioned object.
    vo_id: String,
    /// The RM type name.
    kind: String,
    /// The version ordinal.
    sys_version: u64,
    /// The audit change type, a code the source writes as text or a number.
    change_type: ChangeType,
    /// The template, or `null`.
    #[serde(default)]
    template_id: Option<String>,
}

/// An audit change type as the source writes it.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ChangeType {
    /// A text, such as `modification` or `251`.
    Text(String),
    /// A bare number, such as `251`.
    Code(u64),
}

impl ChangeType {
    /// Returns the change type as one routing token.
    fn into_text(self) -> String {
        match self {
            Self::Text(text) => text,
            Self::Code(code) => code.to_string(),
        }
    }
}

/// Returns the page `body` carries for `reader`.
fn read_page(reader: &ReaderName, body: &str) -> Result<Page, FeedError> {
    let wire: WirePage = serde_json::from_str(body).map_err(|source| FeedError::Malformed {
        reason: String::from("the body is not a page"),
        source: Some(source),
    })?;
    if wire.reader != reader.as_str() {
        return Err(FeedError::Malformed {
            reason: format!("the page was read for reader {reader} and names another reader"),
            source: None,
        });
    }
    let events = wire
        .events
        .into_iter()
        .map(read_event)
        .collect::<Result<Vec<Event>, FeedError>>()?;
    Ok(Page { events })
}

/// Returns the event `envelope` describes.
fn read_event(envelope: serde_json::Map<String, serde_json::Value>) -> Result<Event, FeedError> {
    let wire: WireEvent = serde_json::from_value(serde_json::Value::Object(envelope.clone()))
        .map_err(|source| FeedError::Malformed {
            reason: String::from("an event is not an outbox envelope"),
            source: Some(source),
        })?;
    let seq = Seq::new(wire.seq);
    let malformed = |what: &str| FeedError::Malformed {
        reason: format!("event {seq} carries {what}"),
        source: None,
    };
    let contribution_id = ContributionUid::new(&wire.contribution_id)
        .map_err(|_refused| malformed("a contribution_id that is no identifier"))?;
    let ehr_id = wire
        .ehr_id
        .as_deref()
        .map(EhrId::new)
        .transpose()
        .map_err(|_refused| malformed("an ehr_id that is no identifier"))?;
    let committed_at = wire
        .committed_at
        .parse::<jiff::Timestamp>()
        .map_err(|_refused| malformed("a committed_at that is no RFC 3339 instant"))?;
    if wire.versions.is_empty() {
        return Err(malformed("no version"));
    }
    let versions = wire
        .versions
        .into_iter()
        .map(|version| {
            Ok(ChangedVersion {
                vo_id: uuid::Uuid::parse_str(&version.vo_id)
                    .map_err(|_refused| malformed("a vo_id that is no UUID"))?,
                kind: version.kind,
                sys_version: version.sys_version,
                change_type: version.change_type.into_text(),
                template_id: version.template_id,
            })
        })
        .collect::<Result<Vec<ChangedVersion>, FeedError>>()?;
    Ok(Event {
        position: Position::Seq(seq),
        contribution_id,
        ehr_id,
        committed_at,
        versions,
        envelope,
    })
}

#[cfg(test)]
mod tests {
    use super::read_page;
    use crate::change_feed::FeedError;
    use crate::change_feed::ReaderName;

    fn reader() -> ReaderName {
        ReaderName::new("ferrobridge-events").expect("a valid reader name")
    }

    #[test]
    fn an_event_keeps_members_the_adapter_does_not_read() {
        let body = r#"{"reader":"ferrobridge-events","events":[{"seq":7,"contribution_id":"0d4a3c52-5b51-4b4b-9e0e-000000000001","ehr_id":null,"committed_at":"2026-11-02T10:00:00Z","versions":[{"vo_id":"0d4a3c52-5b51-4b4b-9e0e-000000000002","kind":"PERSON","sys_version":1,"version_tree_id":"1","change_type":249,"template_id":null}]}]}"#;
        let page = read_page(&reader(), body).expect("a page");
        let event = page.events.first().expect("one event");
        assert_eq!(None, event.ehr_id);
        assert_eq!(
            "249",
            event.versions.first().expect("a version").change_type
        );
        assert!(event.envelope.contains_key("seq"));
        let version = event
            .envelope
            .get("versions")
            .and_then(|versions| versions.get(0))
            .expect("the raw version");
        assert!(version.get("version_tree_id").is_some());
    }

    #[test]
    fn a_page_for_another_reader_or_an_event_without_versions_is_malformed() {
        let other = r#"{"reader":"someone-else","events":[]}"#;
        assert!(matches!(
            read_page(&reader(), other),
            Err(FeedError::Malformed { .. })
        ));
        let empty = r#"{"reader":"ferrobridge-events","events":[{"seq":1,"contribution_id":"c","ehr_id":"e","committed_at":"2026-11-02T10:00:00Z","versions":[]}]}"#;
        assert!(matches!(
            read_page(&reader(), empty),
            Err(FeedError::Malformed { .. })
        ));
    }
}
