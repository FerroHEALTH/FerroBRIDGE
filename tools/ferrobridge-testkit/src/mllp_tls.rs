// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Throwaway TLS material for the MLLP face's tests.
//!
//! It holds a CA, a server certificate it signs for `localhost` and
//! `127.0.0.1`, client certificates it signs for a sender's DNS name, and a
//! client connector that presents one or none.
//!
//! Every key is generated in memory when the case runs through `rcgen`, so
//! no key is ever committed. No specification governs the harness; it is
//! FerroBRIDGE's own design.

use std::sync::Arc;

use rcgen::BasicConstraints;
use rcgen::CertificateParams;
use rcgen::CertifiedIssuer;
use rcgen::ExtendedKeyUsagePurpose;
use rcgen::IsCa;
use rcgen::KeyPair;
use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivateKeyDer;
use rustls::pki_types::ServerName;
use rustls::pki_types::pem::PemObject;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// The material could not be generated or used.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PkiError {
    /// A certificate or key could not be generated.
    #[error("a certificate could not be generated")]
    Generate(#[from] rcgen::Error),
    /// A generated PEM could not be read back.
    #[error("a generated PEM could not be read back")]
    Pem(#[from] rustls::pki_types::pem::Error),
    /// The client configuration could not be built.
    #[error("the client TLS configuration could not be built")]
    Tls(#[from] rustls::Error),
    /// The connection or the handshake failed.
    #[error("the TLS connection failed")]
    Connect(#[from] std::io::Error),
}

/// A CA, the server certificate it signed, and the client certificates it
/// signs on request.
pub struct Pki {
    /// The CA, which signs every certificate here.
    ca: CertifiedIssuer<'static, KeyPair>,
    /// The server certificate's PEM.
    server_certificate: String,
    /// The server key's PEM.
    server_key: String,
}

impl std::fmt::Debug for Pki {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pki").finish_non_exhaustive()
    }
}

/// One client certificate and its key, PEM.
#[derive(Clone)]
pub struct ClientIdentity {
    /// The certificate's PEM.
    certificate: String,
    /// The key's PEM.
    key: String,
}

impl std::fmt::Debug for ClientIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientIdentity").finish_non_exhaustive()
    }
}

impl Pki {
    /// Generates a CA and a server certificate for `localhost` and
    /// `127.0.0.1`.
    ///
    /// # Errors
    ///
    /// Returns [`PkiError::Generate`] when a certificate cannot be made.
    pub fn generate() -> Result<Self, PkiError> {
        let mut params = CertificateParams::new(Vec::<String>::new())?;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca = CertifiedIssuer::self_signed(params, KeyPair::generate()?)?;
        let key = KeyPair::generate()?;
        let mut server =
            CertificateParams::new(vec![String::from("localhost"), String::from("127.0.0.1")])?;
        server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let certificate = server.signed_by(&key, &ca)?;
        Ok(Self {
            server_certificate: certificate.pem(),
            server_key: key.serialize_pem(),
            ca,
        })
    }

    /// Returns the CA's certificate, PEM.
    #[must_use]
    pub fn ca(&self) -> String {
        self.ca.pem()
    }

    /// Returns the server certificate, PEM.
    #[must_use]
    pub fn server_certificate(&self) -> &str {
        &self.server_certificate
    }

    /// Returns the server key, PEM.
    #[must_use]
    pub fn server_key(&self) -> &str {
        &self.server_key
    }

    /// Returns a client certificate the CA signs for the DNS name `subject`.
    ///
    /// # Errors
    ///
    /// Returns [`PkiError::Generate`] when the certificate cannot be made.
    pub fn client(&self, subject: &str) -> Result<ClientIdentity, PkiError> {
        let key = KeyPair::generate()?;
        let mut params = CertificateParams::new(vec![subject.to_owned()])?;
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let certificate = params.signed_by(&key, &self.ca)?;
        Ok(ClientIdentity {
            certificate: certificate.pem(),
            key: key.serialize_pem(),
        })
    }

    /// Returns a connector that trusts the CA and presents `identity`, or no
    /// certificate when none is given.
    ///
    /// # Errors
    ///
    /// Returns [`PkiError::Pem`] or [`PkiError::Tls`] when the configuration
    /// cannot be built.
    pub fn connector(&self, identity: Option<&ClientIdentity>) -> Result<TlsConnector, PkiError> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let mut roots = RootCertStore::empty();
        roots.add(CertificateDer::from_pem_slice(self.ca().as_bytes())?)?;
        let builder = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots);
        let config = match identity {
            Some(identity) => builder.with_client_auth_cert(
                vec![CertificateDer::from_pem_slice(
                    identity.certificate.as_bytes(),
                )?],
                PrivateKeyDer::from_pem_slice(identity.key.as_bytes())?,
            )?,
            None => builder.with_no_client_auth(),
        };
        Ok(TlsConnector::from(Arc::new(config)))
    }
}

/// Connects to `address` through `connector`, naming the server
/// `localhost`.
///
/// # Errors
///
/// Returns [`PkiError::Connect`] when the connection or the handshake fails.
pub async fn connect(
    address: std::net::SocketAddr,
    connector: &TlsConnector,
) -> Result<TlsStream<TcpStream>, PkiError> {
    let stream = TcpStream::connect(address).await?;
    let name = ServerName::try_from("localhost")
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
    Ok(connector.connect(name, stream).await?)
}
