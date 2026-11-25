// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[change_feed]` into the source the consumers follow,
//! and the list of the consumer sections that follow it, which owns the
//! checks of every consumer's reader.
//!
//! No specification governs the change feed: our own design.

use super::Config;
use super::error::Error;
use super::section::change_feed::ChangeFeed;
use crate::change_feed::PageSize;
use crate::change_feed::ReaderName;
use crate::change_feed::Settings;
use crate::change_feed::Source;
use crate::change_feed::aql::Start;
use crate::change_feed::follow::Polling;

/// A change-feed consumer section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Consumer {
    /// `[events]`, the AMQP publisher.
    Events,
    /// `[omop_incremental]`, the incremental OMOP load.
    OmopIncremental,
    /// `[fhir_outbound]`, the outbound FHIR lane.
    FhirOutbound,
}

impl Consumer {
    /// Every consumer section, in the order the sections resolve.
    const ALL: [Self; 3] = [Self::Events, Self::OmopIncremental, Self::FhirOutbound];

    /// Returns the key of the section.
    const fn section(self) -> &'static str {
        match self {
            Self::Events => "events",
            Self::OmopIncremental => "omop_incremental",
            Self::FhirOutbound => "fhir_outbound",
        }
    }

    /// Returns whether the section is on in `config` and the reader it names.
    fn of(self, config: &Config) -> (bool, &str) {
        match self {
            Self::Events => (config.events.enabled, &config.events.reader),
            Self::OmopIncremental => (
                config.omop_incremental.enabled,
                &config.omop_incremental.reader,
            ),
            Self::FhirOutbound => (config.fhir_outbound.enabled, &config.fhir_outbound.reader),
        }
    }

    /// Returns how many consumer sections are on in `config`.
    pub(super) fn on(config: &Config) -> usize {
        Self::ALL
            .iter()
            .filter(|consumer| consumer.of(config).0)
            .count()
    }

    /// Returns the reader the section names in `config`, refused when it is
    /// empty, outside the reader alphabet, or the reader of a section that is
    /// on and resolves before it.
    pub(super) fn reader(self, config: &Config) -> Result<ReaderName, Error> {
        let key = format!("{}.reader", self.section());
        let (_, named) = self.of(config);
        if named.is_empty() {
            return Err(Error::Missing { key });
        }
        let reader = ReaderName::new(named).map_err(|source| Error::ReaderName {
            key: key.clone(),
            source,
        })?;
        let taken = Self::ALL
            .iter()
            .take_while(|earlier| **earlier != self)
            .any(|earlier| earlier.of(config) == (true, named));
        if taken {
            return Err(Error::Duplicate {
                key,
                value: named.to_owned(),
            });
        }
        Ok(reader)
    }
}

/// Returns the change feed `section` describes, refused when it has no CDR
/// to read, no state store to keep its cursors and dead letters in, a key
/// out of range, or no consumer to feed.
///
/// `consumers` counts the consumer sections that are on.
pub(super) fn resolve_change_feed(
    section: &ChangeFeed,
    cdr: bool,
    state: bool,
    consumers: usize,
) -> Result<Settings, Error> {
    for (present, needs) in [(cdr, "[cdr]"), (state, "[state]")] {
        if !present {
            return Err(Error::Needs {
                key: String::from("change_feed.enabled"),
                needs: String::from(needs),
            });
        }
    }
    let source = match section.source.as_str() {
        "aql" => Source::Aql {
            start: start_of(&section.start)?,
        },
        "ferroehr" => Source::FerroEhr {
            root: root_of(&section.url)?,
        },
        _ => {
            return Err(Error::FeedSource {
                key: String::from("change_feed.source"),
                value: section.source.clone(),
            });
        }
    };
    let page_size = PageSize::new(section.page_size).ok_or(Error::Bound {
        key: String::from("change_feed.page_size"),
        value: section.page_size,
        reason: "a read asks for 1 to 1000 events",
    })?;
    for (key, value) in [
        ("change_feed.poll_interval", section.poll_interval),
        ("change_feed.backoff", section.backoff),
        ("change_feed.lease", section.lease),
    ] {
        if value.is_zero() {
            return Err(Error::Bound {
                key: key.to_owned(),
                value: 0,
                reason: "a follower waits at least one millisecond",
            });
        }
    }
    if section.max_backoff < section.backoff {
        return Err(Error::Span {
            key: String::from("change_feed.max_backoff"),
            value: section.max_backoff,
            reason: "the longest wait is at least change_feed.backoff",
        });
    }
    let max_attempts = u32::try_from(section.max_attempts)
        .ok()
        .filter(|attempts| (1..=1000).contains(attempts))
        .and_then(std::num::NonZeroU32::new)
        .ok_or(Error::Bound {
            key: String::from("change_feed.max_attempts"),
            value: section.max_attempts,
            reason: "an event fails 1 to 1000 poisoned passes before it is parked",
        })?;
    if consumers == 0 {
        return Err(Error::NoConsumer {
            key: String::from("change_feed.enabled"),
        });
    }
    Ok(Settings {
        source,
        lease: section.lease.get(),
        max_attempts,
        polling: Polling {
            page_size,
            idle: section.poll_interval.get(),
            backoff: crate::retry::Backoff {
                initial: section.backoff.get(),
                max: section.max_backoff.get(),
            },
        },
    })
}

/// Returns the start `value` names.
fn start_of(value: &str) -> Result<Start, Error> {
    match value {
        "now" => Ok(Start::Now),
        "beginning" => Ok(Start::Beginning),
        instant if instant.parse::<jiff::Timestamp>().is_ok() => Ok(Start::At(instant.to_owned())),
        _ => Err(Error::FeedStart {
            key: String::from("change_feed.start"),
            value: value.to_owned(),
        }),
    }
}

/// Returns the extension root `value` names: an http or https base with no
/// query and no fragment.
fn root_of(value: &str) -> Result<url::Url, Error> {
    let root = super::resolve::url_of("change_feed.url", value)?;
    if !matches!(root.scheme(), "http" | "https")
        || root.cannot_be_a_base()
        || root.query().is_some()
        || root.fragment().is_some()
    {
        return Err(Error::FetchBase {
            key: String::from("change_feed.url"),
            value: value.to_owned(),
        });
    }
    Ok(root)
}
