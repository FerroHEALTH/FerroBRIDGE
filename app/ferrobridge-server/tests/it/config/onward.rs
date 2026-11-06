// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[cdr.credentials.onward]` table: the onward lane the facade reaches
//! the CDR under its caller through, and every refusal of it.

use std::collections::BTreeMap;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use ferrohealth_smart::config::ConfigError;
use ferrohealth_smart::kind::IssuerKind;

/// The client credentials grant every case's lane authenticates with.
const OAUTH: &str = concat!(
    "[cdr.credentials.oauth]\n",
    "token_endpoint = \"https://as.example.org/token\"\n",
    "client_id = \"ferrobridge\"\n",
    "client_secret = \"synthetic-secret\"\n\n",
);

/// A complete onward table: a `persons` issuer exchanging under RFC 8693.
const ONWARD: &str = concat!(
    "[cdr.credentials.onward]\n",
    "issuer = \"https://as.example.org\"\n",
    "kind = \"persons\"\n",
    "token_endpoint = \"https://as.example.org/token\"\n",
    "audiences = [\"https://cdr.example/openehr\"]\n",
    "indicators = [\"https://cdr.example/openehr\"]\n",
    "[cdr.credentials.onward.keys]\n",
    "max_onward_lifetime = 300\n",
);

/// Returns the text of a CDR lane carrying `tables` under its credentials.
fn cdr(tables: &str) -> String {
    format!("[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n{tables}")
}

/// Returns the refusal `text` resolves to as `process`.
fn refusal(text: &str, process: Process) -> Result<Error, Box<dyn std::error::Error>> {
    match Config::from_sources(Some(text), &BTreeMap::new()) {
        Err(error) => Ok(error),
        Ok(config) => Ok(config
            .resolve(process)
            .err()
            .ok_or("the configuration should be refused")?),
    }
}

#[test]
fn an_onward_table_resolves_into_the_cdr_lane() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(&cdr(&format!("{OAUTH}{ONWARD}"))), &BTreeMap::new())?
        .resolve(Process::Job)?;
    let onward = settings
        .cdr
        .ok_or("the CDR lane is on")?
        .onward
        .ok_or("the onward lane is on")?;
    assert_eq!(IssuerKind::Persons, onward.kind());
    assert_eq!(
        ["https://cdr.example/openehr"],
        onward.target().audiences(),
        "the target names the CDR alone"
    );
    Ok(())
}

#[test]
fn a_cdr_lane_without_an_onward_table_has_no_onward_lane() -> Result<(), Box<dyn std::error::Error>>
{
    let settings =
        Config::from_sources(Some(&cdr(OAUTH)), &BTreeMap::new())?.resolve(Process::Job)?;
    assert!(settings.cdr.ok_or("the CDR lane is on")?.onward.is_none());
    Ok(())
}

#[test]
fn an_onward_table_without_its_oauth_table_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&cdr(ONWARD), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Credentials { section, source }
                if section == "cdr.credentials" && matches!(source.as_ref(), ConfigError::Missing { .. })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_table_without_a_kind_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let table = ONWARD.replace("kind = \"persons\"\n", "");
    let error = refusal(&cdr(&format!("{OAUTH}{table}")), Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::Credentials { source, .. }
                if matches!(source.as_ref(), ConfigError::Missing { key } if *key == "credentials.onward.kind")
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_table_on_a_server_without_auth_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "[auth]\nenabled = false\n\n{}",
        cdr(&format!("{OAUTH}{ONWARD}"))
    );
    let error = refusal(&text, Process::Server)?;
    assert!(
        matches!(
            &error,
            Error::Needs { key, needs }
                if key == "cdr.credentials.onward" && needs == "auth.enabled"
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_onward_table_on_the_terminology_lane_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "[terminology]\nbase_url = \"http://tx.invalid/r4\"\nwire_version = \"r4\"\n\n{}{}",
        OAUTH.replace("[cdr.", "[terminology."),
        ONWARD.replace("[cdr.", "[terminology."),
    );
    let error = refusal(&text, Process::Job)?;
    assert!(
        matches!(
            &error,
            Error::OnwardLane { section } if section == "terminology.credentials"
        ),
        "{error:?}"
    );
    Ok(())
}
