// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[auth]` section: the resource server's issuer, audience and
//! algorithms, and every refusal of them.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::time::Duration;

use ferrobridge_server::auth::AuthSettings;
use ferrobridge_server::auth::OPERATOR_SCOPE;
use ferrobridge_server::auth::TokenProfile;
use ferrobridge_server::config::{Config, Error};
use jsonwebtoken::Algorithm;

/// The smallest `[auth]` section that boots.
const MINIMAL: &str = "[auth]\nenabled = true\nissuer = \"https://as.example.org/realms/ferro\"\naudience = \"https://bridge.example.org/fhir\"\n";

/// Returns the settings `text` resolves to.
fn settings(text: &str) -> Result<Option<AuthSettings>, Box<dyn StdError>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()?
        .auth)
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

#[test]
fn auth_is_off_until_its_section_turns_it_on() -> Result<(), Box<dyn StdError>> {
    assert_eq!(None, settings("")?);
    assert_eq!(
        None,
        settings("[auth]\nissuer = \"not checked while off\"\n")?
    );
    Ok(())
}

#[test]
fn a_minimal_section_takes_the_defaults() -> Result<(), Box<dyn StdError>> {
    let auth = settings(MINIMAL)?.ok_or("auth is on")?;
    assert_eq!("https://as.example.org/realms/ferro", auth.issuer);
    assert_eq!("https://bridge.example.org/fhir", auth.audience);
    assert_eq!(vec![Algorithm::RS256, Algorithm::ES256], auth.algorithms);
    assert_eq!(TokenProfile::Rfc9068, auth.profile);
    assert_eq!(Duration::from_secs(60), auth.clock_skew);
    assert_eq!(Duration::from_secs(60), auth.jwks_refresh_interval);
    assert_eq!(Duration::from_secs(10), auth.timeout);
    assert!(auth.scopeless_subjects.is_empty());
    assert_eq!(OPERATOR_SCOPE, auth.operator_scope);
    assert!(auth.capabilities.is_empty());
    Ok(())
}

#[test]
fn every_key_reads() -> Result<(), Box<dyn StdError>> {
    let text = format!(
        concat!(
            "{}",
            "algorithms = [\"ES384\", \"PS256\"]\n",
            "token_profile = \"jwt\"\n",
            "clock_skew_seconds = 5\n",
            "jwks_refresh_interval_ms = 1000\n",
            "timeout_ms = 2000\n",
            "scopeless_subjects = [\"svc-ferrofed\"]\n",
            "operator_scope = \"bridge:operator\"\n",
            "capabilities = [\"launch-standalone\", \"client-confidential-asymmetric\"]\n",
        ),
        MINIMAL
    );
    let auth = settings(&text)?.ok_or("auth is on")?;
    assert_eq!(vec![Algorithm::ES384, Algorithm::PS256], auth.algorithms);
    assert_eq!(TokenProfile::Jwt, auth.profile);
    assert_eq!(Duration::from_secs(5), auth.clock_skew);
    assert_eq!(Duration::from_secs(1), auth.jwks_refresh_interval);
    assert_eq!(Duration::from_secs(2), auth.timeout);
    assert!(auth.scopeless_subjects.contains("svc-ferrofed"));
    assert_eq!("bridge:operator", auth.operator_scope);
    assert_eq!(2, auth.capabilities.len());
    Ok(())
}

#[test]
fn the_environment_turns_auth_on() -> Result<(), Box<dyn StdError>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__AUTH__ENABLED"),
            String::from("true"),
        ),
        (
            String::from("FERROBRIDGE__AUTH__ISSUER"),
            String::from("https://as.example.org"),
        ),
        (
            String::from("FERROBRIDGE__AUTH__AUDIENCE"),
            String::from("https://bridge.example.org/fhir"),
        ),
    ]);
    let auth = Config::from_sources(None, &environment)?
        .resolve()?
        .auth
        .ok_or("auth is on")?;
    assert_eq!("https://as.example.org", auth.issuer);
    Ok(())
}

#[test]
fn an_issuer_that_is_no_https_url_is_refused() -> Result<(), Box<dyn StdError>> {
    for issuer in [
        "http://as.example.org",
        "https://as.example.org/?realm=x",
        "https://as.example.org/#x",
    ] {
        let text = MINIMAL.replace("https://as.example.org/realms/ferro", issuer);
        let error = refusal(&text)?;
        assert!(
            matches!(&error, Error::Issuer { key } if key == "auth.issuer"),
            "{issuer}: {error:?}"
        );
    }
    let error = refusal("[auth]\nenabled = true\naudience = \"x\"\n")?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "auth.issuer"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_missing_audience_is_refused() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[auth]\nenabled = true\nissuer = \"https://as.example.org\"\n")?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "auth.audience"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_symmetric_or_unknown_algorithm_is_refused() -> Result<(), Box<dyn StdError>> {
    for algorithm in ["HS256", "none", "RSA-OAEP"] {
        let error = refusal(&format!("{MINIMAL}algorithms = [\"{algorithm}\"]\n"))?;
        assert!(
            matches!(&error, Error::Algorithm { value, .. } if value == algorithm),
            "{algorithm}: {error:?}"
        );
    }
    let error = refusal(&format!("{MINIMAL}algorithms = []\n"))?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "auth.algorithms"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_token_profile_is_refused() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&format!("{MINIMAL}token_profile = \"opaque\"\n"))?;
    assert!(matches!(error, Error::TokenProfile { .. }), "{error:?}");
    Ok(())
}

#[test]
fn an_operator_scope_or_capability_that_is_no_scope_token_is_refused()
-> Result<(), Box<dyn StdError>> {
    let error = refusal(&format!("{MINIMAL}operator_scope = \"two words\"\n"))?;
    assert!(
        matches!(&error, Error::Scope { key } if key == "auth.operator_scope"),
        "{error:?}"
    );
    let error = refusal(&format!("{MINIMAL}capabilities = [\"bad\\\"quote\"]\n"))?;
    assert!(
        matches!(&error, Error::Scope { key } if key == "auth.capabilities"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_key_is_refused() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&format!("{MINIMAL}audiences = [\"x\"]\n"))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}
