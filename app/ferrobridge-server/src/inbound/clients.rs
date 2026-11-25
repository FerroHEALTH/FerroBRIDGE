// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HTTP clients of the inbound recovery calls: one `ferrohealth-smart`
//! `OutboundClient` per source and credential, presenting the client
//! certificate the credential's secret holds, built the first time a source
//! version calls and built again when the version or the secret changes.
//!
//! A call is given 30 seconds, is tried once, follows no redirect, and
//! speaks TLS 1.2 or 1.3. No specification governs these limits: our own
//! design.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::PoisonError;

use ferrohealth_config::tls::{TlsClientSection, TlsVersion};
use ferrohealth_config::units::Duration;
use ferrohealth_secret::secret::Secret;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;
use ferrohealth_smart::config::endpoint::{EndpointSection, RetrySection};
use secrecy::{ExposeSecret as _, SecretString};
use uuid::Uuid;

use super::recover::RecoverError;
use crate::sources::document::SecretRef;
use crate::sources::{Source, Sources};

/// The dotted key a refused recovery client is named by.
const KEY: &str = "inbound.recovery";

/// The clients built so far, by source id and credential secret.
#[derive(Debug, Default)]
pub(super) struct Clients {
    /// Each client with what it was built from.
    held: Mutex<BTreeMap<(Uuid, Option<String>), Held>>,
}

/// One client and what it was built from.
#[derive(Debug)]
struct Held {
    /// The source version.
    version: i64,
    /// The PEM the client presents.
    pem: Option<SecretString>,
    /// The client.
    client: OutboundClient,
}

impl Clients {
    /// Returns the client of `source` that presents the PEM certificate and
    /// key `credentials` names, or none, for calls to `url`.
    ///
    /// # Errors
    /// Returns [`RecoverError`] when the secret cannot be read, holds
    /// nothing, or does not read as a client certificate.
    pub(super) fn client(
        &self,
        sources: &Sources,
        source: &Source,
        credentials: Option<&SecretRef>,
        url: &url::Url,
    ) -> Result<OutboundClient, RecoverError> {
        let pem = match credentials {
            Some(credentials) => Some(
                sources
                    .keys(source, std::slice::from_ref(credentials))
                    .map_err(RecoverError::Secret)?
                    .into_iter()
                    .next()
                    .ok_or(RecoverError::NoCredentials)?,
            ),
            None => None,
        };
        let key = (source.id, credentials.map(|secret| secret.secret.clone()));
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = held.get(&key)
            && found.version == source.version
            && same(found.pem.as_ref(), pem.as_ref())
        {
            return Ok(found.client.clone());
        }
        let client = build(pem.as_ref(), url)?;
        held.retain(|(id, _), entry| *id != source.id || entry.version == source.version);
        held.insert(
            key,
            Held {
                version: source.version,
                pem,
                client: client.clone(),
            },
        );
        Ok(client)
    }
}

/// Whether two PEMs are both absent or hold the same text.
fn same(one: Option<&SecretString>, other: Option<&SecretString>) -> bool {
    match (one, other) {
        (None, None) => true,
        (Some(one), Some(other)) => one.expose_secret() == other.expose_secret(),
        _ => false,
    }
}

/// Builds the client presenting `pem`, its endpoint named by `url`.
fn build(pem: Option<&SecretString>, url: &url::Url) -> Result<OutboundClient, RecoverError> {
    let endpoint = EndpointSection {
        url: SecretUrl::new(url.as_str()),
        timeout: Duration::from_secs(30),
        retry: RetrySection {
            max_attempts: 1,
            ..RetrySection::default()
        },
        tls: TlsClientSection {
            client_identity: pem.cloned().map(Secret::from),
            min_version: TlsVersion::Tls12,
            ..TlsClientSection::default()
        },
        ..EndpointSection::default()
    };
    OutboundClient::build(&endpoint, KEY, &ServerTls).map_err(RecoverError::Identity)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ferrobridge_operator_api::sources::Kind;
    use secrecy::SecretString;
    use uuid::Uuid;

    use super::{Clients, build};
    use crate::inbound::recover::RecoverError;
    use crate::sources::document::{Document, Mllp, SecretRef};
    use crate::sources::{Source, Sources};

    /// The sources over a database no server answers, with no secrets.
    fn sources() -> Sources {
        Sources::new(
            sqlx::postgres::PgPoolOptions::new()
                .acquire_timeout(Duration::from_millis(300))
                .connect_lazy("postgres://bridge@127.0.0.1:1/state?sslmode=disable")
                .expect("a lazy pool"),
            None,
        )
    }

    /// The source `id` at `version`.
    fn source(id: Uuid, version: i64) -> Source {
        Source {
            id,
            name: String::from("lab"),
            kind: Kind::Mllp,
            enabled: true,
            version,
            document: serde_json::json!({}),
            parsed: Document::Mllp(Mllp::default()),
            updated_by: String::new(),
            updated_at: String::new(),
        }
    }

    fn url() -> url::Url {
        url::Url::parse("https://source.invalid/redeliver").expect("a legal URL")
    }

    #[tokio::test]
    async fn a_source_version_builds_one_client_and_a_new_version_replaces_it() {
        let (clients, sources, id) = (Clients::default(), sources(), Uuid::new_v4());
        for version in [1, 1, 2] {
            clients
                .client(&sources, &source(id, version), None, &url())
                .expect("the client builds");
            let held = clients.held.lock().expect("an unpoisoned lock");
            assert_eq!(held.len(), 1);
            assert_eq!(
                held.get(&(id, None)).map(|entry| entry.version),
                Some(version)
            );
        }
    }

    #[tokio::test]
    async fn a_credential_no_secret_directory_answers_is_refused() {
        let refused = Clients::default().client(
            &sources(),
            &source(Uuid::new_v4(), 1),
            Some(&SecretRef {
                secret: String::from("lab-client"),
            }),
            &url(),
        );
        assert!(matches!(refused, Err(RecoverError::Secret(_))));
    }

    #[test]
    fn a_secret_that_holds_no_client_certificate_is_refused() {
        let refused = build(Some(&SecretString::from("no certificate")), &url());
        assert!(matches!(refused, Err(RecoverError::Identity(_))));
    }
}
