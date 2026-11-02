// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vendor-neutral source: composition versions polled over ITS-REST 1.1.0
//! `/query/aql` by commit instant, past a watermark the bridge keeps itself.
//!
//! The query runs over `VERSION v[ALL_VERSIONS]` (QUERY AQL 1.1.0, grammar
//! `versionPredicate`), so every committed version answers, and reads each
//! version's `uid`, `contribution` and `commit_audit` (openEHR RM Common
//! 1.1.0 §6, `VERSION` and `AUDIT_DETAILS`). It is ordered by
//! `time_committed` and then `uid`, since a `LIMIT`ed read is deterministic
//! only with a unique order (QUERY AQL 1.1.0, the `LIMIT` clause).
//!
//! Every event is one composition version. The watermark is the last
//! finished instant with the uids finished at it ([`Committed`]), so a
//! version that shares its instant with a finished one is still read. ITS-REST
//! defines no change feed, so the rest is our own design, with three limits:
//! a change is seen one poll late at most; versions of different EHRs come in
//! commit-instant order, which is not commit order when instants tie or
//! clocks differ; and a deletion is seen only when the CDR answers the
//! deleted version under `CONTAINS COMPOSITION`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;

use openehr_its::rest::generated::query::AdhocQueryExecute;
use openehr_its::rest::generated::query::client::QueryExecuteAdhocQueryBodyOutcome;

use crate::cdr::CdrClient;
use crate::cdr::ids::ContributionUid;
use crate::cdr::ids::EhrId;
use crate::change_feed::ChangeFeed;
use crate::change_feed::ChangedVersion;
use crate::change_feed::Committed;
use crate::change_feed::Event;
use crate::change_feed::FeedError;
use crate::change_feed::Page;
use crate::change_feed::PageSize;
use crate::change_feed::Position;
use crate::change_feed::ReaderName;
use crate::change_feed::watermark::WatermarkStore;

/// The projection every query of the source reads, in column order.
const SELECT: &str = "SELECT e/ehr_id/value, v/uid/value, v/contribution/id/value, \
     v/commit_audit/time_committed/value, v/commit_audit/change_type/defining_code/code_string, \
     c/archetype_details/template_id/value \
     FROM EHR e CONTAINS VERSION v[ALL_VERSIONS] CONTAINS COMPOSITION c";

/// The order every query of the source reads in.
const ORDER: &str = " ORDER BY v/commit_audit/time_committed/value ASC, v/uid/value ASC";

/// The parameter the watermark instant is bound to.
const SINCE: &str = "since";

/// The RM type every event of this source names.
const COMPOSITION: &str = "COMPOSITION";

/// Where a reader with no stored watermark starts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Start {
    /// At the instant of its first read: nothing committed before is offered.
    #[default]
    Now,
    /// At the first committed version.
    Beginning,
    /// At this instant, as RFC 3339 text.
    At(String),
}

/// The vendor-neutral source over one CDR.
#[derive(Debug, Clone)]
pub struct AqlFeed {
    /// The client the queries go through.
    cdr: CdrClient,
    /// The watermarks.
    store: Arc<WatermarkStore>,
    /// Where a reader with no watermark starts.
    start: Start,
}

impl AqlFeed {
    /// Returns the source over `cdr`, keeping its watermarks in `store`.
    #[must_use]
    pub const fn new(cdr: CdrClient, store: Arc<WatermarkStore>, start: Start) -> Self {
        Self { cdr, store, start }
    }

    /// Returns the watermark of `reader`, recording the start of a reader
    /// that has none and does not start at the beginning.
    fn watermark(&self, reader: &ReaderName) -> Result<Option<Committed>, FeedError> {
        if let Some(stored) = self.store.get(reader)? {
            return Ok(Some(stored));
        }
        let text = match &self.start {
            Start::Beginning => return Ok(None),
            Start::At(text) => text.clone(),
            Start::Now => jiff::Timestamp::now().to_string(),
        };
        let at = text
            .parse::<jiff::Timestamp>()
            .map_err(|_refused| FeedError::Malformed {
                reason: format!("the start of reader {reader} names no instant"),
                source: None,
            })?;
        let start = Committed::new(at, text, BTreeSet::new());
        self.store.set(reader, &start)?;
        Ok(Some(start))
    }
}

