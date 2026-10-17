// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An address no server answers on, for the cases that need an unreachable
//! upstream.
//!
//! Dropping a `wiremock::MockServer` does not free its address: wiremock pools
//! released servers and they keep answering, `404` by default. A fixed low
//! port (`127.0.0.1:1`) is closed on most hosts and promised on none, and a
//! socket bound without a listen is refused on Linux but left to time out on
//! macOS. [`closed_port`] asks the system for a free loopback port and
//! releases it at once, so a connect to it is refused immediately; the system
//! hands out ephemeral ports in turn, so the port is not reused within a test.
//! No specification governs this: our own design.

use std::net::{Ipv4Addr, SocketAddr, TcpListener};

/// Why no closed port could be found.
#[derive(Debug, thiserror::Error)]
#[error("cannot find a closed loopback port")]
pub struct ClosedPortError {
    /// What the socket layer reported.
    #[source]
    source: std::io::Error,
}

/// Returns a loopback address nothing listens on.
///
/// # Errors
/// Returns [`ClosedPortError`] when no loopback port can be bound.
pub fn closed_port() -> Result<SocketAddr, ClosedPortError> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .map_err(|source| ClosedPortError { source })?;
    listener
        .local_addr()
        .map_err(|source| ClosedPortError { source })
}

/// Returns `http://` and a loopback address nothing listens on, with no
/// trailing slash.
///
/// # Errors
/// Returns [`ClosedPortError`] when no loopback port can be bound.
pub fn closed_http_url() -> Result<String, ClosedPortError> {
    closed_port().map(|address| format!("http://{address}"))
}

#[cfg(test)]
mod tests {
    use super::closed_port;

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn a_connect_to_a_closed_port_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let address = closed_port()?;
        let error = std::net::TcpStream::connect(address)
            .err()
            .ok_or("nothing listens on a closed port")?;
        assert_eq!(std::io::ErrorKind::ConnectionRefused, error.kind());
        Ok(())
    }
}
