// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[inbound]` into the settings of the intake.

use crate::config::Config;
use crate::config::error::Error;
use crate::inbound::Settings;
use crate::retry::{Backoff, Policy};

/// Returns the intake `config` turns on, refused without the state store or
/// with a zero bound or a backoff above its cap; `None` while it is off.
pub(in crate::config) fn resolve(config: &Config) -> Result<Option<Settings>, Error> {
    let section = &config.inbound;
    if !section.enabled {
        return Ok(None);
    }
    super::needs("inbound.enabled", &[(config.state.is_some(), "state.url")])?;
    let zero = [
        ("inbound.body_limit", section.body_limit == 0),
        ("inbound.workers", section.workers == 0),
        ("inbound.lease", section.lease.get().is_zero()),
        ("inbound.backoff", section.backoff.get().is_zero()),
        ("inbound.max_attempts", section.max_attempts == 0),
        ("inbound.idle", section.idle.get().is_zero()),
        (
            "inbound.recovery_tick",
            section.recovery_tick.get().is_zero(),
        ),
        (
            "inbound.parked_body_retention",
            section.parked_body_retention.get().is_zero(),
        ),
    ];
    if let Some((key, _)) = zero.into_iter().find(|(_, zero)| *zero) {
        return Err(Error::Zero {
            key: key.to_owned(),
        });
    }
    if section.max_backoff.get() < section.backoff.get() {
        return Err(Error::Below {
            key: String::from("inbound.max_backoff"),
            bound: String::from("inbound.backoff"),
        });
    }
    Ok(Some(Settings {
        body_limit: section.body_limit,
        recovery_tick: section.recovery_tick.get(),
        parked_body_retention: section.parked_body_retention.get(),
        workers: crate::inbound::worker::Settings {
            workers: section.workers,
            lease: section.lease.get(),
            policy: Policy {
                backoff: Backoff {
                    initial: section.backoff.get(),
                    max: section.max_backoff.get(),
                },
                max_attempts: section.max_attempts,
            },
            idle: section.idle.get(),
        },
    }))
}
