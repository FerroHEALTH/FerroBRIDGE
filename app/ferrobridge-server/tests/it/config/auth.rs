// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[auth]` section `ferrohealth-smart` defines and the operator scope:
//! the resource server's issuers with their kinds and profiles, audiences and
//! algorithms, the Basic users, each a person, and every refusal of them,
//! the shapes the section no longer takes included.

use std::collections::BTreeMap;
use std::time::Duration;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use ferrobridge_server::smart::OPERATOR_SCOPE;
use ferrobridge_server::smart::SmartSettings;
use ferrohealth_smart::basic::StoreError;
use ferrohealth_smart::config::ConfigError;
use ferrohealth_smart::kind::IssuerKind;
use ferrohealth_smart::metadata::DiscoveryError;
use ferrohealth_smart::resource::ResourceError;
use jsonwebtoken::Algorithm;

/// A surface `[auth]` guards: the FHIRconnect operations over a mapping set.
const GUARDED: &str =
    "[mappings]\ndirectory = \"/srv/mappings\"\ntemplates = \"/srv/templates\"\n\n";

/// The `[auth]` table that opens every section of these cases.
const SECTION: &str = "[auth]\nenabled = true\n";

/// The one `[[auth.issuer]]` table of the smallest section, whose own keys a
/// case writes after it.
const ISSUER: &str = "\n[[auth.issuer]]\nissuer = \"https://as.example.org/realms/ferro\"\nkind = \"persons\"\naudience = [\"https://bridge.example.org/fhir\"]\n";

/// The smallest `[auth]` section that boots.
const MINIMAL: &str = "[auth]\nenabled = true\n\n[[auth.issuer]]\nissuer = \"https://as.example.org/realms/ferro\"\nkind = \"persons\"\naudience = [\"https://bridge.example.org/fhir\"]\n";

/// A Basic user's synthetic Argon2id PHC string.
const PHC: &str = "$argon2id$v=19$m=19456,t=2,p=1$c3ludGhldGljLXNhbHQtMTY$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// Returns the smallest section with `section` in the `[auth]` table and
/// `issuer` in the issuer's.
fn auth(section: &str, issuer: &str) -> String {
    format!("{SECTION}{section}{ISSUER}{issuer}")
}

/// Returns the settings `text` resolves to, beside a guarded surface.
fn settings(text: &str) -> Result<Option<SmartSettings>, Box<dyn std::error::Error>> {
    Ok(
        Config::from_sources(Some(&format!("{GUARDED}{text}")), &BTreeMap::new())?
            .resolve(Process::Server)?
            .auth,
    )
}

