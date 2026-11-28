// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `FerroPIX`, the `FerroHEALTH` patient identity source, as a container on the
//! runner's host network with a PostgreSQL of its own.
//!
//! `FerroPIX` reads a plain `http` issuer and sends its PMIR feed to a plain
//! `http` channel endpoint only on the loopback interface, so it runs on the
//! host network, where a test's OpenID Provider and the bridge under test
//! listen on `127.0.0.1` beside it. It refuses a superuser connection, so
//! its database is owned by a login role of its own. No specification
//! governs the harness: our own design.

use aws_lc_rs::rand::{SecureRandom as _, SystemRandom};
use base64::Engine as _;
use testcontainers::core::{CopyDataSource, IntoContainerPort as _, WaitFor};
use testcontainers::runners::AsyncRunner as _;
use testcontainers::{ContainerAsync, GenericImage, ImageExt as _};

use crate::containers::{
    FERROPIX, HarnessError, POSTGRES, POSTGRES_PORT, await_readiness, log_of, postgres_health_check,
};

/// The directory the configuration and its files are copied to.
const RUN: &str = "/run/ferropix";

/// The login role that owns `FerroPIX`'s database, its password and the
/// database's name.
const ROLE: &str = "ferropix";

/// The script the PostgreSQL image runs once at its first start.
const INIT: &str = "CREATE ROLE ferropix LOGIN PASSWORD 'ferropix';\n\
                    CREATE DATABASE ferropix OWNER ferropix;\n";

/// `FerroPIX` could not be started.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PixError {
    /// Docker refused a container.
    #[error("the {image} container could not be started")]
    Container {
        /// The image that was being started.
        image: &'static str,
        /// What Docker reported.
        #[source]
        source: testcontainers::TestcontainersError,
    },
    /// `FerroPIX` did not become ready.
    #[error("FerroPIX did not become ready; its log:\n{log}")]
    NotReady {
        /// What the readiness probe reported.
        #[source]
        source: Box<HarnessError>,
        /// What the container wrote.
        log: String,
    },
    /// The data key could not be generated.
    #[error("the data key could not be generated")]
    Key,
}

/// A started `FerroPIX` with its database, torn down when it is dropped.
#[derive(Debug)]
pub struct FerroPix {
    /// The server.
    server: ContainerAsync<GenericImage>,
    /// Its database, owned so that its lifetime is this value's.
    _database: ContainerAsync<GenericImage>,
    /// The origin it answers on.
    origin: String,
}

impl FerroPix {
    /// Returns the origin, `http://127.0.0.1:<port>`.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Returns the FHIR base, the origin and `/fhir`.
    #[must_use]
    pub fn fhir_base(&self) -> String {
        format!("{}/fhir", self.origin)
    }

    /// Returns what the container wrote on stdout and stderr so far.
    pub async fn log(&self) -> String {
        log_of(&self.server).await
    }
}

/// Returns the origin of `FerroPIX` on `port`.
#[must_use]
pub fn origin_on(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Starts `FerroPIX` listening on `127.0.0.1:port` with the configuration
/// `config`, over a PostgreSQL started for it, and waits for its readiness
/// endpoint.
///
/// `config` holds every table but `[db]` and `[usage_report]`: the harness
/// appends the database, a fresh data key and the usage report switched
/// off.
///
/// # Errors
///
/// Returns [`PixError::Container`] when Docker refuses a container,
/// [`PixError::Key`] when no data key can be generated, and
/// [`PixError::NotReady`] with `FerroPIX`'s log when it does not answer its
/// readiness probe in time.
pub async fn start(port: u16, config: &str) -> Result<FerroPix, PixError> {
    let database = POSTGRES
        .image()
        .with_wait_for(WaitFor::healthcheck())
        .with_exposed_port(POSTGRES_PORT.tcp())
        .with_health_check(postgres_health_check(ROLE, ROLE))
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_copy_to(
            "/docker-entrypoint-initdb.d/ferropix.sql",
            CopyDataSource::Data(INIT.as_bytes().to_vec()),
        )
        .start()
        .await
        .map_err(|source| PixError::Container {
            image: POSTGRES.repository,
            source,
        })?;
    let database_port = database
        .get_host_port_ipv4(POSTGRES_PORT.tcp())
        .await
        .map_err(|source| PixError::Container {
            image: POSTGRES.repository,
            source,
        })?;
    let database_url =
        format!("postgresql://{ROLE}:{ROLE}@127.0.0.1:{database_port}/{ROLE}?sslmode=disable");
    let full = format!(
        "{config}\n[db]\nurl_file = \"{RUN}/db-url\"\ndata_key_file = \"{RUN}/data-key\"\n\n[usage_report]\nenabled = false\n"
    );
    let server = FERROPIX
        .image()
        .with_network("host")
        .with_env_var("FERROPIX_CONFIG", format!("{RUN}/ferropix.toml"))
        .with_env_var("FERROPIX__SERVER__LISTEN", format!("127.0.0.1:{port}"))
        .with_env_var("FERROPIX__USAGE_REPORT__ENABLED", "false")
        .with_copy_to(
            format!("{RUN}/ferropix.toml"),
            CopyDataSource::Data(full.into_bytes()),
        )
        .with_copy_to(
            format!("{RUN}/db-url"),
            CopyDataSource::Data(database_url.into_bytes()),
        )
        .with_copy_to(
            format!("{RUN}/data-key"),
            CopyDataSource::Data(data_key()?.into_bytes()),
        )
        .start()
        .await
        .map_err(|source| PixError::Container {
            image: FERROPIX.repository,
            source,
        })?;
    let origin = origin_on(port);
    if let Err(source) = await_readiness(&format!("{origin}/health/readiness")).await {
        return Err(PixError::NotReady {
            source: Box::new(source),
            log: log_of(&server).await,
        });
    }
    Ok(FerroPix {
        server,
        _database: database,
        origin,
    })
}

/// Returns a fresh data key: 32 random bytes, base64.
fn data_key() -> Result<String, PixError> {
    let mut key = [0_u8; 32];
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|_unspecified| PixError::Key)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(key))
}
