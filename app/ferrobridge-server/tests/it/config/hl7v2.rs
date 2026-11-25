// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[hl7v2]` section: off until present, the listeners and every
//! refusal. The senders are MLLP sources in the state store, so every
//! per-sender key is gone from the file.

use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::error::Error;
use std::collections::BTreeMap;
use std::time::Duration;

use super::env;
use super::refusal;

/// The listener's TLS material, written once to files the cases name.
static TLS: std::sync::LazyLock<tempfile::TempDir> = std::sync::LazyLock::new(|| {
    let pki = ferrobridge_testkit::mllp_tls::Pki::generate().expect("the TLS material");
    let directory = tempfile::tempdir().expect("a directory");
    for (name, pem) in [
        ("chain.pem", pki.server_certificate().to_owned()),
        ("key.pem", pki.server_key().to_owned()),
        ("authorities.pem", pki.ca()),
    ] {
        std::fs::write(directory.path().join(name), pem).expect("the file is written");
    }
    directory
});

/// Returns the `[hl7v2.tls]` table naming the case's TLS files.
fn tls_table() -> String {
    let path = |name: &str| TLS.path().join(name).display().to_string();
    format!(
        "\n[hl7v2.tls]\ncertificate_file = \"{}\"\nkey_file = \"{}\"\nclient_ca_file = \"{}\"\n",
        path("chain.pem"),
        path("key.pem"),
        path("authorities.pem"),
    )
}

/// A facade and a state store the HL7 v2 face can write through and route
/// from, with `face` as its section.
fn bare_face(face: &str) -> String {
    format!(
        "[auth]\nenabled = false\n\n[state]\nurl = \"postgres://bridge@db.invalid/state?sslmode=disable\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\ncustodian.name = \"Synthetic Hospital\"\ncustodian.identifier_system = \"urn:oid:2.999.1.1\"\ncustodian.identifier_value = \"synthetic-hospital\"\n\n[hl7v2]\nenabled = true\n{face}"
    )
}

/// The section `face` with the TLS table.
fn with_face(face: &str) -> String {
    format!("{}{}", bare_face(face), tls_table())
}

#[test]
fn the_hl7v2_face_is_off_until_its_section_turns_it_on() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::from_sources(None, &BTreeMap::new())?.resolve(Process::Server)?;
    assert!(
        settings.hl7v2.is_none(),
        "no [hl7v2] enabled leaves the face off"
    );
    Ok(())
}

#[test]
fn an_enabled_hl7v2_face_resolves_its_listeners() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_face(concat!(
        "listen = \"0.0.0.0:2576\"\n",
        "idle_timeout = \"0s\"\n",
        "frame_timeout = \"5s\"\n",
        "frame_limit = \"64KiB\"\n",
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let face = settings.hl7v2.ok_or("the face is on")?;
    assert_eq!("0.0.0.0:2576", face.listen.to_string());
    assert_eq!(None, face.timeouts.idle);
    assert_eq!(Some(Duration::from_secs(5)), face.timeouts.frame);
    assert_eq!(64 * 1024, face.frame_limit);
    assert!(face.tls.is_some());
    assert_eq!(None, face.plain);
    Ok(())
}

#[test]
fn an_hl7v2_key_is_set_through_its_environment_name() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_sources(
        Some(&with_face("")),
        &env("FERROBRIDGE__HL7V2__LISTEN", "0.0.0.0:2577"),
    )?;
    let face = config
        .resolve(Process::Server)?
        .hl7v2
        .ok_or("the face is on")?;
    assert_eq!("0.0.0.0:2577", face.listen.to_string());
    Ok(())
}

