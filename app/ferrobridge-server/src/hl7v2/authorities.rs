// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The TLS listener's client authorities, read again when their file
//! changes.
//!
//! The listener requires a client certificate that chains to one of the
//! authorities `[hl7v2.tls] client_ca_file` holds. A rotated file is in force
//! for the next handshake without a restart: each handshake compares the
//! file's modification time with the one it was last read at and reads it
//! again when it moved. A file that no longer reads keeps the authorities
//! read last, and the failure is logged. The verifier sends no list of
//! acceptable authorities (RFC 8446 §4.2.4 makes the list optional), since
//! the list would have to outlive a reload. No specification governs the
//! reload: our own design.

use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::SystemTime;

use rustls::client::danger::HandshakeSignatureValid;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, UnixTime};
use rustls::server::WebPkiClientVerifier;
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    DigitallySignedStruct, DistinguishedName, RootCertStore, ServerConfig, SignatureScheme,
};

/// Why the TLS material could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MaterialError {
    /// A file could not be read.
    #[error("{what} could not be read")]
    File {
        /// The file's role.
        what: &'static str,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A file holds no PEM of its kind.
    #[error("{what} is no PEM it should be")]
    Pem {
        /// The file's role.
        what: &'static str,
        /// What the parser reported.
        #[source]
        source: rustls::pki_types::pem::Error,
    },
    /// The authorities file holds no authority.
    #[error("the client authorities file holds no authority")]
    NoAuthority,
    /// rustls refused the material.
    #[error("the TLS material is refused")]
    Tls(#[from] rustls::Error),
    /// rustls refused to build the verifier.
    #[error("the client verifier could not be built")]
    Verifier(#[from] rustls::server::VerifierBuilderError),
}

/// The authorities of one file, with the modification time they were read
/// at.
#[derive(Debug)]
struct Read {
    /// The file's modification time when it was read.
    modified: Option<SystemTime>,
    /// The verifier over the authorities it held.
    verifier: Arc<dyn ClientCertVerifier>,
}

/// The client authorities of one file, read again when it changes.
#[derive(Debug)]
pub struct Authorities {
    /// The file.
    path: PathBuf,
    /// The provider every verifier is built with.
    provider: Arc<CryptoProvider>,
    /// What the file held when it was last read.
    read: RwLock<Read>,
}

/// Returns the modification time of `path`, `None` when it cannot be read.
fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

/// Returns the verifier over the authorities `path` holds.
fn verifier(
    path: &Path,
    provider: &Arc<CryptoProvider>,
) -> Result<Arc<dyn ClientCertVerifier>, MaterialError> {
    let pem = std::fs::read(path).map_err(|source| MaterialError::File {
        what: "the client authorities file",
        source,
    })?;
    let mut roots = RootCertStore::empty();
    for authority in CertificateDer::pem_slice_iter(&pem) {
        let authority = authority.map_err(|source| MaterialError::Pem {
            what: "the client authorities file",
            source,
        })?;
        roots.add(authority)?;
    }
    if roots.is_empty() {
        return Err(MaterialError::NoAuthority);
    }
    Ok(
        WebPkiClientVerifier::builder_with_provider(Arc::new(roots), Arc::clone(provider))
            .build()?,
    )
}

impl Authorities {
    /// Reads the authorities `path` holds.
    ///
    /// # Errors
    /// Returns [`MaterialError`] when the file cannot be read or holds no
    /// authority.
    pub fn open(path: &Path, provider: Arc<CryptoProvider>) -> Result<Self, MaterialError> {
        let read = Read {
            modified: modified(path),
            verifier: verifier(path, &provider)?,
        };
        Ok(Self {
            path: path.to_path_buf(),
            provider,
            read: RwLock::new(read),
        })
    }

    /// Returns the verifier over the file's authorities, reading the file
    /// again when its modification time moved.
    fn current(&self) -> Arc<dyn ClientCertVerifier> {
        let now = modified(&self.path);
        {
            let read = self.read.read().unwrap_or_else(PoisonError::into_inner);
            if read.modified == now {
                return Arc::clone(&read.verifier);
            }
        }
        let mut read = self.read.write().unwrap_or_else(PoisonError::into_inner);
        match verifier(&self.path, &self.provider) {
            Ok(fresh) => {
                tracing::info!(
                    lane = "hl7v2",
                    "the client authorities file changed and is in force"
                );
                *read = Read {
                    modified: now,
                    verifier: fresh,
                };
            }
            Err(error) => {
                tracing::warn!(lane = "hl7v2", error = %error, "the changed client authorities file does not read; the authorities read last stay in force");
                read.modified = now;
            }
        }
        Arc::clone(&read.verifier)
    }
}

impl ClientCertVerifier for Authorities {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        self.current()
            .verify_client_cert(end_entity, intermediates, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.current().verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.current().verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.current().supported_verify_schemes()
    }
}

/// Returns the acceptor serving the PEM `chain` and `key`, requiring a
/// client certificate that chains to the authorities `authorities` holds,
/// read again when that file changes.
///
/// # Errors
/// Returns [`MaterialError`] when the material does not read or rustls
/// refuses it.
pub fn acceptor(
    chain: &[u8],
    key: &[u8],
    authorities: &Path,
) -> Result<tokio_rustls::TlsAcceptor, MaterialError> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let chain = CertificateDer::pem_slice_iter(chain)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| MaterialError::Pem {
            what: "the certificate chain",
            source,
        })?;
    let key = PrivateKeyDer::from_pem_slice(key).map_err(|source| MaterialError::Pem {
        what: "the private key",
        source,
    })?;
    let verifier = Arc::new(Authorities::open(authorities, Arc::clone(&provider))?);
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_client_cert_verifier(verifier)
        .with_single_cert(chain, key)?;
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
}
