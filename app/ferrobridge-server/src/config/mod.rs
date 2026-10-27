// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration: the defaults, an optional TOML file, the environment and
//! the `--set` pairs, read by `ferrohealth-config` into one tree, then one
//! typed [`Settings`] the run path holds.
//!
//! Every struct refuses an unknown key, every default lives inline in its own
//! `Default` impl, and every credential is reachable through a `<key>_file`
//! sibling read at boot. A lane is off until its section is present. The
//! `[server]`, `[telemetry]` and `[usage_report]` sections are the family's
//! own, from `ferrohealth-server`, `ferrohealth-telemetry` and
//! `ferrohealth-usage-report`. No specification governs the configuration:
//! our own design.

mod auth;
mod credentials;
pub mod error;
mod resolve;
pub mod section;

use ferrohealth_config::loader::{Loaded, Loader};
use ferrohealth_config::provenance::Provenance;
use ferrohealth_server::section::ServerSection;
use ferrohealth_telemetry::section::TelemetrySection;
use ferrohealth_usage_report::section::UsageReportSection;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use error::Error;
use resolve::resolve_cdm;
use resolve::resolve_cdr;
use resolve::resolve_etl;
use resolve::resolve_facade;
use resolve::resolve_hl7v2;
use resolve::resolve_mappings;
use resolve::resolve_terminology;
use section::Cdm;
use section::Cdr;
use section::Etl;
use section::Hl7v2;
use section::Mappings;
use section::Operations;
use section::Operator;
use section::Terminology;
use section::facade::Facade;

/// The environment namespace: `FERROBRIDGE__SERVER__LISTEN` sets
/// `[server] listen`, and `FERROBRIDGE_CONFIG` names the file.
pub const PREFIX: &str = "FERROBRIDGE";

/// The prefix of every environment override.
///
/// The name after it is the dotted key with `__` between segments, upper or
/// lower case: `FERROBRIDGE__SERVER__LISTEN` sets `[server] listen`.
pub const ENV_PREFIX: &str = "FERROBRIDGE__";

/// The environment variable naming the configuration file.
pub const CONFIG_PATH_ENV: &str = "FERROBRIDGE_CONFIG";

/// The variable that sets `telemetry.format`.
pub const FORMAT_ENV: &str = "FERROBRIDGE_LOG_FORMAT";

/// The variable that sets `telemetry.filter`.
pub const FILTER_ENV: &str = "RUST_LOG";

/// The loader of the bridge's tree, with the two console variables as the
/// aliases of the keys they set.
///
/// # Errors
/// Returns the loader's error when the prefix is refused, which it never is
/// for [`PREFIX`].
pub fn loader() -> Result<Loader, ferrohealth_config::error::ConfigError> {
    Ok(Loader::new(PREFIX)?
        .alias(FORMAT_ENV, "telemetry.format")
        .alias(FILTER_ENV, "telemetry.filter"))
}

/// The whole configuration tree, as a file and the environment state it.
///
/// It serializes with every secret redacted (`ferrohealth_secret`'s
/// `Secret`, and the redacted sections of `ferrohealth-smart`), so the
/// operator API and `config check` can answer the running configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The HTTP surface.
    pub server: ServerSection,
    /// The console.
    pub telemetry: TelemetrySection,
    /// The openEHR CDR, when one is configured.
    pub cdr: Option<Cdr>,
    /// The FHIR terminology server, when one is configured.
    pub terminology: Option<Terminology>,
    /// The OMOP CDM database, when one is configured.
    pub cdm: Option<Cdm>,
    /// The OMOP ETL job, when one is configured.
    pub etl: Option<Etl>,
    /// Where the mapping files are read from.
    pub mappings: Mappings,
    /// The FHIR facade, off until its section turns it on.
    pub facade: Facade,
    /// Inbound authentication, on unless its section turns it off: the
    /// section `ferrohealth-smart` defines, with `deny_unknown_fields`.
    pub auth: ferrohealth_smart::config::AuthSection,
    /// The FHIRconnect operations.
    pub operations: Operations,
    /// The HL7 v2 face, off until its section turns it on.
    pub hl7v2: Hl7v2,
    /// The installation report to `FerroPULSE`.
    pub usage_report: UsageReportSection,
    /// The operator API, off until its section turns it on.
    pub operator: Operator,
    /// Where each key came from, which the load records and no file states.
    #[serde(skip)]
    provenance: Provenance,
}

