// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An address no server answers on, for the cases that need an unreachable
//! upstream.
//!
//! Dropping a `wiremock::MockServer` does not free its address: wiremock pools
//! released servers and they keep answering, `404` by default, and a released
//! port can be handed to another test's server before the test connects. A
//! fixed low port (`127.0.0.1:1`) is closed on most hosts and promised on none.
//! On Linux [`closed_port`] keeps a socket bound to the port without a listen,
//! so a connect is refused at once and no other bind can take the port while
//! the returned [`ClosedPort`] is held. Such a socket leaves a connect to time
//! out on macOS, so there the port is released at once, as before. No
//! specification governs this: our own design.

use core::fmt;
use core::ops::Deref;
use std::net::{Ipv4Addr, SocketAddr};

/// Why no closed port could be found.
#[derive(Debug, thiserror::Error)]
#[error("cannot find a closed loopback port")]
pub struct ClosedPortError {
    /// What the socket layer reported.
    #[source]
    source: std::io::Error,
}

/// A loopback address nothing listens on, kept closed while this is held.
#[derive(Debug)]
pub struct ClosedPort {
    address: SocketAddr,
    #[cfg(target_os = "linux")]
    _reservation: socket2::Socket,
}

impl ClosedPort {
    /// Returns the address.
    #[must_use]
    pub const fn address(&self) -> SocketAddr {
        self.address
    }
}

impl fmt::Display for ClosedPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.address.fmt(f)
    }
}

/// `http://` and a closed loopback address, with no trailing slash, kept
/// closed while this is held.
#[derive(Debug)]
pub struct ClosedUrl {
    url: String,
    _port: ClosedPort,
}

impl Deref for ClosedUrl {
    type Target = str;

    fn deref(&self) -> &str {
        &self.url
    }
}

impl fmt::Display for ClosedUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.url)
    }
}

/// Returns a loopback address nothing listens on.
///
/// # Errors
/// Returns [`ClosedPortError`] when no loopback port can be bound.
#[cfg(target_os = "linux")]
pub fn closed_port() -> Result<ClosedPort, ClosedPortError> {
    use socket2::{Domain, Socket, Type};
    let error = |source| ClosedPortError { source };
    let socket = Socket::new(Domain::IPV4, Type::STREAM, None).map_err(error)?;
    socket
        .bind(&SocketAddr::from((Ipv4Addr::LOCALHOST, 0)).into())
        .map_err(error)?;
    let address = socket
        .local_addr()
        .map_err(error)?
        .as_socket()
        .ok_or_else(|| {
            error(std::io::Error::other(
                "the bound address is not an IP address",
            ))
        })?;
    Ok(ClosedPort {
        address,
        _reservation: socket,
    })
}

/// Returns a loopback address nothing listens on.
///
/// # Errors
/// Returns [`ClosedPortError`] when no loopback port can be bound.
#[cfg(not(target_os = "linux"))]
pub fn closed_port() -> Result<ClosedPort, ClosedPortError> {
    let listener = std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .map_err(|source| ClosedPortError { source })?;
    let address = listener
        .local_addr()
        .map_err(|source| ClosedPortError { source })?;
    Ok(ClosedPort { address })
}

/// Returns `http://` and a loopback address nothing listens on, with no
/// trailing slash.
///
/// # Errors
/// Returns [`ClosedPortError`] when no loopback port can be bound.
pub fn closed_http_url() -> Result<ClosedUrl, ClosedPortError> {
    let port = closed_port()?;
    Ok(ClosedUrl {
        url: format!("http://{port}"),
        _port: port,
    })
}

#[cfg(test)]
mod tests {
    use super::closed_port;

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn a_connect_to_a_closed_port_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let port = closed_port()?;
        let error = std::net::TcpStream::connect(port.address())
            .err()
            .ok_or("nothing listens on a closed port")?;
        assert_eq!(std::io::ErrorKind::ConnectionRefused, error.kind());
        Ok(())
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn a_held_closed_port_cannot_be_bound_by_another_listener()
    -> Result<(), Box<dyn std::error::Error>> {
        let port = closed_port()?;
        let error = std::net::TcpListener::bind(port.address())
            .err()
            .ok_or("a held closed port was bound again")?;
        assert_eq!(std::io::ErrorKind::AddrInUse, error.kind());
        Ok(())
    }
}
