// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[cdr.credentials.oauth]` section: the client credentials grant the
//! bridge obtains its CDR token with, and every refusal of it.

use ferrobridge_server::cdr::config::CdrCredentials;
use ferrobridge_server::cdr::oauth::{ClientAuth, Endpoint, OAuthConfig};
use ferrobridge_server::config::{Config, Error};
use secrecy::ExposeSecret;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::io::Write;
use std::time::Duration;

/// Returns the configuration text of a CDR lane whose credentials section
/// holds `credentials` and whose OAuth section holds `oauth`.
fn text(credentials: &str, oauth: &str) -> String {
    format!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n[cdr.credentials]\n{credentials}\n[cdr.credentials.oauth]\n{oauth}"
    )
}

/// Returns the refusal `text` boots into.
fn refusal(text: &str) -> Result<Error, Box<dyn StdError>> {
    match Config::from_sources(Some(text), &BTreeMap::new()) {
        Err(error) => Ok(error),
        Ok(config) => Ok(config
            .resolve()
            .err()
            .ok_or("the configuration should be refused")?),
    }
}

/// Returns the grant `text` resolves to.
fn grant(text: &str) -> Result<OAuthConfig, Box<dyn StdError>> {
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    match settings.cdr.ok_or("the CDR lane is on")?.credentials {
        Some(CdrCredentials::OAuth(oauth)) => Ok(*oauth),
        other => Err(format!("expected the OAuth grant, got {other:?}").into()),
    }
}

#[test]
fn a_grant_reads_every_key_and_its_secret_file() -> Result<(), Box<dyn StdError>> {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "synthetic-secret")?;
    let oauth = grant(&text(
        "",
        &format!(
            concat!(
                "token_endpoint = \"https://as.example.org/token\"\n",
                "client_id = \"ferrobridge\"\n",
                "client_secret_file = {:?}\n",
                "client_auth = \"client_secret_post\"\n",
                "scope = \"openehr system/*.crs\"\n",
                "audience = \"ferroehr\"\n",
                "resource = \"https://cdr.example.org/v1\"\n",
                "refresh_margin_ms = 60000\n",
            ),
            file.path()
        ),
    ))?;
    assert_eq!(
        Endpoint::Token("https://as.example.org/token".parse()?),
        oauth.endpoint
    );
    assert_eq!("ferrobridge", oauth.client_id);
    assert_eq!(
        "synthetic-secret",
        oauth.client_secret.expose_secret(),
        "the trailing newline is not part of the secret"
    );
    assert_eq!(ClientAuth::ClientSecretPost, oauth.client_auth);
    assert_eq!(Some("openehr system/*.crs"), oauth.scope.as_deref());
    assert_eq!(Some("ferroehr"), oauth.audience.as_deref());
    assert_eq!(
        Some("https://cdr.example.org/v1"),
        oauth.resource.as_ref().map(url::Url::as_str)
    );
    assert_eq!(Duration::from_secs(60), oauth.refresh_margin);
    assert!(!format!("{oauth:?}").contains("synthetic-secret"));
    Ok(())
}

#[test]
fn a_grant_defaults_to_client_secret_basic_and_a_thirty_second_margin()
-> Result<(), Box<dyn StdError>> {
    // NOTE: RFC 6749 §2.3.1, every authorization server supports HTTP Basic.
    let oauth = grant(&text(
        "",
        "issuer = \"https://as.example.org/realms/ferro\"\nclient_id = \"ferrobridge\"\nclient_secret = \"s\"\n",
    ))?;
    assert_eq!(
        Endpoint::Issuer(String::from("https://as.example.org/realms/ferro")),
        oauth.endpoint
    );
    assert_eq!(ClientAuth::ClientSecretBasic, oauth.client_auth);
    assert_eq!(Duration::from_secs(30), oauth.refresh_margin);
    assert_eq!(None, oauth.scope);
    Ok(())
}

