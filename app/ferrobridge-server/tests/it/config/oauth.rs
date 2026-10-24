// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[cdr.credentials.oauth]` and `[terminology.credentials.oauth]`
//! sections: the client credentials grant the bridge obtains its token for
//! each upstream with, and every refusal of it.

use ferrobridge_server::cdr::config::CdrCredentials;
use ferrobridge_server::config::Config;
use ferrobridge_server::config::error::Error;
use ferrohealth_smart::client::{ClientAuth, ClientCredentials, Endpoint};
use ferrohealth_smart::config::ConfigError;
use secrecy::ExposeSecret;
use std::collections::BTreeMap;
use std::io::Write;
use std::time::Duration;

/// Returns the configuration text of a CDR lane whose credentials section
/// holds `credentials` and whose OAuth section holds `oauth`.
fn text(credentials: &str, oauth: &str) -> String {
    format!(
        "[cdr]\nbase_url = \"http://cdr.invalid/v1\"\n\n[cdr.credentials]\n{credentials}\n[cdr.credentials.oauth]\n{oauth}"
    )
}

/// Returns the rule of `ferrohealth-smart` a refusal of the table `section`
/// names.
fn rule<'a>(error: &'a Error, section: &str) -> Option<&'a ConfigError> {
    match error {
        Error::Credentials {
            section: named,
            source,
        } if named == section => Some(source.as_ref()),
        _ => None,
    }
}

/// Returns the secret of `grant`, exposed for the assertion.
fn exposed(grant: &ClientCredentials) -> Option<&str> {
    grant
        .client_secret
        .as_ref()
        .map(ExposeSecret::expose_secret)
}

/// Returns the refusal `text` boots into.
fn refusal(text: &str) -> Result<Error, Box<dyn std::error::Error>> {
    match Config::from_sources(Some(text), &BTreeMap::new()) {
        Err(error) => Ok(error),
        Ok(config) => Ok(config
            .resolve()
            .err()
            .ok_or("the configuration should be refused")?),
    }
}

/// Returns the grant `text` resolves to.
fn grant(text: &str) -> Result<ClientCredentials, Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    match settings.cdr.ok_or("the CDR lane is on")?.credentials {
        Some(CdrCredentials::OAuth(oauth)) => Ok(*oauth),
        other => Err(format!("expected the OAuth grant, got {other:?}").into()),
    }
}

#[test]
fn a_grant_reads_every_key_and_its_secret_file() -> Result<(), Box<dyn std::error::Error>> {
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
                "timeout_ms = 4000\n",
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
        Some("synthetic-secret"),
        exposed(&oauth),
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
    assert_eq!(Duration::from_secs(4), oauth.timeout);
    assert!(!format!("{oauth:?}").contains("synthetic-secret"));
    Ok(())
}

#[test]
fn a_grant_defaults_to_client_secret_basic_and_a_thirty_second_margin()
-> Result<(), Box<dyn std::error::Error>> {
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
    assert_eq!(Duration::from_secs(10), oauth.timeout);
    assert_eq!(None, oauth.scope);
    Ok(())
}

#[test]
fn a_grant_is_reachable_through_the_environment() -> Result<(), Box<dyn std::error::Error>> {
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
            assert_eq!(Some("synthetic-secret"), exposed(&oauth));
            assert_eq!(Duration::from_secs(10), oauth.refresh_margin);
            Ok(())
        }
        other => Err(format!("expected the OAuth grant, got {other:?}").into()),
    }
}

#[test]
fn an_unknown_oauth_key_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\ngrant_type = \"password\"\n",
    ))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    assert!(
        format!("{:?}", std::error::Error::source(&error)).contains("grant_type"),
        "the refusal names the key: {error:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_client_auth_method_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: RFC 7523 §2.2's symmetric `client_secret_jwt` is no method the crate
    // supports, so the table does not parse.
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\nclient_auth = \"client_secret_jwt\"\n",
    ))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}