#[async_trait::async_trait]
impl ChangeFeed for AqlFeed {
    async fn page(&self, reader: &ReaderName, limit: PageSize) -> Result<Page, FeedError> {
        let watermark = self.watermark(reader)?;
        let seen = watermark.as_ref().map_or(0, |at| at.versions().len());
        // NOTE: no specification governs this: our own design; the versions
        // finished at the watermark instant answer again, so the read asks for them too.
        let fetch = i64::from(limit.get()).saturating_add(i64::try_from(seen).unwrap_or(i64::MAX));
        let request = AdhocQueryExecute {
            q: match watermark {
                Some(_) => {
                    format!("{SELECT} WHERE v/commit_audit/time_committed/value >= ${SINCE}{ORDER}")
                }
                None => format!("{SELECT}{ORDER}"),
            },
            offset: Some(0),
            fetch: Some(fetch),
            query_parameters: watermark.as_ref().map(|at| {
                BTreeMap::from([(
                    String::from(SINCE),
                    serde_json::Value::String(at.text().to_owned()),
                )])
            }),
            additional_properties: BTreeMap::new(),
        };
        let answered = self
            .cdr
            .query_aql(&request)
            .await
            .map_err(|source| match source {
                crate::cdr::error::CdrError::Client {
                    upstream: Some(upstream),
                    ..
                } if is_refusal(upstream.status()) => FeedError::Refused { upstream },
                crate::cdr::error::CdrError::Client {
                    upstream: Some(upstream),
                    ..
                } if upstream.status().is_server_error() => FeedError::Unavailable { upstream },
                source => FeedError::Cdr { source },
            })?;
        let rows = match answered.outcome {
            QueryExecuteAdhocQueryBodyOutcome::Ok { body, .. } => body.rows,
            QueryExecuteAdhocQueryBodyOutcome::BadRequest { .. }
            | QueryExecuteAdhocQueryBodyOutcome::RequestTimeout { .. } => {
                return Err(FeedError::Query {
                    upstream: Box::new(answered.upstream),
                });
            }
        };
        let mut events = Vec::new();
        let mut position = watermark;
        for row in rows {
            let read = read_row(&row)?;
            if let Some(at) = position.as_ref() {
                if read.at < at.at() {
                    return Err(FeedError::Malformed {
                        reason: String::from(
                            "the CDR answered a version committed before the one it answered last",
                        ),
                        source: None,
                    });
                }
                if read.at == at.at() && at.versions().contains(&read.version_uid) {
                    continue;
                }
            }
            if events.len() >= usize::from(limit.get()) {
                break;
            }
            let next = match position.take() {
                Some(at) if read.at == at.at() => {
                    let mut versions = at.versions().clone();
                    versions.insert(read.version_uid.clone());
                    Committed::new(at.at(), at.text().to_owned(), versions)
                }
                _ => Committed::new(
                    read.at,
                    read.at_text.clone(),
                    BTreeSet::from([read.version_uid.clone()]),
                ),
            };
            events.push(read.event(Position::Committed(next.clone())));
            position = Some(next);
        }
        Ok(Page { events })
    }

    async fn acknowledge(&self, reader: &ReaderName, last: &Position) -> Result<(), FeedError> {
        match last {
            Position::Committed(watermark) => self.store.set(reader, watermark),
            Position::Seq(_) => Err(FeedError::Foreign {
                position: last.clone(),
            }),
        }
    }
}

/// Returns whether `status` refuses the credentials.
fn is_refusal(status: http::StatusCode) -> bool {
    status == http::StatusCode::UNAUTHORIZED || status == http::StatusCode::FORBIDDEN
}

/// One row of the source's query, read.
#[derive(Debug)]
struct Row {
    /// The EHR.
    ehr_id: EhrId,
    /// The version uid, as the CDR wrote it.
    version_uid: String,
    /// The versioned object.
    vo_id: uuid::Uuid,
    /// The trunk number of the version tree id.
    sys_version: u64,
    /// The contribution.
    contribution_id: ContributionUid,
    /// The commit instant.
    at: jiff::Timestamp,
    /// The commit instant as the CDR wrote it.
    at_text: String,
    /// The audit change type code.
    change_type: String,
    /// The template.
    template_id: Option<String>,
}

