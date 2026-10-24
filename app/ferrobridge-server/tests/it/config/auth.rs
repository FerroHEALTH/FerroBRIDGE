// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[auth]` section `ferrohealth-smart` defines and the operator scope:
//! the resource server's issuers, audiences and algorithms, and every
//! refusal of them.

use std::collections::BTreeMap;
use std::time::Duration;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::error::Error;
use ferrobridge_server::smart::OPERATOR_SCOPE;
use ferrobridge_server::smart::SmartSettings;
use ferrohealth_smart::config::ConfigError;
use ferrohealth_smart::metadata::DiscoveryError;
use ferrohealth_smart::resource::ResourceError;
use ferrohealth_smart::resource::TokenProfile;
use jsonwebtoken::Algorithm;

/// A surface `[auth]` guards: the FHIRconnect operations over a mapping set.
const GUARDED: &str =
    "[mappings]\ndirectory = \"/srv/mappings\"\ntemplates = \"/srv/templates\"\n\n";

/// The smallest `[auth]` section that boots.
const MINIMAL: &str = "[auth]\nenabled = true\nissuer = \"https://as.example.org/realms/ferro\"\naudience = [\"https://bridge.example.org/fhir\"]\n";

/// Returns the settings `text` resolves to, beside a guarded surface.
fn settings(text: &str) -> Result<Option<SmartSettings>, Box<dyn std::error::Error>> {
    Ok(
        Config::from_sources(Some(&format!("{GUARDED}{text}")), &BTreeMap::new())?
            .resolve()?
            .auth,
    )
}

/// Returns the refusal `text` boots into, beside a guarded surface.
fn refusal(text: &str) -> Result<Error, Box<dyn std::error::Error>> {
    match Config::from_sources(Some(&format!("{GUARDED}{text}")), &BTreeMap::new()) {
        Err(error) => Ok(error),
        Ok(config) => Ok(config
            .resolve()
            .err()
            .ok_or("the configuration should be refused")?),
    }
}

/// Returns the rule of `ferrohealth-smart` an `[auth]` refusal names.
fn rule(error: &Error) -> Option<&ConfigError> {
    match error {
        Error::Auth { source } => Some(source.as_ref()),
        _ => None,
    }
}

/// Returns the resource-server rule an `[auth]` issuer refusal names.
fn issuer_rule(error: &Error) -> Option<&ResourceError> {
    match rule(error)? {
        ConfigError::Issuer { source, .. } => Some(source.as_ref()),
        _ => None,
    }
}

#[test]
fn auth_is_on_by_default_and_a_guarded_surface_needs_an_issuer() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: no specification governs this: our own design, a surface the
    // section guards never runs open unless `enabled = false` says so.
    let error = refusal("")?;
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::Missing { key: "auth.issuer" })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn enabled_false_turns_auth_off() -> Result<(), Box<dyn std::error::Error>> {
    assert!(settings("[auth]\nenabled = false\n")?.is_none());
    assert!(settings("[auth]\nenabled = false\nissuer = \"not checked while off\"\n")?.is_none());
    Ok(())
}

#[test]
fn a_server_that_serves_no_guarded_surface_resolves_no_resource_server()
-> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(""), &BTreeMap::new())?.resolve()?;
    assert!(settings.auth.is_none());
    Ok(())
}

#[test]
fn a_minimal_section_takes_the_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let auth = settings(MINIMAL)?.ok_or("auth is on")?;
    assert_eq!(1, auth.issuers.len());
    let issuer = &auth.issuers[0];
    assert_eq!("https://as.example.org/realms/ferro", issuer.issuer);
    assert_eq!(
        vec![String::from("https://bridge.example.org/fhir")],
        issuer.audience
    );
    assert_eq!(
        vec![
            Algorithm::RS256,
            Algorithm::RS384,
            Algorithm::PS256,
            Algorithm::PS384,
            Algorithm::ES256,
            Algorithm::ES384,
        ],
        issuer.algorithms
    );
    assert_eq!(TokenProfile::Rfc9068, issuer.profile);
    assert_eq!(Duration::from_secs(60), issuer.clock_skew);
    assert_eq!(Duration::from_secs(60), issuer.jwks_refresh_interval);
    assert_eq!(Duration::from_secs(600), issuer.jwks_max_age);
    assert_eq!(Duration::from_secs(10), issuer.timeout);
    assert!(issuer.scopeless_subjects.is_empty());
    assert!(issuer.jwks_file.is_none());
    assert_eq!(OPERATOR_SCOPE, auth.operator_scope);
    assert_eq!(auth.options.capabilities, Vec::<String>::new());
    assert_eq!(auth.options.scopes_supported, Vec::<String>::new());
    assert!(auth.basic.is_none());
    assert!(auth.realm.is_none());
    Ok(())
}

