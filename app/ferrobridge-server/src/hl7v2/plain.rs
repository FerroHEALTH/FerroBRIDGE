// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The face's two lanes and the check that binds a route to its connection.
//!
//! A route's proofs are the client certificate's subject and the peer's
//! networks, and every proof a route holds must hold. The TLS lane needs a
//! route bound to a subject, and the plain lane, for the devices that cannot
//! speak TLS, one bound to networks.
//!
//! HL7 MLLP Release 2 defines no security layer, and IHE ITI ATNA (ITI TF-2,
//! ITI-19) adds TLS for a node that supports it. A device that cannot is
//! served on a listener of its own, each message held to the networks its
//! route names. No specification governs the exception: our own design.

use std::net::SocketAddr;
use std::sync::Arc;

use ferrobridge_hl7v2::mllp::listener::Connection;
use tokio::net::TcpListener;

use crate::hl7v2::answer::Settled;
use crate::hl7v2::{Face, Listening};
use crate::inbound::verify::{Proofs, Request, Unproven, Verifier};
use crate::sources::document::Route;

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

/// Returns why a message routed by `route` may not be sent on `connection`
/// of `lane`, `None` when every proof of the route holds.
///
/// A route the lane cannot bind, one with no certificate subject on the TLS
/// listener or no network on the plain one, is refused as the lane's own
/// proof failing.
#[must_use]
pub fn unproven(lane: Lane, route: &Route, connection: &Connection) -> Option<Settled> {
    let bound = route.proofs.iter().any(|proof| match lane {
        Lane::Tls => matches!(proof, Verifier::Mtls(_)),
        Lane::Plain => matches!(proof, Verifier::Network(_)),
    });
    if !bound {
        return Some(match lane {
            Lane::Tls => Settled::Certificate,
            Lane::Plain => Settled::Address,
        });
    }
    let headers = http::HeaderMap::new();
    let request = Request {
        headers: &headers,
        body: &[],
        peer: connection.certificate(),
        now: 0,
    };
    let proofs = route
        .proofs
        .iter()
        .fold(Proofs::default(), |proofs, proof| {
            proofs.with(proof, Vec::new())
        });
    let address = connection.peer().map(|peer| peer.ip());
    match proofs.verify(&request, address) {
        Ok(()) => None,
        Err(Some(Unproven {
            proof: Verifier::Network(_),
            ..
        })) => Some(Settled::Address),
        Err(_) => Some(Settled::Certificate),
    }
}

impl Face {
    /// Returns this face answering on the plain listener at `bind`, sharing
    /// its maps, its routes, its usage window and its access log.
    #[must_use]
    pub fn on_plain(&self, bind: SocketAddr) -> Self {
        let mut face = self.clone();
        face.lane = Lane::Plain;
        if let Some((_, listen)) = face.access_log.as_mut() {
            *listen = bind;
        }
        face
    }

    /// Returns the listener this face answers on.
    #[must_use]
    pub const fn lane(&self) -> Lane {
        self.lane
    }

    /// Returns why a message routed by `route` may not be sent on
    /// `connection`, `None` when it may.
    ///
    /// Every proof the route holds must hold for the connection. On the
    /// plain listener a refused connection is closed after its answer, since
    /// its peer sent as a sender it is not.
    pub(super) fn unbound(&self, route: &Route, connection: &Connection) -> Option<Settled> {
        let settled = unproven(self.lane, route, connection)?;
        if self.lane == Lane::Plain {
            tracing::warn!(
                peer = connection.peer().map(|peer| peer.ip().to_string()),
                "a message on the plain listener names a route its connection does not prove; the connection closes"
            );
            connection.close();
        }
        Some(settled)
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

/// Binds the HL7 v2 face's plain listener at `bind` for `face`, warning that
/// the lane carries no TLS.
///
/// A port that cannot be bound refuses the start.
pub(crate) async fn bind(bind: SocketAddr, face: &Face) -> anyhow::Result<StartedPlain> {
    use anyhow::Context;

    let socket = TcpListener::bind(bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    tracing::warn!(
        lane = "hl7v2",
        bind = %bind,
        "the HL7 v2 face listens for plain MLLP: no confidentiality or integrity on the wire, and each sender's identity is its network address alone"
    );
    Ok(StartedPlain {
        socket,
        face: Arc::new(face.on_plain(bind)),
        listening: Listening::default(),
    })
}
