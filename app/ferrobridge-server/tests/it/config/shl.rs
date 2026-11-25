// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[shl]`: off by default, refused without the patient summary
//! or a public origin, and held to the limits SMART Health Cards and Links
//! 1.0.0 sets on a link's URL and its file locations.

use std::collections::BTreeMap;
use std::time::Duration;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;

/// An enabled facade serving the patient summary, with `shl` as its `[shl]`
/// table.
fn with_shl(shl: &str) -> String {
    format!(
        "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n[facade.summary]\nenabled = true\npackages = [\"/var/lib/ferrobridge/packages\"]\noperator = \"Synthetic operator\"\n\n[shl]\n{shl}"
    )
}

/// Resolves `text` for a job, the error as text.
fn resolve(text: &str) -> Result<ferrobridge_server::config::Settings, String> {
    Config::from_sources(Some(text), &BTreeMap::new())
        .map_err(|error| error.to_string())?
        .resolve(Process::Job)
        .map_err(|error| error.to_string())
}

#[test]
fn links_are_off_until_a_deployment_turns_them_on() -> Result<(), String> {
    assert_eq!(None, resolve(&with_shl(""))?.shl);
    Ok(())
}

#[test]
fn an_enabled_lane_takes_the_defaults_the_owner_set() -> Result<(), String> {
    let settings = resolve(&with_shl(
        "enabled = true\nbase_url = \"https://bridge.example.org/\"\n",
    ))?
    .shl
    .ok_or("the lane is on")?;
    assert_eq!("https://bridge.example.org", settings.base_url);
    assert_eq!(Duration::from_hours(24), settings.default_expiry);
    assert_eq!(Duration::from_hours(168), settings.max_expiry);
    assert_eq!(5, settings.passcode_attempts);
    assert_eq!(Duration::from_hours(1), settings.location_lifetime);
    Ok(())
}

#[test]
fn a_lane_outside_its_limits_refuses_to_boot() {
    let origin = format!("https://{}.example", "b".repeat(70));
    for table in [
        String::from(
            "enabled = true\nstore = \"/l.redb\"\nbase_url = \"https://bridge.example\"\n",
        ),
        String::from("enabled = true\n"),
        format!("enabled = true\nbase_url = \"{origin}\"\n"),
        String::from(
            "enabled = true\nbase_url = \"https://b.example\"\nlocation_lifetime = \"2h\"\n",
        ),
        String::from("enabled = true\nbase_url = \"https://b.example\"\ndefault_expiry = \"8d\"\n"),
        String::from("enabled = true\nbase_url = \"https://b.example\"\npasscode_attempts = 0\n"),
    ] {
        assert!(resolve(&with_shl(&table)).is_err(), "{table}");
    }
}

#[test]
fn a_lane_without_the_patient_summary_refuses_to_boot() {
    let text = "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n[shl]\nenabled = true\nbase_url = \"https://b.example\"\n";
    let refused = resolve(text).err().unwrap_or_default();
    assert!(refused.contains("facade.summary.enabled"), "{refused}");
}
