// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[omop_incremental]` section: off until enabled, counted as a
//! consumer of the change feed, and every refusal.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use std::collections::BTreeMap;

use super::refusal;

/// A CDR, a change feed, a CDM database, an `[etl]` section and an OMOCL
/// directory the load runs over.
const BASE: &str = r#"
[cdr]
endpoint.url = "http://cdr.invalid/v1"

[change_feed]
enabled = true
watermarks = "/tmp/feed.redb"
dead_letters = "/tmp/dead-letters.redb"

[cdm]
url = "postgres://db.invalid/cdm?sslmode=disable"

[etl]
aql = "SELECT e/ehr_id/value AS ehr_id, vo/uid/value AS versioned_object_uid, v/uid/value AS version_uid, c AS composition FROM EHR e CONTAINS VERSIONED_OBJECT vo CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c ORDER BY v/uid/value"
type_concept_id = 32817
observation_period_type_concept_id = 32880

[mappings]
omocl = "/tmp/omocl"
"#;

/// Returns `base` with an enabled `[omop_incremental]` that adds `keys`.
fn load(base: &str, keys: &str) -> String {
    format!("{base}\n[omop_incremental]\nenabled = true\n{keys}")
}

#[test]
fn the_incremental_load_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_sources(None, &BTreeMap::new())?;
    assert!(!config.omop_incremental.enabled);
    assert_eq!("", config.omop_incremental.reader);
    assert!(config.resolve(Process::Server)?.omop_incremental.is_none());
    Ok(())
}

#[test]
fn an_enabled_load_is_a_consumer_of_the_change_feed() -> Result<(), Box<dyn std::error::Error>> {
    let text = load(BASE, "reader = \"ferrobridge-omop\"\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let resolved = settings.omop_incremental.ok_or("the load is on")?;
    assert_eq!("ferrobridge-omop", resolved.reader.as_str());
    assert!(
        settings.change_feed.is_some(),
        "the load alone feeds the change feed"
    );
    Ok(())
}

#[test]
fn every_key_reads_from_the_environment() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__OMOP_INCREMENTAL__ENABLED"),
            String::from("true"),
        ),
        (
            String::from("FERROBRIDGE__OMOP_INCREMENTAL__READER"),
            String::from("ferrobridge-omop"),
        ),
    ]);
    let config = Config::from_sources(None, &environment)?;
    assert!(config.omop_incremental.enabled);
    assert_eq!("ferrobridge-omop", config.omop_incremental.reader);
    Ok(())
}

#[test]
fn a_load_without_what_it_runs_over_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    for (dropped, needed) in [
        ("[change_feed]\nenabled = true\n", "change_feed.enabled"),
        ("[cdm]\n", "[cdm]"),
        ("[etl]\n", "[etl]"),
        ("[mappings]\n", "[mappings] omocl"),
    ] {
        let base = drop_section(BASE, dropped);
        let error = refusal(&load(&base, "reader = \"ferrobridge-omop\"\n"))?;
        assert!(
            matches!(&error, Error::Needs { key, needs } if key == "omop_incremental.enabled" && needs == needed),
            "{needed}: {error:?}"
        );
    }
    Ok(())
}

/// Returns `base` without the section that starts with `header`, up to the
/// next blank line.
fn drop_section(base: &str, header: &str) -> String {
    let Some(start) = base.find(header) else {
        return base.to_owned();
    };
    let end = base
        .get(start..)
        .and_then(|rest| rest.find("\n\n"))
        .map_or(base.len(), |end| start + end);
    format!(
        "{}{}",
        base.get(..start).unwrap_or_default(),
        base.get(end..).unwrap_or_default()
    )
}

#[test]
fn a_load_without_a_usable_reader_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let missing = refusal(&load(BASE, ""))?;
    assert!(
        matches!(&missing, Error::Missing { key } if key == "omop_incremental.reader"),
        "{missing:?}"
    );
    let alphabet = refusal(&load(BASE, "reader = \"omop load\"\n"))?;
    assert!(
        matches!(&alphabet, Error::ReaderName { key, .. } if key == "omop_incremental.reader"),
        "{alphabet:?}"
    );
    Ok(())
}

#[test]
fn a_load_under_the_events_lanes_reader_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let text = load(
        &format!(
            "{BASE}\n[events]\nenabled = true\nreader = \"ferrobridge\"\nurl = \"amqp://bridge:synthetic@broker.invalid:5672/%2f\"\nexchange = \"ferroehr.events\"\n"
        ),
        "reader = \"ferrobridge\"\n",
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Duplicate { key, value } if key == "omop_incremental.reader" && value == "ferrobridge"),
        "{error:?}"
    );
    Ok(())
}
