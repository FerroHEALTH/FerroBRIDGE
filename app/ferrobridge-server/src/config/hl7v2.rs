// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings of the HL7 v2 face that `[hl7v2]` resolves into.

/// One sending facility the face accepts, matched against MSH-4.
///
/// MSH-4 is an `HD`: a namespace id in HD.1, or a universal id in HD.2 with
/// its type in HD.3 (HL7 v2.5.1 chapter 2, `HD` data type). No specification
/// governs an allow list: our own design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sender {
    /// A facility named by its namespace id, HD.1.
    Namespace(String),
    /// A facility named by its universal id, HD.2, of the type HD.3.
    Universal {
        /// HD.2.
        id: String,
        /// HD.3, such as `ISO`.
        kind: String,
    },
}

/// A sender declared `origin = "device"`: an analyser feed with no human,
/// whose results commit as acts of the device. No specification governs the
/// declaration: our own design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// The sending facility the device sends as.
    pub sender: Sender,
    /// The namespace its device identifiers are issued in.
    pub identifier_issuer: String,
}

/// The DNS name a sender's client certificate carries in its subject
/// alternative name, which ties the sender to the connection it sends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    /// The sending facility.
    pub sender: Sender,
    /// The DNS name its client certificate carries.
    pub name: String,
}

/// The TLS the MLLP listener serves: its certificate and key, and the
/// authorities a client certificate must chain to.
#[derive(Clone)]
pub struct Tls {
    /// The acceptor, which requires a client certificate.
    pub acceptor: tokio_rustls::TlsAcceptor,
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tls").finish_non_exhaustive()
    }
}

impl Sender {
    /// Returns the facility as MSH-4 writes it: the namespace id, or
    /// `^<universal id>^<type>` (HL7 v2.5.1 chapter 2, `HD` data type).
    #[must_use]
    pub fn facility(&self) -> String {
        match self {
            Self::Namespace(namespace) => namespace.clone(),
            Self::Universal { id, kind } => format!("^{id}^{kind}"),
        }
    }
}

/// A device sender declared `transport = "plain"`: it sends MLLP with no
/// TLS, and the face takes it only from the networks it names.
///
/// HL7 MLLP Release 2 defines no security layer, and IHE ITI ATNA (ITI TF-2,
/// ITI-19) adds TLS only for a node that supports it; the exception for a
/// device that cannot is our own design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainSender {
    /// The sending facility the device sends as.
    pub sender: Sender,
    /// The networks it connects from.
    pub sources: Vec<ipnet::IpNet>,
}

impl PlainSender {
    /// Returns whether `peer` lies in one of the sender's networks, an
    /// IPv4 peer seen as an IPv4-mapped IPv6 address included.
    #[must_use]
    pub fn connects_from(&self, peer: std::net::IpAddr) -> bool {
        let peer = peer.to_canonical();
        self.sources.iter().any(|network| network.contains(&peer))
    }
}

/// The plain MLLP listener and the device senders it serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plain {
    /// The socket address the plain listener binds.
    pub bind: std::net::SocketAddr,
    /// The senders declared `transport = "plain"`, at least one.
    pub senders: Vec<PlainSender>,
}

impl Plain {
    /// Returns whether a connection from `peer` comes from some plain
    /// sender's network.
    #[must_use]
    pub fn admits(&self, peer: std::net::IpAddr) -> bool {
        self.senders.iter().any(|sender| sender.connects_from(peer))
    }
}
