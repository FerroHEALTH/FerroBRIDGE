// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The change-feed adapter: the commit events a CDR serves beside ITS-REST,
//! read by named readers and handed to the consumers that follow them.
//!
//! ITS-REST 1.1.0 defines no change notification, subscription or bulk
//! export, so no specification governs any of this: our own design. The batch
//! ETL over AQL stays the conformant baseline; an event only names what
//! changed, and a consumer reads the change itself over ITS-REST.
//!
//! [`ChangeFeed`] is the source seam, with two implementations: the
//! vendor-neutral [`aql::AqlFeed`], which polls ITS-REST 1.1.0 `/query/aql`
//! past a watermark the bridge keeps itself ([`watermark`]), and the
//! reference CDR's outbox feed ([`ferroehr::FerroEhrFeed`]), which adds
//! commit order and deletions. Each consumer reads under its own
//! [`ReaderName`], which names its durable watermark, and [`follow`] moves the cursor only over the
//! prefix of a page the consumer has fully processed. [`health`] reports each
//! consumer to readiness as degraded, never down, while it cannot follow.

pub mod aql;
pub mod ferroehr;
pub mod follow;
pub mod health;
pub mod watermark;

use std::fmt;

use http::StatusCode;

use crate::cdr::error::CdrError;
use crate::cdr::error::Upstream;
use crate::cdr::ids::ContributionUid;
use crate::cdr::ids::EhrId;

/// The most events one page asks for: the bound of the reference CDR's feed,
/// which the AQL source keeps too.
pub const MAX_PAGE: u16 = 1000;

/// The delivery sequence number of one event, unique and increasing in the
/// order the source commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Seq(u64);

impl Seq {
    /// Returns the sequence number `value`.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Seq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Where one event sits in its source, which the cursor is moved to.
///
/// The two sources order their events differently, so a position compares
/// only with a position of the same source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Position {
    /// The delivery sequence number of the reference CDR's outbox.
    Seq(Seq),
    /// A commit instant and the versions committed at that instant up to and
    /// including this event.
    Committed(Committed),
}

impl Position {
    /// Returns whether this position lies strictly after `earlier`.
    ///
    /// A sequence number follows a smaller one; a commit position follows
    /// one at an earlier instant, or one at the same instant whose versions
    /// it holds with at least one more. Positions of two sources never
    /// follow one another.
    #[must_use]
    pub fn follows(&self, earlier: &Self) -> bool {
        match (self, earlier) {
            (Self::Seq(seq), Self::Seq(before)) => seq > before,
            (Self::Committed(at), Self::Committed(before)) => at.follows(before),
            (Self::Seq(_), Self::Committed(_)) | (Self::Committed(_), Self::Seq(_)) => false,
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Seq(seq) => write!(f, "seq {seq}"),
            Self::Committed(committed) => write!(
                f,
                "{} ({} versions at that instant)",
                committed.text,
                committed.versions.len()
            ),
        }
    }
}

/// A watermark over commit instants: the instant, as the CDR wrote it, and
/// the version uids committed at exactly that instant that are finished.
///
/// Two versions can carry one `time_committed`, so the instant alone cannot
/// tell a finished version from a later one at the same instant; the uids
/// can (no specification governs this: our own design).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// The instant.
    at: jiff::Timestamp,
    /// The instant as the CDR wrote it, which the next query binds.
    text: String,
    /// The version uids finished at the instant.
    versions: std::collections::BTreeSet<String>,
}

impl Committed {
    /// Returns the watermark at `text`, the instant `at` as the CDR wrote it,
    /// with `versions` finished at it.
    #[must_use]
    pub const fn new(
        at: jiff::Timestamp,
        text: String,
        versions: std::collections::BTreeSet<String>,
    ) -> Self {
        Self { at, text, versions }
    }

    /// Returns the instant.
    #[must_use]
    pub const fn at(&self) -> jiff::Timestamp {
        self.at
    }

    /// Returns the instant as the CDR wrote it.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the version uids finished at the instant.
    #[must_use]
    pub const fn versions(&self) -> &std::collections::BTreeSet<String> {
        &self.versions
    }

    /// Returns whether this watermark lies strictly after `earlier`.
    #[must_use]
    pub fn follows(&self, earlier: &Self) -> bool {
        self.at > earlier.at
            || (self.at == earlier.at
                && self.versions.len() > earlier.versions.len()
                && self.versions.is_superset(&earlier.versions))
    }
}

/// The name one consumer reads the feed under, which the source keeps one
/// cursor for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReaderName(String);

