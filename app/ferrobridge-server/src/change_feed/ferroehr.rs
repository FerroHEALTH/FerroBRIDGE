// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reference CDR's outbox feed: `GET {root}/admin/outbox/{reader}` and
//! `POST {root}/admin/outbox/{reader}/ack`, reached with the `[cdr]`
//! credentials.
//!
//! The feed is the reference CDR's own extension outside ITS-REST, so no
//! specification governs it: our own design, written against the contract
//! the reference CDR publishes for it. It serves two outboxes, `clinical`
//! and `party` (`?domain=`), each with its own `seq` and its own cursor per
//! reader; a page reads both, because a reader that never reads one holds
//! that outbox's prune back. A `200` page lists the events after the
//! reader's cursor in `seq` order; a `204` acknowledgement moves the cursor;
//! a `404` names an unknown or inactive reader and a `409` an
//! acknowledgement below the cursor or above the last event. An erasure
//! tombstone (`"event": "erase"`) names what was erased and carries no
//! contribution and no commit instant.

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
use crate::change_feed::Domain;
use crate::change_feed::Event;
use crate::change_feed::FeedError;
use crate::change_feed::Page;
use crate::change_feed::PageSize;
use crate::change_feed::Position;
use crate::change_feed::ReaderName;
use crate::change_feed::Seq;

/// The `event` member of an erasure tombstone.
const ERASE: &str = "erase";

/// The outbox feed of one deployment of the reference CDR.
#[derive(Debug, Clone)]
pub struct FerroEhrFeed {
    /// The client whose engine and credentials every call goes through.
    cdr: CdrClient,
    /// The root of the reference CDR's extensions, the path `/admin/outbox`
    /// lives under.
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

    /// Returns up to `limit` events of the `domain` outbox after the cursor
    /// of `reader`.
    async fn outbox(
        &self,
        reader: &ReaderName,
        domain: Domain,
        limit: PageSize,
    ) -> Result<Vec<Event>, FeedError> {
        let mut request = Request::new(Method::GET, feed_path(reader));
        request.query("limit", limit.get());
        request.query("domain", domain.as_str());
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
        read_page(reader, domain, upstream.body())
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
        // NOTE: no specification governs this: our own design; each outbox
        // is read up to `limit`, so neither one waits behind the other.
        let mut events = self.outbox(reader, Domain::Clinical, limit).await?;
        events.extend(self.outbox(reader, Domain::Party, limit).await?);
        Ok(Page { events })
    }

