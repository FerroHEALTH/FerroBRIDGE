// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[hl7v2]` into the HL7 v2 face's settings.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

use super::super::Hl7v2Settings;
use super::super::error::Error;
use super::super::hl7v2;
use super::super::section::Hl7v2;
use super::super::section::Hl7v2Sender;
use super::super::section::Hl7v2Tls;
use super::ehr_policy_of;
use super::url_of;

/// Returns the sender `sender` names under `key`.
fn sender_of(key: &str, sender: &Hl7v2Sender) -> Result<hl7v2::Sender, Error> {
    let named = |text: &str| !text.is_empty();
    match (
        named(&sender.namespace_id),
        named(&sender.universal_id),
        named(&sender.universal_id_type),
    ) {
        (true, false, false) => Ok(hl7v2::Sender::Namespace(sender.namespace_id.clone())),
        (false, true, true) => Ok(hl7v2::Sender::Universal {
            id: sender.universal_id.clone(),
            kind: sender.universal_id_type.clone(),
        }),
        _ => Err(Error::Sender {
            key: key.to_owned(),
        }),
    }
}

/// Returns whether `sender` is declared `transport = "plain"`.
///
/// # Errors
/// Returns [`Error::Transport`] for a transport other than `tls` and
/// `plain`.
fn is_plain(sender: &Hl7v2Sender) -> Result<bool, Error> {
    match sender.transport.as_str() {
        "tls" => Ok(false),
        "plain" => Ok(true),
        other => Err(Error::Transport {
            key: String::from(SENDERS),
            value: other.to_owned(),
        }),
    }
}

/// The key every sender refusal names.
const SENDERS: &str = "hl7v2.senders";

/// Returns the certificate subject of every TLS sender of `senders`.
///
/// # Errors
/// Returns [`Error::Needs`] for an empty list, [`Error::Transport`] for a
/// transport neither `tls` nor `plain`, [`Error::CertificateSubject`] for a
/// TLS sender that names no subject, and [`Error::SenderTransport`] for a TLS
/// sender naming `source_addresses`.
fn subjects_of(senders: &[Hl7v2Sender]) -> Result<Vec<hl7v2::Subject>, Error> {
    if senders.is_empty() {
        return Err(Error::Needs {
            key: String::from("hl7v2.enabled"),
            needs: String::from(SENDERS),
        });
    }
    let mut subjects = Vec::with_capacity(senders.len());
    for sender in senders {
        if is_plain(sender)? {
            continue;
        }
        if sender.certificate_subject.is_empty() {
            return Err(Error::CertificateSubject {
                key: String::from(SENDERS),
            });
        }
        if !sender.source_addresses.is_empty() {
            return Err(Error::SenderTransport {
                key: String::from(SENDERS),
                reason: "source_addresses is for a sender with transport = \"plain\" alone",
            });
        }
        subjects.push(hl7v2::Subject {
            sender: sender_of(SENDERS, sender)?,
            name: sender.certificate_subject.clone(),
        });
    }
    Ok(subjects)
}

/// Returns the network `value` writes in CIDR notation.
///
/// # Errors
/// Returns [`Error::SourceAddress`] for text that is no network, and
/// [`Error::SourceNetwork`] for one with bits set past its prefix.
fn network_of(value: &str) -> Result<ipnet::IpNet, Error> {
    let network = value
        .parse::<ipnet::IpNet>()
        .map_err(|source| Error::SourceAddress {
            key: String::from("hl7v2.senders.source_addresses"),
            value: value.to_owned(),
            source,
        })?;
    if network.trunc() != network {
        return Err(Error::SourceNetwork {
            key: String::from("hl7v2.senders.source_addresses"),
            value: value.to_owned(),
            network: network.trunc().to_string(),
        });
    }
    Ok(network)
}

/// Returns whether listeners at `one` and `other` would take one port: the
/// same port on the same address, or on an address one of them binds as
/// unspecified. Port 0 asks the system for a port of its own, so it shares
/// with nothing.
fn shared(one: SocketAddr, other: SocketAddr) -> bool {
    one.port() != 0
        && one.port() == other.port()
        && (one.ip() == other.ip() || one.ip().is_unspecified() || other.ip().is_unspecified())
}

