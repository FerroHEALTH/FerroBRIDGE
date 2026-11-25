// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[audit]` section: off by default, every key, the `_file` secret of
//! its credentials, the environment, and every refusal.

use std::collections::BTreeMap;

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;

use super::refusal;

/// A guarded facade the lane can record, and the state store its outbox
/// lives in.
const FACADE: &str = "[state]\nurl = \"postgres://bridge@db.invalid/state?sslmode=disable\"\n\n[auth]\nenabled = true\n\n[auth.issuer.ferro]\nissuer = \"https://as.example.org/realms/ferro\"\naudience = [\"https://bridge.example.org/fhir\"]\n\n[identity_source]\npseudonym_domain = \"urn:oid:2.999.9\"\nsource_domains = [\"urn:oid:2.999.1\"]\nendpoint.url = \"http://127.0.0.1:9/fhir\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\ncustodian.name = \"Synthetic Hospital\"\ncustodian.identifier_system = \"urn:oid:2.999.1.1\"\ncustodian.identifier_value = \"synthetic-hospital\"\n\n";

/// The keys every complete lane names.
const COMPLETE: &str = "endpoint.url = \"https://arr.example.org/fhir/\"\n";

/// Returns a configuration whose `[audit]` section adds `keys` to an
/// enabled lane.
fn lane(keys: &str) -> String {
    format!("{FACADE}[audit]\nenabled = true\n{keys}")
}

#[test]
fn the_audit_lane_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_sources(None, &BTreeMap::new())?;
    assert!(!config.audit.enabled);
    assert_eq!(10_000, config.audit.endpoint.timeout.as_millis());
    assert_eq!(60_000, config.audit.endpoint.retry.max_backoff.as_millis());
    assert!(
        Config::from_sources(Some(FACADE), &BTreeMap::new())?
            .resolve(Process::Server)?
            .audit
            .is_none()
    );
    Ok(())
}

#[test]
fn an_enabled_lane_resolves_every_key() -> Result<(), Box<dyn std::error::Error>> {
    let text = lane(&format!(
        "{COMPLETE}endpoint.retry.max_backoff = \"5s\"\nsource_id = \"bridge-1\"\nsite = \"ward-4\"\n"
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let audit = settings.audit.ok_or("the lane is on")?;
    assert_eq!(
        "https://arr.example.org/fhir/AuditEvent",
        audit.feed.endpoint().as_str()
    );
    assert_eq!("bridge-1", audit.source_id);
    assert_eq!(Some("ward-4"), audit.site.as_deref());
    Ok(())
}

#[test]
fn the_bridge_names_itself_by_the_facade_system_id_by_default()
-> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(&lane(COMPLETE)), &BTreeMap::new())?.resolve(Process::Server)?;
    let audit = settings.audit.ok_or("the lane is on")?;
    assert_eq!("FerroBRIDGE", audit.source_id);
    Ok(())
}

#[test]
fn the_environment_sets_the_repository() -> Result<(), Box<dyn std::error::Error>> {
    let text = lane("");
    let environment = BTreeMap::from([(
        String::from("FERROBRIDGE__AUDIT__ENDPOINT__URL"),
        String::from("https://arr.example.org/r4/"),
    )]);
    let settings = Config::from_sources(Some(&text), &environment)?.resolve(Process::Server)?;
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
        "{COMPLETE}\n[audit.endpoint.credentials]\nuser = \"bridge\"\npassword_file = \"{}\"\n",
        file.display()
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    assert!(
        format!("{:?}", settings.audit.ok_or("the lane is on")?.feed)
            .contains("authorization: true"),
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
        "[auth]\nenabled = false\n\n[identity_source]\npseudonym_domain = \"urn:oid:2.999.9\"\nsource_domains = [\"urn:oid:2.999.1\"]\nendpoint.url = \"http://127.0.0.1:9/fhir\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\ncustodian.name = \"Synthetic Hospital\"\ncustodian.identifier_system = \"urn:oid:2.999.1.1\"\ncustodian.identifier_value = \"synthetic-hospital\"\n\n[audit]\nenabled = true\n{COMPLETE}"
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Needs { key, needs } if key == "audit.enabled" && needs == "auth.enabled"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_lane_without_a_state_store_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let text = lane(COMPLETE).replace(
        "[state]\nurl = \"postgres://bridge@db.invalid/state?sslmode=disable\"\n\n",
        "",
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Needs { key, needs } if key == "audit.enabled" && needs == "[state]"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn the_spool_keys_are_gone() {
    for key in [
        "spool_dir = \"/var/spool/a\"",
        "max_records = 50",
        "max_size = \"4KiB\"",
        "write_timeout = \"300ms\"",
    ] {
        let refused = Config::from_sources(Some(&format!("[audit]\n{key}\n")), &BTreeMap::new());
        assert!(
            matches!(refused, Err(Error::Parse { .. })),
            "{key}: {refused:?}"
        );
    }
}

#[test]
fn a_lane_without_a_repository_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&lane(""))?;
    assert!(
        refused_endpoint(&error, "url"),
        "the endpoint url: {error:?}"
    );
    Ok(())
}

#[test]
fn a_cleartext_repository_is_refused_unless_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let text = lane("endpoint.url = \"http://arr.invalid/fhir/\"\n");
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::AuditBase { key, .. } if key == "audit.endpoint.url"),
        "{error:?}"
    );
    let accepted = lane("endpoint.url = \"http://arr.invalid/fhir/\"\ncleartext = true\n");
    assert!(
        Config::from_sources(Some(&accepted), &BTreeMap::new())?
            .resolve(Process::Server)?
            .audit
            .is_some()
    );
    Ok(())
}

#[test]
fn an_audit_categories_table_is_refused_as_an_unknown_key() {
    let text = lane(&format!(
        "{COMPLETE}\n[audit.categories.templates]\n\"Synthetic.v1\" = [\"Patient-Summaries\"]\n"
    ));
    let error = Config::from_sources(Some(&text), &BTreeMap::new())
        .expect_err("the category map is the audit-categories register");
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
}

#[test]
fn a_zero_timeout_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let error = refusal(&lane(&format!("{COMPLETE}endpoint.timeout = \"0s\"\n")))?;
    assert!(refused_endpoint(&error, "timeout"), "{error:?}");
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

/// Whether `error` is the outbound client's refusal of `[audit.endpoint]`,
/// naming `key` in its cause.
fn refused_endpoint(error: &Error, key: &str) -> bool {
    let Error::Client { upstream, source } = error else {
        return false;
    };
    let mut text = source.to_string();
    let mut cause = std::error::Error::source(source.as_ref());
    while let Some(next) = cause {
        text.push_str(": ");
        text.push_str(&next.to_string());
        cause = next.source();
    }
    *upstream == "audit" && text.contains("audit.endpoint") && text.contains(key)
}
