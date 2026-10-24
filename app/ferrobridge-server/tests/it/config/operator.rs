// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[operator]` section: off by default, and refused without `[auth]`.

use std::collections::BTreeMap;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::error::Error;
use ferrobridge_server::operator::OperatorSettings;

/// The smallest `[auth]` section that boots.
const AUTH: &str = "[auth]\nenabled = true\nissuer = \"https://as.example.org/realms/ferro\"\naudience = [\"https://bridge.example.org/fhir\"]\n";

#[test]
fn the_operator_api_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(""), &BTreeMap::new())?.resolve()?;
    assert_eq!(None, settings.operator);
    Ok(())
}

#[test]
fn the_operator_api_turns_on_beside_auth() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!("{AUTH}\n[operator]\nenabled = true\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        Some(OperatorSettings { mapping_dir: None }),
        settings.operator,
        "a section without mapping_dir saves nothing"
    );
    Ok(())
}

#[test]
fn the_environment_turns_the_operator_api_on() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__OPERATOR__ENABLED"),
        String::from("true"),
    )]);
    let settings = Config::from_sources(Some(AUTH), &environment)?.resolve()?;
    assert!(settings.operator.is_some());
    Ok(())
}

#[test]
fn the_operator_api_without_auth_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let error = Config::from_sources(
        Some("[auth]\nenabled = false\n\n[operator]\nenabled = true\n"),
        &BTreeMap::new(),
    )?
    .resolve()
    .expect_err("an operator API without authentication is refused");
    assert!(
        matches!(&error, Error::Needs { key, needs } if key == "operator.enabled" && needs == "auth.enabled"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn the_operator_api_without_an_issuer_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: no specification governs this: our own design, `[auth]` is on by
    // default, so the operator API refuses to start until it names an issuer.
    let error = Config::from_sources(Some("[operator]\nenabled = true\n"), &BTreeMap::new())?
        .resolve()
        .expect_err("an operator API without an issuer is refused");
    assert!(
        matches!(
            &error,
            Error::Auth { source }
                if matches!(
                    source.as_ref(),
                    ferrohealth_smart::config::ConfigError::Missing { key: "auth.issuer" }
                )
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn the_operator_scope_is_read_from_the_file_and_the_environment() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!("{AUTH}\n[operator]\nenabled = true\n");
    let default = Config::from_sources(Some(&text), &BTreeMap::new())?
        .resolve()?
        .auth
        .ok_or("auth is on beside the operator API")?;
    assert_eq!(
        ferrobridge_server::smart::OPERATOR_SCOPE,
        default.operator_scope
    );
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__OPERATOR__ENABLED"),
            String::from("true"),
        ),
        (
            String::from("FERROBRIDGE__AUTH__PRODUCT_SCOPE"),
            String::from("__console.operator"),
        ),
    ]);
    let auth = Config::from_sources(Some(AUTH), &environment)?
        .resolve()?
        .auth
        .ok_or("auth is on beside the operator API")?;
    assert_eq!("__console.operator", auth.operator_scope);
    Ok(())
}

#[test]
fn an_unknown_operator_key_refuses_to_boot() {
    let error = Config::from_sources(Some("[operator]\nport = 9090\n"), &BTreeMap::new())
        .expect_err("an unknown key is a boot error");
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
}

#[test]
fn the_mapping_dir_is_read_from_the_file_and_the_environment() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{AUTH}\n[operator]\nenabled = true\nmapping_dir = \"/var/lib/ferrobridge/mappings\"\n"
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    assert_eq!(
        Some(OperatorSettings {
            mapping_dir: Some(std::path::PathBuf::from("/var/lib/ferrobridge/mappings")),
        }),
        settings.operator
    );
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__OPERATOR__ENABLED"),
            String::from("true"),
        ),
        (
            String::from("FERROBRIDGE__OPERATOR__MAPPING_DIR"),
            String::from("/srv/console"),
        ),
    ]);
    let settings = Config::from_sources(Some(AUTH), &environment)?.resolve()?;
    assert_eq!(
        Some(std::path::PathBuf::from("/srv/console")),
        settings.operator.and_then(|operator| operator.mapping_dir)
    );
    Ok(())
}