#[test]
fn a_mutual_tls_client_needs_its_certificate() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: RFC 8705 §2, the client authenticates with the certificate the TLS
    // handshake carries.
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_auth = \"tls_client_auth\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "cdr.credentials"),
            Some(ConfigError::Missing {
                key: "credentials.oauth.client_certificate_file"
            })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_private_key_jwt_client_needs_its_key_file() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: RFC 7523 §2.2, the client authenticates with an assertion it signs.
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_auth = \"private_key_jwt\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "cdr.credentials"),
            Some(ConfigError::Missing {
                key: "credentials.oauth.private_key_file"
            })
        ),
        "{error:?}"
    );
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_auth = \"private_key_jwt\"\nprivate_key_file = \"/nonexistent/key.pem\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "cdr.credentials"),
            Some(ConfigError::SecretFile { .. })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_grant_without_a_secret_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "cdr.credentials"),
            Some(ConfigError::Secret {
                key: "credentials.oauth.client_secret"
            })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_secret_file_that_cannot_be_read_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret_file = \"/nonexistent/secret\"\n",
    ))?;
    assert!(
        matches!(rule(&error, "cdr.credentials"), Some(ConfigError::SecretFile { path, .. }) if path == std::path::Path::new("/nonexistent/secret")),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_secret_and_its_file_together_refuse_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\nclient_secret_file = \"/run/secrets/s\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "cdr.credentials"),
            Some(ConfigError::Secret {
                key: "credentials.oauth.client_secret"
            })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_grant_without_a_client_id_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_secret = \"s\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "cdr.credentials"),
            Some(ConfigError::Missing {
                key: "credentials.oauth.client_id"
            })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_grant_with_no_endpoint_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&text("", "client_id = \"c\"\nclient_secret = \"s\"\n"))?;
    assert!(
        matches!(
            rule(&error, "cdr.credentials"),
            Some(ConfigError::Missing {
                key: "credentials.oauth.issuer or credentials.oauth.token_endpoint"
            })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_grant_with_an_issuer_and_a_token_endpoint_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&text(
        "",
        "issuer = \"https://as.example.org\"\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "cdr.credentials"),
            Some(ConfigError::Exclusive {
                left: "credentials.oauth.issuer",
                right: "credentials.oauth.token_endpoint"
            })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_issuer_that_is_not_https_or_carries_a_query_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
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
            matches!(
                rule(&error, "cdr.credentials"),
                Some(ConfigError::OauthIssuer { .. })
            ),
            "{issuer}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn a_scope_that_is_not_scope_tokens_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: RFC 6749 §3.3, scope tokens of NQCHAR separated by one space.
    for scope in ["a  b", "a\\\\b", "a \\\"b\\\"", " a"] {
        let error = refusal(&text(
            "",
            &format!(
                "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\nscope = \"{scope}\"\n"
            ),
        ))?;
        assert!(
            matches!(
                rule(&error, "cdr.credentials"),
                Some(ConfigError::Scope { .. })
            ),
            "{scope:?}: {error:?}"
        );
    }
    // NOTE: no specification governs this: our own design, the crate's, an
    // empty `scope` is the unset key and sends no `scope` parameter.
    let unset = grant(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\nscope = \"\"\n",
    ))?;
    assert_eq!(None, unset.scope);
    Ok(())
}

#[test]
fn a_resource_with_a_fragment_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: RFC 8707 §2, a resource indicator carries no fragment.
    let error = refusal(&text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\nresource = \"https://cdr.example.org/v1#x\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "cdr.credentials"),
            Some(ConfigError::Fragment {
                key: "credentials.oauth.resource"
            })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_grant_beside_a_static_scheme_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&text(
        "user = \"bridge\"\npassword = \"p\"\n",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "cdr.credentials"),
            Some(ConfigError::Exclusive {
                left: "credentials.user",
                right: "credentials.oauth"
            })
        ),
        "{error:?}"
    );
    Ok(())
}

/// Returns the configuration text of a terminology lane whose credentials
/// section holds `credentials` and whose OAuth section holds `oauth`.
fn terminology_text(credentials: &str, oauth: &str) -> String {
    format!(
        "[terminology]\nbase_url = \"http://tx.invalid/r4\"\n\n[terminology.credentials]\n{credentials}\n[terminology.credentials.oauth]\n{oauth}"
    )
}

#[test]
fn the_terminology_lane_obtains_its_token_with_the_grant() -> Result<(), Box<dyn std::error::Error>> {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "synthetic-tx-secret")?;
    let text = terminology_text(
        "",
        &format!(
            concat!(
                "token_endpoint = \"https://as.example.org/token\"\n",
                "client_id = \"ferrobridge\"\n",
                "client_secret_file = {:?}\n",
                "client_auth = \"client_secret_post\"\n",
                "scope = \"system/CodeSystem.rs\"\n",
                "audience = \"ferroterm\"\n",
                "resource = \"https://tx.example.org/r4\"\n",
                "refresh_margin_ms = 60000\n",
            ),
            file.path()
        ),
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let terminology = settings.terminology.ok_or("the terminology lane is on")?;
    assert!(
        matches!(
            terminology.credentials,
            Some(ferrobridge_term::config::Credentials::OAuth(_))
        ),
        "{:?}",
        terminology.credentials
    );
    assert!(
        !format!("{terminology:?}").contains("synthetic-tx-secret"),
        "the client secret stays out of the Debug rendering"
    );
    Ok(())
}

#[test]
fn the_terminology_grant_is_reachable_through_the_environment() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__TERMINOLOGY__BASE_URL"),
            String::from("http://tx.invalid/r4"),
        ),
        (
            String::from("FERROBRIDGE__TERMINOLOGY__CREDENTIALS__OAUTH__ISSUER"),
            String::from("https://as.example.org/realms/ferro"),
        ),
        (
            String::from("FERROBRIDGE__TERMINOLOGY__CREDENTIALS__OAUTH__CLIENT_ID"),
            String::from("ferrobridge"),
        ),
        (
            String::from("FERROBRIDGE__TERMINOLOGY__CREDENTIALS__OAUTH__CLIENT_SECRET"),
            String::from("synthetic-tx-secret"),
        ),
    ]);
    let settings = Config::from_sources(None, &environment)?.resolve()?;
    let terminology = settings.terminology.ok_or("the terminology lane is on")?;
    assert!(matches!(
        terminology.credentials,
        Some(ferrobridge_term::config::Credentials::OAuth(_))
    ));
    Ok(())
}

#[test]
fn a_terminology_grant_beside_a_user_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&terminology_text(
        "user = \"bridge\"\npassword = \"p\"\n",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "terminology.credentials"),
            Some(ConfigError::Exclusive {
                left: "credentials.user",
                right: "credentials.oauth"
            })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_terminology_grant_refusal_names_the_terminology_key() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&terminology_text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error, "terminology.credentials"),
            Some(ConfigError::Secret {
                key: "credentials.oauth.client_secret"
            })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_terminology_oauth_key_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&terminology_text(
        "",
        "token_endpoint = \"https://as.example.org/token\"\nclient_id = \"c\"\nclient_secret = \"s\"\ngrant_type = \"password\"\n",
    ))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}