/// Returns the refusal `text` boots into, beside a guarded surface.
fn refusal(text: &str) -> Result<Error, Box<dyn std::error::Error>> {
    match Config::from_sources(Some(&format!("{GUARDED}{text}")), &BTreeMap::new()) {
        Err(error) => Ok(error),
        Ok(config) => Ok(config
            .resolve(Process::Server)
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
fn auth_is_on_by_default_and_a_guarded_surface_needs_an_issuer()
-> Result<(), Box<dyn std::error::Error>> {
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
    assert!(
        settings(
            "[auth]\nenabled = false\n\n[[auth.issuer]]\nissuer = \"not checked while off\"\n"
        )?
        .is_none()
    );
    Ok(())
}

#[test]
fn a_server_that_serves_no_guarded_surface_resolves_no_resource_server()
-> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(""), &BTreeMap::new())?.resolve(Process::Server)?;
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
    assert_eq!(IssuerKind::Persons, issuer.kind);
    // NOTE: RFC 9068 §4 and §2.2, the default `rfc9068` profile takes the
    // `at+jwt` type and requires the seven claims of the JWT access token.
    assert_eq!(
        vec![String::from("at+jwt"), String::from("application/at+jwt")],
        issuer.profile.token_types
    );
    assert_eq!(
        ["iss", "exp", "aud", "sub", "client_id", "iat", "jti"]
            .map(String::from)
            .to_vec(),
        issuer.profile.required_claims
    );
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
    let text = auth(
        concat!(
            "scopeless_subjects = [{ issuer = \"https://as.example.org/realms/ferro\", sub = \"svc-ferrofed\" }]\n",
            "realm = \"ferrobridge\"\n",
            "capabilities = [\"launch-standalone\", \"client-confidential-asymmetric\"]\n",
            "scopes_supported = [\"system/Condition.rs\", \"__bridge.operator\"]\n",
            "product_scope = \"__bridge.operator\"\n",
        ),
        concat!(
            "algorithms = [\"ES384\", \"PS256\"]\n",
            "token_types = [\"JWT\"]\n",
            "first_party_clients = [\"ferrobridge-console\"]\n",
            "clock_skew_seconds = 5\n",
            "jwks_refresh_interval_seconds = 1\n",
            "jwks_max_age_seconds = 120\n",
            "timeout_ms = 2000\n",
        ),
    );
    let auth = settings(&text)?.ok_or("auth is on")?;
    let issuer = &auth.issuers[0];
    assert_eq!(vec![Algorithm::ES384, Algorithm::PS256], issuer.algorithms);
    assert_eq!(vec![String::from("JWT")], issuer.profile.token_types);
    assert_eq!(
        vec![String::from("ferrobridge-console")],
        issuer.profile.first_party_clients
    );
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
fn several_issuers_read_each_with_its_own_keys() -> Result<(), Box<dyn std::error::Error>> {
    let auth = settings(concat!(
        "[auth]\nenabled = true\n\n",
        "[[auth.issuer]]\nissuer = \"https://one.example.org\"\nkind = \"persons\"\n",
        "audience = [\"https://bridge.example.org/fhir\"]\nclock_skew_seconds = 30\n\n",
        "[[auth.issuer]]\nissuer = \"https://two.example.org\"\nkind = \"machines\"\n",
        "audience = [\"https://bridge.example.org/fhir\"]\nclock_skew_seconds = 10\n",
    ))?
    .ok_or("auth is on")?;
    assert_eq!(2, auth.issuers.len());
    assert_eq!("https://one.example.org", auth.issuers[0].issuer);
    assert_eq!(IssuerKind::Persons, auth.issuers[0].kind);
    assert_eq!(IssuerKind::Machines, auth.issuers[1].kind);
    assert_eq!(Duration::from_secs(30), auth.issuers[0].clock_skew);
    assert_eq!(Duration::from_secs(10), auth.issuers[1].clock_skew);
    assert_eq!(
        vec![String::from("https://bridge.example.org/fhir")],
        auth.issuers[1].audience
    );
    Ok(())
}

/// Two trusted issuers, with `extra` in the `[auth]` table above them and
/// `issuer` in each issuer table.
fn two_issuers(extra: &str, issuer: &str) -> String {
    format!(
        concat!(
            "[auth]\nenabled = true\n{}\n",
            "[[auth.issuer]]\nissuer = \"https://one.example.org\"\nkind = \"persons\"\n",
            "audience = [\"https://bridge.example.org/fhir\"]\n{}\n",
            "[[auth.issuer]]\nissuer = \"https://two.example.org\"\nkind = \"persons\"\n",
            "audience = [\"https://bridge.example.org/fhir\"]\n{}",
        ),
        extra, issuer, issuer
    )
}

#[test]
fn an_issuer_without_its_kind_is_refused_naming_the_key() -> Result<(), Box<dyn std::error::Error>>
{
    let error = refusal(concat!(
        "[auth]\nenabled = true\n\n[[auth.issuer]]\n",
        "issuer = \"https://as.example.org/realms/ferro\"\n",
        "audience = [\"https://bridge.example.org/fhir\"]\n",
    ))?;
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::Missing {
                key: "auth.issuer.kind"
            })
        ),
        "{error:?}"
    );
    let error = refusal(&auth("", "").replace("kind = \"persons\"", "kind = \"robots\""))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}

