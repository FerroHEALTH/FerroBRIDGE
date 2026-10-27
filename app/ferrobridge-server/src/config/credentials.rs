// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of each upstream, every secret read once from its value
//! or its `_file` sibling.

use std::path::Path;

use ferrohealth_secret::file::read_secret;
use ferrohealth_secret::secret::Secret;
use ferrohealth_smart::client::ClientCredentials;
use ferrohealth_smart::config::CredentialsSection;
use ferrohealth_smart::config::Outbound;
use secrecy::SecretString;

use super::error::Error;
use super::section::Cdm;

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
pub(super) fn resolve_credentials(
    section: &str,
    credentials: &CredentialsSection,
) -> Result<Option<Upstream>, Error> {
    let outbound = credentials.resolve().map_err(|source| Error::Credentials {
        section: section.to_owned(),
        source: Box::new(source),
    })?;
    Ok(outbound.map(|outbound| match outbound {
        Outbound::Basic(credential) => Upstream::Basic {
            user: credential.username().to_owned(),
            password: credential.password().clone(),
        },
        Outbound::Oauth(grant) => Upstream::OAuth(grant),
    }))
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
