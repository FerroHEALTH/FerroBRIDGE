// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[operator]` section: off by default, and refused without `[auth]`.

use std::collections::BTreeMap;
use std::error::Error as StdError;

use ferrobridge_server::config::{Config, Error};

/// The smallest `[auth]` section that boots.
const AUTH: &str = "[auth]\nenabled = true\nissuer = \"https://as.example.org/realms/ferro\"\naudience = \"https://bridge.example.org/fhir\"\n";

#[test]
fn the_operator_api_is_off_by_default() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(Some(""), &BTreeMap::new())?.resolve()?;
    assert!(!settings.operator);
    Ok(())
}

#[test]
fn the_operator_api_turns_on_beside_auth() -> Result<(), Box<dyn StdError>> {
    let text = format!("{AUTH}\n[operator]\nenabled = true\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    assert!(settings.operator);
    Ok(())
}

#[test]
fn the_environment_turns_the_operator_api_on() -> Result<(), Box<dyn StdError>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__OPERATOR__ENABLED"),
        String::from("true"),
    )]);
    let settings = Config::from_sources(Some(AUTH), &environment)?.resolve()?;
    assert!(settings.operator);
    Ok(())
}

#[test]
fn the_operator_api_without_auth_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let error = Config::from_sources(Some("[operator]\nenabled = true\n"), &BTreeMap::new())?
        .resolve()
        .expect_err("an operator API without authentication is refused");
    assert!(
        matches!(&error, Error::Needs { key, needs } if key == "operator.enabled" && needs == "auth.enabled"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_operator_key_refuses_to_boot() {
    let error = Config::from_sources(Some("[operator]\nport = 9090\n"), &BTreeMap::new())
        .expect_err("an unknown key is a boot error");
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
}
