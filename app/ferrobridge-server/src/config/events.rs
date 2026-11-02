// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[events]` into the settings of the AMQP publisher.
//!
//! No specification governs the lane: our own design.

use std::collections::BTreeSet;
use std::time::Duration;

use secrecy::ExposeSecret;
use secrecy::SecretString;

use super::credentials::secret;
use super::error::Error;
use super::section::events::Events;
use crate::change_feed::ReaderName;
use crate::events::Queue;
use crate::events::Settings;

/// Returns the lane `events` describes, refused without the change feed it
/// follows, a reader name, an AMQP broker URL or an exchange.
pub(super) fn resolve_events(events: &Events, change_feed: bool) -> Result<Settings, Error> {
    if !change_feed {
        return Err(Error::Needs {
            key: String::from("events.enabled"),
            needs: String::from("change_feed.enabled"),
        });
    }
    if events.reader.is_empty() {
        return Err(Error::Missing {
            key: String::from("events.reader"),
        });
    }
    let reader = ReaderName::new(&events.reader).map_err(|source| Error::ReaderName {
        key: String::from("events.reader"),
        source,
    })?;
    let url = secret(
        "events.url",
        events.url.as_ref(),
        events.url_file.as_deref(),
    )?
    .ok_or(Error::Missing {
        key: String::from("events.url"),
    })?;
    let url = broker_url(&url, events.tls)?;
    if events.exchange.is_empty() {
        return Err(Error::Missing {
            key: String::from("events.exchange"),
        });
    }
    if events.confirm_timeout_ms == 0 {
        return Err(Error::Bound {
            key: String::from("events.confirm_timeout_ms"),
            value: 0,
            reason: "a publish waits at least one millisecond for its confirm",
        });
    }
    let mut names = BTreeSet::new();
    let mut queues = Vec::new();
    for queue in &events.queues {
        for (key, value) in [
            ("events.queues.name", &queue.name),
            ("events.queues.binding", &queue.binding),
        ] {
            if value.is_empty() {
                return Err(Error::Missing {
                    key: key.to_owned(),
                });
            }
        }
        if !names.insert(queue.name.as_str()) {
            return Err(Error::Duplicate {
                key: String::from("events.queues.name"),
                value: queue.name.clone(),
            });
        }
        queues.push(Queue {
            name: queue.name.clone(),
            binding: queue.binding.clone(),
        });
    }
    Ok(Settings {
        reader,
        url,
        exchange: events.exchange.clone(),
        queues,
        confirm_timeout: Duration::from_millis(events.confirm_timeout_ms),
    })
}

/// Returns the broker URL `url` names, an `amqp` URL upgraded to `amqps`
/// when `tls` is set.
///
/// The scheme compares without case (RFC 3986 §3.1); the refusal never
/// repeats the URL, which may carry a password.
fn broker_url(url: &SecretString, tls: bool) -> Result<SecretString, Error> {
    let text = url.expose_secret();
    let refused = || Error::BrokerUrl {
        key: String::from("events.url"),
    };
    let (scheme, rest) = text.split_once("://").ok_or_else(refused)?;
    if scheme.eq_ignore_ascii_case("amqps") || (tls && scheme.eq_ignore_ascii_case("amqp")) {
        Ok(SecretString::from(format!("amqps://{rest}")))
    } else if scheme.eq_ignore_ascii_case("amqp") {
        Ok(url.clone())
    } else {
        Err(refused())
    }
}
