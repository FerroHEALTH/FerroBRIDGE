// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[cdr] committer_issuer`: the issuer a CDR that writes the committer itself
//! names a Basic user under, and its refusal beside any other credential.

use std::collections::BTreeMap;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;

/// Returns a configuration with `[cdr]` carrying `committer_issuer` and the
/// credential tables `tables`.
fn cdr(tables: &str) -> String {
    format!(
        "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\ncommitter_issuer = \"urn:example:cdr\"\n\n{tables}"
    )
}

/// Returns the refusal `text` resolves to.
fn refusal(text: &str) -> Result<Error, Box<dyn std::error::Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve(Process::Job)
        .err()
        .ok_or("the configuration should be refused")?)
}

#[test]
fn a_basic_user_takes_the_committer_issuer() -> Result<(), Box<dyn std::error::Error>> {
    let text = cdr(concat!(
        "[cdr.endpoint.credentials]\n",
        "user = \"synthetic-user\"\n",
        "password = \"synthetic-password\"\n",
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Job)?;
    assert_eq!(
        Some("urn:example:cdr"),
        settings
            .cdr
            .ok_or("the CDR lane is on")?
            .committer_issuer
            .as_deref()
    );
    Ok(())
}

#[test]
fn a_committer_issuer_with_no_basic_user_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let granted = cdr(concat!(
        "[cdr.endpoint.credentials.oauth]\n",
        "token_endpoint = \"https://as.example.org/token\"\n",
        "client_id = \"ferrobridge\"\n",
        "client_secret = \"synthetic-secret\"\n",
    ));
    for text in [granted, cdr("")] {
        let error = refusal(&text)?;
        assert!(
            matches!(
                &error,
                Error::Needs { key, needs }
                    if key == "cdr.committer_issuer" && needs == "cdr.endpoint.credentials.user"
            ),
            "{error:?}"
        );
    }
    Ok(())
}
