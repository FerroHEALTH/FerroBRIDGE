// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[events]` section: off until enabled, every key, and every refusal.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::error::Error;
use secrecy::ExposeSecret;
use std::collections::BTreeMap;
use std::time::Duration;

use super::refusal;

/// A CDR and a change feed the lane can follow.
const FEED: &str = "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n[change_feed]\nenabled = true\nwatermarks = \"/tmp/feed.redb\"\ndead_letters = \"/tmp/dead-letters.redb\"\n\n";

/// Returns a configuration whose `[events]` section adds `keys` to an
/// enabled lane.
fn lane(keys: &str) -> String {
    format!("{FEED}[events]\nenabled = true\n{keys}")
}

/// The keys every complete lane names.
const COMPLETE: &str = "reader = \"ferrobridge\"\nurl = \"amqp://bridge:synthetic@broker.invalid:5672/%2f\"\nexchange = \"ferroehr.events\"\n";

#[test]
fn the_events_lane_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_sources(None, &BTreeMap::new())?;
    assert!(!config.events.enabled);
    assert_eq!(30_000, config.events.confirm_timeout_ms);
    assert!(config.resolve()?.events.is_none());
    Ok(())
}

#[test]
fn an_enabled_lane_resolves_every_key_and_feeds_the_change_feed()
-> Result<(), Box<dyn std::error::Error>> {
    let text = lane(&format!(
        "{COMPLETE}confirm_timeout_ms = 5000\nqueues = [{{ name = \"analytics\", binding = \"COMPOSITION.*.*\" }}, {{ name = \"all\", binding = \"#\" }}]\n"
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let events = settings.events.ok_or("the lane is on")?;
    assert!(
        settings.change_feed.is_some(),
        "the lane is the feed's consumer"
    );
    assert_eq!("ferrobridge", events.reader.as_str());
    assert_eq!("ferroehr.events", events.exchange);
    assert_eq!(Duration::from_millis(5000), events.confirm_timeout);
    assert_eq!(2, events.queues.len());
    assert_eq!(
        "amqp://bridge:synthetic@broker.invalid:5672/%2f",
        events.url.expose_secret()
    );
    Ok(())
}

#[test]
fn tls_upgrades_an_amqp_url_in_any_case() -> Result<(), Box<dyn std::error::Error>> {
    let text = lane(
        &COMPLETE
            .replace("amqp://", "AMQP://")
            .replace("reader", "tls = true\nreader"),
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let events = settings.events.ok_or("the lane is on")?;
    assert_eq!(
        "amqps://bridge:synthetic@broker.invalid:5672/%2f",
        events.url.expose_secret()
    );
    Ok(())
}

#[test]
fn the_broker_url_reads_from_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("broker-url");
    std::fs::write(&file, "amqps://bridge:from-file@broker.invalid:5671/%2f\n")?;
    let text = lane(&format!(
        "reader = \"ferrobridge\"\nexchange = \"ferroehr.events\"\nurl_file = \"{}\"\n",
        file.display()
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let events = settings.events.ok_or("the lane is on")?;
    assert_eq!(
        "amqps://bridge:from-file@broker.invalid:5671/%2f",
        events.url.expose_secret()
    );
    Ok(())
}

#[test]
fn a_lane_without_the_change_feed_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!("[events]\nenabled = true\n{COMPLETE}"))?;
    assert!(
        matches!(&error, Error::Needs { key, needs } if key == "events.enabled" && needs == "change_feed.enabled"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_lane_without_a_reader_a_url_or_an_exchange_is_refused()
-> Result<(), Box<dyn std::error::Error>> {
    for (dropped, key) in [
        ("reader = \"ferrobridge\"\n", "events.reader"),
        (
            "url = \"amqp://bridge:synthetic@broker.invalid:5672/%2f\"\n",
            "events.url",
        ),
        ("exchange = \"ferroehr.events\"\n", "events.exchange"),
    ] {
        let error = refusal(&lane(&COMPLETE.replace(dropped, "")))?;
        assert!(
            matches!(&error, Error::Missing { key: missing } if missing == key),
            "{key}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn a_reader_name_outside_the_alphabet_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&lane(
        &COMPLETE.replace("\"ferrobridge\"", "\"ferro bridge\""),
    ))?;
    assert!(
        matches!(&error, Error::ReaderName { key, .. } if key == "events.reader"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_url_that_is_no_amqp_url_is_refused_without_repeating_it()
-> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&lane(&COMPLETE.replace("amqp://", "https://")))?;
    assert!(
        matches!(&error, Error::BrokerUrl { key } if key == "events.url"),
        "{error:?}"
    );
    assert!(!error.to_string().contains("synthetic"), "{error}");
    Ok(())
}

#[test]
fn a_queue_named_twice_or_without_a_binding_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let twice = refusal(&lane(&format!(
        "{COMPLETE}queues = [{{ name = \"a\", binding = \"#\" }}, {{ name = \"a\", binding = \"#\" }}]\n"
    )))?;
    assert!(
        matches!(&twice, Error::Duplicate { key, .. } if key == "events.queues.name"),
        "{twice:?}"
    );
    let unbound = refusal(&lane(&format!("{COMPLETE}queues = [{{ name = \"a\" }}]\n")))?;
    assert!(
        matches!(&unbound, Error::Missing { key } if key == "events.queues.binding"),
        "{unbound:?}"
    );
    Ok(())
}

#[test]
fn the_running_configuration_never_carries_the_broker_url() -> Result<(), Box<dyn std::error::Error>>
{
    let config = Config::from_sources(Some(&lane(COMPLETE)), &BTreeMap::new())?;
    let rendered = toml::to_string(&config)?;
    assert!(!rendered.contains("synthetic"), "{rendered}");
    Ok(())
}
