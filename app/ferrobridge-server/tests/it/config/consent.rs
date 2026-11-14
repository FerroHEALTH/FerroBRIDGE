// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[facade.consent]`: off by default, an access default the deployment
//! states, and the administrative root the catalogue is read under.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use openehr_rm::draft::access_control::terminology::AccessDefault;
use std::collections::BTreeMap;

/// An enabled facade with `consent` as its `[facade.consent]` table.
fn with_consent(consent: &str) -> String {
    format!(
        "[auth]\nenabled = false\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n[facade.consent]\n{consent}"
    )
}

#[test]
fn the_exchange_is_off_until_a_deployment_turns_it_on() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(&with_consent("")), &BTreeMap::new())?
        .resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(None, facade.consent);
    Ok(())
}

#[test]
fn an_enabled_exchange_reads_its_access_default_and_admin_root()
-> Result<(), Box<dyn std::error::Error>> {
    let text = with_consent(
        "enabled = true\ndefault_access = \"consent_required\"\nadmin_url = \"https://cdr.example/\"\n",
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let consent = settings
        .facade
        .and_then(|facade| facade.consent)
        .ok_or("the exchange is on")?;
    assert_eq!(AccessDefault::ConsentRequired, consent.default_access);
    assert_eq!(
        Some("https://cdr.example/"),
        consent.admin_root.as_ref().map(url::Url::as_str)
    );
    Ok(())
}

#[test]
fn an_enabled_exchange_without_an_access_default_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    let error = Config::from_sources(Some(&with_consent("enabled = true\n")), &BTreeMap::new())?
        .resolve(Process::Server)
        .expect_err("the access default has no default");
    match &error {
        Error::Missing { key } => assert_eq!("facade.consent.default_access", key),
        other => return Err(format!("expected a missing key, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_access_default_outside_the_three_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_consent("enabled = true\ndefault_access = \"closed\"\n");
    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Server)
        .expect_err("the access default is one of three values");
    match &error {
        Error::AccessDefault { key, value } => {
            assert_eq!("facade.consent.default_access", key);
            assert_eq!("closed", value);
        }
        other => return Err(format!("expected an access default error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_unknown_key_in_the_table_is_refused() {
    let text = with_consent("enabled = true\ndefault_access = \"open\"\ncatalogue = \"x\"\n");
    assert!(
        Config::from_sources(Some(&text), &BTreeMap::new()).is_err(),
        "deny_unknown_fields names the key"
    );
}
