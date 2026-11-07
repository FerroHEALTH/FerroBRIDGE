// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! MLLP over TLS with client certificates.
//!
//! This module holds the acceptor that requires a certificate chaining to
//! the configured authorities, and the check that a peer's certificate
//! names the subject a sender is declared under.
//!
//! MLLP Release 1 frames ride on any byte stream, so TLS beneath it changes
//! no frame. The certificate path is validated as RFC 5280 profiles it,
//! through `rustls` and `rustls-webpki`; a sender is tied to the DNS name
//! its certificate's subject alternative name carries (RFC 6125 §6.4, as
//! `rustls-webpki` matches it). Requiring a certificate, and tying each
//! sender to one, is our own design.

use std::sync::Arc;

use rustls::RootCertStore;
use rustls::ServerConfig;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivateKeyDer;
use rustls::pki_types::ServerName;
use rustls::pki_types::pem::PemObject;
use rustls::server::WebPkiClientVerifier;
use tokio_rustls::TlsAcceptor;

/// Why the TLS acceptor could not be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TlsError {
    /// A PEM input holds no item of the kind it is read for, or does not
    /// parse.
    #[error("the {what} PEM could not be read")]
    Pem {
        /// What was read: the certificate chain, the key, or the client
        /// authorities.
        what: &'static str,
        /// What the PEM reader reported.
        #[source]
        source: rustls::pki_types::pem::Error,
    },
    /// The client authorities hold no usable certificate.
    #[error("the client authorities hold no usable certificate")]
    NoAuthority,
    /// A client authority is no usable trust anchor.
    #[error("a client authority is no usable trust anchor")]
    Authority(#[source] rustls::Error),
    /// The client certificate verifier could not be built.
    #[error("the client certificate verifier could not be built")]
    Verifier(#[source] rustls::server::VerifierBuilderError),
    /// The certificate chain and the key do not make a server configuration.
    #[error("the certificate chain and the key do not make a TLS configuration")]
    Config(#[source] rustls::Error),
}

/// Returns the acceptor serving `chain` with `key`, both PEM, and requiring
/// every client to present a certificate that chains to one of
/// `authorities`, PEM.
///
/// # Errors
///
/// Returns [`TlsError::Pem`] for an input that holds nothing or does not
/// parse, [`TlsError::NoAuthority`] and [`TlsError::Authority`] for client
/// authorities that make no trust anchor, [`TlsError::Verifier`] when the
/// client verifier cannot be built, and [`TlsError::Config`] when the chain
/// and the key do not match.
pub fn acceptor(chain: &[u8], key: &[u8], authorities: &[u8]) -> Result<TlsAcceptor, TlsError> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let chain = CertificateDer::pem_slice_iter(chain)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| TlsError::Pem {
            what: "certificate chain",
            source,
        })?;
    let key = PrivateKeyDer::from_pem_slice(key).map_err(|source| TlsError::Pem {
        what: "private key",
        source,
    })?;
    let mut roots = RootCertStore::empty();
    for authority in CertificateDer::pem_slice_iter(authorities) {
        let authority = authority.map_err(|source| TlsError::Pem {
            what: "client authorities",
            source,
        })?;
        roots.add(authority).map_err(TlsError::Authority)?;
    }
    if roots.is_empty() {
        return Err(TlsError::NoAuthority);
    }
    let verifier =
        WebPkiClientVerifier::builder_with_provider(Arc::new(roots), Arc::clone(&provider))
            .build()
            .map_err(TlsError::Verifier)?;
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(TlsError::Config)?
        .with_client_cert_verifier(verifier)
        .with_single_cert(chain, key)
        .map_err(TlsError::Config)?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Returns whether `certificate`, a verified end-entity certificate, is
/// valid for `subject`, a DNS name its subject alternative name carries.
///
/// A certificate that does not parse, or a subject that is no DNS name,
/// names nothing, so the sender is not admitted.
#[must_use]
pub fn names(certificate: &CertificateDer<'_>, subject: &str) -> bool {
    let Ok(name) = ServerName::try_from(subject) else {
        return false;
    };
    webpki::EndEntityCert::try_from(certificate)
        .is_ok_and(|end_entity| end_entity.verify_is_valid_for_subject_name(&name).is_ok())
}
