// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[auth]` section `ferrohealth-smart` defines and the operator scope:
//! the resource server's named issuers with their profile keys, audiences (the facade's base by default) and algorithms, the Basic
//! users, each a person, with the scopes and roles
//! `[basic_grants]` grants them, and every refusal of them,
//! the shapes the section no longer takes included.

use std::collections::BTreeMap;
use std::time::Duration;

use argon2::Argon2;
use argon2::password_hash::PasswordHasher;
use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use ferrobridge_server::smart::OPERATOR_SCOPE;
use ferrobridge_server::smart::SmartSettings;
use ferrohealth_smart::basic::Credential;
use ferrohealth_smart::basic::StoreError;
use ferrohealth_smart::config::ConfigError;
use ferrohealth_smart::jws::algorithm::Algorithm;
use ferrohealth_smart::kind::IssuerKind;
use ferrohealth_smart::metadata::DiscoveryError;
use ferrohealth_smart::resource::ResourceError;
use secrecy::SecretString;

use crate::support::random_secret;

/// A surface `[auth]` guards: the FHIRconnect operations over a mapping set.
const GUARDED: &str = concat!(
    "[mappings]\ndirectory = \"/srv/mappings\"\ntemplates = \"/srv/templates\"\n\n",
    "[facade]\nbase_url = \"https://bridge.example.org/fhir\"\n\n",
);

/// The `[auth]` table that opens every section of these cases.
const SECTION: &str = "[auth]\nenabled = true\n";

/// The one `[auth.issuer.ferro]` table of the smallest section, whose own
/// keys a case writes after it.
const ISSUER: &str = "\n[auth.issuer.ferro]\nissuer = \"https://as.example.org/realms/ferro\"\n";

/// The smallest `[auth]` section that boots, its audience the facade's base.
const MINIMAL: &str = "[auth]\nenabled = true\n\n[auth.issuer.ferro]\nissuer = \"https://as.example.org/realms/ferro\"\n";

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
            "[auth]\nenabled = false\n\n[auth.issuer.off]\nissuer = \"not checked while off\"\n"
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
            Algorithm::Rs256,
            Algorithm::Rs384,
            Algorithm::Ps256,
            Algorithm::Ps384,
            Algorithm::Es256,
            Algorithm::Es384,
        ],
        issuer.algorithms
    );
    assert_eq!(IssuerKind::Smart, issuer.kind);
    // NOTE: RFC 9068 §4 and §2.2, the default profile takes the `at+jwt` type
    // and requires the claims of the JWT access token, `scope` among them.
    assert_eq!(
        vec![String::from("at+jwt"), String::from("application/at+jwt")],
        issuer.profile.token_types
    );
    assert_eq!(
        [
            "iss",
            "exp",
            "aud",
            "sub",
            "client_id",
            "iat",
            "jti",
            "scope"
        ]
        .map(String::from)
        .to_vec(),
        issuer.profile.required_claims
    );
    assert_eq!(Duration::from_secs(60), issuer.clock_skew);
    assert_eq!(Duration::from_secs(60), issuer.jwks_refresh_interval);
    assert_eq!(Duration::from_secs(600), issuer.jwks_max_age);
    assert_eq!(Duration::from_secs(10), issuer.timeout);
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
            "realm = \"ferrobridge\"\n",
            "capabilities = [\"launch-standalone\", \"client-confidential-asymmetric\"]\n",
            "scopes_supported = [\"system/Condition.rs\", \"__bridge.operator\"]\n",
            "product_scope = \"__bridge.operator\"\n",
        ),
        concat!(
            "algorithms = [\"ES384\", \"PS256\"]\n",
            "clock_skew = \"5s\"\n",
            "jwks_refresh_interval = \"1s\"\n",
            "jwks_max_age = \"2min\"\n",
            "timeout = \"2s\"\n",
            "audience = [\"https://bridge.example.org/fhir\", \"https://bridge.example.org/operator\"]\n",
            "\n[auth.issuer.ferro.profile]\n",
            "token_types = [\"JWT\"]\n",
            "max_actor_chain = 2\n",
        ),
    );
    let auth = settings(&text)?.ok_or("auth is on")?;
    let issuer = &auth.issuers[0];
    assert_eq!(vec![Algorithm::Es384, Algorithm::Ps256], issuer.algorithms);
    assert_eq!(vec![String::from("JWT")], issuer.profile.token_types);
    assert_eq!(2, issuer.profile.max_actor_chain);
    assert_eq!(Duration::from_secs(5), issuer.clock_skew);
    assert_eq!(Duration::from_secs(1), issuer.jwks_refresh_interval);
    assert_eq!(Duration::from_secs(120), issuer.jwks_max_age);
    assert_eq!(Duration::from_secs(2), issuer.timeout);
    assert_eq!(
        vec![
            String::from("https://bridge.example.org/fhir"),
            String::from("https://bridge.example.org/operator")
        ],
        issuer.audience,
        "an issuer's own audience replaces the facade's base"
    );
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
        "[auth.issuer.one]\nissuer = \"https://one.example.org\"\n",
        "clock_skew = \"30s\"\n\n",
        "[auth.issuer.two]\nissuer = \"https://two.example.org\"\n",
        "clock_skew = \"10s\"\n",
    ))?
    .ok_or("auth is on")?;
    assert_eq!(2, auth.issuers.len());
    assert_eq!("https://one.example.org", auth.issuers[0].issuer);
    assert_eq!(IssuerKind::Smart, auth.issuers[0].kind);
    assert_eq!(IssuerKind::Smart, auth.issuers[1].kind);
    assert_eq!(Duration::from_secs(30), auth.issuers[0].clock_skew);
    assert_eq!(Duration::from_secs(10), auth.issuers[1].clock_skew);
    assert_eq!(
        vec![String::from("https://bridge.example.org/fhir")],
        auth.issuers[1].audience
    );
    Ok(())
}

