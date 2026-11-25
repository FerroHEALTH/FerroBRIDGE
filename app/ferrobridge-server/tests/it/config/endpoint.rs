// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[facade.endpoint]`: off by default, the WADO-RS base optional and checked
//! as a URL, and settable from the environment.

use std::collections::BTreeMap;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::facade::endpoint::Endpoints;

/// An enabled facade with `endpoint` as its `[facade.endpoint]` table.
fn with_endpoint(endpoint: &str) -> String {
    format!(
        "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n[facade.endpoint]\n{endpoint}"
    )
}

#[test]
fn endpoints_are_off_until_a_deployment_turns_them_on() -> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(&with_endpoint("")), &BTreeMap::new())?.resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(None, facade.endpoints);
    Ok(())
}

#[test]
fn the_environment_turns_them_on_and_names_the_wado_rs_base()
-> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__FACADE__ENDPOINT__ENABLED"),
            String::from("true"),
        ),
        (
            String::from("FERROBRIDGE__FACADE__ENDPOINT__DICOMWEB_WADO_RS"),
            String::from("https://pacs.example.org/dicom-web"),
        ),
    ]);
    let settings =
        Config::from_sources(Some(&with_endpoint("")), &environment)?.resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(
        Some(Endpoints {
            wado_rs: Some(String::from("https://pacs.example.org/dicom-web")),
        }),
        facade.endpoints
    );
    Ok(())
}

#[test]
fn a_wado_rs_base_that_is_no_url_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_sources(
        Some(&with_endpoint(
            "enabled = true\ndicomweb_wado_rs = \"not a url\"\n",
        )),
        &BTreeMap::new(),
    )?;
    assert!(config.resolve(Process::Job).is_err());
    Ok(())
}

#[test]
fn an_unknown_key_refuses_to_boot() {
    assert!(
        Config::from_sources(Some(&with_endpoint("enable = true\n")), &BTreeMap::new()).is_err()
    );
}
