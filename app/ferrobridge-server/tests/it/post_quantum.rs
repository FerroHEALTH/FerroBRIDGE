// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The TLS 1.3 key exchange of the listener and of the CDR, terminology and
//! outbound FHIR clients the configuration builds: each agrees on the hybrid
//! group X25519MLKEM768 (RFC 10024) with a peer that offers it, and on X25519
//! with a peer that offers only classical groups.
//!
//! Every certificate is synthetic, generated at run time and never
//! committed.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use axum::Router;
use axum::routing::get;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::config::{Config, Process, Settings};
use ferrobridge_server::serve_until;
use ferrobridge_testkit::mllp_tls::{Pki, connect};
use ferrobridge_testkit::post_quantum::{Peer, agreed, https_once};
use ferrohealth_config::tls::TlsVersion;
use ferrohealth_server::section::{ServerSection, TlsSection};
use tokio::net::TcpListener;

use crate::support;

type BoxError = Box<dyn Error + Send + Sync>;

/// The answer every stand-in gives: an empty JSON array, which is the CDR's
/// template list.
const ANSWER: &[u8] =
    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n[]";

/// The configuration a CDR, a change feed and a facade make, which the
/// outbound lane follows.
const FEED: &str = "[auth]\nenabled = false\n\n[state]\nurl = \"postgres://bridge@db.invalid/state?sslmode=disable\"\n\n[change_feed]\nenabled = true\n\n[identity_source]\npseudonym_domain = \"urn:oid:2.999.9\"\nsource_domains = [\"urn:oid:2.999.1\"]\nendpoint.url = \"http://127.0.0.1:9/fhir\"\n\n[facade]\nenabled = true\ncomposition_language = \"en\"\ncomposition_territory = \"GB\"\ncustodian.name = \"Synthetic Hospital\"\ncustodian.identifier_system = \"urn:oid:2.999.1.1\"\ncustodian.identifier_value = \"synthetic-hospital\"\n\n";

/// The synthetic material of one case, with the CA written where an
/// endpoint's `tls.trust_roots_without_revocation_file` names it: the test CA
/// publishes no revocation service.
struct Material {
    pki: Pki,
    directory: tempfile::TempDir,
}

impl Material {
    fn new() -> Result<Self, BoxError> {
        let pki = Pki::generate()?;
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("ca.pem"), pki.ca())?;
        std::fs::write(
            directory.path().join("server.pem"),
            pki.server_certificate(),
        )?;
        std::fs::write(directory.path().join("server.key"), pki.server_key())?;
        Ok(Self { pki, directory })
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.directory.path().join(name)
    }

    /// Returns the `endpoint` keys of a table that reaches `port` on
    /// `localhost` under `path`, trusting the synthetic CA.
    fn endpoint(&self, port: u16, path: &str) -> String {
        format!(
            "endpoint.url = \"https://localhost:{port}{path}\"\nendpoint.tls.trust_roots_without_revocation_file = \"{}\"\n",
            self.path("ca.pem").display()
        )
    }
}

/// Returns the settings `text` resolves to for `serve`.
fn settings(text: &str) -> Result<Settings, BoxError> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.resolve(Process::Server)?)
}

#[tokio::test]
async fn the_listener_agrees_on_the_hybrid_group_and_still_serves_a_classical_peer()
-> Result<(), BoxError> {
    let material = Material::new()?;
    let server = ServerSection {
        tls: Some(tls_section(
            &material.path("server.pem"),
            &material.path("server.key"),
        )),
        ..support::settings()
    };
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(async move {
        serve_until(
            listener,
            Router::new().route("/", get(|| async { "ok" })),
            &server,
            ferrohealth_health::lifecycle::Lifecycle::default(),
            async move {
                if stopped.await.is_err() {
                    tracing::debug!("the stop channel closed");
                }
            },
        )
        .await
    });
    for peer in Peer::BOTH {
        let connector = material.pki.connector(None, peer)?;
        let stream = connect(address, &connector).await?;
        assert_eq!(Some(peer.agreed()), agreed(stream.get_ref().1), "{peer:?}");
    }
    stop.send(()).map_err(|()| "the listener ended early")?;
    serving.await??;
    Ok(())
}

/// Returns the `[server.tls]` table serving `certificate` with `key`.
fn tls_section(certificate: &Path, key: &Path) -> TlsSection {
    TlsSection {
        certificate_file: certificate.to_path_buf(),
        key_file: key.to_path_buf(),
        client_auth: ferrohealth_server::section::ClientAuth::None,
        client_ca_file: None,
        reload_interval: None,
        healthcheck_identity_file: None,
        min_version: TlsVersion::default(),
    }
}

#[tokio::test]
async fn the_cdr_client_agrees_on_the_hybrid_group_and_still_reaches_a_classical_cdr()
-> Result<(), BoxError> {
    let material = Material::new()?;
    for peer in Peer::BOTH {
        let (address, served) = https_once(&material.pki, peer, ANSWER).await?;
        let text = format!("[cdr]\n{}", material.endpoint(address.port(), "/v1"));
        let cdr = settings(&text)?.cdr.ok_or("the CDR is configured")?;
        CdrClient::new(&cdr)?.reachability().await?;
        assert_eq!(peer.agreed(), served.await??, "{peer:?}");
    }
    Ok(())
}

#[tokio::test]
async fn the_terminology_client_agrees_on_the_hybrid_group_and_still_reaches_a_classical_server()
-> Result<(), BoxError> {
    let material = Material::new()?;
    for peer in Peer::BOTH {
        let (address, served) = https_once(&material.pki, peer, ANSWER).await?;
        let text = format!(
            "[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[terminology]\n{}",
            material.endpoint(address.port(), "/fhir")
        );
        let terminology = settings(&text)?
            .terminology
            .ok_or("the terminology server is configured")?;
        terminology.reachability().await?;
        assert_eq!(peer.agreed(), served.await??, "{peer:?}");
    }
    Ok(())
}

#[tokio::test]
async fn the_outbound_fhir_client_agrees_on_the_hybrid_group_and_still_reaches_a_classical_server()
-> Result<(), BoxError> {
    let material = Material::new()?;
    for peer in Peer::BOTH {
        let (address, served) = https_once(&material.pki, peer, ANSWER).await?;
        let text = format!(
            "{FEED}[cdr]\nendpoint.url = \"http://cdr.invalid/v1\"\n\n[fhir_outbound]\nenabled = true\nreader = \"fhir-outbound\"\n{}",
            material.endpoint(address.port(), "/r4")
        );
        let outbound = settings(&text)?
            .fhir_outbound
            .ok_or("the outbound lane is on")?;
        outbound.sink.deliver(b"{}").await?;
        assert_eq!(peer.agreed(), served.await??, "{peer:?}");
    }
    Ok(())
}
