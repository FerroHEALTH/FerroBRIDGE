// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A terminology server that asks for a client certificate and whose own
//! certificate chains to a private root, reached through a caller-built HTTP
//! client (`Client::with_http`).
//!
//! Every certificate is synthetic, generated at run time and never
//! committed. No specification governs the stub: our own design.

use ferrobridge_term::client::Client;
use ferrobridge_term::config::{Config, RetryPolicy, WireVersion};
use ferrobridge_term::outcome::LookupOutcome;
use ferrobridge_testkit::fixtures;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
};
use rustls::RootCertStore;
use rustls::crypto::aws_lc_rs;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use std::error::Error as StdError;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

type BoxError = Box<dyn StdError + Send + Sync>;

/// The synthetic material of one run: the private root, the server's chain
/// and key, and the client identity as one PEM.
struct Material {
    root: String,
    server: CertificateDer<'static>,
    server_key: Vec<u8>,
    client_identity: String,
}

/// Returns the parameters and key of a synthetic certificate authority.
fn authority(name: &str) -> Result<(CertificateParams, KeyPair), BoxError> {
    let key = KeyPair::generate()?;
    let mut params = CertificateParams::new(Vec::<String>::new())?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.distinguished_name.push(DnType::CommonName, name);
    Ok((params, key))
}

/// Generates a private root that signs the server's certificate for
/// `localhost` and the client's certificate.
fn material() -> Result<Material, BoxError> {
    let (root_params, root_key) = authority("Synthetic terminology root")?;
    let root = root_params.self_signed(&root_key)?;
    let issuer = Issuer::new(root_params, root_key);

    let server_key = KeyPair::generate()?;
    let mut server = CertificateParams::new(vec![String::from("localhost")])?;
    server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let server = server.signed_by(&server_key, &issuer)?;

    let client_key = KeyPair::generate()?;
    let mut client = CertificateParams::new(Vec::<String>::new())?;
    client.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    client
        .distinguished_name
        .push(DnType::CommonName, "synthetic bridge");
    let client = client.signed_by(&client_key, &issuer)?;

    Ok(Material {
        root: root.pem(),
        server: server.der().clone(),
        server_key: server_key.serialize_der(),
        client_identity: format!("{}{}", client.pem(), client_key.serialize_pem()),
    })
}

/// The `$lookup` answer the stub sends.
fn lookup_answer() -> String {
    serde_json::json!({
        "resourceType": "Parameters",
        "parameter": [
            {"name": "name", "valueString": "FerroBridgeSyntheticSource"},
            {"name": "display", "valueString": fixtures::TERM_MEMBER_DISPLAY}
        ]
    })
    .to_string()
}

/// Starts a stub that requires a client certificate the root signed and
/// answers one request, returning its address and the task that serves it.
async fn stub(
    material: &Material,
) -> Result<(SocketAddr, JoinHandle<Result<(), BoxError>>), BoxError> {
    let provider = Arc::new(aws_lc_rs::default_provider());
    let mut roots = RootCertStore::empty();
    for certificate in rustls::pki_types::pem::PemObject::pem_slice_iter(material.root.as_bytes()) {
        roots.add(certificate?)?;
    }
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
        let body_start = loop {
            let read = tls.read(&mut chunk).await?;
            if read == 0 {
                return Err::<(), BoxError>("the client closed before its request ended".into());
            }
            request.extend_from_slice(chunk.get(..read).ok_or("a read past the buffer")?);
            if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                break end + 4;
            }
        };
        let head =
            String::from_utf8_lossy(request.get(..body_start).ok_or("no head")?).to_lowercase();
        let length: usize = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .map_or(Ok(0), |value| value.trim().parse())?;
        while request.len() < body_start + length {
            let read = tls.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            request.extend_from_slice(chunk.get(..read).ok_or("a read past the buffer")?);
        }
        let body = lookup_answer();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/fhir+json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        tls.write_all(response.as_bytes()).await?;
        tls.shutdown().await?;
        Ok(())
    });
    Ok((address, served))
}

/// Returns the configuration of a client for the stub at `address`, sent
/// once with a short budget.
fn config(address: SocketAddr) -> Result<Config, BoxError> {
    let base = format!("https://localhost:{}/r4", address.port()).parse()?;
    Ok(Config::new(base, WireVersion::R4)
        .with_timeout(Duration::from_secs(5))
        .with_retry(RetryPolicy {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        }))
}

/// Returns an HTTP client that trusts the private root, resolves
/// `localhost` to the stub, and presents `identity` when one is given.
fn http(
    material: &Material,
    address: SocketAddr,
    identity: bool,
) -> Result<reqwest::Client, BoxError> {
    let mut builder = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_pem(material.root.as_bytes())?)
        .resolve("localhost", address);
    if identity {
        builder = builder.identity(reqwest::Identity::from_pem(
            material.client_identity.as_bytes(),
        )?);
    }
    Ok(builder.build()?)
}

#[tokio::test]
async fn a_server_asking_for_a_client_certificate_is_reached_with_the_callers_identity()
-> Result<(), BoxError> {
    let material = material()?;
    let (address, served) = stub(&material).await?;
    let client = Client::with_http(config(address)?, http(&material, address, true)?)?;
    let outcome = client
        .lookup(
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_MEMBER_CODE,
            None,
        )
        .await?;
    let LookupOutcome::Found { display, .. } = outcome else {
        return Err(format!("expected a display, got {outcome:?}").into());
    };
    assert_eq!(fixtures::TERM_MEMBER_DISPLAY, display);
    served.await??;
    Ok(())
}

#[tokio::test]
async fn without_the_client_certificate_the_server_is_not_reached() -> Result<(), BoxError> {
    let material = material()?;
    let (address, served) = stub(&material).await?;
    let client = Client::with_http(config(address)?, http(&material, address, false)?)?;
    let error = client
        .lookup(
            fixtures::TERM_SOURCE_SYSTEM,
            fixtures::TERM_MEMBER_CODE,
            None,
        )
        .await
        .expect_err("the server requires a client certificate");
    assert_eq!("transport", error.kind(), "{error:?}");
    assert!(
        served.await?.is_err(),
        "the stub accepted a client with no certificate"
    );
    Ok(())
}
