// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `_file` secrets and the credentials sections.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::error::Error;
use secrecy::ExposeSecret;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::io::Write;

#[test]
fn a_secret_file_sibling_is_read_at_boot() -> Result<(), Box<dyn StdError>> {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "synthetic-token")?;
    let text = format!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n[cdr.credentials]\nbearer_token_file = {:?}\n",
        file.path()
    );

    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let cdr = settings.cdr.as_ref().ok_or("the CDR lane is on")?;
    match cdr.credentials.as_ref() {
        Some(ferrobridge_server::cdr::config::CdrCredentials::Static(
            openehr_its::rest::client::Credentials::Bearer(token),
        )) => {
            assert_eq!(
                "synthetic-token",
                token.expose_secret(),
                "the trailing newline of the file is not part of the secret"
            );
        }
        other => return Err(format!("expected a bearer token, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_cdm_url_file_sibling_is_read_at_boot() -> Result<(), Box<dyn StdError>> {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "postgres://bridge@db.invalid/cdm?sslmode=disable")?;
    let text = format!("[cdm]\nurl_file = {:?}\n", file.path());

    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        Some("postgres://bridge@db.invalid/cdm?sslmode=disable"),
        settings.cdm.as_ref().map(|cdm| cdm.url.expose_secret())
    );
    Ok(())
}

#[test]
fn a_cdm_tls_ca_file_is_read_through_its_environment_name() -> Result<(), Box<dyn StdError>> {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "not a certificate")?;
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__CDM__TLS_CA_FILE"),
        file.path().display().to_string(),
    )]);
    let text = "[cdm]\nurl = \"postgres://bridge@db.invalid/cdm?sslmode=verify-full\"\n";

    let error = Config::from_sources(Some(text), &environment)?
        .resolve()
        .expect_err("the file holds no certificate");
    assert!(
        matches!(&error, Error::CdmConnection { key, .. } if key == "cdm.tls_ca_file"),
        "the refusal names the key the variable sets: {error:?}"
    );
    Ok(())
}

#[test]
fn a_value_and_its_file_sibling_together_refuse_to_boot() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n",
        "[cdr.credentials]\nbearer_token = \"inline\"\nbearer_token_file = \"/run/secrets/token\"\n",
    );

    let error = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .expect_err("a secret set twice is a boot error");
    match &error {
        Error::Conflict { key } => assert_eq!("cdr.credentials.bearer_token", key),
        other => return Err(format!("expected a conflict, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_secret_file_that_cannot_be_read_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n",
        "[cdr.credentials]\nbearer_token_file = \"/nonexistent/token\"\n",
    );

    let error = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .expect_err("an unreadable secret is a boot error");
    match &error {
        Error::Secret { key, .. } => {
            assert_eq!("cdr.credentials.bearer_token_file", key);
            assert!(
                StdError::source(&error).is_some(),
                "the refusal keeps the file system's own cause"
            );
        }
        other => return Err(format!("expected a secret error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_credentials_section_naming_two_schemes_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n",
        "[cdr.credentials]\nbearer_token = \"t\"\nuser = \"bridge\"\npassword = \"p\"\n",
    );

    let error = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .expect_err("one scheme at a time");
    match &error {
        Error::Scheme { section } => assert_eq!("cdr.credentials", section),
        other => return Err(format!("expected a scheme error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_user_without_a_password_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n",
        "[cdr.credentials]\nuser = \"bridge\"\n",
    );

    let error = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .expect_err("basic authentication needs both halves");
    match &error {
        Error::Missing { key } => assert_eq!("cdr.credentials.password", key),
        other => return Err(format!("expected a missing key, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_user_file_sibling_is_read_at_boot() -> Result<(), Box<dyn StdError>> {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "12345")?;
    let text = format!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n[cdr.credentials]\nuser_file = {:?}\npassword = \"p\"\n",
        file.path()
    );

    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let cdr = settings.cdr.as_ref().ok_or("the CDR lane is on")?;
    match cdr.credentials.as_ref() {
        Some(ferrobridge_server::cdr::config::CdrCredentials::Static(
            openehr_its::rest::client::Credentials::Basic { user, .. },
        )) => {
            assert_eq!(
                "12345", user,
                "the trailing newline is not part of the user"
            );
        }
        other => return Err(format!("expected basic authentication, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_user_and_its_file_sibling_together_refuse_to_boot() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n",
        "[cdr.credentials]\nuser = \"bridge\"\nuser_file = \"/run/secrets/user\"\npassword = \"p\"\n",
    );

    let error = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .expect_err("a user set twice is a boot error");
    match &error {
        Error::Conflict { key } => assert_eq!("cdr.credentials.user", key),
        other => return Err(format!("expected a conflict, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn no_inline_secret_reaches_the_debug_rendering_of_the_tree_or_the_settings()
-> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n",
        "[cdr.credentials]\nuser = \"bridge\"\npassword = \"synthetic-cdr-password\"\n\n",
        "[terminology]\nbase_url = \"http://tx.invalid/fhir\"\n\n",
        "[terminology.credentials]\nbearer_token = \"synthetic-tx-token\"\n\n",
        "[cdm]\nurl = \"postgres://bridge:synthetic-db-password@db.invalid/cdm?sslmode=disable\"\n",
    );
    let config = Config::from_sources(Some(text), &BTreeMap::new())?;
    let tree = format!("{config:?}");
    let settings = format!("{:?}", config.resolve()?);
    for rendered in [&tree, &settings] {
        for secret in [
            "synthetic-cdr-password",
            "synthetic-tx-token",
            "synthetic-db-password",
        ] {
            assert!(
                !rendered.contains(secret),
                "a secret reached the Debug rendering"
            );
        }
    }
    Ok(())
}

#[test]
fn no_inline_client_secret_reaches_the_debug_rendering_of_the_tree() -> Result<(), Box<dyn StdError>>
{
    let text = concat!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n",
        "[cdr.credentials.oauth]\ntoken_endpoint = \"https://idp.invalid/token\"\n",
        "client_id = \"bridge\"\nclient_secret = \"synthetic-client-secret\"\n",
    );
    let config = Config::from_sources(Some(text), &BTreeMap::new())?;
    let tree = format!("{config:?}");
    let settings = format!("{:?}", config.resolve()?);
    assert!(
        !tree.contains("synthetic-client-secret"),
        "the client secret reached the Debug rendering of the tree"
    );
    assert!(
        !settings.contains("synthetic-client-secret"),
        "the client secret reached the Debug rendering of the settings"
    );
    Ok(())
}