#[test]
fn every_key_reads() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        concat!(
            "{}",
            "algorithms = [\"ES384\", \"PS256\"]\n",
            "token_profile = \"jwt\"\n",
            "clock_skew_seconds = 5\n",
            "jwks_refresh_interval_seconds = 1\n",
            "jwks_max_age_seconds = 120\n",
            "timeout_ms = 2000\n",
            "scopeless_subjects = [\"svc-ferrofed\"]\n",
            "realm = \"ferrobridge\"\n",
            "capabilities = [\"launch-standalone\", \"client-confidential-asymmetric\"]\n",
            "scopes_supported = [\"system/Condition.rs\", \"__bridge.operator\"]\n",
            "product_scope = \"__bridge.operator\"\n",
        ),
        MINIMAL
    );
    let auth = settings(&text)?.ok_or("auth is on")?;
    let issuer = &auth.issuers[0];
    assert_eq!(vec![Algorithm::ES384, Algorithm::PS256], issuer.algorithms);
    assert_eq!(TokenProfile::Jwt, issuer.profile);
    assert_eq!(Duration::from_secs(5), issuer.clock_skew);
    assert_eq!(Duration::from_secs(1), issuer.jwks_refresh_interval);
    assert_eq!(Duration::from_secs(120), issuer.jwks_max_age);
    assert_eq!(Duration::from_secs(2), issuer.timeout);
    assert!(issuer.scopeless_subjects.contains("svc-ferrofed"));
    assert_eq!(Some("ferrobridge"), auth.realm.as_deref());
    assert_eq!("__bridge.operator", auth.operator_scope);
    assert_eq!(2, auth.options.capabilities.len());
    assert_eq!(
        vec![
            String::from("system/Condition.rs"),
            String::from("__bridge.operator")
        ],
        auth.options.scopes_supported
    );
    Ok(())
}

#[test]
fn several_issuers_read_each_over_the_sections_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let auth = settings(concat!(
        "[auth]\naudience = [\"https://bridge.example.org/fhir\"]\nclock_skew_seconds = 30\n\n",
        "[[auth.issuer]]\nissuer = \"https://one.example.org\"\n\n",
        "[[auth.issuer]]\nissuer = \"https://two.example.org\"\nclock_skew_seconds = 10\n",
    ))?
    .ok_or("auth is on")?;
    assert_eq!(2, auth.issuers.len());
    assert_eq!("https://one.example.org", auth.issuers[0].issuer);
    assert_eq!(Duration::from_secs(30), auth.issuers[0].clock_skew);
    assert_eq!(Duration::from_secs(10), auth.issuers[1].clock_skew);
    assert_eq!(
        vec![String::from("https://bridge.example.org/fhir")],
        auth.issuers[1].audience
    );
    Ok(())
}

#[test]
fn a_basic_user_needs_exactly_one_hash_form() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!(
        concat!(
            "{}",
            "[[auth.basic.users]]\nuser = \"etl\"\n",
            "password_hash = \"$argon2id$v=19$m=19456,t=2,p=1$c3ludGhldGljLXNhbHQtMTY$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"\n",
            "password_hash_file = \"/run/secrets/etl.phc\"\n",
        ),
        MINIMAL
    ))?;
    assert!(
        matches!(rule(&error), Some(ConfigError::Secret { .. })),
        "{error:?}"
    );
    let error = refusal(&format!(
        "{MINIMAL}[[auth.basic.users]]\nuser = \"etl\"\npassword_hash = \"not-a-phc-string\"\n"
    ))?;
    assert!(
        matches!(rule(&error), Some(ConfigError::Basic { .. })),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn the_environment_turns_auth_on() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__MAPPINGS__DIRECTORY"),
            String::from("/srv/mappings"),
        ),
        (
            String::from("FERROBRIDGE__MAPPINGS__TEMPLATES"),
            String::from("/srv/templates"),
        ),
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
            String::from("[\"https://bridge.example.org/fhir\"]"),
        ),
    ]);
    let auth = Config::from_sources(None, &environment)?
        .resolve()?
        .auth
        .ok_or("auth is on")?;
    assert_eq!("https://as.example.org", auth.issuers[0].issuer);
    let off = BTreeMap::from([(
        String::from("FERROBRIDGE__AUTH__ENABLED"),
        String::from("false"),
    )]);
    let settings = Config::from_sources(Some(GUARDED), &off)?.resolve()?;
    assert!(settings.auth.is_none());
    Ok(())
}

