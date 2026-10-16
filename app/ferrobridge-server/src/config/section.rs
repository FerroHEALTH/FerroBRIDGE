// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The file tree: one struct per TOML section, each refusing an unknown key
//! and holding its defaults in its own `Default` impl.

use serde::Deserialize;
use std::path::PathBuf;

use crate::telemetry::{DEFAULT_FILTER, Format};

/// The mapping set this deployment runs.
///
/// One tree of FHIRconnect mapping files and, optionally, one directory of the
/// operational templates the two operations compile against. With no
/// `templates`, the operations take each template a context names from the
/// `[cdr]`, and a `directory` with neither is refused at boot. Everything is
/// read once at boot, so a mapping that does not compile is a refusal to start
/// rather than a failure on the request that first touches it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mappings {
    /// The directory the FHIRconnect files are read from, recursively.
    pub directory: Option<PathBuf>,
    /// The directory holding the operational templates, as OPT 1.4 XML.
    ///
    /// When set, it wins over the `[cdr]` for the two operations; the facade
    /// always takes its templates from the CDR.
    pub templates: Option<PathBuf>,
    /// The directory the OMOCL files `etl run` maps with are read from,
    /// recursively.
    pub omocl: Option<PathBuf>,
}

/// The FHIR R4 facade lane.
///
/// The lane is off until `enabled` is set, and a disabled facade mounts no
/// route, so a request answers `404` rather than `403`
/// (`docs/architecture.md` §4.6).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Facade {
    /// Whether the facade routes are mounted.
    pub enabled: bool,
    /// The absolute FHIR service base a client reaches this server at.
    pub base_url: String,
    /// The EHR policy: `existing` or `create_on_first_write`.
    pub ehr_policy: String,
    /// The file the identity map is kept in.
    pub identity_store: PathBuf,
    /// The namespace a literal subject reference is read in.
    pub subject_namespace: String,
    /// The `AUDIT_DETAILS.system_id` every commit records.
    pub system_id: String,
    /// The `COMPOSITION.language` every commit carries, ISO 639-1.
    ///
    /// The mapping specification puts this field on "the project performing
    /// the mapping" (`engine/defaults-for-fields.adoc`), and a clinical record
    /// has no correct default language, so the key is empty until a deployment
    /// sets it and an enabled facade refuses to start without it.
    pub composition_language: String,
    /// The `COMPOSITION.territory` every commit carries, ISO 3166-1.
    pub composition_territory: String,
}

impl Default for Facade {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::from("http://127.0.0.1:8080/fhir"),
            ehr_policy: String::from("existing"),
            identity_store: PathBuf::from("identity.redb"),
            subject_namespace: String::from("ferrobridge"),
            system_id: String::from("FerroBRIDGE"),
            composition_language: String::new(),
            composition_territory: String::new(),
        }
    }
}

/// Inbound authentication: the FHIR routes as a SMART App Launch 2.x
/// resource server.
///
/// Off until `enabled` is set. Off, the facade authenticates no caller and
/// serves every request it receives, so a deployment keeps it on a network
/// only trusted callers reach. On, the issuer's metadata and key set are
/// read at boot.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Auth {
    /// Whether every FHIR request needs a bearer access token.
    pub enabled: bool,
    /// The issuer identifier, an `https` URL (RFC 8414 §2).
    pub issuer: String,
    /// The audience every token must name: this deployment's FHIR base.
    pub audience: String,
    /// The signature algorithms a token may use, asymmetric only.
    pub algorithms: Vec<String>,
    /// The claims a token must carry: `rfc9068` or `jwt`.
    pub token_profile: String,
    /// The clock skew `exp`, `nbf` and `iat` are checked with, in seconds.
    pub clock_skew_seconds: u64,
    /// The shortest time between two reads of the key set an unknown `kid`
    /// triggers.
    pub jwks_refresh_interval_ms: u64,
    /// How long one call to the authorization server may take.
    pub timeout_ms: u64,
    /// The subjects a token with no clinical scope gets system-wide access
    /// for.
    pub scopeless_subjects: Vec<String>,
    /// The scope that guards the operator API.
    pub operator_scope: String,
    /// SMART capabilities the discovery document adds, such as
    /// `launch-standalone`.
    pub capabilities: Vec<String>,
}

