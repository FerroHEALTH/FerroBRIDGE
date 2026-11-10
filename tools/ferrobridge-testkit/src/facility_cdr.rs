// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reference CDR's database and server boot, and the reference CDR as a
//! node that checks the facility of every composition a declared relay
//! client commits.
//!
//! [`relay`] boots the CDR twice on one database. The first boot runs with
//! authentication off and uploads the templates, because the CDR refuses to
//! start with an `[[authz.system_client]]` table whose `writes` names a
//! template its store does not hold. The second boot trusts one `machines`
//! issuer, whose key set is copied into the container, and declares the
//! relay client with its provider, facilities and templates, so every commit
//! of that client is checked. No specification governs the harness: our own
//! design.

use crate::containers::{
    CDR, CDR_DATABASE_ROLE, CDR_PORT, CDR_POSTGRES, Cdr, HarnessError, NETWORK_SEQUENCE,
    POSTGRES_PORT, await_readiness, cdr_request, postgres_health_check,
};
use serde::Serialize;
use std::sync::atomic::Ordering;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ContainerRequest, GenericImage, ImageExt};

/// Where the configuration file is copied in the CDR container.
const CONFIG_PATH: &str = "/etc/ferroehr/relay.toml";

/// Where the issuer's key set is copied in the CDR container.
const JWKS_PATH: &str = "/etc/ferroehr/machines.jwks.json";

/// Where a CDR's database answers, as the CDR container reaches it.
#[derive(Debug, Clone)]
pub(crate) struct Placement {
    /// The network the database and the CDR share.
    pub(crate) network: String,
    /// The connection URL of the CDR's role, by container name.
    pub(crate) dsn: String,
}

/// Starts the PostgreSQL the reference CDR image documents, on a network of
/// its own, and returns it with where the CDR reaches it.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses the container.
pub(crate) async fn database() -> Result<(ContainerAsync<GenericImage>, Placement), HarnessError> {
    let sequence = NETWORK_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let process = std::process::id();
    let network = format!("ferrobridge-e2e-{process}-{sequence}");
    let database_name = format!("ferrobridge-e2e-db-{process}-{sequence}");
    let database = CDR_POSTGRES
        .image()
        .with_wait_for(WaitFor::healthcheck())
        .with_health_check(postgres_health_check(CDR_DATABASE_ROLE, CDR_DATABASE_ROLE))
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_network(network.clone())
        .with_container_name(database_name.clone())
        .start()
        .await
        .map_err(|source| HarnessError::Container {
            image: CDR_POSTGRES.repository,
            source,
        })?;
    let dsn = format!(
        "postgres://{CDR_DATABASE_ROLE}:{CDR_DATABASE_ROLE}@{database_name}:{POSTGRES_PORT}/{CDR_DATABASE_ROLE}"
    );
    Ok((database, Placement { network, dsn }))
}

/// Starts the CDR `request` describes beside `database` and waits for its
/// readiness endpoint.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses the container,
/// [`HarnessError::Probe`] when the readiness probe cannot be sent, and
/// [`HarnessError::NotReady`] when the CDR does not answer it in time.
pub(crate) async fn serve(
    database: ContainerAsync<GenericImage>,
    request: ContainerRequest<GenericImage>,
) -> Result<Cdr, HarnessError> {
    let failed = |source| HarnessError::Container {
        image: CDR.repository,
        source,
    };
    let server = request.start().await.map_err(failed)?;
    let host = server.get_host().await.map_err(failed)?;
    let port = server
        .get_host_port_ipv4(CDR_PORT.tcp())
        .await
        .map_err(failed)?;
    let origin = format!("http://{host}:{port}");
    await_readiness(&format!("{origin}/health/readiness")).await?;
    Ok(Cdr {
        server,
        database,
        base_url: format!("{origin}/ferroehr/rest/openehr/v1"),
        extensions_url: format!("{origin}/ferroehr/rest/extensions/v1"),
    })
}

/// One facility identifier a relay client commits for: an
/// `{ issuer, id }` entry of its `facilities`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Facility {
    /// The identifier's issuer, `DV_IDENTIFIER.issuer`.
    pub issuer: String,
    /// The identifier, `DV_IDENTIFIER.id`.
    pub id: String,
}

