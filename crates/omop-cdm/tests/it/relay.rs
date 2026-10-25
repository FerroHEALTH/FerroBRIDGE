// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pool's TLS goes through the writer's configuration: against a
//! stand-in server that answers PostgreSQL's `SSLRequest`
//! (<https://www.postgresql.org/docs/current/protocol-flow.html#PROTOCOL-FLOW-SSL>)
//! and presents a certificate a synthetic root signed, the pool's first
//! message arrives over TLS when the root is the configured CA, and no
//! message arrives when another root is.
//!
//! Every certificate is synthetic, generated at run time and never
//! committed. No specification governs the stand-in: our own design.

use omop_cdm::connection::{CdmConnection, ConnectionError};
use omop_cdm::database::CdmPool;
use omop_cdm::ddl::SchemaName;
use rcgen::KeyPair;
use rcgen::{BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer};
use rustls::crypto::aws_lc_rs;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sqlx::postgres::{PgPoolOptions, PgSslMode};
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

type BoxError = Box<dyn Error + Send + Sync>;

/// The `SSLRequest` message: length 8, request code 80877103.
const SSL_REQUEST: [u8; 8] = [0, 0, 0, 8, 0x04, 0xd2, 0x16, 0x2f];

/// One synthetic root and the server certificate it signed for `127.0.0.1`.
struct Material {
    root: String,
    server: CertificateDer<'static>,
    server_key: Vec<u8>,
}

/// Generates a root named `name` and a server certificate it signs.
fn material(name: &str) -> Result<Material, BoxError> {
    let root_key = KeyPair::generate()?;
    let mut root_params = CertificateParams::new(Vec::<String>::new())?;
    root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    root_params
        .distinguished_name
        .push(DnType::CommonName, name);
    let root = root_params.self_signed(&root_key)?;
    let issuer = Issuer::new(root_params, root_key);
    let server_key = KeyPair::generate()?;
    let mut server = CertificateParams::new(vec![String::from("127.0.0.1")])?;
    server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let server = server.signed_by(&server_key, &issuer)?;
    Ok(Material {
        root: root.pem(),
        server: server.der().clone(),
        server_key: server_key.serialize_der(),
    })
}

/// Starts a stand-in server for one connection: it answers `SSLRequest`
/// with `S`, handshakes with the material's certificate, and returns the
/// first eight bytes the client sends over TLS, or `None` when the handshake
/// fails.
async fn stand_in(
    material: &Material,
) -> Result<(SocketAddr, JoinHandle<Result<Option<[u8; 8]>, BoxError>>), BoxError> {
    let config =
        rustls::ServerConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(
                vec![material.server.clone()],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(material.server_key.clone())),
            )?;
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let served = tokio::spawn(async move {
        let (mut tcp, _) = listener.accept().await?;
        let mut request = [0_u8; 8];
        tcp.read_exact(&mut request).await?;
        if request != SSL_REQUEST {
            return Err::<_, BoxError>("the client sent no SSLRequest".into());
        }
        tcp.write_all(b"S").await?;
        let Ok(mut tls) = acceptor.accept(tcp).await else {
            return Ok(None);
        };
        let mut startup = [0_u8; 8];
        tls.read_exact(&mut startup).await?;
        Ok(Some(startup))
    });
    Ok((address, served))
}

/// Tries one pool connection through `connection`, which fails once the
/// stand-in closes.
async fn try_pool(connection: &CdmConnection) -> Result<(), BoxError> {
    let attempt = CdmPool::connect(
        PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(5)),
        connection.pool_options()?,
        SchemaName::new("cdm")?,
    )
    .await;
    assert!(attempt.is_err(), "the stand-in speaks no PostgreSQL");
    Ok(())
}

#[tokio::test]
async fn the_pool_reaches_the_server_over_tls_trusting_the_configured_ca() -> Result<(), BoxError> {
    let material = material("Synthetic CDM root")?;
    let (address, served) = stand_in(&material).await?;
    let url = format!("postgres://bridge:secret@{address}/cdm?sslmode=verify-full");
    let connection = CdmConnection::new(&url, Some(material.root.as_bytes()))?;
    try_pool(&connection).await?;
    let startup = tokio::time::timeout(Duration::from_secs(10), served).await???;
    let startup = startup.ok_or("the handshake completed")?;
    assert_eq!(
        Some(&[0, 3, 0, 0][..]),
        startup.get(4..8),
        "the pool's StartupMessage, protocol 3.0, arrives over TLS"
    );
    Ok(())
}

#[tokio::test]
async fn the_pool_refuses_a_server_another_root_signed() -> Result<(), BoxError> {
    let presented = material("Synthetic CDM root")?;
    let configured = material("Another synthetic root")?;
    let (address, served) = stand_in(&presented).await?;
    let url = format!("postgres://bridge:secret@{address}/cdm?sslmode=verify-ca");
    let connection = CdmConnection::new(&url, Some(configured.root.as_bytes()))?;
    try_pool(&connection).await?;
    let startup = tokio::time::timeout(Duration::from_secs(10), served).await???;
    assert_eq!(
        None, startup,
        "no message crosses a handshake the CA refuses"
    );
    Ok(())
}

#[tokio::test]
async fn the_pool_never_runs_sqlx_tls_whatever_the_mode() -> Result<(), BoxError> {
    for mode in ["require", "verify-ca", "verify-full"] {
        let connection = CdmConnection::new(
            &format!("postgres://bridge:secret@127.0.0.1:5432/cdm?sslmode={mode}"),
            None,
        )?;
        let options = connection.pool_options()?;
        assert!(
            matches!(options.get_ssl_mode(), PgSslMode::Disable),
            "sqlx's own TLS, with its built-in roots, is never asked for under {mode}"
        );
        let socket = options.get_socket().ok_or("the relay's socket")?;
        assert!(socket.is_dir(), "{socket:?} is the relay's directory");
    }
    let plain = CdmConnection::new(
        "postgres://bridge:secret@127.0.0.1:5432/cdm?sslmode=disable",
        None,
    )?;
    assert_eq!(
        None,
        plain.pool_options()?.get_socket(),
        "disable reaches the server itself"
    );
    Ok(())
}

#[test]
fn outside_a_runtime_the_relay_is_a_typed_error() -> Result<(), BoxError> {
    let connection = CdmConnection::new(
        "postgres://bridge:secret@127.0.0.1:5432/cdm?sslmode=require",
        None,
    )?;
    let refused = connection.pool_options();
    assert!(
        matches!(refused, Err(ConnectionError::Relay { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_tls_url_naming_two_hosts_is_refused() {
    let refused = CdmConnection::new(
        "postgres://bridge:secret@db-a.invalid:5432,db-b.invalid:5432/cdm?sslmode=require",
        None,
    );
    assert!(
        matches!(refused, Err(ConnectionError::Hosts { hosts: 2, .. })),
        "{refused:?}"
    );
}