impl Config {
    /// Reads the configuration of the running process: the file `path`
    /// names, or the one `FERROBRIDGE_CONFIG` names, the environment and the
    /// `--set` pairs `sets`, with where each key came from.
    ///
    /// # Errors
    /// Returns [`Error::Parse`] when the file cannot be read, a key or a
    /// variable is unknown, or a value does not read as its key.
    pub fn load(path: Option<&Path>, sets: &[(String, String)]) -> Result<Loaded<Self>, Error> {
        loader()
            .and_then(|loader| loader.load_with(path, sets))
            .map_err(|source| Error::Parse { source })
    }

    /// Returns the tree `loaded` read, with its provenance.
    #[must_use]
    pub fn from_loaded(loaded: Loaded<Self>) -> Self {
        let (config, provenance) = loaded.into_parts();
        Self {
            provenance,
            ..config
        }
    }

    /// Returns where each key came from.
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Reads `text` as TOML and applies `environment` over it.
    ///
    /// Reading the sources and reading the process environment are separate so
    /// a test can state both: setting an environment variable is `unsafe` in
    /// edition 2024 and this workspace forbids `unsafe`.
    ///
    /// # Errors
    /// Returns [`Error::Parse`] for TOML that does not read as this tree and
    /// for an override that names no key.
    pub fn from_sources(
        text: Option<&str>,
        environment: &BTreeMap<String, String>,
    ) -> Result<Self, Error> {
        loader()
            .and_then(|loader| loader.load_sources(text, None, environment))
            .map(Self::from_loaded)
            .map_err(|source| Error::Parse { source })
    }

    /// Returns whether the tree serves a surface `[auth]` guards: the facade,
    /// the operator API, or the FHIRconnect operations over a mapping set.
    ///
    /// `[auth]` is on by default, so `serve` refuses to start a server that
    /// serves one of them until it names an issuer or turns `[auth]` off; a
    /// server that serves only `/` and the health routes needs neither (no
    /// specification governs this: our own design).
    fn guards_a_surface(&self) -> bool {
        self.facade.enabled
            || self.operator.enabled
            || (self.operations.enabled && self.mappings.directory.is_some())
    }

    /// Resolves this tree into the settings `serve` holds.
    ///
    /// Every `_file` sibling is read here, so a secret reaches the process
    /// once, at boot, and never sits in the configuration tree.
    ///
    /// # Errors
    /// Returns [`Error::Conflict`] when a value and its `_file` sibling are
    /// both set, [`Error::Secret`] when a `_file` cannot be read, the value
    /// errors ([`Error::Listen`], [`Error::Url`], [`Error::WireVersion`],
    /// [`Error::Missing`], [`Error::Client`]) each naming the key that carries
    /// the fault, and [`Error::Auth`] and [`Error::Credentials`] naming the rule
    /// of `ferrohealth-smart` a section breaks. Returns [`Error::Mappings`]
    /// over [`crate::mappings::Error::NoTemplateSource`] when the operations
    /// are enabled and `[mappings] directory` is set with neither
    /// `[mappings] templates` nor `[cdr]`, so the refusal comes before any
    /// network call.
    pub fn resolve(&self) -> Result<Settings, Error> {
        self.resolve_serving(true)
    }

    /// Resolves this tree into the settings of a job that serves no HTTP
    /// surface (`cdm init`, `etl run`, `mapping check`), which holds no
    /// resource server, so `[auth]` is neither read nor required.
    ///
    /// # Errors
    /// Returns every [`Error`] [`Config::resolve`] returns but [`Error::Auth`].
    pub fn resolve_job(&self) -> Result<Settings, Error> {
        self.resolve_serving(false)
    }

