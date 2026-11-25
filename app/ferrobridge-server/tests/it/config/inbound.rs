// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[inbound]`: off until the section turns it on, refused without the
//! state store, with a zero bound or with a backoff above its cap, and every
//! unknown key refused.

use std::collections::BTreeMap;
use std::time::Duration;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::Settings;
use ferrobridge_server::config::error::Error;

/// A `[state]` table on a local database, then `[inbound]` with `rest`.
fn with_inbound(rest: &str) -> String {
    format!(
        "[state]\nurl = \"postgres://bridge@db.invalid/state?sslmode=disable\"\n\n[inbound]\nenabled = true\n{rest}"
    )
}

/// Resolves `text` for the server.
fn resolve(text: &str) -> Result<Settings, Error> {
    Config::from_sources(Some(text), &BTreeMap::new())?.resolve(Process::Server)
}

#[test]
fn the_intake_is_off_until_its_section_turns_it_on() -> Result<(), Error> {
    assert!(resolve("")?.inbound.is_none());
    Ok(())
}

#[test]
fn an_intake_takes_its_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let inbound = resolve(&with_inbound(""))?
        .inbound
        .ok_or("the intake is on")?;
    assert_eq!(1024 * 1024, inbound.body_limit);
    assert_eq!(4, inbound.workers.workers);
    assert_eq!(Duration::from_secs(60), inbound.workers.lease);
    assert_eq!(Duration::from_secs(2), inbound.workers.policy.backoff);
    assert_eq!(Duration::from_secs(600), inbound.workers.policy.max_backoff);
    assert_eq!(10, inbound.workers.policy.max_attempts);
    assert_eq!(Duration::from_secs(5), inbound.workers.idle);
    Ok(())
}

#[test]
fn every_key_is_read() -> Result<(), Box<dyn std::error::Error>> {
    let inbound = resolve(&with_inbound(
        "body_limit = 2048\nworkers = 2\nlease = \"30s\"\nbackoff = \"1s\"\nmax_backoff = \"10s\"\nmax_attempts = 3\nidle = \"1s\"\n",
    ))?
    .inbound
    .ok_or("the intake is on")?;
    assert_eq!(2048, inbound.body_limit);
    assert_eq!(2, inbound.workers.workers);
    assert_eq!(Duration::from_secs(30), inbound.workers.lease);
    assert_eq!(Duration::from_secs(1), inbound.workers.policy.backoff);
    assert_eq!(Duration::from_secs(10), inbound.workers.policy.max_backoff);
    assert_eq!(3, inbound.workers.policy.max_attempts);
    assert_eq!(Duration::from_secs(1), inbound.workers.idle);
    Ok(())
}

#[test]
fn the_environment_sets_a_key() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__INBOUND__WORKERS"),
        String::from("6"),
    )]);
    let inbound = Config::from_sources(Some(&with_inbound("")), &environment)?
        .resolve(Process::Server)?
        .inbound
        .ok_or("the intake is on")?;
    assert_eq!(6, inbound.workers.workers);
    Ok(())
}

#[test]
fn the_intake_needs_the_state_store() {
    assert!(matches!(
        resolve("[inbound]\nenabled = true\n"),
        Err(Error::Needs { key, .. }) if key == "inbound.enabled"
    ));
}

#[test]
fn a_zero_bound_and_a_backoff_above_its_cap_are_refused() {
    for (rest, refused) in [
        ("workers = 0\n", "inbound.workers"),
        ("body_limit = 0\n", "inbound.body_limit"),
        ("max_attempts = 0\n", "inbound.max_attempts"),
        ("lease = \"0s\"\n", "inbound.lease"),
    ] {
        assert!(
            matches!(resolve(&with_inbound(rest)), Err(Error::Zero { key }) if key == refused),
            "{rest}"
        );
    }
    assert!(matches!(
        resolve(&with_inbound("backoff = \"20s\"\nmax_backoff = \"10s\"\n")),
        Err(Error::Below { key, .. }) if key == "inbound.max_backoff"
    ));
}

#[test]
fn an_unknown_key_is_refused() {
    assert!(matches!(
        resolve(&with_inbound("listen = \"0.0.0.0:8443\"\n")),
        Err(Error::Parse { .. })
    ));
}