/// Returns the plain listener `hl7v2` describes with the senders declared
/// `transport = "plain"`, or `None` when it declares neither.
///
/// No specification governs this: our own design. HL7 MLLP Release 2 has no
/// security layer and IHE ITI ATNA (ITI TF-2, ITI-19) adds TLS for a node
/// that supports it, so a plain sender is a device bound to its networks.
///
/// # Errors
/// Returns [`Error::SenderTransport`] for a plain sender that is no device,
/// names no `source_addresses` or names a `certificate_subject`,
/// [`Error::SourceAddress`] and [`Error::SourceNetwork`] for a network that
/// does not read, [`Error::Needs`] for a plain sender with no
/// `[hl7v2.plain]`, [`Error::PlainUnused`] for a `[hl7v2.plain]` with no
/// plain sender, [`Error::Listen`] for a `bind` that does not parse, and
/// [`Error::SharedListener`] for a `bind` the TLS listener at `listen` takes.
fn plain_of(hl7v2: &Hl7v2, listen: SocketAddr) -> Result<Option<hl7v2::Plain>, Error> {
    let refused = |reason| Error::SenderTransport {
        key: String::from(SENDERS),
        reason,
    };
    let mut senders = Vec::new();
    for sender in &hl7v2.senders {
        if !is_plain(sender)? {
            continue;
        }
        if sender.origin != "device" {
            return Err(refused(
                "a sender with transport = \"plain\" is origin = \"device\"",
            ));
        }
        if sender.source_addresses.is_empty() {
            return Err(refused(
                "a sender with transport = \"plain\" names its source_addresses",
            ));
        }
        if !sender.certificate_subject.is_empty() {
            return Err(refused(
                "a sender with transport = \"plain\" carries no certificate_subject",
            ));
        }
        let sources = sender
            .source_addresses
            .iter()
            .map(|value| network_of(value))
            .collect::<Result<Vec<_>, Error>>()?;
        senders.push(hl7v2::PlainSender {
            sender: sender_of(SENDERS, sender)?,
            sources,
        });
    }
    match (hl7v2.plain.as_ref(), senders.is_empty()) {
        (None, true) => Ok(None),
        (None, false) => Err(Error::Needs {
            key: String::from(SENDERS),
            needs: String::from("hl7v2.plain.bind"),
        }),
        (Some(_), true) => Err(Error::PlainUnused {
            key: String::from("hl7v2.plain"),
        }),
        (Some(plain), false) => {
            let bind = plain
                .bind
                .parse::<SocketAddr>()
                .map_err(|source| Error::Listen {
                    key: String::from("hl7v2.plain.bind"),
                    source,
                })?;
            if shared(bind, listen) {
                return Err(Error::SharedListener {
                    key: String::from("hl7v2.plain.bind"),
                    value: bind.to_string(),
                });
            }
            Ok(Some(hl7v2::Plain { bind, senders }))
        }
    }
}

/// Returns the TLS `tls` describes: its three PEM files read once and the
/// acceptor built from them.
///
/// # Errors
/// Returns [`Error::Needs`] for a file not named, [`Error::TlsFile`] for
/// one that cannot be read, and [`Error::Hl7v2Tls`] for material the
/// acceptor refuses.
fn tls_of(tls: &Hl7v2Tls) -> Result<hl7v2::Tls, Error> {
    let read = |key: &str, path: Option<&std::path::PathBuf>| {
        let path = path.ok_or_else(|| Error::Needs {
            key: String::from("hl7v2.enabled"),
            needs: key.to_owned(),
        })?;
        std::fs::read(path).map_err(|source| Error::TlsFile {
            key: key.to_owned(),
            source,
        })
    };
    let chain = read("hl7v2.tls.certificate_file", tls.certificate_file.as_ref())?;
    let key = read("hl7v2.tls.key_file", tls.key_file.as_ref())?;
    let authorities = read("hl7v2.tls.client_ca_file", tls.client_ca_file.as_ref())?;
    let acceptor =
        ferrobridge_hl7v2::mllp::tls::acceptor(&chain, &key, &authorities).map_err(|source| {
            Error::Hl7v2Tls {
                source: Box::new(source),
            }
        })?;
    Ok(hl7v2::Tls { acceptor })
}

