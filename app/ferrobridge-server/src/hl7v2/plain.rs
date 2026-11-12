// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The face's plain lane: MLLP with no TLS for the device senders that
//! cannot speak it, each tied to the networks it connects from.
//!
//! HL7 MLLP Release 2 defines no security layer, and IHE ITI ATNA (ITI TF-2,
//! ITI-19) adds TLS for a node that supports it. A device that cannot is
//! served on a listener of its own, admitted by its peer address at connect
//! and again by its MSH-4 per message. No specification governs the
//! exception: our own design.

use std::net::SocketAddr;
use std::sync::Arc;

use ferrobridge_hl7v2::mllp::listener::Connection;
use ferrobridge_hl7v2::parse::Message;
use tokio::net::TcpListener;

use crate::hl7v2::answer::Settled;
use crate::hl7v2::{Face, Listening, certified};

/// The listener a face answers on.
///
/// No specification governs the plain lane: our own design for the devices
/// that cannot speak TLS (HL7 MLLP Release 2 defines no security layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// MLLP over TLS, each sender tied to its client certificate.
    Tls,
    /// Plain MLLP, each device sender tied to its networks.
    Plain,
}

/// Returns whether the sender MSH-4 of `message` names is a plain sender of
/// `plain` whose networks hold `peer`.
///
/// A plain sender is identified by its network address alone (our own
/// design), so a facility only another network's device or a TLS sender
/// declares is refused.
#[must_use]
pub fn addressed(
    plain: Option<&crate::config::hl7v2::Plain>,
    message: &Message,
    peer: Option<SocketAddr>,
) -> bool {
    let (Some(plain), Some(peer)) = (plain, peer) else {
        return false;
    };
    plain
        .senders
        .iter()
        .any(|sender| sender.sender.names(message) && sender.connects_from(peer.ip()))
}

impl Face {
    /// Returns this face answering on the plain listener at `bind`, sharing
    /// its maps, its usage window and its access log; `None` when no plain
    /// listener is configured.
    #[must_use]
    pub fn on_plain(&self, bind: SocketAddr) -> Option<Self> {
        self.plain.as_ref()?;
        let mut face = self.clone();
        face.lane = Lane::Plain;
        if let Some((_, listen)) = face.access_log.as_mut() {
            *listen = bind;
        }
        Some(face)
    }

    /// Returns the listener this face answers on.
    #[must_use]
    pub const fn lane(&self) -> Lane {
        self.lane
    }

    /// Returns why `message` may not be sent on `connection` by the sender
    /// its MSH-4 names, `None` when it may.
    ///
    /// On the TLS listener the client certificate must name the sender. On
    /// the plain listener the sender must be a plain sender whose networks
    /// hold the peer address, and a refused connection is closed after its
    /// answer, since its peer sent as a sender it is not.
    pub(super) fn unbound(&self, message: &Message, connection: &Connection) -> Option<Settled> {
        match self.lane {
            Lane::Tls => {
                (!certified(&self.subjects, message, connection)).then_some(Settled::Certificate)
            }
            Lane::Plain => {
                if addressed(self.plain.as_ref(), message, connection.peer()) {
                    return None;
                }
                tracing::warn!(
                    peer = connection.peer().map(|peer| peer.ip().to_string()),
                    "a message on the plain listener names no plain sender of its peer address; the connection closes"
                );
                connection.close();
                Some(Settled::Address)
            }
        }
    }
}

/// The HL7 v2 face's plain listener, bound and ready to serve.
pub(crate) struct StartedPlain {
    /// The bound plain MLLP socket.
    pub(crate) socket: TcpListener,
    /// The face on its plain lane.
    pub(crate) face: Arc<Face>,
    /// Whether the plain listener accepts, for the readiness probe.
    pub(crate) listening: Listening,
}

/// Binds the HL7 v2 face's plain listener for `face`, warning once per plain
/// sender that the route carries no TLS.
///
/// A port that cannot be bound refuses the start.
pub(crate) async fn bind(
    plain: &crate::config::hl7v2::Plain,
    face: &Face,
) -> anyhow::Result<StartedPlain> {
    use anyhow::Context;

    let socket = TcpListener::bind(plain.bind)
        .await
        .with_context(|| format!("binding {}", plain.bind))?;
    tracing::info!(
        lane = "hl7v2",
        bind = %plain.bind,
        senders = plain.senders.len(),
        "the HL7 v2 face listens for plain MLLP from its device senders"
    );
    for sender in &plain.senders {
        let networks = sender
            .sources
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        tracing::warn!(
            facility = sender.sender.facility(),
            source_addresses = networks,
            "a device sends plain MLLP: no confidentiality or integrity on the wire, and its identity is its network address alone"
        );
    }
    let face = face
        .on_plain(plain.bind)
        .context("the plain listener has no face of its own")?;
    Ok(StartedPlain {
        socket,
        face: Arc::new(face),
        listening: Listening::default(),
    })
}
