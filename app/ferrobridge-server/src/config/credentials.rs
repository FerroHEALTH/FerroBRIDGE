// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of each upstream, every secret read once from its value
//! or its `_file` sibling.

use std::path::Path;

use ferrohealth_smart::client::ClientCredentials;
use ferrohealth_smart::client::Endpoint;
use ferrohealth_smart::config::CredentialsSection;
use ferrohealth_smart::config::Outbound;
use secrecy::{ExposeSecret, SecretString};

use super::error::Error;
use super::section::Cdm;
use super::section::Secret;

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
/// `token_endpoint`, a client id, the key of a `private_key_jwt` client, and
/// every scope token. The bridge adds the issuer URL rule of RFC 8414 §2 and
/// the fragment rule of RFC 8707 §2, so both refuse the start rather than the
/// first token request.
pub(super) fn resolve_credentials(
    section: &str,
    credentials: &CredentialsSection,
) -> Result<Option<Upstream>, Error> {
    let outbound = credentials.resolve().map_err(|source| Error::Credentials {
        section: section.to_owned(),
        source,
    })?;
    match outbound {
        None => Ok(None),
        Some(Outbound::Basic(credential)) => Ok(Some(Upstream::Basic {
            user: credential.username().to_owned(),
            password: basic_password(section, credentials)?,
        })),
        Some(Outbound::Oauth(grant)) => {
            if let Endpoint::Issuer(issuer) = &grant.endpoint
                && ferrohealth_smart::metadata::issuer_url(issuer).is_err()
            {
                return Err(Error::Issuer {
                    key: format!("{section}.oauth.issuer"),
                });
            }
            if grant
                .resource
                .as_ref()
                .is_some_and(|resource| resource.fragment().is_some())
            {
                return Err(Error::Resource {
                    key: format!("{section}.oauth.resource"),
                });
            }
            Ok(Some(Upstream::OAuth(grant)))
        }
    }
}

/// Returns the Basic password of the table `section`, which
/// `ferrohealth-smart` has already found set exactly once.
fn basic_password(section: &str, credentials: &CredentialsSection) -> Result<SecretString, Error> {
    // NOTE: no specification governs this: our own design; the crate's Basic
    // credential keeps its password private and both clients take user and
    // password, so the password is read from the table the crate checked.
    match (&credentials.password, &credentials.password_file) {
        (Some(password), _) => Ok(SecretString::from(password.expose_secret().to_owned())),
        (None, Some(path)) => {
            let text = SecretString::from(std::fs::read_to_string(path).map_err(|source| {
                Error::Secret {
                    key: format!("{section}.password_file"),
                    path: path.clone(),
                    source,
                }
            })?);
            Ok(SecretString::from(
                text.expose_secret().trim_end_matches(['\n', '\r']),
            ))
        }
        (None, None) => Err(Error::Missing {
            key: format!("{section}.password"),
        }),
    }
}

/// Returns the key whose value `error` refuses: the CA's for a CA fault, the
/// URL's for every other.
pub(super) fn cdm_connection_key(
    error: &omop_cdm::connection::ConnectionError,
    cdm: &Cdm,
) -> &'static str {
    use omop_cdm::connection::ConnectionError;
    match error {
        ConnectionError::NoCertificate
        | ConnectionError::Pem { .. }
        | ConnectionError::Root { .. }
        | ConnectionError::CaWithoutTls => {
            if cdm.tls_ca_file.is_some() {
                "cdm.tls_ca_file"
            } else {
                "cdm.tls_ca"
            }
        }
        _ => {
            if cdm.url_file.is_some() {
                "cdm.url_file"
            } else {
                "cdm.url"
            }
        }
    }
}

/// Returns the secret `key` names, inline or from its `_file` sibling.
///
/// The file's text is a [`SecretString`] from the read, and the trimmed copy
/// is taken from it, so neither form outlives the call unzeroed.
pub(super) fn secret(
    key: &str,
    inline: Option<&Secret>,
    file: Option<&Path>,
) -> Result<Option<SecretString>, Error> {
    match (inline, file) {
        (Some(_), Some(_)) => Err(Error::Conflict {
            key: key.to_owned(),
        }),
        (Some(value), None) => Ok(Some(value.secret().clone())),
        (None, Some(path)) => {
            let text = SecretString::from(std::fs::read_to_string(path).map_err(|source| {
                Error::Secret {
                    key: format!("{key}_file"),
                    path: path.to_path_buf(),
                    source,
                }
            })?);
            Ok(Some(SecretString::from(text.expose_secret().trim())))
        }
        (None, None) => Ok(None),
    }
}