/// Returns the senders of `senders` declared `origin = "device"`.
///
/// # Errors
/// Returns [`Error::SenderOrigin`] for an origin other than empty and
/// `device`, and for a device without its `device_identifier_issuer`.
fn devices_of(senders: &[Hl7v2Sender]) -> Result<Vec<hl7v2::Device>, Error> {
    let mut devices = Vec::new();
    for sender in senders {
        match sender.origin.as_str() {
            "" if sender.device_identifier_issuer.is_empty() => {}
            "device" if !sender.device_identifier_issuer.is_empty() => {
                devices.push(hl7v2::Device {
                    sender: sender_of("hl7v2.senders", sender)?,
                    identifier_issuer: sender.device_identifier_issuer.clone(),
                });
            }
            _ => {
                return Err(Error::SenderOrigin {
                    key: String::from("hl7v2.senders"),
                });
            }
        }
    }
    Ok(devices)
}

/// Returns the timeout `span` states, `None` for `"0s"`.
fn timeout_of(span: ferrohealth_config::units::Duration) -> Option<Duration> {
    (!span.is_zero()).then(|| span.get())
}

/// Returns the HL7 v2 face `hl7v2` describes, refused when the facade it
/// writes through is off.
pub(in crate::config) fn resolve_hl7v2(
    hl7v2: &Hl7v2,
    facade: bool,
) -> Result<Hl7v2Settings, Error> {
    if !facade {
        return Err(Error::Needs {
            key: String::from("hl7v2.enabled"),
            needs: String::from("facade.enabled"),
        });
    }
    let listen = hl7v2
        .listen
        .parse::<SocketAddr>()
        .map_err(|source| Error::Listen {
            key: String::from("hl7v2.listen"),
            source,
        })?;
    let default_charset = ferrobridge_hl7v2::decode::Charset::from_code(&hl7v2.default_charset)
        .ok_or_else(|| Error::Charset {
            key: String::from("hl7v2.default_charset"),
            value: hl7v2.default_charset.clone(),
        })?;
    let unmapped = match hl7v2.unmapped_entries.as_str() {
        "skip_and_count" => crate::facade::ingest::UnmappedEntries::SkipAndCount,
        "refuse" => crate::facade::ingest::UnmappedEntries::Refuse,
        _ => {
            return Err(Error::UnmappedEntries {
                key: String::from("hl7v2.unmapped_entries"),
                value: hl7v2.unmapped_entries.clone(),
            });
        }
    };
    let ehr_policy = hl7v2
        .ehr_policy
        .as_deref()
        .map(|value| ehr_policy_of("hl7v2.ehr_policy", value))
        .transpose()?;
    let mut profiles = BTreeMap::new();
    for claimed in &hl7v2.profiles {
        if !fhir_types::r4::schema::SCHEMAS.is_resource(&claimed.resource_type) {
            return Err(Error::ResourceType {
                key: String::from("hl7v2.profiles.resource_type"),
                value: claimed.resource_type.clone(),
            });
        }
        url_of("hl7v2.profiles.profile", &claimed.profile)?;
        if profiles
            .insert(claimed.resource_type.clone(), claimed.profile.clone())
            .is_some()
        {
            return Err(Error::Duplicate {
                key: String::from("hl7v2.profiles"),
                value: claimed.resource_type.clone(),
            });
        }
    }
    // NOTE: no specification governs this: our own design; a limit past the address
    // space bounds nothing more than the address space does, so it saturates.
    let frame_limit = usize::try_from(hl7v2.frame_limit.bytes()).unwrap_or(usize::MAX);
    if frame_limit == 0 {
        return Err(Error::FrameLimit {
            key: String::from("hl7v2.frame_limit"),
        });
    }
    Ok(Hl7v2Settings {
        listen,
        default_charset,
        unmapped,
        ehr_policy,
        profiles,
        timeouts: ferrobridge_hl7v2::mllp::listener::Timeouts {
            idle: timeout_of(hl7v2.idle_timeout),
            frame: timeout_of(hl7v2.frame_timeout),
        },
        frame_limit,
        log_outcomes: hl7v2.log_outcomes,
        facility_issuer: Some(hl7v2.facility_identifier_issuer.clone())
            .filter(|issuer| !issuer.is_empty()),
        senders: hl7v2
            .senders
            .iter()
            .map(|sender| sender_of("hl7v2.senders", sender))
            .collect::<Result<Vec<_>, Error>>()?,
        devices: devices_of(&hl7v2.senders)?,
        subjects: subjects_of(&hl7v2.senders)?,
        tls: tls_of(&hl7v2.tls)?,
        plain: plain_of(hl7v2, listen)?,
    })
}
