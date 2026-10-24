// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET /operator/configuration`: the configuration the bridge started on,
//! each key with its value and its source, every secret by reference alone.
//! No specification governs the operator surface: our own design.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::io::Write as _;

use ferrobridge_operator_api::configuration::{Configuration, SecretState, Source, Value};
use ferrobridge_server::config::Config;
use ferrobridge_server::telemetry::{Rendering, subscriber};
use http::StatusCode;

use super::{Case, OPERATOR, bytes};
use crate::support;

/// The secret values the configuration below sets, none of which may reach
/// the document or a log line.
const SECRETS: [&str; 4] = [
    "SYNTHETIC-CDR-CLIENT-SECRET",
    "SYNTHETIC-TERMINOLOGY-PASSWORD",
    "SYNTHETIC-CDM-PASSWORD",
    "c3ludGhldGljLXNhbHQtMTY",
];

/// Returns the configuration file of the case, with the terminology
/// password read from `password_file`, and the file's text.
fn file_text(password_file: &std::path::Path) -> String {
    format!(
        concat!(
            "[server]\nlisten = \"127.0.0.1:18080\"\n\n",
            "[auth]\nenabled = true\nissuer = \"https://as.example.org/realms/ferro\"\n",
            "audience = [\"https://bridge.example.org/fhir\"]\n",
            "product_scope = \"__ferrobridge.operator\"\n\n",
            "[[auth.basic.users]]\nuser = \"etl\"\n",
            "password_hash = \"$argon2id$v=19$m=19456,t=2,p=1$c3ludGhldGljLXNhbHQtMTY$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"\n\n",
            "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n",
            "[cdr.credentials.oauth]\ntoken_endpoint = \"https://as.example.org/token\"\n",
            "client_id = \"ferrobridge\"\nclient_secret = \"SYNTHETIC-CDR-CLIENT-SECRET\"\n\n",
            "[terminology]\nbase_url = \"http://term.invalid/fhir\"\n\n",
            "[terminology.credentials]\nuser = \"bridge\"\npassword_file = {}\n\n",
            "[cdm]\nurl = \"postgres://bridge:SYNTHETIC-CDM-PASSWORD@db.invalid/cdm?sslmode=disable\"\n\n",
            "[operator]\nenabled = true\n",
        ),
        toml::Value::String(password_file.display().to_string())
    )
}

/// The environment of the case: one override over the file.
fn environment() -> BTreeMap<String, String> {
    BTreeMap::from([(
        String::from("FERROBRIDGE__SERVER__REQUEST_TIMEOUT_MS"),
        String::from("5000"),
    )])
}

/// Resolves the case's configuration and returns the running configuration
/// the operator API answers, with the password file kept alive.
fn running() -> Result<(Configuration, tempfile::NamedTempFile), Box<dyn StdError>> {
    let mut password = tempfile::NamedTempFile::new()?;
    writeln!(password, "SYNTHETIC-TERMINOLOGY-PASSWORD")?;
    let settings =
        Config::from_sources(Some(&file_text(password.path())), &environment())?.resolve()?;
    let configuration = settings
        .configuration
        .ok_or("an operator API answers its configuration")?;
    Ok((configuration, password))
}

/// Returns the value and source of `key` in `configuration`.
fn entry<'c>(
    configuration: &'c Configuration,
    key: &str,
) -> Result<(&'c Value, Source), Box<dyn StdError>> {
    let entry = configuration
        .entry(key)
        .ok_or_else(|| format!("no entry for {key}"))?;
    Ok((&entry.value, entry.source))
}

