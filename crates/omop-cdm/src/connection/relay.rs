// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pool's TLS: a relay on a Unix socket that the `sqlx` pool reaches in
//! plaintext, and that reaches the server over the writer's TLS
//! configuration.
//!
//! sqlx 0.9 builds its own rustls configuration, adds its built-in root set
//! to any CA it is given, and takes no connector of the caller's, so the
//! pool never runs TLS itself: each connection it opens to the relay's
//! socket becomes one TCP connection to the server, upgraded with the
//! PostgreSQL `SSLRequest` and handshaken with the configuration the writer
//! uses (<https://www.postgresql.org/docs/current/protocol-flow.html#PROTOCOL-FLOW-SSL>).
//! Both clients therefore trust exactly what [`super::CdmConnection`] trusts.
//!
//! The socket lives in a directory only the process's user can enter, which
//! `tempfile` creates with mode `0700`. No specification governs the relay:
//! our own design.

use std::io;
use std::path::Path;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixListener, UnixStream};
use tokio_postgres::tls::{MakeTlsConnect, TlsConnect};
use tokio_postgres_rustls::MakeRustlsConnect;

/// The `SSLRequest` message: its length, 8, then the request code 80877103
/// (PostgreSQL protocol, "Message Formats", `SSLRequest`).
const SSL_REQUEST: [u8; 8] = [0, 0, 0, 8, 0x04, 0xd2, 0x16, 0x2f];

/// The server the relay reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Upstream {
    /// The host name or address, as the URL names it; also the name the
    /// server certificate is checked against.
    pub(super) host: String,
    /// The TCP port.
    pub(super) port: u16,
}

/// A running relay; dropping it stops accepting and removes the socket.
#[derive(Debug)]
pub(super) struct Relay {
    directory: tempfile::TempDir,
    accept: tokio::task::JoinHandle<()>,
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.accept.abort();
    }
}

impl Relay {
    /// Starts a relay to `upstream` over `tls` on the current Tokio runtime.
    ///
    /// The socket is named as libpq names a server's,
    /// `.s.PGSQL.<port>` in the directory [`Relay::directory`] returns.
    pub(super) fn start(upstream: Upstream, tls: MakeRustlsConnect) -> io::Result<Self> {
        let runtime = tokio::runtime::Handle::try_current().map_err(io::Error::other)?;
        let directory = tempfile::Builder::new()
            .prefix("ferrobridge-cdm-")
            .tempdir()?;
        let socket = directory.path().join(format!(".s.PGSQL.{}", upstream.port));
        let bound = std::os::unix::net::UnixListener::bind(&socket)?;
        bound.set_nonblocking(true)?;
        let entered = runtime.enter();
        let listener = UnixListener::from_std(bound)?;
        drop(entered);
        let accept = runtime.spawn(accept(listener, upstream, tls));
        Ok(Self { directory, accept })
    }

    /// Returns the directory the socket is in.
    pub(super) fn directory(&self) -> &Path {
        self.directory.path()
    }

    /// Returns whether the relay still accepts: its runtime is alive and the
    /// listener has not failed.
    pub(super) fn is_running(&self) -> bool {
        !self.accept.is_finished()
    }
}

/// Accepts the pool's connections until the listener fails, relaying each on
/// a task of its own.
async fn accept(listener: UnixListener, upstream: Upstream, tls: MakeRustlsConnect) {
    while let Ok((client, _)) = listener.accept().await {
        let upstream = upstream.clone();
        let tls = tls.clone();
        // NOTE: no specification governs this: our own design; a connection
        // that fails is closed, and the pool reports the end of its stream.
        tokio::spawn(async move { drop(relay(client, &upstream, tls).await) });
    }
}

/// Relays one pool connection to the server over TLS.
async fn relay(
    mut client: UnixStream,
    upstream: &Upstream,
    mut tls: MakeRustlsConnect,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut server = TcpStream::connect((upstream.host.as_str(), upstream.port)).await?;
    server.write_all(&SSL_REQUEST).await?;
    let mut answer = [0; 1];
    server.read_exact(&mut answer).await?;
    if answer != *b"S" {
        return Err("the server refused TLS".into());
    }
    let connect = MakeTlsConnect::<TcpStream>::make_tls_connect(&mut tls, &upstream.host)?;
    let mut encrypted = connect.connect(server).await?;
    tokio::io::copy_bidirectional(&mut client, &mut encrypted).await?;
    Ok(())
}