/// The client credentials client the CDR declares as a system client, of the
/// `relay` origin, the `device` origin or both, and the `machines` issuer whose
/// tokens name it.
#[derive(Debug, Clone)]
pub struct RelayClient {
    /// The issuer identifier its tokens carry in `iss`.
    pub issuer: String,
    /// The audience its tokens carry in `aud`.
    pub audience: String,
    /// The JWK Set of the issuer's public keys.
    pub keys: jsonwebtoken::jwk::JwkSet,
    /// The client id its tokens name.
    pub client_id: String,
    /// The healthcare provider it acts for.
    pub provider: Facility,
    /// The facility identifiers it may commit for.
    pub facilities: Vec<Facility>,
    /// The operational templates it may commit, each uploaded before the
    /// declaring boot: the template id and the OPT 1.4 document.
    pub templates: Vec<(String, Vec<u8>)>,
    /// The origins it commits under: `relay`, `device` or both.
    pub origins: Vec<String>,
    /// The issuers of the device identifiers a `device` client names.
    pub device_identifier_issuers: Vec<String>,
}

/// A relay CDR could not be booted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RelayError {
    /// A container could not be started or did not become ready.
    #[error(transparent)]
    Harness(#[from] HarnessError),
    /// The configuration could not be written.
    #[error("the relay configuration could not be written")]
    Configuration(#[source] toml::ser::Error),
    /// The key set could not be written.
    #[error("the key set could not be written")]
    Keys(#[source] serde_json::Error),
    /// A template upload could not be sent.
    #[error("the template {template} could not be uploaded")]
    Upload {
        /// The template id.
        template: String,
        /// What the HTTP stack reported.
        #[source]
        source: reqwest::Error,
    },
    /// The CDR refused a template upload.
    #[error("the CDR answered {status} to the upload of the template {template}")]
    Refused {
        /// The template id.
        template: String,
        /// The status it answered.
        status: reqwest::StatusCode,
    },
    /// The first boot's server could not be removed.
    #[error("the first boot's CDR could not be removed")]
    Remove(#[source] testcontainers::TestcontainersError),
}

/// The configuration file of the declaring boot.
#[derive(Debug, Serialize)]
struct Configuration<'a> {
    auth: Auth<'a>,
    authz: Authz<'a>,
}

/// `[auth]`: one `machines` issuer whose key set is the copied file, and
/// the relay client admitted to `system/` AQL.
#[derive(Debug, Serialize)]
struct Auth<'a> {
    enabled: bool,
    issuer: Vec<TrustedIssuer<'a>>,
    smart: Smart<'a>,
}

/// One `[[auth.issuer]]` table.
#[derive(Debug, Serialize)]
struct TrustedIssuer<'a> {
    issuer: &'a str,
    kind: &'a str,
    audience: Vec<&'a str>,
    jwks_file: &'a str,
    token_types: Vec<&'a str>,
}

/// `[auth.smart]`.
#[derive(Debug, Serialize)]
struct Smart<'a> {
    backend_clients: Vec<&'a str>,
}

/// `[authz]`: the provider the deployment acts for, which the CDR requires
/// once authentication is on, and the relay client.
#[derive(Debug, Serialize)]
struct Authz<'a> {
    party_binding: PartyBinding<'a>,
    system_client: Vec<SystemClient<'a>>,
}

/// `[authz.party_binding]`.
#[derive(Debug, Serialize)]
struct PartyBinding<'a> {
    provider: &'a Facility,
}

/// One `[[authz.system_client]]` table.
#[derive(Debug, Serialize)]
struct SystemClient<'a> {
    issuer: &'a str,
    client_id: &'a str,
    provider: &'a Facility,
    facilities: &'a [Facility],
    writes: Vec<&'a str>,
    origins: &'a [String],
    device_identifier_issuers: &'a [String],
}

impl RelayClient {
    /// Returns the configuration file declaring this client.
    fn configuration(&self) -> Result<String, RelayError> {
        let configuration = Configuration {
            auth: Auth {
                enabled: true,
                issuer: vec![TrustedIssuer {
                    issuer: &self.issuer,
                    kind: "machines",
                    audience: vec![&self.audience],
                    jwks_file: JWKS_PATH,
                    token_types: vec!["at+jwt"],
                }],
                smart: Smart {
                    backend_clients: vec![&self.client_id],
                },
            },
            authz: Authz {
                party_binding: PartyBinding {
                    provider: &self.provider,
                },
                system_client: vec![SystemClient {
                    issuer: &self.issuer,
                    client_id: &self.client_id,
                    provider: &self.provider,
                    facilities: &self.facilities,
                    writes: self.templates.iter().map(|(id, _)| id.as_str()).collect(),
                    origins: &self.origins,
                    device_identifier_issuers: &self.device_identifier_issuers,
                }],
            },
        };
        toml::to_string(&configuration).map_err(RelayError::Configuration)
    }
}

