// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[usage_report]` section: on by default with no state directory, every
//! key, the environment switch, and every refusal.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::error::Error;
use ferrobridge_server::usage_report::config::{Deployment, ENDPOINT};
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::path::Path;

use super::env;
use super::refusal;

#[test]
fn the_report_is_on_by_default_with_no_state_directory() -> Result<(), Box<dyn StdError>> {
    let report = Config::from_sources(None, &BTreeMap::new())?
        .resolve()?
        .usage_report;
    assert!(report.enabled);
    assert_eq!(ENDPOINT, report.endpoint.as_str());
    assert_eq!(None, report.state_dir, "no state directory keeps it silent");
    assert_eq!(Deployment::Unknown, report.deployment);
    Ok(())
}

#[test]
fn every_usage_report_key_is_read_from_the_file() -> Result<(), Box<dyn StdError>> {
    let text = concat!(
        "[usage_report]\n",
        "enabled = false\n",
        "endpoint = \"http://127.0.0.1:9/v1/report\"\n",
        "state_dir = \"/var/lib/ferrobridge/state\"\n",
        "deployment = \"compose\"\n",
    );
    let report = Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()?
        .usage_report;
    assert!(!report.enabled);
    assert_eq!("http://127.0.0.1:9/v1/report", report.endpoint.as_str());
    assert_eq!(
        Some(Path::new("/var/lib/ferrobridge/state")),
        report.state_dir.as_deref()
    );
    assert_eq!(Deployment::Compose, report.deployment);
    Ok(())
}

#[test]
fn the_environment_switches_the_report_off() -> Result<(), Box<dyn StdError>> {
    let report = Config::from_sources(None, &env("FERROBRIDGE__USAGE_REPORT__ENABLED", "false"))?
        .resolve()?
        .usage_report;
    assert!(!report.enabled);
    let report = Config::from_sources(
        None,
        &env("FERROBRIDGE__USAGE_REPORT__STATE_DIR", "/srv/state"),
    )?
    .resolve()?
    .usage_report;
    assert_eq!(Some(Path::new("/srv/state")), report.state_dir.as_deref());
    Ok(())
}

#[test]
fn a_collector_off_https_or_with_credentials_is_refused() -> Result<(), Box<dyn StdError>> {
    for endpoint in [
        "http://report.example/v1/report",
        "https://user:pw@report.example/v1/report",
        "ftp://report.example/",
    ] {
        let error = refusal(&format!("[usage_report]\nendpoint = \"{endpoint}\"\n"))?;
        assert!(
            matches!(&error, Error::ReportEndpoint { key } if key == "usage_report.endpoint"),
            "{endpoint}: {error:?}"
        );
    }
    let error = refusal("[usage_report]\nendpoint = \"not a url\"\n")?;
    assert!(
        matches!(&error, Error::Url { key, .. } if key == "usage_report.endpoint"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_deployment_or_key_is_refused_by_name() {
    for text in [
        "[usage_report]\ndeployment = \"k8s\"\n",
        "[usage_report]\nslow_ms = 5\n",
    ] {
        let error =
            Config::from_sources(Some(text), &BTreeMap::new()).expect_err("the tree refuses it");
        assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    }
}
