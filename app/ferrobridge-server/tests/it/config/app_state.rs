// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[facade.app_state]`: off by default, its store file named by the
//! deployment, and settable from the environment.

use std::collections::BTreeMap;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;

/// An enabled facade with `app_state` as its `[facade.app_state]` table.
fn with_app_state(app_state: &str) -> String {
    format!(
        "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[identity_source]\npseudonym_domain = \"urn:oid:2.999.9\"\nsource_domains = [\"urn:oid:2.999.1\"]\nendpoint.url = \"http://127.0.0.1:9/fhir\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n[facade.app_state]\n{app_state}"
    )
}

#[test]
fn app_state_is_off_until_a_deployment_turns_it_on() -> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(&with_app_state("")), &BTreeMap::new())?.resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert!(!facade.app_state);
    Ok(())
}

#[test]
fn an_enabled_endpoint_keeps_its_state_in_the_state_store() -> Result<(), Box<dyn std::error::Error>>
{
    let settings =
        Config::from_sources(Some(&with_app_state("enabled = true\n")), &BTreeMap::new())?
            .resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert!(facade.app_state);
    Ok(())
}

#[test]
fn the_environment_turns_it_on() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__FACADE__APP_STATE__ENABLED"),
        String::from("true"),
    )]);
    let settings =
        Config::from_sources(Some(&with_app_state("")), &environment)?.resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert!(facade.app_state);
    Ok(())
}

#[test]
fn an_unknown_key_refuses_to_boot() {
    assert!(
        Config::from_sources(Some(&with_app_state("enable = true\n")), &BTreeMap::new()).is_err()
    );
}

#[test]
fn the_removed_store_key_refuses_to_boot() {
    assert!(
        Config::from_sources(
            Some(&with_app_state(
                "enabled = true\nstore = \"/var/lib/ferrobridge/app-state.redb\"\n"
            )),
            &BTreeMap::new()
        )
        .is_err()
    );
}