#[test]
fn an_hl7v2_face_without_the_facade_or_the_state_store_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    match &refusal("[hl7v2]\nenabled = true\n")? {
        Error::Needs { key, needs } => {
            assert_eq!(
                ("hl7v2.enabled", "facade.enabled"),
                (key.as_str(), needs.as_str())
            );
        }
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    let without_state = with_face("").replace(
        "[state]\nurl = \"postgres://bridge@db.invalid/state?sslmode=disable\"\n\n",
        "",
    );
    match &refusal(&without_state)? {
        Error::Needs { key, needs } => {
            assert_eq!(("hl7v2.enabled", "[state]"), (key.as_str(), needs.as_str()));
        }
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn every_moved_per_sender_key_is_gone() {
    for line in [
        "senders = [{ namespace_id = \"NORTHLAB\", certificate_subject = \"northlab.example.org\" }]\n",
        "profiles = [{ resource_type = \"Observation\", profile = \"http://example.org/p\" }]\n",
        "unmapped_entries = \"refuse\"\n",
        "ehr_policy = \"existing\"\n",
        "default_charset = \"8859/1\"\n",
        "facility_identifier_issuer = \"urn:oid:2.999.1\"\n",
        "log_outcomes = true\n",
        "concept_maps = \"/srv/v2mappings/package\"\n",
    ] {
        let error = Config::from_sources(Some(&with_face(line)), &BTreeMap::new())
            .expect_err("the key is refused");
        assert!(matches!(error, Error::Parse { .. }), "{line}: {error:?}");
    }
}

#[test]
fn every_bad_hl7v2_value_refuses_to_boot_naming_its_key() -> Result<(), Box<dyn std::error::Error>>
{
    match &refusal(&with_face("listen = \"nowhere\"\n"))? {
        Error::Listen { key, .. } => assert_eq!("hl7v2.listen", key),
        other => return Err(format!("expected a listen refusal, got {other:?}").into()),
    }
    match &refusal(&with_face("frame_limit = \"0B\"\n"))? {
        Error::FrameLimit { key } => assert_eq!("hl7v2.frame_limit", key),
        other => return Err(format!("expected a frame-limit refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_hl7v2_face_without_a_listener_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    match &refusal(&bare_face(""))? {
        Error::Needs { key, needs } => {
            assert_eq!("hl7v2.enabled", key);
            assert_eq!("[hl7v2.tls] or [hl7v2.plain]", needs);
        }
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_tls_table_naming_some_files_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let text = bare_face(&format!(
        "\n[hl7v2.tls]\ncertificate_file = \"{}\"\n",
        TLS.path().join("chain.pem").display()
    ));
    match &refusal(&text)? {
        Error::Needs { needs, .. } => assert_eq!("hl7v2.tls.key_file", needs),
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn tls_material_that_makes_no_listener_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let empty = tempfile::NamedTempFile::new()?;
    let text = format!(
        "{}{}",
        bare_face(""),
        tls_table().replace(
            &TLS.path().join("authorities.pem").display().to_string(),
            &empty.path().display().to_string()
        )
    );
    assert!(
        matches!(refusal(&text)?, Error::Hl7v2Tls { .. }),
        "client authorities that hold nothing refuse the start"
    );
    Ok(())
}

/// The `[hl7v2.plain]` table binding `bind`.
fn plain_table(bind: &str) -> String {
    format!("\n[hl7v2.plain]\nbind = \"{bind}\"\n")
}

#[test]
fn a_plain_listener_resolves_beside_or_without_tls() -> Result<(), Box<dyn std::error::Error>> {
    let both = format!(
        "{}{}",
        with_face("listen = \"0.0.0.0:2575\"\n"),
        plain_table("10.20.0.5:2576")
    );
    let face = Config::from_sources(Some(&both), &BTreeMap::new())?
        .resolve(Process::Server)?
        .hl7v2
        .ok_or("the face is on")?;
    assert_eq!(Some("10.20.0.5:2576".parse()?), face.plain);
    assert!(face.tls.is_some());
    let alone = format!("{}{}", bare_face(""), plain_table("127.0.0.1:2575"));
    let face = Config::from_sources(Some(&alone), &BTreeMap::new())?
        .resolve(Process::Server)?
        .hl7v2
        .ok_or("the face is on")?;
    assert!(
        face.tls.is_none(),
        "a face of plain devices alone needs no TLS"
    );
    Ok(())
}

#[test]
fn a_plain_listener_on_the_tls_address_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    // The TLS listener binds 0.0.0.0:2575, which holds port 2575 on every address.
    for bind in ["0.0.0.0:2575", "10.20.0.5:2575", "[::]:2575"] {
        let text = format!(
            "{}{}",
            with_face("listen = \"0.0.0.0:2575\"\n"),
            plain_table(bind)
        );
        match &refusal(&text)? {
            Error::SharedListener { key, .. } => assert_eq!("hl7v2.plain.bind", key),
            other => return Err(format!("{bind}: unexpected refusal {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn a_plain_bind_that_is_no_socket_address_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    let text = format!("{}{}", bare_face(""), plain_table("lab:2576"));
    match &refusal(&text)? {
        Error::Listen { key, .. } => assert_eq!("hl7v2.plain.bind", key),
        other => return Err(format!("expected a listen refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_unknown_hl7v2_key_refuses_to_boot() {
    assert!(
        Config::from_sources(Some(&with_face("color = \"blue\"\n")), &BTreeMap::new()).is_err()
    );
}