impl Default for Auth {
    fn default() -> Self {
        Self {
            enabled: false,
            issuer: String::new(),
            audience: String::new(),
            algorithms: vec![String::from("RS256"), String::from("ES256")],
            token_profile: String::from("rfc9068"),
            clock_skew_seconds: 60,
            jwks_refresh_interval_ms: 60_000,
            timeout_ms: 10_000,
            scopeless_subjects: Vec::new(),
            operator_scope: String::from(crate::auth::OPERATOR_SCOPE),
            capabilities: Vec::new(),
        }
    }
}

/// The HL7 v2 face: an MLLP listener whose messages enter the facade's
/// ingest service.
///
/// The face is off until `enabled` is set, and it needs the facade: the
/// ingest service, the identity map and the programs it writes through are
/// the facade's. No specification governs this configuration: our own
/// design.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hl7v2 {
    /// Whether the MLLP listener runs.
    pub enabled: bool,
    /// The socket address the MLLP listener binds.
    pub listen: String,
    /// The HL7 table 0211 code a message with an empty MSH-18 is read in.
    pub default_charset: String,
    /// The directory the v2-to-FHIR `ConceptMap` files are read from: the
    /// `package` directory of `hl7.fhir.uv.v2mappings`.
    pub concept_maps: Option<PathBuf>,
    /// Directories of supplement `ConceptMap` files, loaded over the guide in
    /// order; a map with the url of a loaded one replaces it.
    pub supplements: Vec<PathBuf>,
    /// What an entry no program maps does to the message: `skip_and_count`
    /// or `refuse`.
    pub unmapped_entries: String,
    /// The EHR policy of the face, `existing` or `create_on_first_write`;
    /// unset, the facade's.
    pub ehr_policy: Option<String>,
    /// The profile each resource type claims when the message map wrote none,
    /// so the facade's program selection reaches the context that claims it.
    pub profiles: Vec<Hl7v2Profile>,
    /// How long a connection may sit between frames before it is closed; `0`
    /// never closes it.
    pub idle_timeout_ms: u64,
    /// How long one frame may take from its first byte to its trailer before
    /// it is answered `AR`; `0` waits for the trailer.
    pub frame_timeout_ms: u64,
    /// The largest message one frame may carry, in bytes.
    pub frame_limit_bytes: usize,
    /// Whether an `AE` or an `AR` also logs, at debug level, the counted
    /// outcomes of the run by kind.
    pub log_outcomes: bool,
    /// The sending facilities (MSH-4) the face accepts; empty accepts any.
    pub senders: Vec<Hl7v2Sender>,
}

impl Default for Hl7v2 {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: String::from("127.0.0.1:2575"),
            default_charset: String::from("ASCII"),
            concept_maps: None,
            supplements: Vec::new(),
            unmapped_entries: String::from("skip_and_count"),
            ehr_policy: None,
            profiles: Vec::new(),
            idle_timeout_ms: 300_000,
            frame_timeout_ms: 30_000,
            frame_limit_bytes: 1024 * 1024,
            log_outcomes: false,
            senders: Vec::new(),
        }
    }
}

/// One sending facility the HL7 v2 face accepts: a namespace id alone, or a
/// universal id with its type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hl7v2Sender {
    /// HD.1 of MSH-4.
    pub namespace_id: String,
    /// HD.2 of MSH-4.
    pub universal_id: String,
    /// HD.3 of MSH-4, such as `ISO`.
    pub universal_id_type: String,
}

/// The profile one resource type of a mapped message claims.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hl7v2Profile {
    /// The R4 resource type, such as `Observation`.
    pub resource_type: String,
    /// The canonical URL written into `meta.profile`.
    pub profile: String,
}

/// The FHIRconnect operations lane.
///
/// The two operations are pure transformations that reach no CDR on a
/// request, so they are served whenever `[mappings] directory` is set and its
/// templates load, from `[mappings] templates` or, at boot, from the `[cdr]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Operations {
    /// Whether the two operations are served.
    pub enabled: bool,
    /// The `Device` reference the `Provenance` `agent.who` carries when a call
    /// supplies no `context.who`.
    pub device_reference: String,
    /// The composer an inbound run fills in when no mapping does.
    pub composer: String,
    /// The composition language, as an ISO 639-1 code.
    pub composition_language: Option<String>,
    /// The composition territory, as an ISO 3166-1 alpha-2 code.
    pub composition_territory: Option<String>,
}

impl Default for Operations {
    fn default() -> Self {
        Self {
            enabled: true,
            device_reference: format!(
                "Device/{}-{}",
                crate::state::PRODUCT.to_ascii_lowercase(),
                crate::state::VERSION
            ),
            composer: String::from("FHIRconnect"),
            composition_language: None,
            composition_territory: None,
        }
    }
}