#[test]
fn each_key_carries_its_value_and_whether_the_file_the_environment_or_the_default_set_it()
-> Result<(), Box<dyn StdError>> {
    let (configuration, _password) = running()?;
    assert_eq!(
        (
            &Value::String(String::from("127.0.0.1:18080")),
            Source::File
        ),
        entry(&configuration, "server.listen")?
    );
    assert_eq!(
        (&Value::Integer(5000), Source::Environment),
        entry(&configuration, "server.request_timeout_ms")?
    );
    assert_eq!(
        (&Value::Integer(10_000), Source::Default),
        entry(&configuration, "server.shutdown_timeout_ms")?
    );
    assert_eq!(
        (&Value::Boolean(true), Source::File),
        entry(&configuration, "operator.enabled")?
    );
    assert_eq!(
        (
            &Value::String(String::from("__ferrobridge.operator")),
            Source::File
        ),
        entry(&configuration, "auth.product_scope")?
    );
    assert_eq!(None, configuration.file, "the sources named no file path");
    configuration.read_at.parse::<jiff::Timestamp>()?;
    Ok(())
}

#[test]
fn a_secret_is_its_state_and_its_file_never_its_value() -> Result<(), Box<dyn StdError>> {
    let (configuration, password) = running()?;
    assert_eq!(
        (&Value::Secret(SecretState::Set), Source::File),
        entry(&configuration, "cdr.credentials.oauth.client_secret")?
    );
    assert_eq!(
        (
            &Value::Secret(SecretState::File {
                path: password.path().display().to_string()
            }),
            Source::File
        ),
        entry(&configuration, "terminology.credentials.password")?
    );
    assert!(
        configuration
            .entry("terminology.credentials.password_file")
            .is_none(),
        "a secret and its file are one entry"
    );
    assert_eq!(
        (&Value::Secret(SecretState::Unset), Source::Default),
        entry(&configuration, "cdr.credentials.password")?
    );
    assert_eq!(
        (&Value::Secret(SecretState::Set), Source::File),
        entry(&configuration, "cdm.url")?
    );
    let (users, _) = entry(&configuration, "auth.basic.users")?;
    let Value::List(users) = users else {
        return Err(format!("the users are a list: {users:?}").into());
    };
    let Some(Value::Table(user)) = users.first() else {
        return Err(format!("one user table: {users:?}").into());
    };
    assert_eq!(
        Some(&Value::Secret(SecretState::Set)),
        user.get("password_hash")
    );
    let text = serde_json::to_string(&configuration)?;
    for secret in SECRETS {
        assert!(
            !text.contains(secret),
            "{secret} reached the document: {text}"
        );
    }
    Ok(())
}

#[test]
fn no_secret_reaches_the_answer_or_the_log() -> Result<(), Box<dyn StdError>> {
    let logs = support::Logs::default();
    let capture = subscriber(Rendering::Json, "trace", false, logs.clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let body = tracing::subscriber::with_default(capture, || {
        runtime.block_on(async {
            let (configuration, _password) = running()?;
            let case = Case::answering(Some(configuration.clone())).await?;
            let response = case.get("/operator/configuration", Some(OPERATOR)).await?;
            assert_eq!(StatusCode::OK, response.status());
            let body = bytes(response).await?;
            let answered: Configuration = serde_json::from_slice(&body)?;
            assert_eq!(configuration, answered);
            Ok::<_, Box<dyn StdError>>(String::from_utf8(body)?)
        })
    })?;
    let logged = logs.text();
    assert!(
        logged.contains("/operator/configuration"),
        "the request is logged: {logged}"
    );
    for secret in SECRETS {
        assert!(
            !body.contains(secret),
            "{secret} reached the answer: {body}"
        );
        assert!(
            !logged.contains(secret),
            "{secret} reached the log: {logged}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_operator_api_with_no_running_configuration_answers_404() -> Result<(), Box<dyn StdError>>
{
    let case = Case::new().await?;
    let response = case.get("/operator/configuration", Some(OPERATOR)).await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

#[test]
fn an_unset_operator_scope_is_answered_as_the_scope_the_bridge_runs_on()
-> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[auth]\nenabled = true\nissuer = \"https://as.example.org/realms/ferro\"\n",
        "audience = [\"https://bridge.example.org/fhir\"]\n\n",
        "[operator]\nenabled = true\n",
    );
    let configuration = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()?
        .configuration
        .ok_or("an operator API answers its configuration")?;
    assert_eq!(
        (
            &Value::String(String::from("__ferrobridge.operator")),
            Source::Default
        ),
        entry(&configuration, "auth.product_scope")?
    );
    Ok(())
}
