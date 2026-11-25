// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[facade.summary]`: off by default, its packages and operator required
//! once it is on, and settable from the environment.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;

/// An enabled facade with `summary` as its `[facade.summary]` table.
fn with_summary(summary: &str) -> String {
    format!(
        "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[identity_source]\npseudonym_domain = \"urn:oid:2.999.9\"\nsource_domains = [\"urn:oid:2.999.1\"]\nendpoint.url = \"http://127.0.0.1:9/fhir\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n[facade.summary]\n{summary}"
    )
}

#[test]
fn the_summary_is_off_until_a_deployment_turns_it_on() -> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(&with_summary("")), &BTreeMap::new())?.resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(None, facade.summary);
    Ok(())
}

#[test]
fn an_enabled_summary_reads_its_packages_and_names_its_operator()
-> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(
        Some(&with_summary(
            "enabled = true\npackages = [\"/var/lib/ferrobridge/packages/hl7.fhir.r4.core\", \"/var/lib/ferrobridge/packages/hl7.fhir.uv.ips.tgz\"]\noperator = \"Synthetic operator\"\n",
        )),
        &BTreeMap::new(),
    )?
    .resolve(Process::Job)?;
    let summary = settings
        .facade
        .ok_or("the facade lane is on")?
        .summary
        .ok_or("the summary is on")?;
    assert_eq!(
        vec![
            PathBuf::from("/var/lib/ferrobridge/packages/hl7.fhir.r4.core"),
            PathBuf::from("/var/lib/ferrobridge/packages/hl7.fhir.uv.ips.tgz"),
        ],
        summary.packages
    );
    assert_eq!("Synthetic operator", summary.operator);
    Ok(())
}

#[test]
fn an_enabled_summary_without_packages_or_operator_refuses_to_boot() {
    for table in [
        "enabled = true\noperator = \"Synthetic operator\"\n",
        "enabled = true\npackages = [\"/var/lib/ferrobridge/packages\"]\n",
    ] {
        let resolved = Config::from_sources(Some(&with_summary(table)), &BTreeMap::new())
            .map_err(|error| error.to_string())
            .and_then(|config| {
                config
                    .resolve(Process::Job)
                    .map_err(|error| error.to_string())
            });
        assert!(resolved.is_err(), "{table}");
    }
}

#[test]
fn the_environment_turns_it_on() -> Result<(), Box<dyn std::error::Error>> {
    let environment = BTreeMap::from([
        (
            String::from("FERROBRIDGE__FACADE__SUMMARY__ENABLED"),
            String::from("true"),
        ),
        (
            String::from("FERROBRIDGE__FACADE__SUMMARY__OPERATOR"),
            String::from("Synthetic operator"),
        ),
    ]);
    let settings = Config::from_sources(
        Some(&with_summary(
            "packages = [\"/var/lib/ferrobridge/packages\"]\n",
        )),
        &environment,
    )?
    .resolve(Process::Job)?;
    let facade = settings.facade.ok_or("the facade lane is on")?;
    assert_eq!(
        Some("Synthetic operator"),
        facade
            .summary
            .as_ref()
            .map(|summary| summary.operator.as_str())
    );
    Ok(())
}

#[test]
fn an_unknown_key_refuses_to_boot() {
    assert!(
        Config::from_sources(Some(&with_summary("enable = true\n")), &BTreeMap::new()).is_err()
    );
}