/// The HTTP surface.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// The socket address to bind.
    pub listen: String,
    /// How long one request may take before the server answers `408`.
    pub request_timeout_ms: u64,
    /// How long the drain may take after the stop signal.
    pub shutdown_timeout_ms: u64,
    /// The largest request body the server reads before answering `413`.
    pub body_limit_bytes: usize,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:8080"),
            request_timeout_ms: 30_000,
            shutdown_timeout_ms: 10_000,
            body_limit_bytes: 1024 * 1024,
        }
    }
}

/// The console.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Telemetry {
    /// The rendering: `auto`, `json` or `pretty`.
    pub format: Format,
    /// The `tracing` filter directive.
    pub filter: String,
    /// The query parameter names whose values may reach the request log.
    ///
    /// Empty by default, so no query value is ever logged until a deployment
    /// names a parameter it knows carries no identifiable content.
    pub logged_query_parameters: Vec<String>,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            format: Format::Auto,
            filter: String::from(DEFAULT_FILTER),
            logged_query_parameters: Vec::new(),
        }
    }
}

/// The openEHR CDR lane.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cdr {
    /// The openEHR REST API root, the path `/ehr` and `/query` live under.
    pub base_url: String,
    /// How long one call may take, connection included.
    pub timeout_ms: u64,
    /// The retry budget for idempotent calls.
    pub retry: Retry,
    /// The credentials the CDR expects.
    pub credentials: CdrCredentials,
}

impl Default for Cdr {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            timeout_ms: 30_000,
            retry: Retry::default(),
            credentials: CdrCredentials::default(),
        }
    }
}

/// The credentials the CDR expects: the static schemes every upstream
/// takes, or a token the bridge obtains for itself.
///
/// Every secret is reachable inline or through its `_file` sibling; setting
/// both is a boot error, and so is a static scheme beside `oauth`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CdrCredentials {
    /// An RFC 6750 bearer token.
    pub bearer_token: Option<String>,
    /// A file holding the bearer token, read at boot.
    pub bearer_token_file: Option<PathBuf>,
    /// The user name of RFC 7617 basic authentication.
    pub user: Option<String>,
    /// A file holding the user name, read at boot.
    pub user_file: Option<PathBuf>,
    /// The password of RFC 7617 basic authentication.
    pub password: Option<String>,
    /// A file holding the password, read at boot.
    pub password_file: Option<PathBuf>,
    /// The OAuth 2.0 client credentials grant (RFC 6749 §4.4) the bridge
    /// obtains its CDR token with.
    pub oauth: Option<OAuthClient>,
}

impl CdrCredentials {
    /// Returns the static schemes of this section, the ones every upstream
    /// takes.
    #[must_use]
    pub fn static_schemes(&self) -> Credentials {
        Credentials {
            bearer_token: self.bearer_token.clone(),
            bearer_token_file: self.bearer_token_file.clone(),
            user: self.user.clone(),
            user_file: self.user_file.clone(),
            password: self.password.clone(),
            password_file: self.password_file.clone(),
        }
    }
}

/// The OAuth 2.0 client credentials grant the bridge obtains its CDR token
/// with (RFC 6749 §4.4).
///
/// Exactly one of `issuer` and `token_endpoint` is set.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OAuthClient {
    /// The issuer identifier whose metadata names the token endpoint
    /// (RFC 8414 §2).
    pub issuer: Option<String>,
    /// The token endpoint, when it is not discovered (RFC 6749 §3.2).
    pub token_endpoint: Option<String>,
    /// The client identifier (RFC 6749 §2.2).
    pub client_id: String,
    /// The client secret.
    pub client_secret: Option<String>,
    /// A file holding the client secret, read at boot.
    pub client_secret_file: Option<PathBuf>,
    /// How the client authenticates: `client_secret_basic` or
    /// `client_secret_post` (RFC 6749 §2.3.1).
    pub client_auth: crate::cdr::oauth::ClientAuth,
    /// The `scope` request parameter (RFC 6749 §3.3).
    pub scope: Option<String>,
    /// The `audience` request parameter, for an authorization server that
    /// reads one.
    pub audience: Option<String>,
    /// The `resource` request parameter (RFC 8707 §2).
    pub resource: Option<String>,
    /// How long before the token expires a new one is fetched.
    pub refresh_margin_ms: u64,
}

impl Default for OAuthClient {
    fn default() -> Self {
        Self {
            issuer: None,
            token_endpoint: None,
            client_id: String::new(),
            client_secret: None,
            client_secret_file: None,
            client_auth: crate::cdr::oauth::ClientAuth::default(),
            scope: None,
            audience: None,
            resource: None,
            refresh_margin_ms: 30_000,
        }
    }
}

