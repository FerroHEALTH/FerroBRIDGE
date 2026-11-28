// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[identity_source.cache]`: refused without `[auth]` or `[state]`, its
//! channel endpoint and admitted clients required, and its bounds defaulted.

use std::collections::BTreeMap;
use std::time::Duration;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::Settings;
use ferrobridge_server::config::error::Error;

/// The identity source with a cache table whose body is `cache`.
fn cached(cache: &str) -> String {
    format!(
        "[identity_source]\npseudonym_domain = \"urn:oid:2.999.9\"\nsource_domains = [\"urn:oid:2.999.1\"]\n\n[identity_source.endpoint]\nurl = \"http://127.0.0.1:9/fhir\"\n\n[identity_source.cache]\n{cache}\n"
    )
}

/// A `[state]` table on a local database.
const STATE: &str = "[state]\nurl = \"postgres://bridge@db.invalid/state?sslmode=disable\"\n";

/// The cache body every case but the missing-key one carries.
const FEED: &str =
    "feed_url = \"https://bridge.example.org/pmir/feed\"\nfeed_clients = [\"ferropix\"]\n";

/// Resolves `text` for a job.
fn resolve(text: &str) -> Result<Settings, Error> {
    Config::from_sources(Some(text), &BTreeMap::new())?.resolve(Process::Job)
}

#[test]
fn the_cache_needs_auth_and_the_state_store() -> Result<(), Box<dyn std::error::Error>> {
    for (text, needs) in [
        (
            format!("{}[auth]\nenabled = false\n\n{STATE}", cached(FEED)),
            "[auth]",
        ),
        (cached(FEED), "[state]"),
    ] {
        match resolve(&text) {
            Err(Error::Needs { key, needs: found }) => {
                assert_eq!("identity_source.cache", key);
                assert_eq!(needs, found);
            }
            other => return Err(format!("expected a needs error, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn the_channel_endpoint_and_the_admitted_clients_are_required()
-> Result<(), Box<dyn std::error::Error>> {
    for (body, key) in [
        (
            "feed_clients = [\"ferropix\"]\n",
            "identity_source.cache.feed_url",
        ),
        (
            "feed_url = \"https://bridge.example.org/pmir/feed\"\n",
            "identity_source.cache.feed_clients",
        ),
    ] {
        match resolve(&format!("{}{STATE}", cached(body))) {
            Err(Error::Missing { key: missing }) => assert_eq!(key, missing),
            other => return Err(format!("expected a missing key, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn the_cache_takes_its_default_bounds() -> Result<(), Box<dyn std::error::Error>> {
    let settings = resolve(&format!("{}{STATE}", cached(FEED)))?;
    let identity = settings
        .identity_source
        .ok_or("the identity source is resolved")?;
    let cache = identity.cache_settings().ok_or("the cache is configured")?;
    assert_eq!(10_000, cache.bounds.entries);
    assert_eq!(Duration::from_secs(300), cache.bounds.max_age);
    assert_eq!(Duration::from_secs(30), cache.bounds.status_every);
    assert_eq!(["ferropix"], cache.feed_clients.as_slice());
    Ok(())
}

#[test]
fn a_cache_of_no_entries_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    match resolve(&format!(
        "{}{STATE}",
        cached(&format!("{FEED}entries = 0\n"))
    )) {
        Err(Error::Zero { key }) => assert_eq!("identity_source.cache.entries", key),
        other => return Err(format!("expected a zero bound, got {other:?}").into()),
    }
    Ok(())
}