    /// Resolves this tree, reading `[auth]` when `serves` and a surface it
    /// guards is served.
    fn resolve_serving(&self, serves: bool) -> Result<Settings, Error> {
        if let Some(directory) = self.mappings.directory.as_ref()
            && self.operations.enabled
            && self.mappings.templates.is_none()
            && self.cdr.is_none()
        {
            return Err(Error::Mappings {
                source: Box::new(crate::mappings::Error::NoTemplateSource {
                    directory: directory.clone(),
                }),
            });
        }
        if self.server.caller_rate.is_some() {
            return Err(Error::CallerRate);
        }
        Ok(Settings {
            server: self.server.clone(),
            telemetry: self.telemetry.clone(),
            cdr: self.cdr.as_ref().map(resolve_cdr).transpose()?,
            terminology: self
                .terminology
                .as_ref()
                .map(resolve_terminology)
                .transpose()?,
            display_fallback: self
                .terminology
                .as_ref()
                .is_some_and(|terminology| terminology.display_fallback),
            cdm: self.cdm.as_ref().map(resolve_cdm).transpose()?,
            etl: self.etl.as_ref().map(resolve_etl).transpose()?,
            mapping_directory: self.mappings.directory.clone(),
            omocl_directory: self.mappings.omocl.clone(),
            mappings: resolve_mappings(&self.mappings)?,
            facade: if self.facade.enabled {
                Some(resolve_facade(&self.facade)?)
            } else {
                None
            },
            auth: if serves && self.guards_a_surface() {
                auth::resolve_auth(&self.auth)?
            } else {
                None
            },
            hl7v2: if self.hl7v2.enabled {
                Some(resolve_hl7v2(&self.hl7v2, self.facade.enabled)?)
            } else {
                None
            },
            operations: OperationsSettings {
                enabled: self.operations.enabled,
                device_reference: self.operations.device_reference.clone(),
                composer: self.operations.composer.clone(),
                composition_language: self.operations.composition_language.clone(),
                composition_territory: self.operations.composition_territory.clone(),
            },
            usage_report: self.usage_report.clone(),
            operator: resolve::resolve_operator(&self.operator, &self.auth)?,
            configuration: if serves && self.operator.enabled {
                Some(
                    crate::operator::configuration::document(self, jiff::Timestamp::now())
                        .map_err(|source| Error::Render { source })?,
                )
            } else {
                None
            },
        })
    }
}

/// The facade lane, resolved.
#[derive(Debug, Clone)]
pub struct FacadeSettings {
    /// What the facade itself was configured with.
    pub settings: crate::facade::Settings,
    /// The file the identity map is kept in.
    pub identity_store: PathBuf,
    /// The page sizes a search answers with.
    pub search: crate::facade::search::Limits,
    /// What may be fetched to resolve a reference the request does not carry.
    pub references: crate::facade::references::Policy,
    /// The identifier resolve a chained `patient.identifier` runs, when on.
    pub identifier: Option<crate::facade::search::identifier::IdentifierSearch>,
    /// The queries the party types are searched with, when on.
    pub party_search: Option<crate::facade::search::party::PartySearch>,
}

/// The HL7 v2 face, resolved.
#[derive(Debug, Clone)]
pub struct Hl7v2Settings {
    /// The socket address the MLLP listener binds.
    pub listen: SocketAddr,
    /// The character set a message with an empty MSH-18 is read in.
    pub default_charset: ferrobridge_hl7v2::decode::Charset,
    /// The directory of the guide's `ConceptMap` files.
    pub concept_maps: PathBuf,
    /// The supplement directories, in load order.
    pub supplements: Vec<PathBuf>,
    /// What an entry no program maps does to the message.
    pub unmapped: crate::facade::ingest::UnmappedEntries,
    /// The EHR policy of the face, when it differs from the facade's.
    pub ehr_policy: Option<crate::facade::ehr::Policy>,
    /// The profile each resource type claims when the message map wrote
    /// none, by resource type.
    pub profiles: BTreeMap<String, String>,
    /// How long a connection and a frame may wait on the peer.
    pub timeouts: ferrobridge_hl7v2::mllp::Timeouts,
    /// The largest message one frame may carry, in bytes.
    pub frame_limit: usize,
    /// Whether an `AE` or an `AR` also logs the counted outcomes.
    pub log_outcomes: bool,
    /// The sending facilities the face accepts; empty accepts any.
    pub senders: Vec<crate::hl7v2::Sender>,
}

