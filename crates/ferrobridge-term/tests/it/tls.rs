// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A terminology server that asks for a client certificate and whose own
//! certificate chains to a private root, reached through a client built from
//! its endpoint table with the TLS a builder makes (`Client::from_endpoint`).
//!
//! Every certificate is synthetic, generated at run time and never
//! committed. No specification governs the stub: our own design.

use ferrobridge_term::client::Client;
use ferrobridge_term::config::WireVersion;
use ferrobridge_term::outcome::LookupOutcome;
use ferrobridge_testkit::fixtures;
use ferrohealth_smart::client::PresentedTls;
use ferrohealth_smart::config::TlsBuilder;
use ferrohealth_smart::config::endpoint::EndpointSection;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
};
use rustls::RootCertStore;
use rustls::crypto::aws_lc_rs;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

type BoxError = Box<dyn Error + Send + Sync>;

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

/// The TLS a test builds from the run's material: the private root as the
/// only trust anchor, and the client identity when `identity` is set.
struct Pem<'m> {
    material: &'m Material,
    identity: bool,
}

impl TlsBuilder for Pem<'_> {
    fn build(
        &self,
        _key: &str,
        _section: &ferrohealth_config::tls::TlsClientSection,
    ) -> Result<PresentedTls, BoxError> {
        let provider = Arc::new(aws_lc_rs::default_provider());
        let mut roots = RootCertStore::empty();
        for certificate in
            rustls::pki_types::pem::PemObject::pem_slice_iter(self.material.root.as_bytes())
        {
            roots.add(certificate?)?;
        }
        let builder = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots);
        if !self.identity {
            return Ok(PresentedTls::new(
                Arc::new(builder.with_no_client_auth()),
                None,
            ));
        }
        let identity = self.material.client_identity.as_bytes();
        let chain = rustls::pki_types::pem::PemObject::pem_slice_iter(identity)
            .collect::<Result<Vec<CertificateDer<'static>>, _>>()?;
        let key = <PrivateKeyDer<'static> as rustls::pki_types::pem::PemObject>::from_pem_slice(
            identity,
        )?;
        let leaf = chain.first().cloned();
        let config = builder.with_client_auth_cert(chain, key)?;
        Ok(PresentedTls::new(Arc::new(config), leaf))
    }
}

/// Returns a client for the stub at `address` that builds its TLS through
/// `tls`, sent once with a short budget.
fn client(address: SocketAddr, tls: &Pem<'_>) -> Result<Client, BoxError> {
    let table: EndpointSection = serde_json::from_value(serde_json::json!({
        "url": format!("https://localhost:{}/r4", address.port()),
        "timeout": "5s",
        "retry": {"max_attempts": 1, "initial_backoff": "1ms", "max_backoff": "1ms"},
    }))?;
    Ok(Client::from_endpoint(
        &table,
        "terminology.endpoint",
        tls,
        WireVersion::R4,
    )?)
}

#[tokio::test]
async fn a_server_asking_for_a_client_certificate_is_reached_with_the_callers_identity()
-> Result<(), BoxError> {
    let material = material()?;
    let (address, served) = stub(&material).await?;
    let client = client(
        address,
        &Pem {
            material: &material,
            identity: true,
        },
    )?;
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
    let client = client(
        address,
        &Pem {
            material: &material,
            identity: false,
        },
    )?;
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
