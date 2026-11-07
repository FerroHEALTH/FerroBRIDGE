// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[hl7v2]` section: off until present, every key, and every refusal.

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

/// The sender a case names when it sets none.
const SENDER: &str =
    "senders = [{ namespace_id = \"NORTHLAB\", certificate_subject = \"northlab.example.org\" }]\n";

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

/// A facade the HL7 v2 face can write through, with `face` as its section,
/// the default sender when `face` names none, and the TLS table.
fn with_face(face: &str) -> String {
    let sender = if face.contains("senders =") {
        ""
    } else {
        SENDER
    };
    format!("{}{sender}{}", bare_face(face), tls_table())
}

/// A facade the HL7 v2 face can write through, with `face` as its whole
/// section and no TLS table.
fn bare_face(face: &str) -> String {
    format!(
        "[auth]\nenabled = false\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\n\n[hl7v2]\nenabled = true\n{face}"
    )
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
fn an_enabled_hl7v2_face_resolves_every_key() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_face(concat!(
        "listen = \"0.0.0.0:2576\"\n",
        "default_charset = \"8859/1\"\n",
        "concept_maps = \"/srv/v2mappings/package\"\n",
        "supplements = [\"/srv/v2mappings/local\"]\n",
        "unmapped_entries = \"refuse\"\n",
        "ehr_policy = \"create_on_first_write\"\n",
        "idle_timeout_ms = 0\n",
        "frame_timeout_ms = 5000\n",
        "frame_limit_bytes = 65536\n",
        "log_outcomes = true\n",
        "senders = [{ namespace_id = \"NORTHLAB\", certificate_subject = \"northlab.example.org\" }, { universal_id = \"1.2.3.4\", universal_id_type = \"ISO\", certificate_subject = \"iso.example.org\" }]\n",
        "profiles = [{ resource_type = \"Observation\", profile = \"http://example.org/fhir/StructureDefinition/lab\" }]\n",
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let face = settings.hl7v2.ok_or("the face is on")?;
    assert_eq!("0.0.0.0:2576", face.listen.to_string());
    assert_eq!(
        ferrobridge_hl7v2::decode::Charset::Iso8859(1),
        face.default_charset
    );
    assert_eq!(
        std::path::Path::new("/srv/v2mappings/package"),
        face.concept_maps
    );
    assert_eq!(
        vec![std::path::PathBuf::from("/srv/v2mappings/local")],
        face.supplements
    );
    assert_eq!(
        ferrobridge_server::facade::ingest::UnmappedEntries::Refuse,
        face.unmapped
    );
    assert_eq!(
        Some(ferrobridge_server::facade::ehr::Policy::CreateOnFirstWrite),
        face.ehr_policy
    );
    assert_eq!(
        None, face.timeouts.idle,
        "0 never closes an idle connection"
    );
    assert_eq!(Some(Duration::from_secs(5)), face.timeouts.frame);
    assert_eq!(65536, face.frame_limit);
    assert!(face.log_outcomes);
    assert_eq!(
        vec![
            ferrobridge_server::config::hl7v2::Sender::Namespace(String::from("NORTHLAB")),
            ferrobridge_server::config::hl7v2::Sender::Universal {
                id: String::from("1.2.3.4"),
                kind: String::from("ISO"),
            },
        ],
        face.senders
    );
    assert_eq!(
        Some("http://example.org/fhir/StructureDefinition/lab"),
        face.profiles.get("Observation").map(String::as_str)
    );
    Ok(())
}

#[test]
fn the_hl7v2_defaults_skip_and_count_with_both_timeouts() -> Result<(), Box<dyn std::error::Error>>
{
    let text = with_face("concept_maps = \"/srv/v2mappings/package\"\n");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let face = settings.hl7v2.ok_or("the face is on")?;
    assert_eq!("127.0.0.1:2575", face.listen.to_string());
    assert_eq!(
        ferrobridge_hl7v2::decode::Charset::Ascii,
        face.default_charset
    );
    assert_eq!(
        ferrobridge_server::facade::ingest::UnmappedEntries::SkipAndCount,
        face.unmapped
    );
    assert_eq!(None, face.ehr_policy, "the facade's policy stands");
    assert_eq!(Some(Duration::from_millis(300_000)), face.timeouts.idle);
    assert_eq!(Some(Duration::from_secs(30)), face.timeouts.frame);
    assert_eq!(1024 * 1024, face.frame_limit);
    assert!(!face.log_outcomes);
    assert_eq!(1, face.senders.len(), "the case names its one sender");
    Ok(())
}

#[test]
fn an_hl7v2_key_is_set_through_its_environment_name() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_face("concept_maps = \"/srv/v2mappings/package\"\n");
    let settings = Config::from_sources(
        Some(&text),
        &env("FERROBRIDGE__HL7V2__LISTEN", "0.0.0.0:2577"),
    )?
    .resolve(Process::Server)?;
    let face = settings.hl7v2.ok_or("the face is on")?;
    assert_eq!("0.0.0.0:2577", face.listen.to_string());
    Ok(())
}

#[test]
fn an_hl7v2_face_without_the_facade_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    match &refusal("[hl7v2]\nenabled = true\nconcept_maps = \"/srv/p\"\n")? {
        Error::Needs { key, needs } => {
            assert_eq!("hl7v2.enabled", key);
            assert_eq!("facade.enabled", needs);
        }
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_hl7v2_face_without_its_concept_maps_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    match &refusal(&with_face(""))? {
        Error::Missing { key } => assert_eq!("hl7v2.concept_maps", key),
        other => return Err(format!("expected a missing key, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn every_bad_hl7v2_value_refuses_to_boot_naming_its_key() -> Result<(), Box<dyn std::error::Error>>
{
    let maps = "concept_maps = \"/srv/p\"\n";
    let cases = [
        ("listen = \"nowhere\"\n", "hl7v2.listen"),
        ("default_charset = \"EBCDIC\"\n", "hl7v2.default_charset"),
        ("unmapped_entries = \"drop\"\n", "hl7v2.unmapped_entries"),
        ("ehr_policy = \"always\"\n", "hl7v2.ehr_policy"),
        ("frame_limit_bytes = 0\n", "hl7v2.frame_limit_bytes"),
        (
            "profiles = [{ resource_type = \"Observatoin\", profile = \"http://example.org/p\" }]\n",
            "hl7v2.profiles.resource_type",
        ),
        (
            "profiles = [{ resource_type = \"Observation\", profile = \"not a url\" }]\n",
            "hl7v2.profiles.profile",
        ),
        (
            "profiles = [{ resource_type = \"Observation\", profile = \"http://example.org/a\" }, { resource_type = \"Observation\", profile = \"http://example.org/b\" }]\n",
            "hl7v2.profiles",
        ),
        (
            "senders = [{ namespace_id = \"A\", universal_id = \"1.2\", universal_id_type = \"ISO\" }]\n",
            "hl7v2.senders",
        ),
        ("senders = [{ universal_id = \"1.2\" }]\n", "hl7v2.senders"),
        ("senders = [{}]\n", "hl7v2.senders"),
        (
            "senders = [{ namespace_id = \"A\", origin = \"robot\" }]\n",
            "hl7v2.senders",
        ),
        (
            "senders = [{ namespace_id = \"A\", origin = \"device\" }]\n",
            "hl7v2.senders",
        ),
        (
            "senders = [{ namespace_id = \"A\", device_identifier_issuer = \"urn:example:devices\" }]\n",
            "hl7v2.senders",
        ),
    ];
    for (line, expected) in cases {
        let error = refusal(&with_face(&format!("{maps}{line}")))?;
        let key = match &error {
            Error::Listen { key, .. }
            | Error::Charset { key, .. }
            | Error::UnmappedEntries { key, .. }
            | Error::EhrPolicy { key, .. }
            | Error::FrameLimit { key }
            | Error::ResourceType { key, .. }
            | Error::Url { key, .. }
            | Error::Duplicate { key, .. }
            | Error::Sender { key }
            | Error::SenderOrigin { key } => key.as_str(),
            other => return Err(format!("{line}: unexpected refusal {other:?}").into()),
        };
        assert_eq!(expected, key, "{line}");
    }
    Ok(())
}

#[test]
fn an_unknown_hl7v2_key_refuses_to_boot() {
    let error = Config::from_sources(
        Some(&with_face("sending_facilities = []\n")),
        &BTreeMap::new(),
    )
    .expect_err("an unknown key is refused");
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
}

#[test]
fn a_device_sender_resolves_with_its_identifier_issuer() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_face(concat!(
        "concept_maps = \"/srv/v2mappings/package\"\n",
        "senders = [{ namespace_id = \"NORTHLAB\", certificate_subject = \"northlab.example.org\" }, { namespace_id = \"ANALYSER\", origin = \"device\", device_identifier_issuer = \"urn:example:devices\", certificate_subject = \"analyser.example.org\" }]\n",
    ));
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let face = settings.hl7v2.ok_or("the face is on")?;
    assert_eq!(2, face.senders.len(), "a device is a sender too");
    assert_eq!(
        vec![ferrobridge_server::config::hl7v2::Device {
            sender: ferrobridge_server::config::hl7v2::Sender::Namespace(String::from("ANALYSER")),
            identifier_issuer: String::from("urn:example:devices"),
        }],
        face.devices
    );
    Ok(())
}

#[test]
fn an_hl7v2_face_without_senders_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{}{}",
        bare_face("concept_maps = \"/srv/v2mappings/package\"\n"),
        tls_table()
    );
    match &refusal(&text)? {
        Error::Needs { key, needs } => {
            assert_eq!("hl7v2.enabled", key);
            assert_eq!("hl7v2.senders", needs);
        }
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_sender_without_its_certificate_subject_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    let text = with_face(concat!(
        "concept_maps = \"/srv/v2mappings/package\"\n",
        "senders = [{ namespace_id = \"NORTHLAB\" }]\n",
    ));
    match &refusal(&text)? {
        Error::CertificateSubject { key } => assert_eq!("hl7v2.senders", key),
        other => return Err(format!("expected a subject refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_hl7v2_face_without_tls_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    // The MLLP face serves TLS with client certificates and nothing else.
    let text = format!(
        "{}{SENDER}",
        bare_face("concept_maps = \"/srv/v2mappings/package\"\n")
    );
    match &refusal(&text)? {
        Error::Needs { key, needs } => {
            assert_eq!("hl7v2.enabled", key);
            assert_eq!("hl7v2.tls.certificate_file", needs);
        }
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn tls_material_that_makes_no_listener_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let empty = tempfile::NamedTempFile::new()?;
    let text = format!(
        "{}{SENDER}{}",
        bare_face("concept_maps = \"/srv/v2mappings/package\"\n"),
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