/// Two trusted issuers, `one` and `two`, with `extra` in the `[auth]` table
/// above them and `issuer` in each issuer table.
fn two_issuers(extra: &str, issuer: &str) -> String {
    format!(
        concat!(
            "[auth]\nenabled = true\n{}\n",
            "[auth.issuer.one]\nissuer = \"https://one.example.org\"\n",
            "{}\n",
            "[auth.issuer.two]\nissuer = \"https://two.example.org\"\n",
            "{}",
        ),
        extra, issuer, issuer
    )
}

#[test]
fn an_issuer_is_a_smart_issuer_unless_it_declares_federation()
-> Result<(), Box<dyn std::error::Error>> {
    // NOTE: SMART App Launch 2.2.0; every issuer is an authorization server read
    // from its standard claims, and `federation` the one kind a table declares.
    for kind in ["persons", "smart", "robots"] {
        let error = refusal(&auth("", &format!("kind = \"{kind}\"\n")))?;
        assert!(matches!(error, Error::Parse { .. }), "{kind}: {error:?}");
    }
    Ok(())
}

#[test]
fn the_flat_issuer_form_is_refused_at_load() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(concat!(
        "[auth]\nenabled = true\nissuer = \"https://as.example.org/realms/ferro\"\n",
        "audience = [\"https://bridge.example.org/fhir\"]\n",
    ))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}

#[test]
fn a_section_level_issuer_setting_is_refused_as_an_unknown_key()
-> Result<(), Box<dyn std::error::Error>> {
    for (line, removed) in [
        (
            "audience = [\"https://bridge.example.org/fhir\"]\n",
            "auth.audience",
        ),
        ("algorithms = [\"ES256\"]\n", "auth.algorithms"),
        ("clock_skew = \"30s\"\n", "auth.clock_skew"),
        ("jwks_file = \"/run/jwks.json\"\n", "auth.jwks_file"),
        ("timeout = \"2s\"\n", "auth.timeout"),
        ("mode = \"introspection\"\n", "auth.mode"),
        ("role_claims = []\n", "auth.role_claims"),
    ] {
        let error = refusal(&auth(line, ""))?;
        assert!(
            matches!(error, Error::Parse { .. }) && chain(&error).contains(removed),
            "{line}: {}",
            chain(&error)
        );
    }
    Ok(())
}

#[test]
fn the_token_profile_key_is_refused_as_an_unknown_key() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&auth("token_profile = \"jwt\"\n", ""))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    let error = refusal(&auth("", "token_profile = \"jwt\"\n"))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}

#[test]
fn a_profile_key_on_the_issuer_table_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    for line in ["token_types = [\"JWT\"]\n", "max_actor_chain = 2\n"] {
        let error = refusal(&auth("", line))?;
        assert!(matches!(error, Error::Parse { .. }), "{line}: {error:?}");
    }
    Ok(())
}