#[test]
fn an_issuer_that_is_no_https_url_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    for issuer in [
        "http://as.example.org",
        "https://as.example.org/?realm=x",
        "https://as.example.org/#x",
    ] {
        let text = MINIMAL.replace("https://as.example.org/realms/ferro", issuer);
        let error = refusal(&text)?;
        assert!(
            matches!(
                issuer_rule(&error),
                Some(ResourceError::Discovery(DiscoveryError::Insecure { .. }))
            ),
            "{issuer}: {error:?}"
        );
    }
    let error = refusal("[auth]\nenabled = true\naudience = [\"x\"]\n")?;
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::Missing { key: "auth.issuer" })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_loopback_issuer_may_be_plain_http() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: no specification governs this: our own design, the crate's, a test
    // issuer on a loopback host has a transport nobody can observe.
    let text = MINIMAL.replace(
        "https://as.example.org/realms/ferro",
        "http://127.0.0.1:8080/realms/ferro",
    );
    let auth = settings(&text)?.ok_or("auth is on")?;
    assert_eq!("http://127.0.0.1:8080/realms/ferro", auth.issuers[0].issuer);
    Ok(())
}

#[test]
fn a_missing_audience_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal("[auth]\nenabled = true\nissuer = \"https://as.example.org\"\n")?;
    assert!(
        matches!(issuer_rule(&error), Some(ResourceError::NoAudience)),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_symmetric_or_unknown_algorithm_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!("{MINIMAL}algorithms = [\"HS256\"]\n"))?;
    assert!(
        matches!(
            issuer_rule(&error),
            Some(ResourceError::SymmetricAlgorithm {
                algorithm: Algorithm::HS256
            })
        ),
        "{error:?}"
    );
    for algorithm in ["none", "RSA-OAEP"] {
        let error = refusal(&format!("{MINIMAL}algorithms = [\"{algorithm}\"]\n"))?;
        assert!(
            matches!(error, Error::Parse { .. }),
            "{algorithm}: {error:?}"
        );
    }
    let error = refusal(&format!("{MINIMAL}algorithms = []\n"))?;
    assert!(
        matches!(issuer_rule(&error), Some(ResourceError::NoAlgorithm)),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_token_profile_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!("{MINIMAL}token_profile = \"opaque\"\n"))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}

#[test]
fn a_clock_skew_over_five_minutes_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!("{MINIMAL}clock_skew_seconds = 301\n"))?;
    assert!(
        matches!(issuer_rule(&error), Some(ResourceError::ClockSkew { .. })),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_operator_scope_or_capability_that_is_no_scope_token_is_refused()
-> Result<(), Box<dyn std::error::Error>> {
    for scope in ["two words", "__two words"] {
        let error = refusal(&format!("{MINIMAL}product_scope = \"{scope}\"\n"))?;
        assert!(
            matches!(rule(&error), Some(ConfigError::ProductScope { .. })),
            "{scope}: {error:?}"
        );
    }
    let error = refusal(&format!("{MINIMAL}capabilities = [\"bad\\\"quote\"]\n"))?;
    assert!(
        matches!(rule(&error), Some(ConfigError::Capability { .. })),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_operator_scope_outside_the_smart_extension_forms_is_refused() -> Result<(), Box<dyn std::error::Error>>
{
    // NOTE: SMART App Launch 2.2.0 scopes-and-launch-context.html,
    // Extensibility: an extension scope is a full URI or starts with `__`.
    for scope in ["ferrobridge.operator", "bridge:operator"] {
        let error = refusal(&format!("{MINIMAL}product_scope = \"{scope}\"\n"))?;
        assert!(
            matches!(rule(&error), Some(ConfigError::ProductScope { .. })),
            "{scope}: {error:?}"
        );
    }
    let uri = settings(&format!(
        "{MINIMAL}product_scope = \"https://bridge.example.org/scope/operator\"\n"
    ))?
    .ok_or("auth is on")?;
    assert_eq!(
        "https://bridge.example.org/scope/operator",
        uri.operator_scope
    );
    Ok(())
}

#[test]
fn an_unknown_key_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    for key in [
        "audiences = [\"x\"]",
        "jwks_refresh_interval_ms = 1000",
        "operator_scope = \"__x\"",
    ] {
        let error = refusal(&format!("{MINIMAL}{key}\n"))?;
        assert!(matches!(error, Error::Parse { .. }), "{key}: {error:?}");
    }
    let error = refusal(&format!("{MINIMAL}\n[operator]\nscope = \"__x\"\n"))?;
    assert!(
        matches!(error, Error::Parse { .. }),
        "the operator scope is [auth] product_scope: {error:?}"
    );
    Ok(())
}

#[test]
fn a_basic_only_section_is_refused_because_the_facade_names_an_issuer()
-> Result<(), Box<dyn std::error::Error>> {
    // NOTE: SMART App Launch 2.2.0, Conformance, Metadata, `token_endpoint` is
    // required, and only an issuer's metadata names it for the facade.
    let error = refusal(concat!(
        "[auth]\nenabled = true\n",
        "[[auth.basic.users]]\nuser = \"etl\"\n",
        "password_hash = \"$argon2id$v=19$m=19456,t=2,p=1$c3ludGhldGljLXNhbHQtMTY$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"\n",
    ))?;
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::Missing { key: "auth.issuer" })
        ),
        "{error:?}"
    );
    Ok(())
}
