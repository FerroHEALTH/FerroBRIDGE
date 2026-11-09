// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A CDR that asks for a client certificate and whose own certificate chains
//! to a private root, reached with the TLS `[cdr.endpoint.tls]` builds: the
//! configuration the client's engine is handed.
//!
//! Every certificate is synthetic, generated at run time and never committed.
//! No specification governs the stub: our own design.

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use http::StatusCode;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
};
use rustls::RootCertStore;
use rustls::crypto::aws_lc_rs;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

type BoxError = Box<dyn Error + Send + Sync>;

/// The synthetic material of one run.
struct Material {
    root: CertificateDer<'static>,
    server: CertificateDer<'static>,
    server_key: Vec<u8>,
    client: CertificateDer<'static>,
    client_key: Vec<u8>,
}

/// Generates a private root that signs a server certificate for `localhost`
/// and a client certificate.
fn material() -> Result<Material, BoxError> {
    let root_key = KeyPair::generate()?;
    let mut root = CertificateParams::new(Vec::<String>::new())?;
    root.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    root.distinguished_name
        .push(DnType::CommonName, "Synthetic CDR root");
    let root_certificate = root.self_signed(&root_key)?;
    let issuer = Issuer::new(root, root_key);
    let server_key = KeyPair::generate()?;
    let mut server = CertificateParams::new(vec![String::from("localhost")])?;
    server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let server = server.signed_by(&server_key, &issuer)?;
    let client_key = KeyPair::generate()?;
    let mut client = CertificateParams::new(Vec::<String>::new())?;
    client.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    let client = client.signed_by(&client_key, &issuer)?;
    Ok(Material {
        root: root_certificate.der().clone(),
        server: server.der().clone(),
        server_key: server_key.serialize_der(),
        client: client.der().clone(),
        client_key: client_key.serialize_der(),
    })
}

/// Starts a stub that requires a client certificate the root signed and
/// answers one request `200` with an empty template list, returning its address and its task.
async fn stub(
    material: &Material,
) -> Result<(SocketAddr, JoinHandle<Result<(), BoxError>>), BoxError> {
    let provider = Arc::new(aws_lc_rs::default_provider());
    let mut roots = RootCertStore::empty();
    roots.add(material.root.clone())?;
    let verifier =
        WebPkiClientVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&provider))
            .build()?;
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_client_cert_verifier(verifier)
        .with_single_cert(
            vec![material.server.clone()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(material.server_key.clone())),
        )?;
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let served = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await?;
        let mut tls = acceptor.accept(tcp).await?;
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = tls.read(&mut chunk).await?;
            if read == 0 {
                return Err::<(), BoxError>("the client closed before its request ended".into());
            }
            request.extend_from_slice(chunk.get(..read).ok_or("a read past the buffer")?);
        }
        tls.write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n[]",
        )
            .await?;
        tls.shutdown().await?;
        Ok(())
    });
    Ok((address, served))
}

/// Returns the client TLS a `[cdr.endpoint.tls]` table naming the root and,
/// when `identity` is set, the client identity builds.
fn client_tls(material: &Material, identity: bool) -> Result<rustls::ClientConfig, BoxError> {
    let mut roots = RootCertStore::empty();
    roots.add(material.root.clone())?;
    let builder =
        rustls::ClientConfig::builder_with_provider(Arc::new(aws_lc_rs::default_provider()))
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots);
    Ok(if identity {
        builder.with_client_auth_cert(
            vec![material.client.clone()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(material.client_key.clone())),
        )?
    } else {
        builder.with_no_client_auth()
    })
}

/// Returns a client of the stub at `address` speaking `tls`.
fn client(address: SocketAddr, tls: rustls::ClientConfig) -> Result<CdrClient, BoxError> {
    let base = format!("https://localhost:{}/v1", address.port()).parse()?;
    Ok(CdrClient::new(
        &CdrConfig::new(base).with_tls(Arc::new(tls)),
    )?)
}

#[tokio::test]
async fn a_cdr_asking_for_a_client_certificate_is_reached_with_the_tables_identity()
-> Result<(), BoxError> {
    let material = material()?;
    let (address, served) = stub(&material).await?;
    let status = client(address, client_tls(&material, true)?)?
        .reachability()
        .await?;
    assert_eq!(StatusCode::OK, status);
    served.await??;
    Ok(())
}

#[tokio::test]
async fn without_the_client_certificate_the_cdr_is_not_reached() -> Result<(), BoxError> {
    let material = material()?;
    let (address, served) = stub(&material).await?;
    let outcome = client(address, client_tls(&material, false)?)?
        .reachability()
        .await;
    assert!(outcome.is_err(), "the stub requires a client certificate");
    assert!(
        served.await?.is_err(),
        "the stub accepted a client with no certificate"
    );
    Ok(())
}
