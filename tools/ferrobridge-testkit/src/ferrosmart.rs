// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FerroSMART, the `FerroHEALTH` SMART App Launch 2.x authorization server, as
//! a container on the runner's host network.
//!
//! On the host network FerroSMART's issuer `http://127.0.0.1:<port>` is the
//! same URL for FerroSMART itself, for a server under test on the host and for
//! the test client, and FerroSMART reaches a resource server, an upstream
//! provider and its database on the loopback interface. Its clients go in
//! first with a one-shot `ferrosmart client apply`, which migrates the
//! database itself. No specification governs the harness: our own design.

use std::time::Duration;

use aws_lc_rs::encoding::AsDer as _;
use base64::Engine as _;
use testcontainers::core::wait::ExitWaitStrategy;
use testcontainers::core::{CopyDataSource, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ContainerRequest, GenericImage, ImageExt};

use crate::containers::{FERROSMART, HarnessError, await_readiness, log_of};

/// The directory every file of a run is copied to in the container.
const ETC: &str = "/etc/ferrosmart";

/// How long the client documents may take to apply.
const APPLY_BUDGET: Duration = Duration::from_secs(120);

/// FerroSMART could not be started.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SmartError {
    /// A container could not be started, or did not become ready.
    #[error("FerroSMART could not be started; its log:\n{log}")]
    Harness {
        /// What the harness reported.
        #[source]
        source: Box<HarnessError>,
        /// What the container wrote, when it could be read.
        log: String,
    },
    /// Docker refused a container.
    #[error("the FerroSMART container could not be started")]
    Container(#[source] testcontainers::TestcontainersError),
    /// `ferrosmart client apply` did not succeed.
    #[error("ferrosmart client apply exited with {code:?}; its log:\n{log}")]
    Apply {
        /// The exit code.
        code: Option<i64>,
        /// What the container wrote.
        log: String,
    },
    /// The signing key could not be generated.
    #[error("the signing key could not be generated")]
    Key,
    /// A client document could not be written.
    #[error("a client document could not be written")]
    Json(#[source] serde_json::Error),
}

/// What one FerroSMART run is configured with.
#[derive(Debug, Clone)]
pub struct Launch {
    /// The loopback port it listens on and its issuer names.
    pub port: u16,
    /// The database URL, as FerroSMART on the host network reaches it.
    pub database_url: String,
    /// The configuration file, which may name the files of `files` under
    /// `/etc/ferrosmart/`.
    pub config: String,
    /// The files copied beside the configuration, each a file name and its
    /// content.
    pub files: Vec<(String, Vec<u8>)>,
    /// The client documents `ferrosmart client apply` writes.
    pub clients: Vec<serde_json::Value>,
}

/// A started FerroSMART, torn down when it is dropped.
#[derive(Debug)]
pub struct FerroSmart {
    /// The container, owned so that its lifetime is this value's.
    server: ContainerAsync<GenericImage>,
    /// The issuer identifier.
    issuer: String,
}

impl FerroSmart {
    /// Returns the issuer identifier, `http://127.0.0.1:<port>`.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Returns the container.
    #[must_use]
    pub fn container(&self) -> &ContainerAsync<GenericImage> {
        &self.server
    }

    /// Returns what the container wrote on stdout and stderr so far.
    pub async fn log(&self) -> String {
        log_of(&self.server).await
    }
}

/// Returns the issuer identifier of FerroSMART on `port`.
#[must_use]
pub fn issuer_on(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Returns a fresh RSA key as PKCS#8 PEM, the form `[ferrosmart.keys]
/// private_key_files` reads.
///
/// # Errors
///
/// Returns [`SmartError::Key`] when no key can be generated.
pub fn signing_key_pem() -> Result<String, SmartError> {
    let pair = aws_lc_rs::rsa::KeyPair::generate(aws_lc_rs::rsa::KeySize::Rsa3072)
        .map_err(|_unspecified| SmartError::Key)?;
    let der = pair.as_der().map_err(|_unspecified| SmartError::Key)?;
    Ok(pkcs8_pem(der.as_ref()))
}

/// Returns a fresh P-384 key as PKCS#8 PEM, the form
/// `[ferrosmart.backend.keys] private_key_file` reads under `ES384`.
///
/// # Errors
///
/// Returns [`SmartError::Key`] when no key can be generated.
pub fn backend_key_pem() -> Result<String, SmartError> {
    let document = aws_lc_rs::signature::EcdsaKeyPair::generate_pkcs8(
        &aws_lc_rs::signature::ECDSA_P384_SHA384_ASN1_SIGNING,
        &aws_lc_rs::rand::SystemRandom::new(),
    )
    .map_err(|_unspecified| SmartError::Key)?;
    Ok(pkcs8_pem(document.as_ref()))
}

/// Returns the PKCS#8 DER `der` as PEM (RFC 7468 §10).
#[must_use]
pub fn pkcs8_pem(der: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = String::from("-----BEGIN PRIVATE KEY-----\n");
    for line in encoded.as_bytes().chunks(64) {
        pem.push_str(&String::from_utf8_lossy(line));
        pem.push('\n');
    }
    pem.push_str("-----END PRIVATE KEY-----\n");
    pem
}

/// Applies the clients of `launch`, then starts FerroSMART and waits for its
/// readiness endpoint.
///
/// # Errors
///
/// Returns [`SmartError`] when a container cannot start, `client apply`
/// fails, or FerroSMART does not become ready; each carries the container's
/// log.
pub async fn start(launch: &Launch) -> Result<FerroSmart, SmartError> {
    let mut names = Vec::new();
    let mut apply = request(
        FERROSMART
            .image()
            .with_wait_for(WaitFor::exit(ExitWaitStrategy::new())),
        launch,
    );
    for (index, client) in launch.clients.iter().enumerate() {
        let name = format!("{ETC}/client-{index}.json");
        let body = serde_json::to_vec(client).map_err(SmartError::Json)?;
        apply = apply.with_copy_to(name.clone(), CopyDataSource::Data(body));
        names.push(name);
    }
    let mut command = vec![String::from("client"), String::from("apply")];
    command.extend(names);
    let applied = apply
        .with_cmd(command)
        .with_startup_timeout(APPLY_BUDGET)
        .start()
        .await
        .map_err(SmartError::Container)?;
    let code = applied.exit_code().await.map_err(SmartError::Container)?;
    if code != Some(0) {
        return Err(SmartError::Apply {
            code,
            log: log_of(&applied).await,
        });
    }
    drop(applied);
    let server = request(FERROSMART.image(), launch)
        .start()
        .await
        .map_err(SmartError::Container)?;
    let issuer = issuer_on(launch.port);
    if let Err(source) = await_readiness(&format!("{issuer}/health/readiness")).await {
        return Err(SmartError::Harness {
            source: Box::new(source),
            log: log_of(&server).await,
        });
    }
    Ok(FerroSmart { server, issuer })
}

/// Returns the request of `image`, FerroSMART's, on the host network with
/// the configuration and the files of `launch` copied in.
fn request(image: GenericImage, launch: &Launch) -> ContainerRequest<GenericImage> {
    let config = format!("{ETC}/ferrosmart.toml");
    let mut request = image
        .with_network("host")
        .with_env_var("FERROSMART_CONFIG", config.clone())
        .with_env_var(
            "FERROSMART__SERVER__LISTEN",
            format!("127.0.0.1:{}", launch.port),
        )
        .with_env_var(
            "FERROSMART__FERROSMART__DATABASE__URL",
            launch.database_url.clone(),
        )
        .with_env_var("FERROSMART__TELEMETRY__FORMAT", "json")
        .with_env_var("FERROSMART__TELEMETRY__FILTER", "info")
        .with_copy_to(
            config,
            CopyDataSource::Data(launch.config.clone().into_bytes()),
        );
    for (name, content) in &launch.files {
        request = request.with_copy_to(
            format!("{ETC}/{name}"),
            CopyDataSource::Data(content.clone()),
        );
    }
    request
}
