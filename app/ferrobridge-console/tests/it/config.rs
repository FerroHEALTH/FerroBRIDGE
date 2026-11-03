// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration: the file, the `FERROBRIDGE_CONSOLE__` overrides over
//! it, the checks of each value, and the `_file` secrets.

use std::collections::BTreeMap;
use std::time::Duration;

use ferrobridge_console::config::Config;
use ferrobridge_console::config::error::Error;
use ferrohealth_secret::file::SecretError;

fn environment(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn no_file_and_no_environment_resolve_to_the_defaults() {
    let settings = Config::from_sources(None, &BTreeMap::new())
        .expect("the defaults read")
        .resolve()
        .expect("the defaults resolve");
    assert_eq!("127.0.0.1:3000", settings.listen.to_string());
    assert_eq!("http://127.0.0.1:8080/", settings.bridge.base.as_str());
    assert_eq!(Duration::from_secs(30), settings.bridge.timeout);
    assert_eq!("en", settings.locale.tag(), "English unless configured");
}

#[test]
fn an_override_wins_over_the_file_and_reads_as_toml_or_text() {
    let text = "[bridge]\nbase_url = \"http://file.example/\"\ntimeout_ms = 10\n";
    let settings = Config::from_sources(
        Some(text),
        &environment(&[
            (
                "FERROBRIDGE_CONSOLE__BRIDGE__BASE_URL",
                "https://env.example/root",
            ),
            ("FERROBRIDGE_CONSOLE__BRIDGE__TIMEOUT_MS", "2500"),
            ("FERROBRIDGE_CONSOLE__SERVER__LISTEN", "0.0.0.0:8443"),
        ]),
    )
    .expect("the configuration reads")
    .resolve()
    .expect("the configuration resolves");
    assert_eq!("https://env.example/root/", settings.bridge.base.as_str());
    assert_eq!(Duration::from_millis(2500), settings.bridge.timeout);
    assert_eq!("0.0.0.0:8443", settings.listen.to_string());
}

#[test]
fn an_unknown_key_in_the_file_is_refused_at_its_line_without_its_value() {
    let text = "[server]\nlisten = \"127.0.0.1:3000\"\npassword = \"hunter2\"\n";
    let error = Config::from_sources(Some(text), &BTreeMap::new()).expect_err("an unknown key");
    let Error::Load { source } = &error else {
        panic!("a refused load, got {error:?}");
    };
    assert!(source.to_string().contains("server.password"), "{source}");
    let shown = format!("{error} {error:?} {source}");
    assert!(!shown.contains("hunter2"), "{shown}");
}

#[test]
fn an_unknown_key_from_the_environment_is_refused_after_the_overrides() {
    let error = Config::from_sources(
        None,
        &environment(&[("FERROBRIDGE_CONSOLE__BRIDGE__TOKEN", "s3cret")]),
    )
    .expect_err("an unknown key");
    let Error::Load { source } = &error else {
        panic!("a refused load, got {error:?}");
    };
    assert!(
        source
            .to_string()
            .contains("FERROBRIDGE_CONSOLE__BRIDGE__TOKEN"),
        "the refusal names the variable: {source}"
    );
    let shown = format!("{error} {error:?} {source}");
    assert!(!shown.contains("s3cret"), "{shown}");
}

#[test]
fn text_that_is_not_toml_is_refused_as_syntax() {
    let error = Config::from_sources(Some("[server\n"), &BTreeMap::new()).expect_err("no TOML");
    assert!(matches!(error, Error::Load { .. }), "{error:?}");
}

#[test]
fn each_value_its_key_does_not_admit_is_refused_by_key() {
    for (text, key) in [
        ("[server]\nlisten = \"localhost\"\n", "server.listen"),
        ("[server]\nlocale = \"not a tag\"\n", "server.locale"),
        ("[bridge]\nbase_url = \"\"\n", "bridge.base_url"),
        ("[bridge]\nbase_url = \"not a url\"\n", "bridge.base_url"),
        (
            "[bridge]\nbase_url = \"ftp://bridge.example/\"\n",
            "bridge.base_url",
        ),
        (
            "[bridge]\nbase_url = \"https://u:p@bridge.example/\"\n",
            "bridge.base_url",
        ),
        (
            "[bridge]\nbase_url = \"https://bridge.example/?q=1\"\n",
            "bridge.base_url",
        ),
        ("[bridge]\ntimeout_ms = 0\n", "bridge.timeout_ms"),
    ] {
        let error = Config::from_sources(Some(text), &BTreeMap::new())
            .expect("the tree reads")
            .resolve()
            .expect_err("the value is refused");
        let named = match &error {
            Error::Address { key, .. }
            | Error::Locale { key, .. }
            | Error::Url { key, .. }
            | Error::UrlShape { key, .. }
            | Error::Zero { key }
            | Error::Missing { key } => key.as_str(),
            other => panic!("a refusal by key, got {other:?}"),
        };
        assert_eq!(key, named, "{text}");
    }
}

/// An `[oidc]` section with every required key, the provider on loopback.
const OIDC: &str = "[oidc]\nissuer = \"http://127.0.0.1:8081/realm\"\n\
    authorization_endpoint = \"http://127.0.0.1:8081/auth\"\n\
    token_endpoint = \"http://127.0.0.1:8081/token\"\n\
    jwks_uri = \"http://127.0.0.1:8081/jwks\"\nclient_id = \"ferrobridge-console\"\n\
    redirect_uri = \"https://console.example.org/auth/callback\"\n";

#[test]
fn a_complete_oidc_section_resolves_and_reads_its_secret_file() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("client-secret");
    std::fs::write(&path, "s3cret\n").expect("the secret file");
    let text = format!("{OIDC}client_secret_file = {path:?}\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())
        .expect("the tree reads")
        .resolve()
        .expect("the section resolves");
    let oidc = settings.oidc.expect("sign-in is configured");
    assert_eq!("http://127.0.0.1:8081/realm", oidc.issuer());
    assert_eq!("https://console.example.org", oidc.origin());
    assert!(oidc.endpoints_given(), "no discovery for written endpoints");
    assert!(settings.session.secure_cookie());
    let shown = format!("{oidc:?}");
    assert!(!shown.contains("s3cret"), "{shown}");
    let missing = format!(
        "{OIDC}client_secret_file = {:?}\n",
        dir.path().join("absent")
    );
    let error = Config::from_sources(Some(&missing), &BTreeMap::new())
        .expect("the tree reads")
        .resolve()
        .expect_err("the secret file is read when the section resolves");
    assert!(
        matches!(&error, Error::SecretFile { source: SecretError::Read { key, .. } } if key == "oidc.client_secret"),
        "{error:?}"
    );
}

#[test]
fn each_oidc_or_session_value_its_key_does_not_admit_is_refused_by_key() {
    for (text, key) in [
        (
            OIDC.replace(
                "http://127.0.0.1:8081/token",
                "http://idp.example.org/token",
            ),
            "oidc.token_endpoint",
        ),
        (OIDC.replace("ferrobridge-console", " "), "oidc.client_id"),
        (
            OIDC.replace("/auth/callback", "/auth/callback#x"),
            "oidc.redirect_uri",
        ),
        (
            format!("{OIDC}scopes = [\"profile\"]\n"),
            "oidc.scopes (openid)",
        ),
        (
            format!("{OIDC}scopes = [\"openid\", \"offline_access\"]\n"),
            "oidc.scopes",
        ),
        (
            format!("{OIDC}post_logout_redirect_uri = \"https://console.example.org/\"\n"),
            "oidc.end_session_endpoint (for post_logout_redirect_uri)",
        ),
        (
            format!("[session]\nsecure_cookie = false\n\n{OIDC}"),
            "session.secure_cookie",
        ),
        (
            String::from("[session]\nmax_sessions = 0\n"),
            "session.max_sessions",
        ),
        (
            String::from("[session]\nidle_timeout_s = 0\n"),
            "session.idle_timeout_s",
        ),
    ] {
        let error = Config::from_sources(Some(&text), &BTreeMap::new())
            .expect("the tree reads")
            .resolve()
            .expect_err("the value is refused");
        let named = match &error {
            Error::Url { key, .. }
            | Error::UrlShape { key, .. }
            | Error::Zero { key }
            | Error::Missing { key }
            | Error::Scope { key, .. }
            | Error::InsecureCookie { key } => key.as_str(),
            other => panic!("a refusal by key, got {other:?}"),
        };
        assert_eq!(key, named, "{text}");
    }
}
