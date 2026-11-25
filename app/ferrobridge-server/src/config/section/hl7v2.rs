// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[hl7v2]` section.
//!
//! No specification governs the configuration: our own design.

use ferrohealth_config::units::{ByteSize, Duration};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The HL7 v2 face: an MLLP listener whose messages enter the facade's
/// ingest service.
///
/// The face is off until `enabled` is set, and it needs the facade: the
/// ingest service, the identity map and the programs it writes through are
/// the facade's. No specification governs this configuration: our own
/// design.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hl7v2 {
    /// Whether the MLLP listener runs.
    pub enabled: bool,
    /// The socket address the TLS listener binds, when `[hl7v2.tls]` is
    /// set.
    pub listen: String,
    /// How long a connection may sit between frames before it is closed; `0`
    /// never closes it.
    pub idle_timeout: Duration,
    /// How long one frame may take from its first byte to its trailer before
    /// it is answered `AR`; `0` waits for the trailer.
    pub frame_timeout: Duration,
    /// The largest message one frame may carry, in bytes.
    pub frame_limit: ByteSize,
    /// The TLS the MLLP listener serves, with the client certificates it
    /// requires; unset, no TLS listener runs.
    pub tls: Hl7v2Tls,
    /// The plain MLLP listener for the devices that cannot speak TLS;
    /// absent, no plain listener runs.
    pub plain: Option<Hl7v2Plain>,
}

/// The `[hl7v2.plain]` table: a second MLLP listener with no TLS, for the
/// devices that cannot speak it, each message held to its route's networks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hl7v2Plain {
    /// The socket address the plain listener binds, never the TLS
    /// listener's.
    pub bind: String,
}

/// The `[hl7v2.tls]` table: MLLP over TLS with client certificates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
#[expect(
    clippy::struct_field_names,
    reason = "the keys are [server.tls]'s, certificate_file, key_file and client_ca_file"
)]
pub struct Hl7v2Tls {
    /// The PEM file of the listener's certificate chain.
    pub certificate_file: Option<PathBuf>,
    /// The PEM file of the listener's private key.
    pub key_file: Option<PathBuf>,
    /// The PEM file of the authorities a client certificate must chain to.
    pub client_ca_file: Option<PathBuf>,
}

impl Default for Hl7v2 {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: String::from("127.0.0.1:2575"),
            idle_timeout: Duration::from_secs(300),
            frame_timeout: Duration::from_secs(30),
            frame_limit: ByteSize::from_mib(1),
            tls: Hl7v2Tls::default(),
            plain: None,
        }
    }
}
