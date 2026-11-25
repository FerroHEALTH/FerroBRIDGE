// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[state]` and `[state.seal]`: absent until configured, every unknown key
//! refused, the URL held to the `sslmode` rules `[cdm]` follows, and the
//! sealing keys read from their files with the first one sealing.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::time::Duration;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::Settings;
use ferrobridge_server::config::error::Error;
use ferrobridge_server::store::seal::{KeyId, SealingKey};

/// The base64 of 32 bytes of `A`, made for the tests.
const KEY_A: &str = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=";

/// The base64 of 32 bytes of `B`, made for the tests.
const KEY_B: &str = "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=";

/// A `[state]` table on a local database, with `rest` after it.
fn with_state(rest: &str) -> String {
    format!("[state]\nurl = \"postgres://bridge@db.invalid/state?sslmode=disable\"\n{rest}")
}

/// Resolves `text` for a job.
fn resolve(text: &str) -> Result<Settings, Error> {
    Config::from_sources(Some(text), &BTreeMap::new())?.resolve(Process::Job)
}

#[test]
fn the_store_is_absent_until_a_deployment_configures_it() -> Result<(), Error> {
    assert!(resolve("")?.state.is_none());
    Ok(())
}

#[test]
fn a_configured_store_takes_its_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let state = resolve(&with_state(""))?
        .state
        .ok_or("the store is configured")?;
    assert_eq!(10, state.max_connections);
    assert_eq!(Duration::from_secs(5), state.acquire_timeout);
    assert!(state.seal.is_none());
    Ok(())
}

#[test]
fn an_unknown_key_in_any_table_of_the_section_is_refused() {
    for text in [
        with_state("pool_size = 4\n"),
        with_state("[state.seal]\ncurrent = \"a\"\n"),
        with_state(&format!(
            "[[state.seal.keys]]\nid = \"a\"\nkey = \"{KEY_A}\"\nkid = \"a\"\n"
        )),
    ] {
        assert!(matches!(resolve(&text), Err(Error::Parse { .. })), "{text}");
    }
}

#[test]
fn a_section_without_a_url_is_refused_naming_it() {
    assert!(matches!(
        resolve("[state]\nmax_connections = 4\n"),
        Err(Error::Missing { key }) if key == "state.url"
    ));
}

#[test]
fn a_url_that_names_no_sslmode_is_refused_naming_the_url() {
    assert!(matches!(
        resolve("[state]\nurl = \"postgres://bridge@db.invalid/state\"\n"),
        Err(Error::StateConnection { key, .. }) if key == "state.url"
    ));
    assert!(matches!(
        resolve("[state]\nurl = \"postgres://bridge@db.invalid/state?sslmode=prefer\"\n"),
        Err(Error::StateConnection { key, .. }) if key == "state.url"
    ));
}

#[test]
fn a_ca_with_a_plaintext_url_is_refused_naming_the_ca() {
    assert!(matches!(
        resolve(&with_state("tls_ca = \"-----BEGIN CERTIFICATE-----\"\n")),
        Err(Error::StateConnection { key, .. }) if key == "state.tls_ca"
    ));
}

#[test]
fn an_empty_pool_or_no_wait_is_refused() {
    for (rest, named) in [
        ("max_connections = 0\n", "state.max_connections"),
        ("acquire_timeout = \"0s\"\n", "state.acquire_timeout"),
    ] {
        assert!(
            matches!(resolve(&with_state(rest)), Err(Error::Bound { key, .. }) if key == named),
            "{rest}"
        );
    }
}

#[test]
fn the_first_sealing_key_seals_and_every_one_opens() -> Result<(), Box<dyn std::error::Error>> {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "{KEY_B}")?;
    let text = with_state(&format!(
        "[state.seal]\nlookup_key = \"{KEY_A}\"\n\n[[state.seal.keys]]\nid = \"2026-11\"\nkey_file = \"{}\"\n\n[[state.seal.keys]]\nid = \"2026-05\"\nkey = \"{KEY_A}\"\n",
        file.path().display()
    ));
    let state = resolve(&text)?.state.ok_or("the store is configured")?;
    let seal = state.seal.ok_or("the keys are configured")?;
    let (id, key) = seal.current();
    assert_eq!("2026-11", id.as_str());
    assert_eq!(&[b'B'; 32], key.bytes());
    let older = KeyId::new("2026-05")?;
    assert_eq!(
        Some(&[b'A'; 32]),
        seal.opening(&older).map(SealingKey::bytes)
    );
    Ok(())
}

#[test]
fn a_sealing_key_that_is_not_32_bytes_or_named_twice_is_refused() {
    let short = with_state("[[state.seal.keys]]\nid = \"a\"\nkey = \"QUFB\"\n");
    assert!(matches!(
        resolve(&short),
        Err(Error::SealKey { key, .. }) if key == "state.seal.keys.key"
    ));
    let twice = with_state(&format!(
        "[[state.seal.keys]]\nid = \"a\"\nkey = \"{KEY_A}\"\n\n[[state.seal.keys]]\nid = \"a\"\nkey = \"{KEY_B}\"\n"
    ));
    assert!(matches!(
        resolve(&twice),
        Err(Error::Duplicate { key, value }) if key == "state.seal.keys.id" && value == "a"
    ));
    let unnamed = with_state(&format!("[[state.seal.keys]]\nkey = \"{KEY_A}\"\n"));
    assert!(matches!(
        resolve(&unnamed),
        Err(Error::SealKey { key, .. }) if key == "state.seal.keys.id"
    ));
    let keyless = with_state("[[state.seal.keys]]\nid = \"a\"\n");
    assert!(matches!(
        resolve(&keyless),
        Err(Error::Missing { key }) if key == "state.seal.keys.key"
    ));
    assert!(matches!(
        resolve(&with_state("[state.seal]\n")),
        Err(Error::Missing { key }) if key == "state.seal.keys"
    ));
    let lookupless = with_state(&format!(
        "[[state.seal.keys]]\nid = \"a\"\nkey = \"{KEY_A}\"\n"
    ));
    assert!(matches!(
        resolve(&lookupless),
        Err(Error::Missing { key }) if key == "state.seal.lookup_key"
    ));
    let short_lookup = with_state(&format!(
        "[state.seal]\nlookup_key = \"QUFB\"\n\n[[state.seal.keys]]\nid = \"a\"\nkey = \"{KEY_A}\"\n"
    ));
    assert!(matches!(
        resolve(&short_lookup),
        Err(Error::SealKey { key, .. }) if key == "state.seal.lookup_key"
    ));
}

#[test]
fn the_url_reaches_the_store_through_the_environment() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__STATE__URL"),
        String::from("postgres://bridge@db.invalid/state?sslmode=disable"),
    )]);
    let settings = Config::from_sources(None, &environment)?.resolve(Process::Job)?;
    assert!(settings.state.is_some());
    Ok(())
}
