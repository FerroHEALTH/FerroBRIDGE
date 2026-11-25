// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The peers of the post-quantum key exchange cases.
//!
//! One peer offers the hybrid group X25519MLKEM768 (RFC 10024), one offers
//! only classical groups, and a stand-in HTTPS server answers one request
//! and reports the group its handshake agreed on.
//!
//! No specification governs the harness; it is FerroBRIDGE's own design.

use std::net::SocketAddr;
use std::sync::Arc;

use rustls::crypto::CryptoProvider;
use rustls::crypto::aws_lc_rs::{default_provider, kx_group};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{NamedGroup, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

use crate::mllp_tls::{Pki, PkiError};

/// What a peer offers in the TLS 1.3 key exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Peer {
    /// X25519MLKEM768 first, then X25519.
    Hybrid,
    /// X25519, P-256 and P-384, and no hybrid group.
    Classical,
}

impl Peer {
    /// Both peers, the hybrid one first.
    pub const BOTH: [Self; 2] = [Self::Hybrid, Self::Classical];

    /// Returns the `aws-lc-rs` provider limited to the peer's groups.
    #[must_use]
    pub fn provider(self) -> Arc<CryptoProvider> {
        let mut provider = default_provider();
        provider.kx_groups = match self {
            Self::Hybrid => vec![kx_group::X25519MLKEM768, kx_group::X25519],
            Self::Classical => vec![kx_group::X25519, kx_group::SECP256R1, kx_group::SECP384R1],
        };
        Arc::new(provider)
    }

    /// Returns the group a bridge that prefers X25519MLKEM768 agrees on with
    /// this peer.
    #[must_use]
    pub const fn agreed(self) -> NamedGroup {
        match self {
            Self::Hybrid => NamedGroup::X25519MLKEM768,
            Self::Classical => NamedGroup::X25519,
        }
    }
}

/// Returns the group the handshake of `connection` agreed on, if it ended.
#[must_use]
pub fn agreed<Data>(connection: &rustls::ConnectionCommon<Data>) -> Option<NamedGroup> {
    connection
        .negotiated_key_exchange_group()
        .map(rustls::crypto::SupportedKxGroup::name)
}

/// Returns the server configuration `peer` presents `pki`'s server
/// certificate with, asking no client certificate.
///
/// # Errors
///
/// Returns [`PkiError::Pem`] or [`PkiError::Tls`] when the configuration
/// cannot be built.
pub fn server_config(pki: &Pki, peer: Peer) -> Result<ServerConfig, PkiError> {
    let chain = CertificateDer::pem_slice_iter(pki.server_certificate().as_bytes())
        .collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_slice(pki.server_key().as_bytes())?;
    Ok(ServerConfig::builder_with_provider(peer.provider())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(chain, key)?)
}

/// Starts an HTTPS stand-in on a local port that serves one request as
/// `peer`, answers it with `answer`, a whole HTTP/1.1 response, and resolves
/// to the group the handshake agreed on.
///
/// # Errors
///
/// Returns [`PkiError::Pem`] or [`PkiError::Tls`] when the configuration
/// cannot be built, and [`PkiError::Connect`] when no local port binds. The
/// task fails with [`PkiError::Connect`] when the exchange fails.
pub async fn https_once(
    pki: &Pki,
    peer: Peer,
    answer: &'static [u8],
) -> Result<(SocketAddr, JoinHandle<Result<NamedGroup, PkiError>>), PkiError> {
    let acceptor = TlsAcceptor::from(Arc::new(server_config(pki, peer)?));
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let served = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await?;
        let mut tls = acceptor.accept(tcp).await?;
        let group = agreed(tls.get_ref().1)
            .ok_or_else(|| std::io::Error::other("the handshake agreed on no group"))?;
        read_request(&mut tls).await?;
        tls.write_all(answer).await?;
        tls.shutdown().await?;
        Ok(group)
    });
    Ok((address, served))
}

/// Reads one HTTP/1.1 request from `stream`: its head, then the body its
/// `content-length` names.
async fn read_request<S>(stream: &mut S) -> std::io::Result<()>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut request = Vec::new();
    let head = loop {
        if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
        if stream.read_buf(&mut request).await? == 0 {
            return Err(std::io::Error::other(
                "the client closed before its request ended",
            ));
        }
    };
    let length = String::from_utf8_lossy(&request)
        .lines()
        .take_while(|line| !line.is_empty())
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while request.len() < head + length {
        if stream.read_buf(&mut request).await? == 0 {
            return Err(std::io::Error::other(
                "the client closed before its body ended",
            ));
        }
    }
    Ok(())
}