    async fn acknowledge(&self, reader: &ReaderName, last: &Position) -> Result<(), FeedError> {
        let Position::Outbox { domain, seq } = *last else {
            return Err(FeedError::Foreign {
                position: last.clone(),
            });
        };
        let mut request = Request::new(Method::POST, format!("{}/ack", feed_path(reader)));
        request.query("domain", domain.as_str());
        request
            .json_body(&serde_json::json!({ "last_seq": seq.get() }), None)
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
                last: Box::new(last.clone()),
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
    /// `erase` on an erasure tombstone, absent on a commit.
    #[serde(default)]
    event: Option<String>,
    /// The contribution, absent on an erasure.
    #[serde(default)]
    contribution_id: Option<String>,
    /// The EHR, absent or `null` for a demographic event.
    #[serde(default)]
    ehr_id: Option<String>,
    /// The commit instant, RFC 3339, absent on an erasure.
    #[serde(default)]
    committed_at: Option<String>,
    /// The changed or erased objects.
    versions: Vec<WireVersion>,
}

/// One changed version on the wire.
#[derive(Debug, Deserialize)]
struct WireVersion {
    /// The versioned object, absent on the erasure of a whole EHR.
    #[serde(default)]
    vo_id: Option<String>,
    /// The RM type name.
    kind: String,
    /// The version ordinal, absent on an erasure.
    #[serde(default)]
    sys_version: Option<u64>,
    /// The version tree id.
    #[serde(default)]
    version_tree_id: Option<String>,
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
    /// A text, such as `251` or `erase`.
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

/// Returns the events of the `domain` outbox `body` carries for `reader`.
fn read_page(reader: &ReaderName, domain: Domain, body: &str) -> Result<Vec<Event>, FeedError> {
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
    wire.events
        .into_iter()
        .map(|envelope| read_event(domain, envelope))
        .collect()
}

/// Returns the event `envelope` of the `domain` outbox describes.
fn read_event(
    domain: Domain,
    envelope: serde_json::Map<String, serde_json::Value>,
) -> Result<Event, FeedError> {
    let wire: WireEvent = serde_json::from_value(serde_json::Value::Object(envelope.clone()))
        .map_err(|source| FeedError::Malformed {
            reason: String::from("an event is not an outbox envelope"),
            source: Some(source),
        })?;
    let seq = Seq::new(wire.seq);
    let malformed = |what: &str| FeedError::Malformed {
        reason: format!("{} event {seq} carries {what}", domain.as_str()),
        source: None,
    };
    let erasure = match wire.event.as_deref() {
        None => false,
        Some(ERASE) => true,
        Some(_) => return Err(malformed("an event kind other than erase")),
    };
    let contribution_id = wire
        .contribution_id
        .as_deref()
        .map(ContributionUid::new)
        .transpose()
        .map_err(|_refused| malformed("a contribution_id that is no identifier"))?;
    let ehr_id = wire
        .ehr_id
        .as_deref()
        .map(EhrId::new)
        .transpose()
        .map_err(|_refused| malformed("an ehr_id that is no identifier"))?;
    let committed_at = wire
        .committed_at
        .as_deref()
        .map(str::parse::<jiff::Timestamp>)
        .transpose()
        .map_err(|_refused| malformed("a committed_at that is no RFC 3339 instant"))?;
    if !erasure && (contribution_id.is_none() || committed_at.is_none()) {
        return Err(malformed(
            "a commit with no contribution_id or no committed_at",
        ));
    }
    if wire.versions.is_empty() {
        return Err(malformed("no version"));
    }
    let versions = wire
        .versions
        .into_iter()
        .map(|version| {
            let vo_id = version
                .vo_id
                .as_deref()
                .map(uuid::Uuid::parse_str)
                .transpose()
                .map_err(|_refused| malformed("a vo_id that is no UUID"))?;
            if !erasure && (vo_id.is_none() || version.sys_version.is_none()) {
                return Err(malformed("a version with no vo_id or no sys_version"));
            }
            Ok(ChangedVersion {
                vo_id,
                kind: version.kind,
                sys_version: version.sys_version,
                version_tree_id: version.version_tree_id,
                change_type: version.change_type.into_text(),
                template_id: version.template_id,
            })
        })
        .collect::<Result<Vec<ChangedVersion>, FeedError>>()?;
    Ok(Event {
        position: Position::Outbox { domain, seq },
        erasure,
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
    use crate::change_feed::Domain;
    use crate::change_feed::FeedError;
    use crate::change_feed::Position;
    use crate::change_feed::ReaderName;
    use crate::change_feed::Seq;

    fn reader() -> ReaderName {
        ReaderName::new("ferrobridge-events").expect("a valid reader name")
    }

    #[test]
    fn a_party_event_keeps_members_the_adapter_does_not_read() {
        let body = r#"{"reader":"ferrobridge-events","events":[{"seq":7,"contribution_id":"0d4a3c52-5b51-4b4b-9e0e-000000000001","ehr_id":null,"committed_at":"2026-11-02T10:00:00Z","versions":[{"vo_id":"0d4a3c52-5b51-4b4b-9e0e-000000000002","kind":"PERSON","sys_version":1,"version_tree_id":"1","change_type":"249","template_id":null}]}]}"#;
        let events = read_page(&reader(), Domain::Party, body).expect("a page");
        let event = events.first().expect("one event");
        assert_eq!(
            Position::Outbox {
                domain: Domain::Party,
                seq: Seq::new(7)
            },
            event.position
        );
        assert_eq!(None, event.ehr_id);
        let version = event.versions.first().expect("a version");
        assert_eq!("249", version.change_type);
        assert_eq!(Some("1"), version.version_tree_id.as_deref());
        assert!(event.envelope.contains_key("seq"));
    }

    #[test]
    fn an_erasure_tombstone_carries_no_contribution() {
        let body = r#"{"reader":"ferrobridge-events","events":[{"seq":57,"event":"erase","ehr_id":"7d44b88c-4199-4bad-97dc-d78268e01398","versions":[{"kind":"EHR","change_type":"erase"}]}]}"#;
        let events = read_page(&reader(), Domain::Clinical, body).expect("a page");
        let event = events.first().expect("one event");
        assert!(event.erasure);
        assert_eq!(None, event.contribution_id);
        assert_eq!(None, event.committed_at);
        let version = event.versions.first().expect("the erased EHR");
        assert_eq!((None, None), (version.vo_id, version.sys_version));
        assert_eq!("erase", version.change_type);
    }

    #[test]
    fn a_commit_without_its_contribution_or_an_unknown_event_kind_is_malformed() {
        let bodies = [
            r#"{"reader":"ferrobridge-events","events":[{"seq":1,"ehr_id":"e","committed_at":"2026-11-02T10:00:00Z","versions":[{"vo_id":"0d4a3c52-5b51-4b4b-9e0e-000000000002","kind":"COMPOSITION","sys_version":1,"change_type":"249"}]}]}"#,
            r#"{"reader":"ferrobridge-events","events":[{"seq":1,"event":"restrict","ehr_id":"e","versions":[{"kind":"EHR","change_type":"erase"}]}]}"#,
            r#"{"reader":"someone-else","events":[]}"#,
            r#"{"reader":"ferrobridge-events","events":[{"seq":1,"contribution_id":"c","ehr_id":"e","committed_at":"2026-11-02T10:00:00Z","versions":[]}]}"#,
        ];
        for body in bodies {
            assert!(
                matches!(
                    read_page(&reader(), Domain::Clinical, body),
                    Err(FeedError::Malformed { .. })
                ),
                "{body}"
            );
        }
    }
}
