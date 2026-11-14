// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[facade.consent]`: off by default, an access default the deployment
//! states, and the administrative root the catalogue is read under.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use openehr_rm::draft::access_control::terminology::AccessDefault;
use std::collections::BTreeMap;

/// The CDR lane with an onward table, the lane the exchange needs.
const CDR: &str = concat!(
    "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n",
    "[cdr.endpoint.credentials.oauth]\n",
    "token_endpoint = \"https://as.example.org/token\"\n",
    "client_id = \"ferrobridge\"\n",
    "client_secret = \"synthetic-secret\"\n",
    "resource = \"https://cdr.example/openehr\"\n\n",
    "[cdr.endpoint.credentials.onward]\n",
    "issuer = \"https://as.example.org\"\n",
    "kind = \"persons\"\n",
    "token_endpoint = \"https://as.example.org/token\"\n",
    "audiences = [\"https://cdr.example/openehr\"]\n",
    "indicators = [\"https://cdr.example/openehr\"]\n",
    "[cdr.endpoint.credentials.onward.keys]\n",
    "max_onward_lifetime = \"5min\"\n\n",
);

/// An enabled facade with `consent` as its `[facade.consent]` table, after
/// the CDR lane `cdr`.
fn with_cdr(cdr: &str, consent: &str) -> String {
    format!(
        "{cdr}[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n[facade.consent]\n{consent}"
    )
}

/// An enabled facade over an onward CDR lane with `consent` as its
/// `[facade.consent]` table.
fn with_consent(consent: &str) -> String {
    with_cdr(CDR, consent)
}

/// The enabled exchange every refusal case starts from.
const ENABLED: &str =
    "enabled = true\ndefault_access = \"open\"\nextensions_url = \"https://cdr.example/ext/v1\"\n";

#[test]
fn the_exchange_is_off_until_a_deployment_turns_it_on() -> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(&with_consent("")), &BTreeMap::new())?.resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(None, facade.consent);
    Ok(())
}

#[test]
fn an_enabled_exchange_reads_its_access_default_and_extensions_root()
-> Result<(), Box<dyn std::error::Error>> {
    let text = with_consent(
        "enabled = true\ndefault_access = \"consent_required\"\nextensions_url = \"https://cdr.example/ext/v1\"\n",
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Job)?;
    let consent = settings
        .facade
        .and_then(|facade| facade.consent)
        .ok_or("the exchange is on")?;
    assert_eq!(AccessDefault::ConsentRequired, consent.default_access);
    assert_eq!(
        "https://cdr.example/ext/v1",
        consent.extensions_root.as_str()
    );
    Ok(())
}

#[test]
fn an_enabled_exchange_without_an_access_default_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    let error = Config::from_sources(
        Some(&with_consent(
            "enabled = true\nextensions_url = \"https://cdr.example/ext/v1\"\n",
        )),
        &BTreeMap::new(),
    )?
    .resolve(Process::Job)
    .expect_err("the access default has no default");
    match &error {
        Error::Missing { key } => assert_eq!("facade.consent.default_access", key),
        other => return Err(format!("expected a missing key, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_access_default_outside_the_three_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_consent(&ENABLED.replace("\"open\"", "\"closed\""));
    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Job)
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
    let text = with_consent(&format!("{ENABLED}catalogue = \"x\"\n"));
    assert!(
        Config::from_sources(Some(&text), &BTreeMap::new()).is_err(),
        "deny_unknown_fields names the key"
    );
}

#[test]
fn an_enabled_exchange_without_an_extensions_root_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    let text = with_consent("enabled = true\ndefault_access = \"open\"\n");
    let error = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve(Process::Job)
        .expect_err("the EHR_ACCESS reads live under the extensions root");
    match &error {
        Error::Missing { key } => assert_eq!("facade.consent.extensions_url", key),
        other => return Err(format!("expected a missing key, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_enabled_exchange_without_an_onward_lane_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    let plain = "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n";
    let error = Config::from_sources(Some(&with_cdr(plain, ENABLED)), &BTreeMap::new())?
        .resolve(Process::Job)
        .expect_err("the directives are read for the caller, never under the bridge's credential");
    match &error {
        Error::Needs { key, needs } => {
            assert_eq!("facade.consent.enabled", key);
            assert_eq!("cdr.endpoint.credentials.onward", needs);
        }
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    Ok(())
}
