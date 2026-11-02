// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[change_feed]` into the source the consumers follow.
//!
//! No specification governs the change feed: our own design.

use std::time::Duration;

use super::error::Error;
use super::section::change_feed::ChangeFeed;
use crate::change_feed::PageSize;
use crate::change_feed::Settings;
use crate::change_feed::Source;
use crate::change_feed::aql::Start;
use crate::change_feed::follow::Polling;

/// Returns the change feed `section` describes, refused when it has no CDR
/// to read, a key out of range, or no consumer to feed.
///
/// `consumers` counts the consumer sections that are on.
pub(super) fn resolve_change_feed(
    section: &ChangeFeed,
    cdr: bool,
    consumers: usize,
) -> Result<Settings, Error> {
    if !cdr {
        return Err(Error::Needs {
            key: String::from("change_feed.enabled"),
            needs: String::from("[cdr]"),
        });
    }
    let source = match section.source.as_str() {
        "aql" => Source::Aql {
            watermarks: section.watermarks.clone().ok_or_else(|| Error::Missing {
                key: String::from("change_feed.watermarks"),
            })?,
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
        ("change_feed.poll_interval_ms", section.poll_interval_ms),
        ("change_feed.backoff_ms", section.backoff_ms),
    ] {
        if value == 0 {
            return Err(Error::Bound {
                key: key.to_owned(),
                value,
                reason: "a follower waits at least one millisecond",
            });
        }
    }
    if section.max_backoff_ms < section.backoff_ms {
        return Err(Error::Bound {
            key: String::from("change_feed.max_backoff_ms"),
            value: section.max_backoff_ms,
            reason: "the longest wait is at least change_feed.backoff_ms",
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
    let dead_letters = section.dead_letters.clone().ok_or_else(|| Error::Missing {
        key: String::from("change_feed.dead_letters"),
    })?;
    if consumers == 0 {
        return Err(Error::NoConsumer {
            key: String::from("change_feed.enabled"),
        });
    }
    Ok(Settings {
        source,
        dead_letters,
        max_attempts,
        polling: Polling {
            page_size,
            idle: Duration::from_millis(section.poll_interval_ms),
            backoff: Duration::from_millis(section.backoff_ms),
            max_backoff: Duration::from_millis(section.max_backoff_ms),
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