impl Row {
    /// Returns the event this row is, at `position`.
    fn event(self, position: Position) -> Event {
        let mut envelope = serde_json::Map::new();
        envelope.insert(
            String::from("contribution_id"),
            serde_json::Value::from(self.contribution_id.as_str()),
        );
        envelope.insert(
            String::from("ehr_id"),
            serde_json::Value::from(self.ehr_id.as_str()),
        );
        envelope.insert(
            String::from("committed_at"),
            serde_json::Value::from(self.at_text.as_str()),
        );
        let version = serde_json::json!({
                "vo_id": self.vo_id.to_string(),
                "kind": COMPOSITION,
                "sys_version": self.sys_version,
                "change_type": self.change_type,
            "template_id": self.template_id,
        });
        envelope.insert(
            String::from("versions"),
            serde_json::Value::Array(vec![version]),
        );
        Event {
            position,
            contribution_id: self.contribution_id,
            ehr_id: Some(self.ehr_id),
            committed_at: self.at,
            versions: vec![ChangedVersion {
                vo_id: self.vo_id,
                kind: String::from(COMPOSITION),
                sys_version: self.sys_version,
                change_type: self.change_type,
                template_id: self.template_id,
            }],
            envelope,
        }
    }
}

/// Returns the text of column `index` of `row`, refusing a missing cell.
fn text<'r>(row: &'r [serde_json::Value], index: usize, what: &str) -> Result<&'r str, FeedError> {
    row.get(index)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| FeedError::Malformed {
            reason: format!("a row carries no {what}"),
            source: None,
        })
}

/// Returns the row `cells` holds.
fn read_row(cells: &[serde_json::Value]) -> Result<Row, FeedError> {
    let malformed = |reason: &str| FeedError::Malformed {
        reason: reason.to_owned(),
        source: None,
    };
    let ehr_id = EhrId::new(text(cells, 0, "ehr_id")?)
        .map_err(|_refused| malformed("a row carries an ehr_id that is no identifier"))?;
    let version_uid = text(cells, 1, "version uid")?.to_owned();
    let mut parts = version_uid.split("::");
    let vo_id = parts
        .next()
        .and_then(|object| uuid::Uuid::parse_str(object).ok())
        .ok_or_else(|| malformed("a row carries a version uid that names no UUID"))?;
    let sys_version = parts
        .nth(1)
        .and_then(|tree| tree.split('.').next())
        .and_then(|trunk| trunk.parse::<u64>().ok())
        .ok_or_else(|| malformed("a row carries a version uid that names no version tree id"))?;
    let contribution_id = ContributionUid::new(text(cells, 2, "contribution id")?)
        .map_err(|_refused| malformed("a row carries a contribution id that is no identifier"))?;
    let at_text = text(cells, 3, "time_committed")?.to_owned();
    let at = at_text
        .parse::<jiff::Timestamp>()
        .map_err(|_refused| malformed("a row carries a time_committed with no offset"))?;
    let change_type = text(cells, 4, "change type")?.to_owned();
    let template_id = cells
        .get(5)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok(Row {
        ehr_id,
        version_uid,
        vo_id,
        sys_version,
        contribution_id,
        at,
        at_text,
        change_type,
        template_id,
    })
}

#[cfg(test)]
mod tests {
    use super::read_row;
    use serde_json::json;

    #[test]
    fn a_row_names_its_versioned_object_and_trunk_version() {
        let row = read_row(&[
            json!("7d44b88c-4199-4bad-97dc-d78268e01398"),
            json!("8849182c-82ad-4088-a07f-48ead4180515::synthetic.cdr::3"),
            json!("0d4a3c52-5b51-4b4b-9e0e-000000000001"),
            json!("2026-11-02T10:00:00.123+01:00"),
            json!("251"),
            json!(null),
        ])
        .expect("a row");
        assert_eq!(3, row.sys_version);
        assert_eq!(
            "8849182c-82ad-4088-a07f-48ead4180515",
            row.vo_id.to_string()
        );
        assert_eq!(None, row.template_id);
    }

    #[test]
    fn a_row_whose_instant_has_no_offset_is_malformed() {
        let refused = read_row(&[
            json!("7d44b88c-4199-4bad-97dc-d78268e01398"),
            json!("8849182c-82ad-4088-a07f-48ead4180515::synthetic.cdr::1"),
            json!("0d4a3c52-5b51-4b4b-9e0e-000000000001"),
            json!("2026-11-02T10:00:00"),
            json!("249"),
            json!("Synthetic vital signs"),
        ]);
        assert!(refused.is_err());
    }
}