#[test]
fn the_flat_issuer_form_is_refused_naming_the_issuer_table()
-> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(concat!(
        "[auth]\nenabled = true\nissuer = \"https://as.example.org/realms/ferro\"\n",
        "audience = [\"https://bridge.example.org/fhir\"]\n",
    ))?;
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::Removed { key, replacement })
                if key.starts_with("auth.issuer =") && replacement.contains("[[auth.issuer]]")
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_section_level_issuer_setting_is_refused_naming_the_issuer_table()
-> Result<(), Box<dyn std::error::Error>> {
    for (line, removed) in [
        (
            "audience = [\"https://bridge.example.org/fhir\"]\n",
            "auth.audience",
        ),
        ("algorithms = [\"ES256\"]\n", "auth.algorithms"),
        ("clock_skew_seconds = 30\n", "auth.clock_skew_seconds"),
        ("jwks_file = \"/run/jwks.json\"\n", "auth.jwks_file"),
        ("timeout_ms = 2000\n", "auth.timeout_ms"),
        ("mode = \"introspection\"\n", "auth.mode"),
        ("role_claims = []\n", "auth.role_claims"),
    ] {
        let error = refusal(&auth(line, ""))?;
        assert!(
            matches!(
                rule(&error),
                Some(ConfigError::Removed { key, .. }) if *key == removed
            ),
            "{line}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn the_token_profile_key_is_refused_naming_the_profile() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&auth("token_profile = \"jwt\"\n", ""))?;
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::Removed { key: "auth.token_profile", replacement })
                if replacement.contains("profile")
        ),
        "{error:?}"
    );
    let error = refusal(&auth("", "token_profile = \"jwt\"\n"))?;
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::Removed {
                key: "auth.issuer.token_profile",
                ..
            })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_profile_preset_reads_with_a_key_set_over_it() -> Result<(), Box<dyn std::error::Error>> {
    let read = settings(&auth(
        "",
        concat!(
            "profile = \"keycloak\"\nfirst_party_clients = [\"ferrobridge-console\"]\n",
            "app_only = [{ subjects = [\"service-account-etl\"] }]\n",
        ),
    ))?
    .ok_or("auth is on")?;
    let profile = &read.issuers[0].profile;
    assert_eq!(vec![String::from("azp")], profile.client_claims);
    assert_eq!(
        vec![String::from("ferrobridge-console")],
        profile.first_party_clients
    );
    // NOTE: no specification governs this: our own design, the crate's; the
    // `keycloak` preset names no service account, so the table must.
    let error = refusal(&auth("", "profile = \"keycloak\"\n"))?;
    assert!(
        matches!(rule(&error), Some(ConfigError::Issuer { .. })),
        "{error:?}"
    );
    let error = refusal(&auth("", "profile = \"unknown\"\n"))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}

#[test]
fn a_scopeless_subject_written_as_a_string_is_refused_naming_the_table_form()
-> Result<(), Box<dyn std::error::Error>> {
    for entry in ["svc-ferrofed", "https://one.example.org#svc-ferrofed"] {
        let error = refusal(&two_issuers(
            &format!("scopeless_subjects = [\"{entry}\"]\n"),
            "",
        ))?;
        assert!(
            matches!(
                rule(&error),
                Some(ConfigError::Removed { key, replacement })
                    if key.starts_with("auth.scopeless_subjects")
                        && replacement.contains("{ issuer, sub }")
            ),
            "{entry}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn an_entry_naming_an_untrusted_issuer_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&two_issuers(
        "scopeless_subjects = [{ issuer = \"https://three.example.org\", sub = \"svc-ferrofed\" }]\n",
        "",
    ))?;
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::UnknownIssuer { key: "auth.scopeless_subjects", issuer })
                if issuer == "https://three.example.org"
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_scopeless_subject_and_the_role_claims_hold_for_their_issuer_alone()
-> Result<(), Box<dyn std::error::Error>> {
    let auth = settings(&format!(
        concat!(
            "[auth]\nenabled = true\n",
            "scopeless_subjects = [{{ issuer = \"https://one.example.org\", sub = \"svc-ferrofed\" }}]\n\n",
            "[[auth.issuer]]\nissuer = \"https://one.example.org\"\nkind = \"persons\"\n",
            "audience = [\"https://bridge.example.org/fhir\"]\nrole_claims = []\n\n",
            "[[auth.issuer]]\nissuer = \"https://two.example.org\"\nkind = \"persons\"\n",
            "audience = [\"https://bridge.example.org/fhir\"]\n{}",
        ),
        "role_claims = [\"realm_access.roles\"]\n"
    ))?
    .ok_or("auth is on")?;
    assert!(auth.issuers[0].scopeless_subjects.contains("svc-ferrofed"));
    assert!(auth.issuers[1].scopeless_subjects.is_empty());
    assert_eq!(Vec::<String>::new(), auth.issuers[0].role_claims);
    assert_eq!(
        vec![String::from("realm_access.roles")],
        auth.issuers[1].role_claims
    );
    Ok(())
}

