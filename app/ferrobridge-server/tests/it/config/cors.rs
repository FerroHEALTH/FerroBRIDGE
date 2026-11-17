// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[facade.cors]` table: the origins the REST routes admit.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use ferrobridge_server::facade::cors::OriginError;
use std::collections::BTreeMap;

use super::facade::ENABLED;

#[test]
fn by_default_the_rest_routes_admit_no_origin() -> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(ENABLED), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(0, facade.cors.origins().count());
    Ok(())
}

#[test]
fn the_listed_origins_resolve_in_their_serialization() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{ENABLED}\n[facade.cors]\nallowed_origins = [\"https://App.Example/\", \"http://localhost:4567\"]\n"
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    let origins: Vec<&str> = facade
        .cors
        .origins()
        .map(|origin| origin.as_str())
        .collect();
    assert_eq!(
        vec!["http://localhost:4567", "https://app.example"],
        origins
    );
    Ok(())
}

#[test]
fn the_origins_are_set_from_the_environment() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__FACADE__CORS__ALLOWED_ORIGINS"),
        String::from("[\"https://app.example\"]"),
    )]);
    let settings = Config::from_sources(Some(ENABLED), &environment)?.resolve(Process::Server)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(1, facade.cors.origins().count());
    Ok(())
}

// SMART App Launch 2.2.0, App Launch, Considerations for CORS support: the
// REST routes admit a client's registered origins, so a wildcard is refused.
#[test]
fn a_wildcard_or_a_value_that_is_no_origin_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    for (value, reason) in [
        ("*", OriginError::Unparsable),
        ("null", OriginError::Unparsable),
        ("ftp://app.example", OriginError::Scheme),
        ("https://app.example/fhir", OriginError::Beyond),
    ] {
        let text = format!("{ENABLED}\n[facade.cors]\nallowed_origins = [\"{value}\"]\n");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve(Process::Server)
            .expect_err("only an http or https origin is admitted");
        match &error {
            Error::CorsOrigin {
                key,
                value: held,
                source,
            } => {
                assert_eq!("facade.cors.allowed_origins", key);
                assert_eq!(value, held);
                assert_eq!(reason, *source, "{value}");
            }
            other => return Err(format!("expected a CORS origin error, got {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn an_unknown_cors_key_refuses_to_boot() {
    let text = format!("{ENABLED}\n[facade.cors]\nallow_credentials = true\n");
    assert!(Config::from_sources(Some(&text), &BTreeMap::new()).is_err());
}