/// Why a text is no reader name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReaderNameError {
    /// The name is empty.
    #[error("a reader name is not empty")]
    Empty,
    /// The name carries a character outside `A-Z`, `a-z`, `0-9`, `_`, `-`
    /// and `.`.
    #[error("a reader name carries {character:?}; the characters are A-Z, a-z, 0-9, _, - and .")]
    Character {
        /// The first character refused.
        character: char,
    },
    /// The name is longer than 128 characters.
    #[error("a reader name is at most 128 characters")]
    Long,
}

impl ReaderName {
    /// Returns the reader name `text`.
    ///
    /// # Errors
    /// Returns [`ReaderNameError`] when `text` is empty, longer than 128
    /// characters, or carries a character other than an ASCII letter, a
    /// digit, `_`, `-` or `.`.
    pub fn new(text: &str) -> Result<Self, ReaderNameError> {
        // NOTE: no specification governs reader names: our own design; the
        // narrow alphabet keeps a name one path segment and one log token.
        if text.is_empty() {
            return Err(ReaderNameError::Empty);
        }
        if let Some(character) = text
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')))
        {
            return Err(ReaderNameError::Character { character });
        }
        if text.len() > 128 {
            return Err(ReaderNameError::Long);
        }
        Ok(Self(text.to_owned()))
    }

    /// Returns the name as it travels.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ReaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How many events one page asks for, from 1 to [`MAX_PAGE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSize(u16);

impl PageSize {
    /// Returns the page size `value`, or `None` outside 1 to [`MAX_PAGE`].
    #[must_use]
    pub fn new(value: u64) -> Option<Self> {
        u16::try_from(value)
            .ok()
            .filter(|size| (1..=MAX_PAGE).contains(size))
            .map(Self)
    }

    /// Returns the number of events.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl Default for PageSize {
    fn default() -> Self {
        Self(100)
    }
}

/// One changed versioned object of an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedVersion {
    /// The versioned object's identifier.
    pub vo_id: uuid::Uuid,
    /// The RM type name of the object, such as `COMPOSITION`.
    pub kind: String,
    /// The version ordinal.
    pub sys_version: u64,
    /// The audit change type, as the source writes it.
    pub change_type: String,
    /// The operational template of a composition, when there is one.
    pub template_id: Option<String>,
}

/// One committed contribution, as the feed serves it: identity and
/// provenance, never clinical content.
#[derive(Clone)]
pub struct Event {
    /// Where the event sits in its source.
    pub position: Position,
    /// The contribution the change belongs to.
    pub contribution_id: ContributionUid,
    /// The EHR, absent for a demographic contribution.
    pub ehr_id: Option<EhrId>,
    /// The commit instant.
    pub committed_at: jiff::Timestamp,
    /// One entry per changed versioned object.
    pub versions: Vec<ChangedVersion>,
    /// The event as the source served it, members this type does not read
    /// included, so a consumer can pass it on unchanged.
    pub envelope: serde_json::Map<String, serde_json::Value>,
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // NOTE: no specification governs this: our own design; the raw
        // envelope is left out so a log line holds the identifiers only.
        f.debug_struct("Event")
            .field("position", &self.position)
            .field("contribution_id", &self.contribution_id)
            .field("ehr_id", &self.ehr_id)
            .field("versions", &self.versions.len())
            .finish_non_exhaustive()
    }
}

impl Event {
    /// Returns the delivery sequence number, when the source assigns one.
    #[must_use]
    pub const fn seq(&self) -> Option<Seq> {
        match &self.position {
            Position::Seq(seq) => Some(*seq),
            Position::Committed(_) => None,
        }
    }
}

/// One page of events after a reader's cursor, in the order of its source.
#[derive(Debug, Clone)]
pub struct Page {
    /// The events, possibly none.
    pub events: Vec<Event>,
}

/// A source of commit events, each reader with its own cursor.
#[async_trait::async_trait]
pub trait ChangeFeed: fmt::Debug + Send + Sync {
    /// Returns up to `limit` events after the cursor of `reader`, without
    /// moving it.
    ///
    /// # Errors
    /// Returns [`FeedError`] for every answer that is not a page.
    async fn page(&self, reader: &ReaderName, limit: PageSize) -> Result<Page, FeedError>;

    /// Moves the cursor of `reader` to `last`, the last event it has fully
    /// processed.
    ///
    /// # Errors
    /// Returns [`FeedError`] for every answer that did not move the cursor.
    async fn acknowledge(&self, reader: &ReaderName, last: &Position) -> Result<(), FeedError>;
}