/// The FHIR terminology lane.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Terminology {
    /// The FHIR service base URL.
    pub base_url: String,
    /// The release the server answers in: `stu3`, `r4` or `r4b`.
    pub wire_version: String,
    /// How long one call may take, connection included.
    pub timeout_ms: u64,
    /// The retry budget.
    pub retry: Retry,
    /// The credentials the server expects.
    pub credentials: Credentials,
}

impl Default for Terminology {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            wire_version: String::from("r4"),
            timeout_ms: 30_000,
            retry: Retry::default(),
            credentials: Credentials::default(),
        }
    }
}

/// The OMOP CDM database lane.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cdm {
    /// The PostgreSQL connection URL.
    pub url: Option<String>,
    /// A file holding the connection URL, read at boot.
    pub url_file: Option<PathBuf>,
    /// The PEM CA the database's certificate is checked against, for the
    /// URL's `sslmode` of `require`, `verify-ca` or `verify-full`.
    pub tls_ca: Option<String>,
    /// A file holding that CA, read at boot.
    pub tls_ca_file: Option<PathBuf>,
    /// The schema the CDM tables live in.
    pub schema: String,
    /// The schema the bridge keeps its natural-key side table in.
    pub bridge_schema: String,
    /// What the writer does for an EHR with no `PERSON`:
    /// `create_on_first_sight` or `existing`.
    pub person_policy: String,
}

impl Default for Cdm {
    fn default() -> Self {
        Self {
            url: None,
            url_file: None,
            tls_ca: None,
            tls_ca_file: None,
            schema: String::from("cdm"),
            bridge_schema: String::from("ferrobridge"),
            person_policy: String::from("create_on_first_sight"),
        }
    }
}

/// The OMOP ETL job.
///
/// The queries are parsed and checked when the configuration loads, so a
/// query the runner cannot read refuses the start of any job.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Etl {
    /// The AQL that selects each composition whole, aliased `ehr_id`,
    /// `versioned_object_uid`, `version_uid` and `composition`.
    pub aql: String,
    /// How many rows one page of a query asks for.
    pub page_size: u32,
    /// The `*_type_concept_id` the mappings write; no default, because it
    /// records a provenance only the deployment knows.
    pub type_concept_id: Option<i32>,
    /// The `period_type_concept_id` of every observation period.
    pub observation_period_type_concept_id: Option<i32>,
    /// The visit derivation, off until its section is present.
    pub visits: Option<EtlVisits>,
    /// What a run does with a composition the CDR reports deleted: `remove`
    /// or `report`.
    pub deletions: String,
}

impl Default for Etl {
    fn default() -> Self {
        Self {
            aql: String::new(),
            page_size: 100,
            type_concept_id: None,
            observation_period_type_concept_id: None,
            visits: None,
            // NOTE: no specification governs this: our own design; keeping the rows of a
            // record the source deleted is the defect, so a confirmed deletion removes them.
            deletions: String::from("remove"),
        }
    }
}

/// The visit derivation of the OMOP ETL job.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EtlVisits {
    /// The AQL aliased `ehr_id`, `visit_source`, `visit_start` and
    /// `visit_end`.
    pub aql: String,
    /// The `visit_concept_id` every visit carries.
    pub visit_concept_id: Option<i32>,
    /// The `visit_type_concept_id` every visit carries.
    pub visit_type_concept_id: Option<i32>,
}

/// How often, and how far apart, a call is retried.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Retry {
    /// How many times a call is sent at most, the first try included.
    pub max_attempts: u32,
    /// The delay before the second attempt.
    pub initial_backoff_ms: u64,
    /// The ceiling every later delay is clamped to.
    pub max_backoff_ms: u64,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 200,
            max_backoff_ms: 5_000,
        }
    }
}

/// The credentials one upstream expects.
///
/// Every secret is reachable inline or through its `_file` sibling; setting
/// both is a boot error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Credentials {
    /// An RFC 6750 bearer token.
    pub bearer_token: Option<String>,
    /// A file holding the bearer token, read at boot.
    pub bearer_token_file: Option<PathBuf>,
    /// The user name of RFC 7617 basic authentication.
    pub user: Option<String>,
    /// A file holding the user name, read at boot.
    pub user_file: Option<PathBuf>,
    /// The password of RFC 7617 basic authentication.
    pub password: Option<String>,
    /// A file holding the password, read at boot.
    pub password_file: Option<PathBuf>,
}
