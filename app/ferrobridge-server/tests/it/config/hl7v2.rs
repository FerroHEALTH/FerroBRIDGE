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
        "unmapped_entries = \"refuse\"\n",
        "ehr_policy = \"create_on_first_write\"\n",
        "idle_timeout = \"0s\"\n",
        "frame_timeout = \"5s\"\n",
        "frame_limit = \"64KiB\"\n",
        "log_outcomes = true\n",
        "facility_identifier_issuer = \"urn:oid:2.999.1\"\n",
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
    assert_eq!(Some("urn:oid:2.999.1"), face.facility_issuer.as_deref());
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
    let text = with_face("");
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
    assert_eq!(
        None, face.facility_issuer,
        "a facility namespace id names no issuer by default"
    );
    assert_eq!(1, face.senders.len(), "the case names its one sender");
    Ok(())
}

#[test]
fn an_hl7v2_key_is_set_through_its_environment_name() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_face("");
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
    match &refusal("[hl7v2]\nenabled = true\n")? {
        Error::Needs { key, needs } => {
            assert_eq!("hl7v2.enabled", key);
            assert_eq!("facade.enabled", needs);
        }
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_concept_map_directory_or_a_supplement_directory_is_an_unknown_key() {
    // The v2-to-FHIR maps are generated into the bridge, so no directory is read.
    for line in [
        "concept_maps = \"/srv/v2mappings/package\"\n",
        "supplements = [\"/srv/v2mappings/local\"]\n",
    ] {
        let error = Config::from_sources(Some(&with_face(line)), &BTreeMap::new())
            .expect_err("the key is refused");
        assert!(matches!(error, Error::Parse { .. }), "{line}: {error:?}");
    }
}

#[test]
fn every_bad_hl7v2_value_refuses_to_boot_naming_its_key() -> Result<(), Box<dyn std::error::Error>>
{
    let cases = [
        ("listen = \"nowhere\"\n", "hl7v2.listen"),
        ("default_charset = \"EBCDIC\"\n", "hl7v2.default_charset"),
        ("unmapped_entries = \"drop\"\n", "hl7v2.unmapped_entries"),
        ("ehr_policy = \"always\"\n", "hl7v2.ehr_policy"),
        ("frame_limit = \"0B\"\n", "hl7v2.frame_limit"),
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
        let error = refusal(&with_face(line))?;
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
    let text = with_face(
        "senders = [{ namespace_id = \"NORTHLAB\", certificate_subject = \"northlab.example.org\" }, { namespace_id = \"ANALYSER\", origin = \"device\", device_identifier_issuer = \"urn:example:devices\", certificate_subject = \"analyser.example.org\" }]\n",
    );
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
    let text = format!("{}{}", bare_face(""), tls_table());
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
    let text = with_face("senders = [{ namespace_id = \"NORTHLAB\" }]\n");
    match &refusal(&text)? {
        Error::CertificateSubject { key } => assert_eq!("hl7v2.senders", key),
        other => return Err(format!("expected a subject refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn an_hl7v2_face_without_tls_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    // The MLLP face serves TLS with client certificates and nothing else.
    let text = format!("{}{SENDER}", bare_face(""));
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

/// A plain device sender on `network`, as a `senders` entry.
fn plain_device(network: &str) -> String {
    format!(
        "{{ namespace_id = \"ANALYSER\", origin = \"device\", device_identifier_issuer = \"urn:example:devices\", transport = \"plain\", source_addresses = [\"{network}\"] }}"
    )
}

/// The face with the default TLS sender and `plain` beside it, then `table`.
fn with_plain(plain: &str, table: &str) -> String {
    format!(
        "{}senders = [{{ namespace_id = \"NORTHLAB\", certificate_subject = \"northlab.example.org\" }}, {plain}]\n{}{table}",
        bare_face("listen = \"0.0.0.0:2575\"\n"),
        tls_table(),
    )
}

/// The `[hl7v2.plain]` table binding `bind`.
fn plain_table(bind: &str) -> String {
    format!("\n[hl7v2.plain]\nbind = \"{bind}\"\n")
}

#[test]
fn a_plain_device_resolves_on_its_own_listener() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_plain(
        &plain_device("10.20.0.0/24"),
        &plain_table("10.20.0.5:2576"),
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let face = settings.hl7v2.ok_or("the face is on")?;
    let plain = face.plain.ok_or("the plain listener is configured")?;
    assert_eq!("10.20.0.5:2576", plain.bind.to_string());
    assert_eq!(
        vec![ferrobridge_server::config::hl7v2::PlainSender {
            sender: ferrobridge_server::config::hl7v2::Sender::Namespace(String::from("ANALYSER")),
            sources: vec!["10.20.0.0/24".parse()?],
        }],
        plain.senders
    );
    assert_eq!(
        1,
        face.subjects.len(),
        "the plain device carries no certificate subject"
    );
    assert_eq!(2, face.senders.len(), "the device is a listed sender too");
    Ok(())
}

#[test]
fn the_face_serves_tls_alone_unless_a_sender_asks_for_plain()
-> Result<(), Box<dyn std::error::Error>> {
    let settings =
        Config::from_sources(Some(&with_face("")), &BTreeMap::new())?.resolve(Process::Server)?;
    let face = settings.hl7v2.ok_or("the face is on")?;
    assert!(face.plain.is_none(), "transport defaults to tls");
    Ok(())
}

#[test]
fn a_plain_sender_that_is_no_device_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let sender = "{ namespace_id = \"ANALYSER\", transport = \"plain\", source_addresses = [\"10.20.0.21/32\"] }";
    match &refusal(&with_plain(sender, &plain_table("10.20.0.5:2576")))? {
        Error::SenderTransport { key, reason } => {
            assert_eq!("hl7v2.senders", key);
            assert!(reason.contains("origin = \"device\""), "{reason}");
        }
        other => return Err(format!("expected a transport refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_plain_sender_without_source_addresses_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    let sender = "{ namespace_id = \"ANALYSER\", origin = \"device\", device_identifier_issuer = \"urn:example:devices\", transport = \"plain\" }";
    match &refusal(&with_plain(sender, &plain_table("10.20.0.5:2576")))? {
        Error::SenderTransport { key, reason } => {
            assert_eq!("hl7v2.senders", key);
            assert!(reason.contains("source_addresses"), "{reason}");
        }
        other => return Err(format!("expected a transport refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_plain_sender_naming_a_certificate_subject_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    let sender = "{ namespace_id = \"ANALYSER\", origin = \"device\", device_identifier_issuer = \"urn:example:devices\", transport = \"plain\", source_addresses = [\"10.20.0.21/32\"], certificate_subject = \"analyser.example.org\" }";
    assert!(
        matches!(
            refusal(&with_plain(sender, &plain_table("10.20.0.5:2576")))?,
            Error::SenderTransport { .. }
        ),
        "a plain sender has no client certificate"
    );
    Ok(())
}

#[test]
fn a_tls_sender_naming_source_addresses_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    let text = with_face(
        "senders = [{ namespace_id = \"NORTHLAB\", certificate_subject = \"northlab.example.org\", source_addresses = [\"10.20.0.21/32\"] }]\n",
    );
    assert!(
        matches!(refusal(&text)?, Error::SenderTransport { .. }),
        "source_addresses binds a plain sender alone"
    );
    Ok(())
}

#[test]
fn an_unknown_transport_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_face(
        "senders = [{ namespace_id = \"NORTHLAB\", certificate_subject = \"northlab.example.org\", transport = \"udp\" }]\n",
    );
    match &refusal(&text)? {
        Error::Transport { key, value } => {
            assert_eq!("hl7v2.senders", key);
            assert_eq!("udp", value);
        }
        other => return Err(format!("expected a transport refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_source_address_that_is_no_network_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    for network in [
        "",
        "10.20.0.21",
        "10.20.0.256/32",
        "10.20.0.0/33",
        "lab-vlan",
    ] {
        let text = with_plain(&plain_device(network), &plain_table("10.20.0.5:2576"));
        match &refusal(&text)? {
            Error::SourceAddress { key, value, .. } => {
                assert_eq!("hl7v2.senders.source_addresses", key);
                assert_eq!(network, value);
            }
            other => return Err(format!("{network}: unexpected refusal {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn a_source_address_with_host_bits_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    let text = with_plain(
        &plain_device("10.20.0.21/24"),
        &plain_table("10.20.0.5:2576"),
    );
    match &refusal(&text)? {
        Error::SourceNetwork { network, .. } => assert_eq!("10.20.0.0/24", network),
        other => return Err(format!("expected a network refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_plain_sender_without_the_plain_listener_refuses_to_boot()
-> Result<(), Box<dyn std::error::Error>> {
    match &refusal(&with_plain(&plain_device("10.20.0.21/32"), ""))? {
        Error::Needs { key, needs } => {
            assert_eq!("hl7v2.senders", key);
            assert_eq!("hl7v2.plain.bind", needs);
        }
        other => return Err(format!("expected a needs error, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_plain_listener_with_no_plain_sender_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>>
{
    let text = format!("{}{}", with_face(""), plain_table("10.20.0.5:2576"));
    match &refusal(&text)? {
        Error::PlainUnused { key } => assert_eq!("hl7v2.plain", key),
        other => return Err(format!("expected an unused refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_plain_listener_on_the_tls_address_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    // The TLS listener binds 0.0.0.0:2575, which holds port 2575 on every address.
    for bind in ["0.0.0.0:2575", "10.20.0.5:2575", "[::]:2575"] {
        let text = with_plain(&plain_device("10.20.0.21/32"), &plain_table(bind));
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
    match &refusal(&with_plain(
        &plain_device("10.20.0.21/32"),
        &plain_table("lab:2576"),
    ))? {
        Error::Listen { key, .. } => assert_eq!("hl7v2.plain.bind", key),
        other => return Err(format!("expected a listen refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn the_running_configuration_names_each_sender_s_transport_and_the_plain_bind()
-> Result<(), Box<dyn std::error::Error>> {
    use ferrobridge_operator_api::configuration::Value;

    let text = with_plain(
        &plain_device("10.20.0.21/32"),
        &plain_table("10.20.0.5:2576"),
    );
    let config = Config::from_sources(Some(&text), &BTreeMap::new())?;
    let document = ferrobridge_server::config::document::document(&config, jiff::Timestamp::now())?;
    let bind = document.entry("hl7v2.plain.bind").ok_or("the plain bind")?;
    assert_eq!(Value::String(String::from("10.20.0.5:2576")), bind.value);
    let senders = document.entry("hl7v2.senders").ok_or("the senders")?;
    let Value::List(senders) = &senders.value else {
        return Err(format!("a list of senders, got {:?}", senders.value).into());
    };
    let transports: Vec<_> = senders
        .iter()
        .map(|sender| match sender {
            Value::Table(table) => table.get("transport").cloned(),
            _ => None,
        })
        .collect();
    assert_eq!(
        vec![
            Some(Value::String(String::from("tls"))),
            Some(Value::String(String::from("plain"))),
        ],
        transports
    );
    let Some(Value::Table(device)) = senders.get(1) else {
        return Err("the plain device's table".into());
    };
    assert_eq!(
        Some(&Value::List(vec![Value::String(String::from(
            "10.20.0.21/32"
        ))])),
        device.get("source_addresses")
    );
    Ok(())
}

/// A face whose one sender is the plain device on `network`, with
/// `[hl7v2.plain]` and then `table`.
fn all_plain(network: &str, table: &str) -> String {
    format!(
        "{}senders = [{}]\n{}{table}",
        bare_face(""),
        plain_device(network),
        plain_table("10.20.0.5:2576"),
    )
}

#[test]
fn a_face_of_plain_devices_alone_needs_no_tls() -> Result<(), Box<dyn std::error::Error>> {
    let text = all_plain("10.20.0.21/32", "");
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let face = settings.hl7v2.ok_or("the face is on")?;
    assert!(face.tls.is_none(), "no TLS listener without a TLS sender");
    assert_eq!(
        0,
        face.subjects.len(),
        "no sender carries a certificate subject"
    );
    assert_eq!(
        "10.20.0.5:2576",
        face.plain.ok_or("the plain listener")?.bind.to_string()
    );
    Ok(())
}

#[test]
fn a_tls_table_with_no_tls_sender_refuses_to_boot() -> Result<(), Box<dyn std::error::Error>> {
    match &refusal(&all_plain("10.20.0.21/32", &tls_table()))? {
        Error::TlsUnused { key } => assert_eq!("hl7v2.tls", key),
        other => return Err(format!("expected an unused refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_tls_sender_still_needs_its_tls_files() -> Result<(), Box<dyn std::error::Error>> {
    let text = format!(
        "{}senders = [{{ namespace_id = \"NORTHLAB\", certificate_subject = \"northlab.example.org\" }}, {}]\n{}",
        bare_face(""),
        plain_device("10.20.0.21/32"),
        plain_table("10.20.0.5:2576"),
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
fn a_plain_bind_on_the_default_listen_address_is_taken_without_tls()
-> Result<(), Box<dyn std::error::Error>> {
    // With no TLS listener, `listen` binds nothing, so the plain listener may take its port.
    let text = format!(
        "{}senders = [{}]\n{}",
        bare_face(""),
        plain_device("10.20.0.21/32"),
        plain_table("127.0.0.1:2575"),
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
    let face = settings.hl7v2.ok_or("the face is on")?;
    assert_eq!(
        "127.0.0.1:2575",
        face.plain.ok_or("the plain listener")?.bind.to_string()
    );
    Ok(())
}