#[test]
fn the_issuer_array_form_is_refused_at_load() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(concat!(
        "[auth]\nenabled = true\n\n[[auth.issuer]]\n",
        "issuer = \"https://as.example.org/realms/ferro\"\n",
    ))?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}

#[test]
fn an_issuer_name_outside_the_name_form_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&MINIMAL.replace("[auth.issuer.ferro]", "[auth.issuer.Ferro-1]"))?;
    assert!(
        matches!(rule(&error), Some(ConfigError::IssuerName { name }) if name == "Ferro-1"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_described_issuer_naming_no_issuer_table_is_refused() -> Result<(), Box<dyn std::error::Error>>
{
    let error = refusal(&two_issuers("described_issuer = \"three\"\n", ""))?;
    assert!(
        matches!(
            rule(&error),
            Some(ConfigError::UnknownIssuer { name, known, .. })
                if name == "three" && known == &[String::from("one"), String::from("two")]
        ),
        "{error:?}"
    );
    let auth = settings(&two_issuers("described_issuer = \"two\"\n", ""))?.ok_or("auth is on")?;
    assert_eq!(
        Some("https://two.example.org"),
        auth.described_issuer
            .as_ref()
            .map(ferrohealth_secret::secret_url::SecretUrl::expose)
    );
    Ok(())
}

#[test]
fn the_role_claims_hold_for_their_issuer_alone() -> Result<(), Box<dyn std::error::Error>> {
    let auth = settings(&format!(
        concat!(
            "[auth]\nenabled = true\n\n",
            "[auth.issuer.one]\nissuer = \"https://one.example.org\"\n",
            "role_claims = []\n\n",
            "[auth.issuer.two]\nissuer = \"https://two.example.org\"\n",
            "{}",
        ),
        "role_claims = [\"realm_access.roles\"]\n"
    ))?
    .ok_or("auth is on")?;
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
fn a_basic_grant_of_a_system_scope_is_refused_naming_the_user()
-> Result<(), Box<dyn std::error::Error>> {
    // NOTE: RFC 6749 §4.4, a machine authenticates with the client credentials
    // grant, and every Basic credential is a person's, never a system's.
    for (scopes, refused) in [
        ("[\"system/Condition.rs\"]", "system/Condition.rs"),
        ("[\"user/Condition.r\", \"system/*.read\"]", "system/*.read"),
    ] {
        let error = refusal(&format!(
            "{MINIMAL}[[auth.basic.users]]\nuser = \"etl\"\npassword_hash = \"{PHC}\"\n\n[basic_grants.etl]\nscopes = {scopes}\n"
        ))?;
        assert!(
            matches!(
                &error,
                Error::BasicGrants {
                    source: StoreError::SystemScope { username, scope },
                } if username == "etl" && scope == refused
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

#[test]
fn a_basic_grant_naming_an_unlisted_user_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: no specification governs this: our own design; a grant names a
    // user `[[auth.basic.users]]` lists, so a misspelt user-id never grants.
    for users in [
        format!("[[auth.basic.users]]\nuser = \"operator\"\npassword_hash = \"{PHC}\"\n\n"),
        String::new(),
    ] {
        let error = refusal(&format!(
            "{MINIMAL}{users}[basic_grants.operater]\nscopes = [\"__ferrobridge.operator\"]\n"
        ))?;
        assert!(
            matches!(
                &error,
                Error::BasicGrants {
                    source: StoreError::UnknownUser { username },
                } if username == "operater"
            ),
            "{users}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn a_basic_users_scopes_or_roles_key_is_refused_as_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    for line in [
        "scopes = [\"user/Condition.r\"]\n",
        "roles = [\"auditor\"]\n",
    ] {
        let error = refusal(&format!(
            "{MINIMAL}[[auth.basic.users]]\nuser = \"operator\"\npassword_hash = \"{PHC}\"\n{line}"
        ))?;
        assert!(matches!(error, Error::Parse { .. }), "{line}: {error:?}");
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
fn a_basic_person_is_granted_the_scopes_and_roles_of_basic_grants()
-> Result<(), Box<dyn std::error::Error>> {
    let password = random_secret();
    let hash = Argon2::default()
        .hash_password_with_salt(password.as_bytes(), b"synthetic-salt-16")
        .map_err(|error| error.to_string())?
        .to_string();
    let auth = settings(&format!(
        concat!(
            "{}",
            "[[auth.basic.users]]\nuser = \"operator\"\npassword_hash = \"{}\"\n\n",
            "[[auth.basic.users]]\nuser = \"clinician\"\npassword_hash = \"{}\"\n\n",
            "[basic_grants.operator]\n",
            "scopes = [\"user/Condition.r\", \"__ferrobridge.operator\"]\n",
            "roles = [\"auditor\"]\n",
        ),
        MINIMAL, hash, hash
    ))?
    .ok_or("auth is on")?;
    let store = auth.basic.ok_or("the Basic users are read")?;
    let header = |user: &str| {
        Credential::new(user, SecretString::from(password.clone())).map(|c| c.header_value())
    };
    let operator = store.verify(&header("operator")?)?;
    assert_eq!("user/Condition.r __ferrobridge.operator", operator.scope);
    assert_eq!(vec![String::from("auditor")], operator.roles);
    let clinician = store.verify(&header("clinician")?)?;
    assert_eq!("", clinician.scope, "a user with no grant holds no scope");
    assert!(clinician.roles.is_empty(), "{:?}", clinician.roles);
    Ok(())
}

#[test]
fn the_environment_turns_auth_on_over_the_files_issuer() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: no specification governs this: our own design; an environment
    // variable sets one key over the file, here the section switch.
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
    assert_eq!(IssuerKind::Smart, auth.issuers[0].kind);
    let off = BTreeMap::from([(
        String::from("FERROBRIDGE__AUTH__ENABLED"),
        String::from("false"),
    )]);
    let settings = Config::from_sources(Some(GUARDED), &off)?.resolve(Process::Server)?;
    assert!(settings.auth.is_none());
    Ok(())
}

#[test]
fn the_flat_issuer_variable_is_refused_at_load() -> Result<(), Box<dyn std::error::Error>> {
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
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
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
    let error = refusal("[auth]\nenabled = true\n\n[auth.issuer.ferro]\ntimeout = \"2s\"\n")?;
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
fn an_issuer_without_an_audience_takes_the_facade_base() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: RFC 9068 §4, the `aud` of a token for this server names it, and the
    // facade's base is the resource the bridge serves.
    let auth = Config::from_sources(
        Some(&format!(
            "[mappings]\ndirectory = \"/srv/mappings\"\ntemplates = \"/srv/templates\"\n\n[facade]\nbase_url = \"https://other.example.org/r4\"\n\n{MINIMAL}"
        )),
        &BTreeMap::new(),
    )?
    .resolve(Process::Server)?
    .auth
    .ok_or("auth is on")?;
    assert_eq!(
        vec![String::from("https://other.example.org/r4")],
        auth.issuers[0].audience
    );
    Ok(())
}

#[test]
fn a_symmetric_or_unknown_algorithm_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    for algorithm in ["HS256", "none", "RSA-OAEP"] {
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

// NOTE: RFC 9964 §8.1.4 registers the JWS algorithm names ML-DSA-44, ML-DSA-65 and ML-DSA-87.
#[test]
fn the_ml_dsa_algorithms_read_by_their_registered_names() -> Result<(), Box<dyn std::error::Error>>
{
    let auth = settings(&format!(
        "{MINIMAL}algorithms = [\"ML-DSA-44\", \"ML-DSA-65\", \"ML-DSA-87\", \"ES256\"]\n"
    ))?
    .ok_or("auth is on")?;
    assert_eq!(
        vec![
            Algorithm::MlDsa44,
            Algorithm::MlDsa65,
            Algorithm::MlDsa87,
            Algorithm::Es256
        ],
        auth.issuers[0].algorithms
    );
    for algorithm in ["ML-DSA-99", "MLDSA65", "ml-dsa-65", "ML-DSA-65-ES256"] {
        let error = refusal(&format!("{MINIMAL}algorithms = [\"{algorithm}\"]\n"))?;
        assert!(
            matches!(error, Error::Parse { .. }),
            "{algorithm}: {error:?}"
        );
    }
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
    let error = refusal(&format!("{MINIMAL}clock_skew = \"301s\"\n"))?;
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
        "[auth]\n\n[auth.issuer.ferro]\nissuer = \"https://as.example.org/realms/ferro\"\nmode = \"introspection\"\n\n[auth.issuer.ferro.introspection]\ncache_max_age = \"30s\"\n\n[auth.issuer.ferro.introspection.client]\nclient_id = \"ferrobridge-rs\"\nclient_secret = \"synthetic-secret\"\n",
    )?
    .ok_or("auth is on")?;
    assert!(settings.issuers.is_empty(), "no key set is read");
    let [introspection] = settings.introspection.as_slice() else {
        return Err("one introspection check".into());
    };
    assert_eq!(IssuerKind::Smart, introspection.kind);
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

#[test]
fn two_tables_naming_one_issuer_refuse_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: no specification governs this: our own design, the crate's; one issuer is
    // trusted in one table, so a later table never replaces an earlier one unseen.
    let error = refusal(&format!(
        "{MINIMAL}\n[auth.issuer.again]\nissuer = \"https://as.example.org/realms/ferro\"\n"
    ))?;
    assert!(
        matches!(rule(&error), Some(ConfigError::DuplicateIssuer { .. })),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn the_environment_addresses_an_issuer_by_its_name() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: no specification governs this: our own design, the crate's; an issuer
    // table's key is set from the environment under the table's name.
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__AUTH__ISSUER__FERRO__CLOCK_SKEW"),
        String::from("15s"),
    )]);
    let auth = Config::from_sources(Some(&format!("{GUARDED}{MINIMAL}")), &environment)?
        .resolve(Process::Server)?
        .auth
        .ok_or("auth is on")?;
    assert_eq!(Duration::from_secs(15), auth.issuers[0].clock_skew);
    Ok(())
}

#[test]
fn the_environment_alone_declares_a_named_issuer() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__AUTH__ISSUER__CLINIC__ISSUER"),
        String::from("https://auth.example.org/realms/clinic"),
    )]);
    let auth = Config::from_sources(Some(GUARDED), &environment)?
        .resolve(Process::Server)?
        .auth
        .ok_or("auth is on")?;
    assert_eq!(
        "https://auth.example.org/realms/clinic",
        auth.issuers[0].issuer
    );
    assert_eq!(
        vec![String::from("https://bridge.example.org/fhir")],
        auth.issuers[0].audience
    );
    Ok(())
}

/// The `acr` mapping of the issuer, eIDAS URIs as `FerroSMART` writes them.
const LEVELS: &str = "\n[auth.issuer.ferro.assurance]\nlevels = { \"http://eidas.europa.eu/LoA/low\" = \"low\", \"http://eidas.europa.eu/LoA/substantial\" = \"substantial\" }\n";

#[test]
fn an_assurance_minimum_reaches_the_settings_over_the_issuer_mapping()
-> Result<(), Box<dyn std::error::Error>> {
    let resolved = settings(&format!(
        "[assurance]\nfhir = \"substantial\"\nshl = \"low\"\n\n{MINIMAL}{LEVELS}"
    ))?
    .ok_or("[auth] is on")?;
    assert_eq!(
        Some(ferrohealth_smart::assurance::AssuranceLevel::Substantial),
        resolved.assurance.fhir
    );
    let mapped = resolved.issuers[0]
        .assurance
        .as_ref()
        .ok_or("the mapping")?;
    assert_eq!(2, mapped.levels.len());
    Ok(())
}

#[test]
fn an_assurance_minimum_no_issuer_reaches_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&format!("[assurance]\nshl = \"high\"\n\n{MINIMAL}{LEVELS}"))?;
    assert!(
        matches!(
            &error,
            Error::Assurance {
                source: ferrobridge_server::assurance::AssuranceError::Unreachable {
                    key: "assurance.shl",
                    minimum: "high"
                }
            }
        ),
        "{error:?}"
    );
    let unmapped = refusal(&format!("[assurance]\nfhir = \"low\"\n\n{MINIMAL}"))?;
    assert!(matches!(unmapped, Error::Assurance { .. }), "{unmapped:?}");
    Ok(())
}

#[test]
fn an_assurance_minimum_without_auth_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal("[assurance]\nfhir = \"substantial\"\n\n[auth]\nenabled = false\n")?;
    assert!(
        matches!(&error, Error::Needs { key, needs } if key == "assurance" && needs == "auth.enabled"),
        "{error:?}"
    );
    let unknown = refusal("[assurance]\noperator = \"high\"\n")?;
    assert!(matches!(unknown, Error::Parse { .. }), "{unknown:?}");
    Ok(())
}
