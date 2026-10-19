// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The file tree: one struct per TOML section, each refusing an unknown key
//! and holding its defaults in its own `Default` impl.

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::telemetry::{DEFAULT_FILTER, Format};

pub mod facade;

/// A secret written inline in the configuration: a [`SecretString`] from the
/// parse, so its `Debug` rendering is redacted and its memory is zeroed when
/// it is dropped.
///
/// It compares by value so a parsed tree can be compared with another. No
/// specification governs this: our own design.
#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub struct Secret(SecretString);

impl Secret {
    /// Returns the secret.
    #[must_use]
    pub const fn secret(&self) -> &SecretString {
        &self.0
    }
}

impl From<&str> for Secret {
    fn from(value: &str) -> Self {
        Self(SecretString::from(value))
    }
}

impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret() == other.0.expose_secret()
    }
}

impl Eq for Secret {}

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

/// The operator API: the engine state and the mapping files, read under the
/// operator scope.
///
/// Off until `enabled` is set, and refused at boot while `[auth]` is off, so
/// the API is never served to an unauthenticated caller. No specification
/// governs the operator surface: our own design.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Operator {
    /// Whether the operator routes are mounted under `/operator`.
    pub enabled: bool,
    /// The writable directory console saves go to; unset, the API saves
    /// nothing.
    pub mapping_dir: Option<PathBuf>,
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
    pub credentials: Credentials<OAuthClient>,
    /// Whether the bridge reaches the ITS-REST 1.1.0 Demographic API of the
    /// CDR, a module the release marks `x-status: DEVELOPMENT`.
    pub demographic: bool,
}

impl Default for Cdr {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            timeout_ms: 30_000,
            retry: Retry::default(),
            credentials: Credentials::default(),
            demographic: false,
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
    pub client_secret: Option<Secret>,
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
    pub credentials: Credentials<NoGrant>,
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
    pub url: Option<Secret>,
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
    /// What the writer does for an EHR with no `PERSON` row:
    /// `create_on_first_sight` (a placeholder row) or `existing` (a refusal).
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
    /// The OHDSI `vocabulary_id` each terminology id names, a URI or an OID
    /// among them, added over the engine's defaults.
    pub vocabulary_aliases: BTreeMap<String, String>,
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
            vocabulary_aliases: BTreeMap::new(),
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

/// The credentials one upstream expects: the static schemes every upstream
/// takes, and the token grant `Grant` names for the lanes that take one.
///
/// Every secret is reachable inline or through its `_file` sibling; setting
/// both is a boot error, and so is a static scheme beside `oauth`. The CDR
/// takes an [`OAuthClient`]; the terminology lane takes [`NoGrant`], so an
/// `oauth` table under it is refused when the file is read. One shape serves
/// both because `deny_unknown_fields` cannot be combined with
/// `#[serde(flatten)]` (<https://serde.rs/field-attrs.html#flatten>).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Credentials<Grant> {
    /// An RFC 6750 bearer token.
    pub bearer_token: Option<Secret>,
    /// A file holding the bearer token, read at boot.
    pub bearer_token_file: Option<PathBuf>,
    /// The user name of RFC 7617 basic authentication.
    pub user: Option<String>,
    /// A file holding the user name, read at boot.
    pub user_file: Option<PathBuf>,
    /// The password of RFC 7617 basic authentication.
    pub password: Option<Secret>,
    /// A file holding the password, read at boot.
    pub password_file: Option<PathBuf>,
    /// The OAuth 2.0 client credentials grant (RFC 6749 §4.4) the bridge
    /// obtains its token with, for a lane that takes one.
    pub oauth: Option<Grant>,
}

impl<Grant> Default for Credentials<Grant> {
    fn default() -> Self {
        Self {
            bearer_token: None,
            bearer_token_file: None,
            user: None,
            user_file: None,
            password: None,
            password_file: None,
            oauth: None,
        }
    }
}

/// The grant of a lane that obtains no token for itself: no value exists,
/// and an `oauth` table is refused when the file is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoGrant {}

impl<'de> Deserialize<'de> for NoGrant {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Err(serde::de::Error::custom(
            "this lane obtains no token with an OAuth grant, so it takes no `oauth` table",
        ))
    }
}

/// The `FerroHEALTH` installation report to `FerroPULSE`.
///
/// On by default, and silent without a writable `state_dir`, because without
/// a stable instance id each restart would count as a new installation. A CI
/// environment keeps it off whatever `enabled` says. No specification
/// governs this configuration: our own design.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UsageReport {
    /// Whether the server sends the report.
    pub enabled: bool,
    /// The collector endpoint: `https`, or `http` to a loopback host.
    pub endpoint: String,
    /// The writable directory the instance id is kept in.
    pub state_dir: Option<PathBuf>,
    /// How the server was deployed: `helm`, `compose`, `binary` or `unknown`.
    pub deployment: crate::usage_report::config::Deployment,
}

impl Default for UsageReport {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: String::from(crate::usage_report::config::ENDPOINT),
            state_dir: None,
            deployment: crate::usage_report::config::Deployment::Unknown,
        }
    }
}
