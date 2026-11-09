// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[fhir_outbound]` section: off by default, every key, the `_file`
//! secret of its credentials, the environment, and every refusal.

use std::collections::BTreeMap;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;

use super::refusal;

/// A CDR, a change feed and a facade the lane can follow and map through.
const FEED: &str = "[auth]\nenabled = false\n\n[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[change_feed]\nenabled = true\nwatermarks = \"/tmp/feed.redb\"\ndead_letters = \"/tmp/dead-letters.redb\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n";

/// The keys every complete lane names.
const COMPLETE: &str = "reader = \"fhir-outbound\"\nendpoint.url = \"https://fhir.example.org/r4\"\nemitted = \"/var/lib/ferrobridge/emitted.redb\"\n";

/// Returns a configuration whose `[fhir_outbound]` section adds `keys` to an
/// enabled lane.
fn lane(keys: &str) -> String {
    format!("{FEED}[fhir_outbound]\nenabled = true\n{keys}")
}

#[test]
fn the_outbound_lane_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_sources(None, &BTreeMap::new())?;
    assert!(!config.fhir_outbound.enabled);
    assert_eq!(30_000, config.fhir_outbound.endpoint.timeout.as_millis());
    assert!(config.resolve(Process::Server)?.fhir_outbound.is_none());
    Ok(())
}

#[test]
fn an_enabled_lane_resolves_every_key_and_follows_the_change_feed()
-> Result<(), Box<dyn std::error::Error>> {
    let text = lane(&format!("{COMPLETE}endpoint.timeout = \"5s\"\n"));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let outbound = settings.fhir_outbound.ok_or("the lane is on")?;
    assert!(
        settings.change_feed.is_some(),
        "the lane is the feed's consumer"
    );
    assert_eq!("fhir-outbound", outbound.reader.as_str());
    assert_eq!("https://fhir.example.org/r4", outbound.sink.base().as_str());
    assert_eq!(
        std::path::Path::new("/var/lib/ferrobridge/emitted.redb"),
        outbound.emitted
    );
    Ok(())
}

#[test]
fn the_environment_sets_the_endpoint() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__FHIR_OUTBOUND__ENDPOINT__URL"),
        String::from("https://fhir.example.org/other"),
    )]);
    let settings =
        Config::from_sources(Some(&lane(COMPLETE)), &environment)?.resolve(Process::Server)?;
    let outbound = settings.fhir_outbound.ok_or("the lane is on")?;
    assert_eq!(
        "https://fhir.example.org/other",
        outbound.sink.base().as_str()
    );
    Ok(())
}

#[test]
fn the_basic_password_reads_from_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("fhir-password");
    std::fs::write(&file, "synthetic-password\n")?;
    let text = lane(&format!(
        "{COMPLETE}\n[fhir_outbound.endpoint.credentials]\nuser = \"bridge\"\npassword_file = \"{}\"\n",
        file.display()
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let rendered = format!("{:?}", settings.fhir_outbound.ok_or("the lane is on")?.sink);
    assert!(
        rendered.contains("Basic"),
        "the endpoint is reached with the Basic credential"
    );
    assert!(
        !rendered.contains("synthetic-password"),
        "the password is redacted in the rendering"
    );
    Ok(())
}

#[test]
fn the_lane_needs_the_change_feed_and_the_facade() -> Result<(), Box<dyn std::error::Error>> {
    for (text, needed) in [
        (
            format!("[auth]\nenabled = false\n\n[fhir_outbound]\nenabled = true\n{COMPLETE}"),
            "change_feed.enabled",
        ),
        (
            format!(
                "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[change_feed]\nenabled = true\nwatermarks = \"/tmp/feed.redb\"\ndead_letters = \"/tmp/dead-letters.redb\"\n\n[fhir_outbound]\nenabled = true\n{COMPLETE}"
            ),
            "facade.enabled",
        ),
    ] {
        let error = refusal(&text)?;
        assert!(
            matches!(&error, Error::Needs { key, needs } if key == "fhir_outbound.enabled" && needs == needed),
            "{needed}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn a_missing_key_is_refused_by_name() -> Result<(), Box<dyn std::error::Error>> {
    for (keys, missing) in [
        (
            "endpoint.url = \"https://fhir.example.org/r4\"\nemitted = \"/tmp/e.redb\"\n",
            "fhir_outbound.reader",
        ),
        (
            "reader = \"fhir-outbound\"\nendpoint.url = \"https://fhir.example.org/r4\"\n",
            "fhir_outbound.emitted",
        ),
    ] {
        let error = refusal(&lane(keys))?;
        assert!(
            matches!(&error, Error::Missing { key } if key == missing),
            "{missing}: {error:?}"
        );
    }
    let error = refusal(&lane(
        "reader = \"fhir-outbound\"\nemitted = \"/tmp/e.redb\"\n",
    ))?;
    assert!(
        refused_endpoint(&error, "url"),
        "the endpoint url: {error:?}"
    );
    Ok(())
}

#[test]
fn a_cleartext_endpoint_is_refused_unless_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let keys = COMPLETE.replace("https://fhir.example.org", "http://fhir.invalid");
    let error = refusal(&lane(&keys))?;
    assert!(
        matches!(&error, Error::FetchBase { key, .. } if key == "fhir_outbound.endpoint.url"),
        "{error:?}"
    );
    let accepted = lane(&format!("{keys}cleartext = true\n"));
    assert!(
        Config::from_sources(Some(&accepted), &BTreeMap::new())?
            .resolve(Process::Server)?
            .fhir_outbound
            .is_some()
    );
    Ok(())
}

#[test]
fn another_consumers_reader_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{}[events]\nenabled = true\nreader = \"fhir-outbound\"\nurl = \"amqp://broker.invalid\"\nexchange = \"x\"\n",
        lane(COMPLETE)
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Duplicate { key, .. } if key == "fhir_outbound.reader"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn the_facade_itself_is_refused_as_the_endpoint() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{}endpoint.url = \"https://bridge.example.org/fhir\"\n",
        lane("reader = \"fhir-outbound\"\nemitted = \"/tmp/e.redb\"\n")
    )
    .replace(
        "[facade]\nenabled = true\n",
        "[facade]\nenabled = true\nbase_url = \"https://bridge.example.org/fhir\"\n",
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Duplicate { key, .. } if key == "fhir_outbound.endpoint.url"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_zero_timeout_and_an_unknown_key_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&lane(&format!("{COMPLETE}endpoint.timeout = \"0s\"\n")))?;
    assert!(refused_endpoint(&error, "timeout"), "{error:?}");
    assert!(
        Config::from_sources(
            Some(&lane(&format!("{COMPLETE}url = \"x\"\n"))),
            &BTreeMap::new()
        )
        .is_err()
    );
    Ok(())
}

/// Whether `error` is the outbound client's refusal of
/// `[fhir_outbound.endpoint]`, naming `key` in its cause.
fn refused_endpoint(error: &Error, key: &str) -> bool {
    let Error::Client { upstream, source } = error else {
        return false;
    };
    let mut text = source.to_string();
    let mut cause = std::error::Error::source(source.as_ref());
    while let Some(next) = cause {
        text.push_str(": ");
        text.push_str(&next.to_string());
        cause = next.source();
    }
    *upstream == "fhir_outbound" && text.contains("fhir_outbound.endpoint") && text.contains(key)
}
