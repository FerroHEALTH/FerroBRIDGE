// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The face's two lanes and the check that binds a route to its connection.
//!
//! The client certificate's subject binds a route on the TLS lane, and the
//! peer's network on the plain lane for the devices that cannot speak TLS.
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

/// Returns whether `peer` lies in one of the networks `route` names, an
/// IPv4 peer seen as an IPv4-mapped IPv6 address included.
#[must_use]
pub fn addressed(route: &Route, peer: Option<SocketAddr>) -> bool {
    let Some(peer) = peer else {
        return false;
    };
    let peer = peer.ip().to_canonical();
    route
        .source_cidrs
        .iter()
        .filter_map(|network| network.parse::<ipnet::IpNet>().ok())
        .any(|network| network.contains(&peer))
}

/// Returns whether the client certificate of `connection` names the subject
/// `route` is bound to.
#[must_use]
pub fn certified(route: &Route, connection: &Connection) -> bool {
    let (Some(certificate), Some(subject)) = (
        connection.certificate(),
        route.certificate_subject.as_deref(),
    ) else {
        return false;
    };
    ferrobridge_hl7v2::mllp::tls::names(certificate, subject)
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
    /// On the TLS listener the client certificate must name the subject the
    /// route is bound to. On the plain listener the peer address must lie in
    /// one of the route's networks, and a refused connection is closed after
    /// its answer, since its peer sent as a sender it is not.
    pub(super) fn unbound(&self, route: &Route, connection: &Connection) -> Option<Settled> {
        match self.lane {
            Lane::Tls => (!certified(route, connection)).then_some(Settled::Certificate),
            Lane::Plain => {
                if addressed(route, connection.peer()) {
                    return None;
                }
                tracing::warn!(
                    peer = connection.peer().map(|peer| peer.ip().to_string()),
                    "a message on the plain listener names a route that holds none of its peer's networks; the connection closes"
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
