// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of each upstream, every secret read once from its value
//! or its `_file` sibling.

use std::path::Path;

use ferrohealth_secret::file::read_secret;
use ferrohealth_secret::secret::Secret;
use ferrohealth_smart::client::ClientCredentials;
use ferrohealth_smart::config::credentials::CredentialsSection;
use ferrohealth_smart::config::credentials::Outbound;
use secrecy::SecretString;

use super::error::Error;

/// What a `[<lane>.credentials]` table resolves to.
pub(super) enum Upstream {
    /// RFC 7617 basic authentication.
    Basic {
        /// The user-id, which is not a secret.
        user: String,
        /// The password.
        password: SecretString,
    },
    /// The OAuth 2.0 client credentials grant (RFC 6749 §4.4).
    OAuth(Box<ClientCredentials>),
}

/// Returns the credential the table `section` holds, or `None` when it names
/// none.
///
/// `ferrohealth-smart` checks the table: Basic or the grant and never both,
/// exactly one of each secret and its `_file`, exactly one of `issuer` and
/// `token_endpoint`, the issuer URL rule of RFC 8414 §2, a client id, the
/// key of a `private_key_jwt` client, every scope token, and a resource
/// indicator without a fragment (RFC 8707 §2).
///
/// An `onward` table is refused: only the CDR lane reaches its upstream on a
/// caller's behalf, and it resolves that table itself.
pub(super) fn resolve_credentials(
    section: &str,
    credentials: &CredentialsSection,
) -> Result<Option<Upstream>, Error> {
    if credentials.onward.is_some() {
        return Err(Error::OnwardLane {
            section: section.to_owned(),
        });
    }
    let outbound = credentials
        .resolve_with_tls(&ferrohealth_smart::config::ServerTls)
        .map_err(|source| Error::Credentials {
            section: section.to_owned(),
            source: Box::new(source),
        })?;
    outbound
        .map(|outbound| match outbound {
            Outbound::Basic(credential) => Ok(Upstream::Basic {
                user: credential.username().to_owned(),
                password: credential.password().clone(),
            }),
            Outbound::Oauth(grant) => Ok(Upstream::OAuth(grant)),
            // NOTE: no specification governs this: our own design; a form a later
            // ferrohealth-smart adds refuses the start rather than reach the upstream bare.
            _ => Err(Error::CredentialForm {
                section: section.to_owned(),
            }),
        })
        .transpose()
}

/// The keys of one PostgreSQL connection section, `[cdm]` or `[state]`.
pub(super) struct ConnectionKeys<'a> {
    /// The section's name.
    pub section: &'static str,
    /// Whether the URL comes from `url_file`.
    pub url_file: bool,
    /// The PEM CA, inline.
    pub tls_ca: Option<&'a str>,
    /// The file holding the PEM CA.
    pub tls_ca_file: Option<&'a Path>,
}

impl ConnectionKeys<'_> {
    /// Returns the configured CA, read from `tls_ca` or `tls_ca_file`.
    ///
    /// # Errors
    /// Returns [`Error::Conflict`] when both are set and [`Error::Secret`]
    /// when the file cannot be read.
    pub(super) fn ca(&self) -> Result<Option<Vec<u8>>, Error> {
        match (self.tls_ca, self.tls_ca_file) {
            (Some(_), Some(_)) => Err(Error::Conflict {
                key: format!("{}.tls_ca", self.section),
            }),
            (Some(pem), None) => Ok(Some(pem.as_bytes().to_vec())),
            (None, Some(path)) => std::fs::read(path)
                .map(Some)
                .map_err(|source| Error::Secret {
                    key: format!("{}.tls_ca_file", self.section),
                    path: path.to_path_buf(),
                    source,
                }),
            (None, None) => Ok(None),
        }
    }

    /// Returns the key whose value `error` refuses: the CA's for a CA fault,
    /// the URL's for every other.
    pub(super) fn refused(&self, error: &omop_cdm::connection::ConnectionError) -> String {
        use omop_cdm::connection::ConnectionError;
        let key = match error {
            ConnectionError::NoCertificate
            | ConnectionError::Pem { .. }
            | ConnectionError::Root { .. }
            | ConnectionError::CaWithoutTls => {
                if self.tls_ca_file.is_some() {
                    "tls_ca_file"
                } else {
                    "tls_ca"
                }
            }
            _ => {
                if self.url_file {
                    "url_file"
                } else {
                    "url"
                }
            }
        };
        format!("{}.{key}", self.section)
    }
}

/// Returns the secret `key` names, inline or from the file its `_file`
/// sibling names, through `ferrohealth-secret`.
///
/// # Errors
/// Returns [`Error::SecretFile`] when both are set, or the file cannot be read
/// or holds nothing.
pub(super) fn secret(
    key: &str,
    inline: Option<&Secret>,
    file: Option<&Path>,
) -> Result<Option<SecretString>, Error> {
    read_secret(key, inline.map(Secret::to_secret_string), file)
        .map_err(|source| Error::SecretFile { source })
}
