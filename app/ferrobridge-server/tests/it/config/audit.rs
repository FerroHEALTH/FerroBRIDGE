// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[audit]` section: off by default, every key, the `_file` secret of
//! its credentials, the environment, and every refusal.

use std::collections::BTreeMap;
use std::time::Duration;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::error::Error;

use super::refusal;

/// A guarded facade the lane can record.
const FACADE: &str = "[auth]\nenabled = true\nissuer = \"https://as.example.org/realms/ferro\"\naudience = [\"https://bridge.example.org/fhir\"]\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n";

/// The keys every complete lane names.
const COMPLETE: &str =
    "base_url = \"https://arr.example.org/fhir/\"\nspool_dir = \"/var/spool/ferrobridge-audit\"\n";

/// Returns a configuration whose `[audit]` section adds `keys` to an
/// enabled lane.
fn lane(keys: &str) -> String {
    format!("{FACADE}[audit]\nenabled = true\n{keys}")
}

#[test]
fn the_audit_lane_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_sources(None, &BTreeMap::new())?;
    assert!(!config.audit.enabled);
    assert_eq!(10_000, config.audit.timeout_ms);
    assert_eq!(60_000, config.audit.retry_max_ms);
    assert_eq!(10_000, config.audit.max_records);
    assert_eq!(64 << 20, config.audit.max_bytes);
    assert_eq!(2_000, config.audit.write_timeout_ms);
    assert!(
        Config::from_sources(Some(FACADE), &BTreeMap::new())?
            .resolve()?
            .audit
            .is_none()
    );
    Ok(())
}

#[test]
fn an_enabled_lane_resolves_every_key() -> Result<(), Box<dyn std::error::Error>> {
    let text = lane(&format!(
        "{COMPLETE}retry_max_ms = 5000\nmax_records = 50\nmax_bytes = 4096\nwrite_timeout_ms = 300\nsource_id = \"bridge-1\"\nsite = \"ward-4\"\n\n[audit.categories.templates]\n\"Synthetic Summary.v1\" = [\"Patient-Summaries\"]\n\"Synthetic Admin.v1\" = \"none\"\n\n[audit.categories.archetypes]\n\"openEHR-EHR-COMPOSITION.report.v1\" = [\"Laboratory-Reports\"]\n"
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let audit = settings.audit.ok_or("the lane is on")?;
    assert_eq!(
        "https://arr.example.org/fhir/AuditEvent",
        audit.feed.endpoint().as_str()
    );
    assert_eq!(
        std::path::Path::new("/var/spool/ferrobridge-audit"),
        audit.spool
    );
    assert_eq!(50, audit.bounds.max_messages);
    assert_eq!(4096, audit.bounds.max_bytes);
    assert_eq!(Duration::from_millis(300), audit.bounds.write_timeout);
    assert_eq!(Duration::from_millis(5000), audit.retry_max);
    assert_eq!("bridge-1", audit.source_id);
    assert_eq!(Some("ward-4"), audit.site.as_deref());
    assert_eq!(2, audit.categories.templates().count());
    assert_eq!(1, audit.categories.archetypes().count());
    Ok(())
}

#[test]
fn the_bridge_names_itself_by_the_facade_system_id_by_default()
-> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(Some(&lane(COMPLETE)), &BTreeMap::new())?.resolve()?;
    let audit = settings.audit.ok_or("the lane is on")?;
    assert_eq!("FerroBRIDGE", audit.source_id);
    Ok(())
}

#[test]
fn the_environment_sets_the_repository() -> Result<(), Box<dyn std::error::Error>> {
    let text = lane("spool_dir = \"/var/spool/ferrobridge-audit\"\n");
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__AUDIT__BASE_URL"),
        String::from("https://arr.example.org/r4/"),
    )]);
    let settings = Config::from_sources(Some(&text), &environment)?.resolve()?;
    let audit = settings.audit.ok_or("the lane is on")?;
    assert_eq!(
        "https://arr.example.org/r4/AuditEvent",
        audit.feed.endpoint().as_str()
    );
    Ok(())
}

#[test]
fn the_basic_password_reads_from_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let file = directory.path().join("arr-password");
    std::fs::write(&file, "synthetic-password\n")?;
    let text = lane(&format!(
        "{COMPLETE}\n[audit.credentials]\nuser = \"bridge\"\npassword_file = \"{}\"\n",
        file.display()
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    assert!(
        format!("{:?}", settings.audit.ok_or("the lane is on")?.feed).contains("credentials: true"),
        "the repository is reached with a credential"
    );
    Ok(())
}

#[test]
fn the_lane_needs_the_facade() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!("[audit]\nenabled = true\n{COMPLETE}");
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Needs { key, needs } if key == "audit.enabled" && needs == "facade.enabled"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn the_lane_needs_authenticated_callers() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "[auth]\nenabled = false\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n[audit]\nenabled = true\n{COMPLETE}"
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Needs { key, needs } if key == "audit.enabled" && needs == "auth.enabled"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_lane_without_a_repository_or_a_spool_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    for (text, missing) in [
        (lane("spool_dir = \"/var/spool/a\"\n"), "audit.base_url"),
        (
            lane("base_url = \"https://arr.example.org/fhir/\"\n"),
            "audit.spool_dir",
        ),
    ] {
        let error = refusal(&text)?;
        assert!(
            matches!(&error, Error::Missing { key } if key == missing),
            "{missing}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn a_cleartext_repository_is_refused_unless_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let text = lane("base_url = \"http://arr.invalid/fhir/\"\nspool_dir = \"/var/spool/a\"\n");
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::AuditBase { key, .. } if key == "audit.base_url"),
        "{error:?}"
    );
    let accepted = lane(
        "base_url = \"http://arr.invalid/fhir/\"\ncleartext = true\nspool_dir = \"/var/spool/a\"\n",
    );
    assert!(
        Config::from_sources(Some(&accepted), &BTreeMap::new())?
            .resolve()?
            .audit
            .is_some()
    );
    Ok(())
}

#[test]
fn a_category_map_naming_no_category_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let text = lane(&format!(
        "{COMPLETE}\n[audit.categories.templates]\n\"Synthetic.v1\" = [\"No-Such-Category\"]\n"
    ));
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Categories { key, .. } if key == "audit.categories"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_zero_bound_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    for key in ["timeout_ms", "write_timeout_ms", "max_bytes", "max_records"] {
        let error = refusal(&lane(&format!("{COMPLETE}{key} = 0\n")))?;
        assert!(
            matches!(&error, Error::Bound { key: bound, .. } if *bound == format!("audit.{key}")),
            "{key}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn an_unknown_key_is_refused() {
    assert!(
        Config::from_sources(
            Some(&lane(&format!("{COMPLETE}spool = \"x\"\n"))),
            &BTreeMap::new()
        )
        .is_err()
    );
}
