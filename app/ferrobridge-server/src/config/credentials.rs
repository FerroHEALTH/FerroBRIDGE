// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of each upstream, every secret read once from its value
//! or its `_file` sibling.

use secrecy::{ExposeSecret, SecretString};
use std::path::Path;
use std::time::Duration;

use super::error::Error;
use super::resolve::url_of;
use super::section::Cdm;
use super::section::Credentials;
use super::section::OAuthClient;
use super::section::Secret;
use crate::cdr::oauth::Endpoint;
use crate::cdr::oauth::OAuthConfig;

/// Returns the client credentials grant the OAuth section `section` holds.
pub(super) fn resolve_oauth(section: &str, oauth: &OAuthClient) -> Result<OAuthConfig, Error> {
    let endpoint = match (oauth.issuer.as_deref(), oauth.token_endpoint.as_deref()) {
        (Some(_), Some(_)) => {
            return Err(Error::OAuthEndpoint {
                section: section.to_owned(),
            });
        }
        (None, None) => {
            return Err(Error::Missing {
                key: format!("{section}.token_endpoint"),
            });
        }
        (Some(issuer), None) => {
            let key = format!("{section}.issuer");
            let url = url_of(&key, issuer)?;
            if url.scheme() != "https" || url.query().is_some() || url.fragment().is_some() {
                return Err(Error::Issuer { key });
            }
            Endpoint::Issuer(issuer.to_owned())
        }
        (None, Some(token_endpoint)) => Endpoint::Token(url_of(
            &format!("{section}.token_endpoint"),
            token_endpoint,
        )?),
    };
    if oauth.client_id.is_empty() {
        return Err(Error::Missing {
            key: format!("{section}.client_id"),
        });
    }
    let secret_key = format!("{section}.client_secret");
    let client_secret = secret(
        &secret_key,
        oauth.client_secret.as_ref(),
        oauth.client_secret_file.as_deref(),
    )?
    .ok_or(Error::Missing { key: secret_key })?;
    if let Some(scope) = oauth.scope.as_deref()
        && !is_scope(scope)
    {
        return Err(Error::Scope {
            key: format!("{section}.scope"),
        });
    }
    let resource = oauth
        .resource
        .as_deref()
        .map(|resource| {
            let key = format!("{section}.resource");
            let url = url_of(&key, resource)?;
            if url.fragment().is_some() {
                return Err(Error::Resource { key });
            }
            Ok(url)
        })
        .transpose()?;
    let mut config = OAuthConfig::new(endpoint, oauth.client_id.clone(), client_secret);
    config.client_auth = oauth.client_auth;
    config.scope.clone_from(&oauth.scope);
    config.audience.clone_from(&oauth.audience);
    config.resource = resource;
    config.refresh_margin = Duration::from_millis(oauth.refresh_margin_ms);
    Ok(config)
}

/// Returns whether `scope` is the `scope` of RFC 6749 §3.3: scope tokens of
/// `%x21 / %x23-5B / %x5D-7E`, each separated by one space.
pub(super) fn is_scope(scope: &str) -> bool {
    scope.split(' ').all(|token| {
        !token.is_empty()
            && token
                .bytes()
                .all(|byte| matches!(byte, 0x21 | 0x23..=0x5B | 0x5D..=0x7E))
    })
}

/// The RFC 7617 basic authentication a credentials section resolves to.
pub(super) struct Basic {
    /// The user name, which is not a secret.
    pub(super) user: String,
    /// The password.
    pub(super) password: SecretString,
}

/// Returns the basic authentication `credentials` describes, or `None` when
/// it names none.
pub(super) fn resolve_credentials(
    section: &str,
    credentials: &Credentials,
) -> Result<Option<Basic>, Error> {
    let password = secret(
        &format!("{section}.password"),
        credentials.password.as_ref(),
        credentials.password_file.as_deref(),
    )?;
    let user_name = credentials.user.as_deref().map(Secret::from);
    let user = secret(
        &format!("{section}.user"),
        user_name.as_ref(),
        credentials.user_file.as_deref(),
    )?
    .map(|user| user.expose_secret().to_owned());
    match (user, password) {
        (Some(user), Some(password)) => Ok(Some(Basic { user, password })),
        (Some(_), None) => Err(Error::Missing {
            key: format!("{section}.password"),
        }),
        (None, Some(_)) => Err(Error::Missing {
            key: format!("{section}.user"),
        }),
        (None, None) => Ok(None),
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