#[test]
fn a_basic_user_needs_exactly_one_hash_form() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!(
        concat!(
            "{}",
            "[[auth.basic.users]]\nuser = \"clinician\"\n",
            "password_hash = \"{}\"\n",
            "password_hash_file = \"/run/secrets/clinician.phc\"\n",
        ),
        MINIMAL, PHC
    ))?;
    assert!(
        matches!(rule(&error), Some(ConfigError::Secret { .. })),
        "{error:?}"
    );
    let error = refusal(&format!(
        "{MINIMAL}[[auth.basic.users]]\nuser = \"clinician\"\npassword_hash = \"not-a-phc-string\"\n"
    ))?;
    assert!(
        matches!(rule(&error), Some(ConfigError::Basic { .. })),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_basic_user_holding_a_system_scope_is_refused_naming_the_user()
-> Result<(), Box<dyn std::error::Error>> {
    // NOTE: RFC 6749 §4.4, a machine authenticates with the client credentials
    // grant, and every Basic credential is a person's, never a system's.
    for (scopes, refused) in [
        ("[\"system/Condition.rs\"]", "system/Condition.rs"),
        ("[\"user/Condition.r\", \"system/*.read\"]", "system/*.read"),
    ] {
        let error = refusal(&format!(
            "{MINIMAL}[[auth.basic.users]]\nuser = \"etl\"\npassword_hash = \"{PHC}\"\nscopes = {scopes}\n"
        ))?;
        assert!(
            matches!(
                rule(&error),
                Some(ConfigError::Basic {
                    source: StoreError::SystemScope { username, scope },
                }) if username == "etl" && scope == refused
            ),
            "{scopes}: {error:?}"
        );
        let chain = chain(&error);
        assert!(
            chain.contains("\"etl\"") && chain.contains(&format!("\"{refused}\"")),
            "the refusal names the user and the scope: {chain}"
        );
    }
    Ok(())
}

/// Returns the display of `error` and of every cause under it, joined.
fn chain(error: &Error) -> String {
    let mut text = error.to_string();
    let mut cause = std::error::Error::source(error);
    while let Some(next) = cause {
        text.push_str(": ");
        text.push_str(&next.to_string());
        cause = next.source();
    }
    text
}

#[test]
fn a_basic_person_with_user_and_operator_scopes_is_admitted()
-> Result<(), Box<dyn std::error::Error>> {
    let auth = settings(&format!(
        concat!(
            "{}",
            "[[auth.basic.users]]\nuser = \"operator\"\npassword_hash = \"{}\"\n",
            "scopes = [\"user/Condition.r\", \"__ferrobridge.operator\"]\n",
        ),
        MINIMAL, PHC
    ))?
    .ok_or("auth is on")?;
    assert!(auth.basic.is_some(), "the Basic user is read");
    Ok(())
}

#[test]
fn the_environment_turns_auth_on_over_the_files_issuer() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: no specification governs this: our own design; an environment
    // variable sets a key, so the `[[auth.issuer]]` tables live in the file.
    let file = format!("{GUARDED}[auth]\nenabled = false\n{ISSUER}");
    let on = BTreeMap::from([(
        String::from("FERROBRIDGE__AUTH__ENABLED"),
        String::from("true"),
    )]);
    let auth = Config::from_sources(Some(&file), &on)?
        .resolve(Process::Server)?
        .auth
        .ok_or("auth is on")?;
    assert_eq!(
        "https://as.example.org/realms/ferro",
        auth.issuers[0].issuer
    );
    assert_eq!(IssuerKind::Persons, auth.issuers[0].kind);
    let off = BTreeMap::from([(
        String::from("FERROBRIDGE__AUTH__ENABLED"),
        String::from("false"),
    )]);
    let settings = Config::from_sources(Some(GUARDED), &off)?.resolve(Process::Server)?;
    assert!(settings.auth.is_none());
    Ok(())
}