#[test]
fn a_grant_is_reachable_through_the_environment() -> Result<(), Box<dyn StdError>> {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "synthetic-secret")?;
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__CDR__BASE_URL"),
            String::from("http://cdr.invalid/v1"),
        ),
        (
            String::from("FERROBRIDGE__CDR__CREDENTIALS__OAUTH__TOKEN_ENDPOINT"),
            String::from("https://as.example.org/token"),
        ),
        (
            String::from("FERROBRIDGE__CDR__CREDENTIALS__OAUTH__CLIENT_ID"),
            String::from("ferrobridge"),
        ),
        (
            String::from("FERROBRIDGE__CDR__CREDENTIALS__OAUTH__CLIENT_SECRET_FILE"),
            file.path().display().to_string(),
        ),
        (
            String::from("FERROBRIDGE__CDR__CREDENTIALS__OAUTH__REFRESH_MARGIN_MS"),
            String::from("10000"),
        ),
    ]);
    let settings = Config::from_sources(None, &environment)?.resolve()?;
    match settings.cdr.ok_or("the CDR lane is on")?.credentials {
        Some(CdrCredentials::OAuth(oauth)) => {
            assert_eq!("synthetic-secret", oauth.client_secret.expose_secret());
            assert_eq!(Duration::from_secs(10), oauth.refresh_margin);
            Ok(())
        }
        other => Err(format!("expected the OAuth grant, got {other:?}").into()),
    }
}

#[test]
fn an_unknown_oauth_key_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\ngrant_type = \"password\"\n",
    ))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    assert!(
        format!("{:?}", StdError::source(&error)).contains("grant_type"),
        "the refusal names the key: {error:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_client_auth_method_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\nclient_auth = \"private_key_jwt\"\n",
    ))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}

#[test]
fn a_grant_without_a_secret_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\n",
    ))?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "cdr.credentials.oauth.client_secret"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_secret_file_that_cannot_be_read_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret_file = \"/nonexistent/secret\"\n",
    ))?;
    assert!(
        matches!(&error, Error::Secret { key, .. } if key == "cdr.credentials.oauth.client_secret_file"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_secret_and_its_file_together_refuse_to_boot() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\nclient_secret_file = \"/run/secrets/s\"\n",
    ))?;
    assert!(
        matches!(&error, Error::Conflict { key } if key == "cdr.credentials.oauth.client_secret"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_grant_without_a_client_id_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_secret = \"s\"\n",
    ))?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "cdr.credentials.oauth.client_id"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_grant_with_no_endpoint_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&text("", "client_id = \"c\"\nclient_secret = \"s\"\n"))?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "cdr.credentials.oauth.token_endpoint"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_grant_with_an_issuer_and_a_token_endpoint_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&text(
        "",
        "issuer = \"https://as.example.org\"\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\n",
    ))?;
    assert!(
        matches!(&error, Error::OAuthEndpoint { section } if section == "cdr.credentials.oauth"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_issuer_that_is_not_https_or_carries_a_query_refuses_to_boot() -> Result<(), Box<dyn StdError>>
{
    // NOTE: RFC 8414 §2, an issuer is an https URL with no query or fragment.
    for issuer in [
        "http://as.example.org",
        "https://as.example.org?tenant=1",
        "https://as.example.org#x",
    ] {
        let error = refusal(&text(
            "",
            &format!("issuer = \"{issuer}\"\nclient_id = \"c\"\nclient_secret = \"s\"\n"),
        ))?;
        assert!(
            matches!(&error, Error::Issuer { key } if key == "cdr.credentials.oauth.issuer"),
            "{issuer}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn a_scope_that_is_not_scope_tokens_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    // NOTE: RFC 6749 §3.3, scope tokens of NQCHAR separated by one space.
    for scope in ["", "a  b", "a\\\\b", "a \\\"b\\\""] {
        let error = refusal(&text(
            "",
            &format!(
                "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\nscope = \"{scope}\"\n"
            ),
        ))?;
        assert!(
            matches!(&error, Error::Scope { key } if key == "cdr.credentials.oauth.scope"),
            "{scope:?}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn a_resource_with_a_fragment_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    // NOTE: RFC 8707 §2, a resource indicator carries no fragment.
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\nresource = \"https://cdr.example.org/v1#x\"\n",
    ))?;
    assert!(
        matches!(&error, Error::Resource { key } if key == "cdr.credentials.oauth.resource"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_grant_beside_a_static_scheme_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&text(
        "bearer_token = \"t\"\n",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\n",
    ))?;
    assert!(
        matches!(&error, Error::OAuthScheme { section } if section == "cdr.credentials"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn the_terminology_lane_takes_no_oauth_section() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[terminology]\nbase_url = \"http://tx.invalid/r4\"\n\n",
        "[terminology.credentials.oauth]\nclient_id = \"c\"\n",
    );
    let error = refusal(text)?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}

#[test]
fn the_terminology_oauth_refusal_names_the_table() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[terminology]\nbase_url = \"http://tx.invalid/r4\"\n\n",
        "[terminology.credentials.oauth]\nclient_id = \"c\"\n",
    );
    let error = refusal(text)?;
    let cause = StdError::source(&error)
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(cause.contains("oauth"), "{cause}");
    Ok(())
}
