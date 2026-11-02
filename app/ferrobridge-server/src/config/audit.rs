// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution of `[audit]` into the settings of the access-record lane.
//!
//! No specification governs the configuration: our own design.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use ehds_logging::map::CategoryMap;
use ehds_logging::map::Matching;
use ferrohealth_smart::client::cache::TokenSource;
use ihe_atna::atna::bearer::Bearer;
use ihe_atna::atna::feed::Credentials;
use ihe_atna::atna::feed::CredentialsError;
use ihe_atna::atna::feed::FeedRepository;
use ihe_atna::atna::spool::Bounds;
use secrecy::ExposeSecret;
use secrecy::SecretString;

use super::credentials::Upstream;
use super::credentials::resolve_credentials;
use super::error::Error;
use super::resolve::url_of;
use super::section::audit::Audit;
use crate::audit::Settings;

/// Returns the lane `audit` describes, refused without the facade it records
/// for, without `[auth]`, whose callers it names, and without a repository, a
/// spool or a category map it can read.
///
/// `system_id` is the facade's, which the bridge names itself by when
/// `[audit] source_id` is empty.
pub(super) fn resolve_audit(
    audit: &Audit,
    facade: bool,
    auth: bool,
    system_id: &str,
) -> Result<Settings, Error> {
    if !facade {
        return Err(Error::Needs {
            key: String::from("audit.enabled"),
            needs: String::from("facade.enabled"),
        });
    }
    // NOTE: Regulation (EU) 2025/327 Annex II 3.2(b) asks for the natural person who
    // accessed, which an open surface never names, so the lane needs `[auth]`.
    if !auth {
        return Err(Error::Needs {
            key: String::from("audit.enabled"),
            needs: String::from("auth.enabled"),
        });
    }
    let base = url_of("audit.base_url", &audit.base_url)?;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|source| Error::Client {
            upstream: "audit",
            source: Box::new(source),
        })?;
    let timeout = Duration::from_millis(audit.timeout_ms);
    let feed = if audit.cleartext {
        FeedRepository::cleartext(base, http, timeout)
    } else {
        FeedRepository::new(base, http, timeout)
    }
    .map_err(|source| Error::AuditBase {
        key: String::from("audit.base_url"),
        source,
    })?;
    let feed = match resolve_credentials("audit.credentials", &audit.credentials)? {
        Some(Upstream::Basic { user, password }) => {
            feed.with_credentials(Arc::new(Basic::new(&user, &password)))
        }
        Some(Upstream::OAuth(grant)) => {
            let source = TokenSource::new(*grant).map_err(|source| Error::Client {
                upstream: "audit",
                source: Box::new(source),
            })?;
            feed.with_credentials(Arc::new(Bearer::from(source)))
        }
        None => feed,
    };
    let spool = audit.spool_dir.clone().ok_or(Error::Missing {
        key: String::from("audit.spool_dir"),
    })?;
    for (key, value) in [
        ("audit.timeout_ms", audit.timeout_ms),
        ("audit.write_timeout_ms", audit.write_timeout_ms),
        ("audit.max_bytes", audit.max_bytes),
    ] {
        if value == 0 {
            return Err(Error::Bound {
                key: key.to_owned(),
                value: 0,
                reason: "it bounds every record above zero",
            });
        }
    }
    if audit.max_records == 0 {
        return Err(Error::Bound {
            key: String::from("audit.max_records"),
            value: 0,
            reason: "the spool holds at least one record",
        });
    }
    // NOTE: openEHR BASE Release 1.3.0, Identification Package, "Composite Identifiers and
    // Case": template and archetype ids compare case-insensitively.
    let categories = CategoryMap::declare_with(
        Matching::OpenEhr,
        &audit.categories.national,
        &audit.categories.templates,
        &audit.categories.archetypes,
    )
    .map_err(|source| Error::Categories {
        key: String::from("audit.categories"),
        source,
    })?;
    Ok(Settings {
        feed,
        spool,
        bounds: Bounds {
            max_messages: audit.max_records,
            max_bytes: audit.max_bytes,
            write_timeout: Duration::from_millis(audit.write_timeout_ms),
        },
        retry_max: Duration::from_millis(audit.retry_max_ms),
        source_id: if audit.source_id.is_empty() {
            system_id.to_owned()
        } else {
            audit.source_id.clone()
        },
        site: audit.site.clone().filter(|site| !site.is_empty()),
        categories,
    })
}

/// An RFC 7617 Basic credential as the `Authorization` value of every Feed
/// request.
struct Basic(SecretString);

impl Basic {
    /// Returns the credential of `user` with `password`.
    fn new(user: &str, password: &SecretString) -> Self {
        let pair = format!("{user}:{}", password.expose_secret());
        let encoded = base64::engine::general_purpose::STANDARD.encode(pair);
        Self(SecretString::from(format!("Basic {encoded}")))
    }
}

impl std::fmt::Debug for Basic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Basic").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl Credentials for Basic {
    async fn authorization(&self) -> Result<SecretString, CredentialsError> {
        Ok(self.0.clone())
    }

    async fn refused(&self, _refused: &SecretString) {}
}