#[test]
fn the_flat_issuer_variable_is_refused_naming_the_issuer_table()
-> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__AUTH__ISSUER"),
        String::from("https://as.example.org"),
    )]);
    let error = match Config::from_sources(Some(GUARDED), &environment) {
        Err(error) => error,
        Ok(config) => config
            .resolve(Process::Server)
            .err()
            .ok_or("the flat issuer should be refused")?,
    };
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::Removed { replacement, .. }) if replacement.contains("[[auth.issuer]]")
        ),
        "{error:?}"
    );
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
    let error = refusal(
        "[auth]\nenabled = true\n\n[[auth.issuer]]\nkind = \"persons\"\naudience = [\"x\"]\n",
    )?;
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
    let error = refusal(
        "[auth]\nenabled = true\n\n[[auth.issuer]]\nissuer = \"https://as.example.org\"\nkind = \"persons\"\n",
    )?;
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
fn an_unknown_profile_key_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!("{MINIMAL}token_shape = \"opaque\"\n"))?;
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
        let error = refusal(&auth(&format!("product_scope = \"{scope}\"\n"), ""))?;
        assert!(
            matches!(rule(&error), Some(ConfigError::ProductScope { .. })),
            "{scope}: {error:?}"
        );
    }
    let error = refusal(&auth("capabilities = [\"bad\\\"quote\"]\n", ""))?;
    assert!(
        matches!(rule(&error), Some(ConfigError::Capability { .. })),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_operator_scope_outside_the_smart_extension_forms_is_refused()
-> Result<(), Box<dyn std::error::Error>> {
    // NOTE: SMART App Launch 2.2.0 scopes-and-launch-context.html,
    // Extensibility: an extension scope is a full URI or starts with `__`.
    for scope in ["ferrobridge.operator", "bridge:operator"] {
        let error = refusal(&auth(&format!("product_scope = \"{scope}\"\n"), ""))?;
        assert!(
            matches!(rule(&error), Some(ConfigError::ProductScope { .. })),
            "{scope}: {error:?}"
        );
    }
    let uri = settings(&auth(
        "product_scope = \"https://bridge.example.org/scope/operator\"\n",
        "",
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
        let error = refusal(&auth(&format!("{key}\n"), ""))?;
        assert!(matches!(error, Error::Parse { .. }), "{key}: {error:?}");
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
        "[[auth.basic.users]]\nuser = \"clinician\"\n",
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

#[test]
fn introspection_mode_resolves_the_introspection_check_in_place_of_the_issuers()
-> Result<(), Box<dyn std::error::Error>> {
    let settings = settings(
        "[auth]\n\n[[auth.issuer]]\nissuer = \"https://as.example.org/realms/ferro\"\nkind = \"persons\"\naudience = [\"https://bridge.example.org/fhir\"]\nmode = \"introspection\"\n\n[auth.issuer.introspection]\ncache_max_age_seconds = 30\n\n[auth.issuer.introspection.client]\nclient_id = \"ferrobridge-rs\"\nclient_secret = \"synthetic-secret\"\n",
    )?
    .ok_or("auth is on")?;
    assert!(settings.issuers.is_empty(), "no key set is read");
    let [introspection] = settings.introspection.as_slice() else {
        return Err("one introspection check".into());
    };
    assert_eq!(IssuerKind::Persons, introspection.kind);
    assert_eq!("https://as.example.org/realms/ferro", introspection.issuer);
    assert_eq!(Duration::from_secs(30), introspection.cache_max_age);
    assert_eq!(OPERATOR_SCOPE, settings.operator_scope);
    Ok(())
}

#[test]
fn introspection_mode_without_its_table_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!("{MINIMAL}mode = \"introspection\"\n"))?;
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::Missing {
                key: "auth.issuer.introspection"
            })
        ),
        "{error:?}"
    );
    Ok(())
}