/// The settings the run path holds, with every secret already read.
#[derive(Debug)]
pub struct Settings {
    /// The HTTP surface.
    pub server: ServerSection,
    /// The console.
    pub telemetry: TelemetrySection,
    /// The CDR client configuration, when the lane is on.
    pub cdr: Option<crate::cdr::config::CdrConfig>,
    /// The terminology client configuration, when the lane is on.
    pub terminology: Option<ferrobridge_term::config::Config>,
    /// Whether a code the terminology server does not know takes its code as
    /// display (`[terminology] display_fallback`).
    pub display_fallback: bool,
    /// The OMOP CDM database, when the lane is on.
    pub cdm: Option<CdmSettings>,
    /// The OMOP ETL job, when it is configured.
    pub etl: Option<crate::etl::EtlSettings>,
    /// The directory the mapping files are read from, when one is configured.
    pub mapping_directory: Option<PathBuf>,
    /// The directory the OMOCL mapping files are read from, when one is
    /// configured.
    pub omocl_directory: Option<PathBuf>,
    /// The mapping set the operations compile from disk, when both
    /// directories are set.
    ///
    /// With `None` and a [`Settings::mapping_directory`], the operations take
    /// their templates from the CDR instead.
    pub mappings: Option<MappingSettings>,
    /// The facade lane, when it is on.
    pub facade: Option<FacadeSettings>,
    /// The resource server, when inbound authentication is on and a surface
    /// it guards is served.
    pub auth: Option<crate::smart::SmartSettings>,
    /// The HL7 v2 face, when it is on.
    pub hl7v2: Option<Hl7v2Settings>,
    /// The FHIRconnect operations lane.
    pub operations: OperationsSettings,
    /// The installation report to `FerroPULSE`.
    pub usage_report: UsageReportSection,
    /// The operator API, when it is mounted.
    pub operator: Option<crate::operator::OperatorSettings>,
    /// The running configuration the operator API answers, when it is
    /// mounted.
    pub configuration: Option<ferrobridge_operator_api::configuration::Configuration>,
}

/// The OMOP CDM database, resolved.
#[derive(Debug, Clone)]
pub struct CdmSettings {
    /// The PostgreSQL connection URL, which may carry a password.
    pub url: SecretString,
    /// The connection both CDM clients open, with its TLS settled.
    pub connection: omop_cdm::connection::CdmConnection,
    /// The schema the CDM tables live in.
    pub schema: omop_cdm::ddl::SchemaName,
    /// The schema the natural-key side table lives in.
    pub bridge_schema: omop_cdm::ddl::SchemaName,
    /// What the writer does for an EHR with no `PERSON`.
    pub person_policy: omop_cdm::writer::input::PersonPolicy,
    /// The pseudonymisation every ETL writer applies, when configured.
    pub pseudonymiser: Option<omop_cdm::writer::pseudonym::Pseudonymiser>,
}

/// The mapping set, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingSettings {
    /// The directory holding the FHIRconnect mapping files.
    pub directory: PathBuf,
    /// The directory holding the operational templates.
    pub templates: PathBuf,
}

/// The FHIRconnect operations lane, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSettings {
    /// Whether the two operations are served.
    pub enabled: bool,
    /// The `Device` reference the `Provenance` `agent.who` defaults to.
    pub device_reference: String,
    /// The composer an inbound run fills in when no mapping does.
    pub composer: String,
    /// The composition language, as an ISO 639-1 code.
    pub composition_language: Option<String>,
    /// The composition territory, as an ISO 3166-1 alpha-2 code.
    pub composition_territory: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::Config;
    use super::error::Error;
    use std::collections::BTreeMap;

    #[test]
    fn an_absent_file_and_an_empty_environment_leave_the_defaults() {
        let config = Config::from_sources(None, &BTreeMap::new()).expect("the defaults parse");
        assert_eq!(format!("{:?}", Config::default()), format!("{config:?}"));
        assert_eq!("127.0.0.1:8080", config.server.listen.to_string());
        assert_eq!(1024 * 1024, config.server.body_limit_bytes);
        assert_eq!(10_000, config.server.shutdown_timeout_ms);
        assert_eq!(
            config.telemetry.logged_query_parameters,
            Vec::<String>::new()
        );
    }

    #[test]
    fn an_override_under_a_value_that_is_not_a_section_is_refused() {
        let environment = BTreeMap::from([(
            String::from("FERROBRIDGE__SERVER__LISTEN__PORT"),
            String::from("1"),
        )]);
        let error =
            Config::from_sources(Some("[server]\nlisten = \"127.0.0.1:1\"\n"), &environment)
                .expect_err("listen is a string, not a section");
        assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    }

    #[test]
    fn an_unknown_key_refuses_to_boot_and_the_message_names_it() {
        let error = Config::from_sources(Some("[server]\nlisten_port = 8080\n"), &BTreeMap::new())
            .expect_err("an unknown key is refused");
        assert!(
            error.to_string().contains("listen_port")
                || format!("{:?}", std::error::Error::source(&error)).contains("listen_port"),
            "the refusal names the key: {error:?}"
        );
    }
}
