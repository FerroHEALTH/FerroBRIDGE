// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[change_feed]` section: off until enabled, and every refusal.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use std::collections::BTreeMap;

use super::refusal;

/// A CDR section the feed can read.
const CDR: &str = "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n";

#[test]
fn the_change_feed_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_sources(None, &BTreeMap::new())?;
    assert!(!config.change_feed.enabled);
    assert_eq!("aql", config.change_feed.source);
    assert_eq!("now", config.change_feed.start);
    assert_eq!(100, config.change_feed.page_size);
    assert_eq!(5, config.change_feed.max_attempts);
    assert_eq!(None, config.change_feed.dead_letters);
    let settings = config.resolve(Process::Server)?;
    assert!(settings.change_feed.is_none());
    Ok(())
}

#[test]
fn every_key_reads_from_the_environment() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__CHANGE_FEED__SOURCE"),
            String::from("ferroehr"),
        ),
        (
            String::from("FERROBRIDGE__CHANGE_FEED__URL"),
            String::from("https://ehr.invalid/ferroehr/rest/extensions/v1"),
        ),
        (
            String::from("FERROBRIDGE__CHANGE_FEED__WATERMARKS"),
            String::from("/var/lib/ferrobridge/feed.redb"),
        ),
        (
            String::from("FERROBRIDGE__CHANGE_FEED__DEAD_LETTERS"),
            String::from("/var/lib/ferrobridge/dead-letters.redb"),
        ),
        (
            String::from("FERROBRIDGE__CHANGE_FEED__MAX_ATTEMPTS"),
            String::from("8"),
        ),
        (
            String::from("FERROBRIDGE__CHANGE_FEED__START"),
            String::from("beginning"),
        ),
        (
            String::from("FERROBRIDGE__CHANGE_FEED__PAGE_SIZE"),
            String::from("250"),
        ),
        (
            String::from("FERROBRIDGE__CHANGE_FEED__POLL_INTERVAL"),
            String::from("5s"),
        ),
        (
            String::from("FERROBRIDGE__CHANGE_FEED__BACKOFF"),
            String::from("200ms"),
        ),
        (
            String::from("FERROBRIDGE__CHANGE_FEED__MAX_BACKOFF"),
            String::from("30s"),
        ),
    ]);
    let config = Config::from_sources(None, &environment)?;
    let section = &config.change_feed;
    assert_eq!("ferroehr", section.source);
    assert_eq!(
        "https://ehr.invalid/ferroehr/rest/extensions/v1",
        section.url
    );
    assert_eq!(
        Some(std::path::PathBuf::from("/var/lib/ferrobridge/feed.redb")),
        section.watermarks
    );
    assert_eq!(
        Some(std::path::PathBuf::from(
            "/var/lib/ferrobridge/dead-letters.redb"
        )),
        section.dead_letters
    );
    assert_eq!(8, section.max_attempts);
    assert_eq!("beginning", section.start);
    assert_eq!(
        (250, 5000, 200, 30_000),
        (
            section.page_size,
            section.poll_interval.as_millis(),
            section.backoff.as_millis(),
            section.max_backoff.as_millis()
        )
    );
    Ok(())
}

#[test]
fn an_enabled_feed_without_a_cdr_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal("[change_feed]\nenabled = true\nwatermarks = \"/tmp/feed.redb\"\n")?;
    assert!(
        matches!(&error, Error::Needs { key, needs } if key == "change_feed.enabled" && needs == "[cdr]"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_enabled_feed_no_consumer_follows_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!(
        "{CDR}[change_feed]\nenabled = true\nwatermarks = \"/tmp/feed.redb\"\ndead_letters = \"/tmp/dead-letters.redb\"\n"
    ))?;
    assert!(
        matches!(&error, Error::NoConsumer { key } if key == "change_feed.enabled"),
        "{error:?}"
    );
    Ok(())
}

/// Returns the refusal of an enabled feed whose section adds `keys`.
fn refused_with(keys: &str) -> Result<Error, Box<dyn std::error::Error>> {
    refusal(&format!("{CDR}[change_feed]\nenabled = true\n{keys}"))
}

#[test]
fn an_unknown_source_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refused_with("source = \"kafka\"\n")?;
    assert!(
        matches!(&error, Error::FeedSource { key, value } if key == "change_feed.source" && value == "kafka"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn the_aql_source_without_a_watermark_file_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refused_with("")?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "change_feed.watermarks"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_start_that_names_no_instant_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refused_with("watermarks = \"/tmp/feed.redb\"\nstart = \"yesterday\"\n")?;
    assert!(
        matches!(&error, Error::FeedStart { key, .. } if key == "change_feed.start"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn the_ferroehr_source_without_a_usable_url_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let missing = refused_with("source = \"ferroehr\"\n")?;
    assert!(
        matches!(&missing, Error::Missing { key } if key == "change_feed.url"),
        "{missing:?}"
    );
    for url in [
        "ftp://ehr.invalid/ext",
        "https://ehr.invalid/ext?x=1",
        "https://ehr.invalid/ext#f",
    ] {
        let error = refused_with(&format!("source = \"ferroehr\"\nurl = \"{url}\"\n"))?;
        assert!(
            matches!(&error, Error::FetchBase { key, .. } if key == "change_feed.url"),
            "{url}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn a_page_size_outside_one_to_a_thousand_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    for size in [0_u64, 1001] {
        let error = refused_with(&format!(
            "watermarks = \"/tmp/feed.redb\"\npage_size = {size}\n"
        ))?;
        assert!(
            matches!(&error, Error::Bound { key, value, .. } if key == "change_feed.page_size" && *value == size),
            "{size}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn a_zero_wait_or_a_ceiling_below_the_backoff_is_refused() -> Result<(), Box<dyn std::error::Error>>
{
    for (keys, refused) in [
        ("poll_interval = \"0s\"\n", "change_feed.poll_interval"),
        ("backoff = \"0s\"\n", "change_feed.backoff"),
        (
            "backoff = \"5s\"\nmax_backoff = \"1s\"\n",
            "change_feed.max_backoff",
        ),
    ] {
        let error = refused_with(&format!("watermarks = \"/tmp/feed.redb\"\n{keys}"))?;
        assert!(
            matches!(&error, Error::Bound { key, .. } | Error::Span { key, .. } if key == refused),
            "{keys}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn an_unknown_key_is_refused() {
    let refused = Config::from_sources(Some("[change_feed]\nreader = \"x\"\n"), &BTreeMap::new());
    assert!(matches!(refused, Err(Error::Parse { .. })), "{refused:?}");
}

#[test]
fn an_enabled_feed_without_a_dead_letter_store_is_refused() -> Result<(), Box<dyn std::error::Error>>
{
    let error = refused_with("watermarks = \"/tmp/feed.redb\"\n")?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "change_feed.dead_letters"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_bound_of_poisoned_passes_outside_one_to_a_thousand_is_refused()
-> Result<(), Box<dyn std::error::Error>> {
    for attempts in [0_u64, 1001] {
        let error = refused_with(&format!(
            "watermarks = \"/tmp/feed.redb\"\ndead_letters = \"/tmp/dead-letters.redb\"\nmax_attempts = {attempts}\n"
        ))?;
        assert!(
            matches!(&error, Error::Bound { key, value, .. } if key == "change_feed.max_attempts" && *value == attempts),
            "{attempts}: {error:?}"
        );
    }
    Ok(())
}
