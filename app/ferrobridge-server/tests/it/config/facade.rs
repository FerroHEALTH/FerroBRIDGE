// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[facade]` section: off until present, its policy, its store, and
//! its refusals.

use ferrobridge_server::config::{Config, Error};
use std::collections::BTreeMap;
use std::error::Error as StdError;

#[test]
fn the_facade_is_off_until_its_section_turns_it_on() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(None, &BTreeMap::new())?.resolve()?;
    assert!(
        settings.facade.is_none(),
        "the facade lane is off until [facade] enabled is set"
    );
    assert!(settings.mapping_directory.is_none());
    Ok(())
}

#[test]
fn an_enabled_facade_resolves_its_policy_and_its_store() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[mappings]\ndirectory = \"/srv/mappings\"\ntemplates = \"/srv/templates\"\n\n",
        "[facade]\nenabled = true\n",
        "base_url = \"https://bridge.invalid/fhir\"\n",
        "ehr_policy = \"create_on_first_write\"\n",
        "identity_store = \"/var/lib/ferrobridge/identity.redb\"\n",
        "composition_language = \"en\"\ncomposition_territory = \"GB\"\n",
    );

    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!("https://bridge.invalid/fhir", facade.settings.base_url);
    assert_eq!(
        ferrobridge_server::facade::ehr::Policy::CreateOnFirstWrite,
        facade.settings.ehr_policy
    );
    assert_eq!("en", facade.settings.language);
    assert_eq!("GB", facade.settings.territory);
    assert_eq!(
        std::path::Path::new("/var/lib/ferrobridge/identity.redb"),
        facade.identity_store
    );
    assert_eq!(
        Some(std::path::Path::new("/srv/mappings")),
        settings.mapping_directory.as_deref()
    );
    Ok(())
}

#[test]
fn an_ehr_policy_the_facade_does_not_know_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[facade]\nenabled = true\nehr_policy = \"whatever\"\n",
        "composition_language = \"en\"\ncomposition_territory = \"GB\"\n",
    );

    let error = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .expect_err("the policy is one of two values");
    match &error {
        Error::EhrPolicy { key, value } => {
            assert_eq!("facade.ehr_policy", key);
            assert_eq!("whatever", value);
        }
        other => return Err(format!("expected an ehr policy error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_enabled_facade_without_a_composition_language_refuses_to_boot()
-> Result<(), Box<dyn StdError>> {
    let text = "[facade]\nenabled = true\ncomposition_territory = \"GB\"\n";

    let error = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .expect_err("a clinical record has no default language");
    match &error {
        Error::Missing { key } => assert_eq!("facade.composition_language", key),
        other => return Err(format!("expected a missing key, got {other:?}").into()),
    }
    Ok(())
}

/// The smallest enabled facade section.
const ENABLED: &str =
    "[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n";

#[test]
fn the_search_page_sizes_default_and_resolve_as_configured() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(Some(ENABLED), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(
        ferrobridge_server::facade::search::Limits::default(),
        facade.search
    );
    let text = format!("{ENABLED}search_count = 5\nsearch_count_max = 50\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(5, facade.search.default_count);
    assert_eq!(50, facade.search.max_count);
    Ok(())
}

#[test]
fn a_search_page_size_no_page_can_hold_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    for (keys, refused) in [
        ("search_count_max = 0\n", "facade.search_count_max"),
        ("search_count = 0\n", "facade.search_count"),
        (
            "search_count = 60\nsearch_count_max = 50\n",
            "facade.search_count",
        ),
    ] {
        let text = format!("{ENABLED}{keys}");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve()
            .expect_err("the page size is refused");
        match &error {
            Error::SearchCount { key, .. } => assert_eq!(refused, key, "{keys}"),
            other => return Err(format!("expected a search count error, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn by_default_the_facade_fetches_no_absolute_reference() -> Result<(), Box<dyn StdError>> {
    // engine/references.adoc: "If not possible, the engine proceeds with the
    // mapping", so an empty allow-list is a deployment that fetches nothing.
    let settings = Config::from_sources(Some(ENABLED), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert!(facade.references.fetch_bases().is_empty());
    assert_eq!(
        ferrobridge_server::facade::references::DEFAULT_MAX_DEPTH,
        facade.references.max_depth()
    );
    Ok(())
}

#[test]
fn the_reference_section_resolves_its_bases_and_its_depth() -> Result<(), Box<dyn StdError>> {
    let text = format!(
        "{ENABLED}\n[facade.references]\nfetch_bases = [\"https://sender.invalid/fhir\"]\nmax_depth = 1\ntimeout_ms = 2500\n"
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    let bases: Vec<&str> = facade
        .references
        .fetch_bases()
        .iter()
        .map(url::Url::as_str)
        .collect();
    assert_eq!(vec!["https://sender.invalid/fhir"], bases);
    assert_eq!(1, facade.references.max_depth());
    Ok(())
}

#[test]
fn the_reference_bases_are_set_from_the_environment() -> Result<(), Box<dyn StdError>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__FACADE__REFERENCES__FETCH_BASES"),
        String::from("[\"https://sender.invalid/fhir\"]"),
    )]);
    let settings = Config::from_sources(Some(ENABLED), &environment)?.resolve()?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(1, facade.references.fetch_bases().len());
    Ok(())
}

#[test]
fn a_fetch_base_that_is_no_http_base_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    for value in [
        "ftp://sender.invalid/fhir",
        "https://sender.invalid/fhir?x=1",
    ] {
        let text = format!("{ENABLED}\n[facade.references]\nfetch_bases = [\"{value}\"]\n");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve()
            .expect_err("only an http or https base is fetched from");
        match &error {
            Error::FetchBase { key, value: held } => {
                assert_eq!("facade.references.fetch_bases", key);
                assert_eq!(value, held);
            }
            other => return Err(format!("expected a fetch base error, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn an_unknown_reference_key_refuses_to_boot() {
    let text = format!("{ENABLED}\n[facade.references]\nfetch_everything = true\n");
    assert!(Config::from_sources(Some(&text), &BTreeMap::new()).is_err());
}