/// Why a feed call did not answer a page or move a cursor.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FeedError {
    /// The call never completed: the request, the transport, the timeout, or
    /// the renewal of a refused token.
    #[error("the feed could not be reached")]
    Cdr {
        /// What the CDR client reported.
        #[source]
        source: CdrError,
    },
    /// The source refused the credentials (`401` or `403`).
    #[error("the feed refused the configured credentials with {}", upstream.status())]
    Refused {
        /// The answer.
        upstream: Box<Upstream>,
    },
    /// The source does not know the reader, or the reader is inactive
    /// (`404`).
    #[error("the feed knows no active reader {reader}")]
    UnknownReader {
        /// The reader asked for.
        reader: ReaderName,
        /// The answer.
        upstream: Box<Upstream>,
    },
    /// The source refused the acknowledgement: `last` is below the cursor or
    /// above the last event (`409`).
    #[error("the feed refused to move reader {reader} to {last}")]
    Conflict {
        /// The reader acknowledged for.
        reader: ReaderName,
        /// The sequence number acknowledged.
        last: Seq,
        /// The answer.
        upstream: Box<Upstream>,
    },
    /// The source failed (`5xx`).
    #[error("the feed answered {}", upstream.status())]
    Unavailable {
        /// The answer.
        upstream: Box<Upstream>,
    },
    /// The source answered a status the feed does not document.
    #[error("the feed answered {status}, which it does not document for this call")]
    Undocumented {
        /// The status.
        status: StatusCode,
        /// The answer.
        upstream: Box<Upstream>,
    },
    /// The source refused the query that reads it (`400` or `408` of
    /// `query_execute_adhoc_query_body`).
    #[error("the CDR refused the change query with {}", upstream.status())]
    Query {
        /// The answer.
        upstream: Box<Upstream>,
    },
    /// A position from another source was handed to this one.
    #[error("{position} is no position of this source")]
    Foreign {
        /// The position.
        position: Position,
    },
    /// The watermark store refused a read or a write.
    #[error("the watermark store refused the call")]
    Store {
        /// What `redb` reported.
        #[source]
        source: Box<redb::Error>,
    },
    /// The acknowledged position lies at or before the stored watermark.
    #[error("the watermark of reader {reader} is past {position}")]
    Behind {
        /// The reader.
        reader: ReaderName,
        /// The position acknowledged.
        position: Position,
    },
    /// The stored watermark does not read.
    #[error("the stored watermark of reader {reader} does not read")]
    Watermark {
        /// The reader.
        reader: ReaderName,
        /// Why.
        #[source]
        source: serde_json::Error,
    },
    /// A page body that is not the documented page.
    #[error("the feed answered a page that does not read: {reason}")]
    Malformed {
        /// What is wrong, never the body itself.
        reason: String,
        /// The JSON error behind it, when there is one.
        #[source]
        source: Option<serde_json::Error>,
    },
}

impl FeedError {
    /// Returns the status the source answered, when it answered.
    #[must_use]
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Refused { upstream }
            | Self::UnknownReader { upstream, .. }
            | Self::Conflict { upstream, .. }
            | Self::Unavailable { upstream }
            | Self::Query { upstream } => Some(upstream.status()),
            Self::Undocumented { status, .. } => Some(*status),
            Self::Cdr { .. }
            | Self::Malformed { .. }
            | Self::Foreign { .. }
            | Self::Store { .. }
            | Self::Behind { .. }
            | Self::Watermark { .. } => None,
        }
    }
}

/// The source the consumers follow, resolved from `[change_feed]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The vendor-neutral AQL source.
    Aql {
        /// The `redb` file of the watermarks.
        watermarks: std::path::PathBuf,
        /// Where a reader with no watermark starts.
        start: aql::Start,
    },
    /// The reference CDR's outbox feed.
    FerroEhr {
        /// The root of the CDR's extensions.
        root: url::Url,
    },
}

/// The change feed, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The source.
    pub source: Source,
    /// How the followers pace their reads.
    pub polling: follow::Polling,
}

/// Returns the source `settings` names, reached through `cdr`.
///
/// # Errors
/// Returns [`FeedError::Store`] when the watermark file of the AQL source
/// cannot be opened.
pub fn open(
    settings: &Settings,
    cdr: crate::cdr::CdrClient,
) -> Result<std::sync::Arc<dyn ChangeFeed>, FeedError> {
    Ok(match &settings.source {
        Source::Aql { watermarks, start } => std::sync::Arc::new(aql::AqlFeed::new(
            cdr,
            std::sync::Arc::new(watermark::WatermarkStore::open(watermarks)?),
            start.clone(),
        )),
        Source::FerroEhr { root } => {
            std::sync::Arc::new(ferroehr::FerroEhrFeed::new(cdr, root.clone()))
        }
    })
}