/// Starts the reference CDR declaring `client` as its relay client, with
/// its templates stored, and waits for its readiness endpoint.
///
/// Every commit the CDR answers carries the facility check of a declared
/// relay client: a composition naming no facility of `client.facilities` is
/// refused with `422`.
///
/// # Errors
///
/// Returns [`RelayError`] when a container cannot be started, a template is
/// refused, or the configuration cannot be written.
pub async fn relay(client: &RelayClient) -> Result<Cdr, RelayError> {
    let configuration = client.configuration()?;
    let keys = serde_json::to_vec(&client.keys).map_err(RelayError::Keys)?;
    let (database, placement) = database().await?;
    let first = serve(
        database,
        cdr_request(placement.dsn.clone(), placement.network.clone()),
    )
    .await?;
    upload(first.base_url(), &client.templates).await?;
    let Cdr {
        server, database, ..
    } = first;
    server.rm().await.map_err(RelayError::Remove)?;
    let request = cdr_request(placement.dsn, placement.network)
        .with_env_var("FERROEHR__AUTH__ENABLED", "true")
        .with_env_var("FERROEHR_CONFIG", CONFIG_PATH)
        .with_copy_to(CONFIG_PATH, configuration.into_bytes())
        .with_copy_to(JWKS_PATH, keys);
    Ok(serve(database, request).await?)
}

/// Uploads each template of `templates` through the Definition API.
async fn upload(base_url: &str, templates: &[(String, Vec<u8>)]) -> Result<(), RelayError> {
    let client = reqwest::Client::new();
    for (template, document) in templates {
        let response = client
            .post(format!("{base_url}/definition/template/adl1.4"))
            .header(reqwest::header::CONTENT_TYPE, "application/xml")
            .body(document.clone())
            .send()
            .await
            .map_err(|source| RelayError::Upload {
                template: template.clone(),
                source,
            })?;
        if response.status() != reqwest::StatusCode::CREATED {
            return Err(RelayError::Refused {
                template: template.clone(),
                status: response.status(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Facility, RelayClient};

    #[test]
    fn a_device_client_declares_its_identifier_issuers() {
        let client = RelayClient {
            issuer: String::from("https://machines.example"),
            audience: String::from("ferroehr-machines"),
            keys: jsonwebtoken::jwk::JwkSet { keys: Vec::new() },
            client_id: String::from("ferrobridge"),
            provider: Facility {
                issuer: String::from("urn:oid:2.999.7"),
                id: String::from("clinic"),
            },
            facilities: Vec::new(),
            templates: Vec::new(),
            origins: vec![String::from("device")],
            device_identifier_issuers: vec![String::from("urn:oid:2.999.9")],
        };
        let written: toml::Table = toml::from_str(&client.configuration().unwrap()).unwrap();
        let declared = &written["authz"]["system_client"][0];
        assert_eq!("device", declared["origins"][0].as_str().unwrap());
        assert_eq!(
            "urn:oid:2.999.9",
            declared["device_identifier_issuers"][0].as_str().unwrap()
        );
    }

    #[test]
    fn the_configuration_declares_the_client_as_a_relay_of_its_templates() {
        let client = RelayClient {
            issuer: String::from("https://machines.example"),
            audience: String::from("ferroehr-machines"),
            keys: jsonwebtoken::jwk::JwkSet { keys: Vec::new() },
            client_id: String::from("ferrobridge"),
            provider: Facility {
                issuer: String::from("urn:oid:2.999.7"),
                id: String::from("clinic"),
            },
            facilities: vec![Facility {
                issuer: String::from("urn:oid:2.999.1"),
                id: String::from("NORTHWARD"),
            }],
            templates: vec![(String::from("synthetic.v1"), Vec::new())],
            origins: vec![String::from("relay")],
            device_identifier_issuers: Vec::new(),
        };
        let written: toml::Table = toml::from_str(&client.configuration().unwrap()).unwrap();
        let declared = &written["authz"]["system_client"][0];
        assert_eq!("ferrobridge", declared["client_id"].as_str().unwrap());
        assert_eq!("relay", declared["origins"][0].as_str().unwrap());
        assert_eq!("synthetic.v1", declared["writes"][0].as_str().unwrap());
        assert_eq!(
            "NORTHWARD",
            declared["facilities"][0]["id"].as_str().unwrap()
        );
        assert_eq!(
            "machines",
            written["auth"]["issuer"][0]["kind"].as_str().unwrap()
        );
    }
}
