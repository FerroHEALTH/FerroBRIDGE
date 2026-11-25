// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[facade]` section and the tables under it.

use ferrohealth_config::units::Duration;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The FHIR R4 facade lane.
///
/// The lane is off until `enabled` is set, and a disabled facade mounts no
/// route, so a request answers `404` rather than `403`
/// (no specification governs this: our own design).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Facade {
    /// Whether the facade routes are mounted.
    pub enabled: bool,
    /// The absolute FHIR service base a client reaches this server at.
    pub base_url: String,
    /// The EHR policy: `existing` or `create_on_first_write`.
    pub ehr_policy: String,
    /// The `Identifier.system` whose identifier a run that offers the subject of
    /// the EHR writes as `EHR_STATUS.subject.external_ref`; empty names none.
    pub ehr_status_identifier_system: String,
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
    /// The page size of a search that names no `_count`.
    pub search_count: u32,
    /// The largest page one search answers; a larger `_count` is answered
    /// with this many.
    pub search_count_max: u32,
    /// How a reference the request does not carry is resolved.
    pub references: FacadeReferences,
    /// How a chained `patient.identifier` finds its patient.
    pub identifier_search: FacadeIdentifierSearch,
    /// How the party types are searched by `identifier`.
    pub party_search: FacadePartySearch,
    /// The `DV_IDENTIFIER.issuer` of each binding key the CDR binds a login
    /// to; a party carrying one is provisioned through SCIM and the facade
    /// refuses to write it. Empty refuses none.
    pub binding_identifier_issuers: Vec<String>,
    /// The exchange of access directives as FHIR `Consent`, IHE BPPC and
    /// IHE APPC.
    pub consent: FacadeConsent,
    /// SMART App State, served as `Basic`.
    pub app_state: FacadeAppState,
    /// SMART launch `Task`s, served as `Task`.
    pub launch_task: FacadeLaunchTask,
    /// Cross-origin resource sharing on the REST routes.
    pub cors: FacadeCors,
    /// The patient summary, `Patient/[id]/$summary`.
    pub summary: FacadeSummary,
}

impl Default for Facade {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::from("http://127.0.0.1:8080/fhir"),
            ehr_policy: String::from("existing"),
            ehr_status_identifier_system: String::new(),
            system_id: String::from("FerroBRIDGE"),
            composition_language: String::new(),
            composition_territory: String::new(),
            search_count: crate::facade::search::Limits::default().default_count,
            search_count_max: crate::facade::search::Limits::default().max_count,
            references: FacadeReferences::default(),
            identifier_search: FacadeIdentifierSearch::default(),
            party_search: FacadePartySearch::default(),
            binding_identifier_issuers: Vec::new(),
            consent: FacadeConsent::default(),
            app_state: FacadeAppState::default(),
            launch_task: FacadeLaunchTask::default(),
            summary: FacadeSummary::default(),
            cors: FacadeCors::default(),
        }
    }
}

/// The search on the types a party context serves, `[type]?identifier=…`,
/// run with demographic AQL as Cadasto's experimental AQL demographic
/// extension documents it.
///
/// Off, a party type offers no search. On, each type under `types` is
/// searched with its own query and every other party type offers none. No
/// query has a default: the tenant's archetypes and the extension both move,
/// so the deployment writes each (no specification governs this: our own
/// design).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FacadePartySearch {
    /// Whether the party types under `types` are searched.
    pub enabled: bool,
    /// The queries, by the resource type they search.
    pub types: BTreeMap<String, FacadePartySearchType>,
}

/// The queries one party type is searched with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FacadePartySearchType {
    /// The AQL an `identifier` value with no system runs, binding
    /// `$identifier`; its first column is the party's `uid/value`, and the
    /// facade pages it. Empty, the type takes no `identifier`.
    pub query: String,
    /// The AQL a `[system]|[value]` value runs, binding `$identifier` and
    /// `$system`; empty refuses a value naming a system.
    pub system_query: String,
    /// The AQL a `name` value runs, binding `$name` and, when it compares
    /// them, `$name_lower` and `$name_capitalised`, each a prefix pattern.
    /// Empty, the type takes no `name`.
    pub name_query: String,
}

/// How the facade resolves a reference a request names and does not carry.
///
/// "The engine should attempt to GET it from the sending site. If not
/// possible, the engine proceeds with the mapping" (`engine/references.adoc`).
/// A relative reference is read from this server; an absolute one is fetched
/// only from a base listed here, so by default nothing is fetched (no
/// specification governs the policy: our own design).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FacadeReferences {
    /// The FHIR service bases an absolute reference may be fetched from.
    pub fetch_bases: Vec<String>,
    /// How many levels of references are followed.
    pub max_depth: u32,
    /// How long one fetch may take, connection included.
    pub timeout: Duration,
}

impl Default for FacadeReferences {
    fn default() -> Self {
        Self {
            fetch_bases: Vec::new(),
            max_depth: crate::facade::references::DEFAULT_MAX_DEPTH,
            timeout: Duration::from_secs(10),
        }
    }
}

/// The chained search through a patient's identifier,
/// `Observation?patient.identifier=[system|]value`, resolved with demographic
/// AQL as Cadasto's experimental AQL demographic extension documents it.
///
/// Off, no chained parameter is supported. The default `query` is the resolve
/// observed on Cadasto's platform (no specification governs it: our own design).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FacadeIdentifierSearch {
    /// Whether `patient.identifier` and `subject.identifier` are supported.
    pub enabled: bool,
    /// The AQL a value with no system runs, binding `$identifier`; its first
    /// column is the party's `uid/value`.
    pub query: String,
    /// The AQL a `[system]|[value]` value runs, binding `$identifier` and
    /// `$system`; empty refuses a value naming a system.
    pub system_query: String,
}

impl Default for FacadeIdentifierSearch {
    fn default() -> Self {
        Self {
            enabled: false,
            query: String::from(
                "SELECT p/uid/value FROM PERSON p WHERE p/details[openEHR-DEMOGRAPHIC-ITEM_TREE.person_details.v2]/items[openEHR-DEMOGRAPHIC-CLUSTER.person_identifier.v2]/items[at0001]/value/id = $identifier ORDER BY p/uid/value ASC",
            ),
            system_query: String::new(),
        }
    }
}

/// The exchange of a patient's access directives as FHIR `Consent`, IHE BPPC
/// and IHE APPC, through the CDR's `EHR_ACCESS`.
///
/// Off, `Consent` is no resource type of the facade. The access default has
/// no default: it decides who reaches an EHR whose `EHR_ACCESS` the
/// exchange writes first, so the deployment states it (no specification
/// governs this: our own design).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FacadeConsent {
    /// Whether `Consent` is served.
    pub enabled: bool,
    /// The root the CDR serves its extensions under: the `EHR_ACCESS` reads
    /// (`/ehr/{ehr_id}/ehr_access`) and the controller's `ACCESS_POLICY`
    /// (`/admin/access_policy`).
    pub extensions_url: String,
    /// The `default_access` of the first settings the exchange writes:
    /// `open`, `restricted` or `consent_required`.
    pub default_access: String,
}

/// Cross-origin resource sharing on the facade's REST routes.
///
/// The discovery document and the `CapabilityStatement` answer any origin
/// whatever this says; the REST routes answer only the origins listed here,
/// the "client's registered origin(s)" of SMART App Launch 2.2.0, App Launch,
/// §Considerations for Cross-Origin Resource Sharing (CORS) support, which a
/// resource server holding no client registry takes from its deployment (no
/// specification governs the list's home: our own design).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FacadeCors {
    /// The origins a browser app may call the REST routes from, each an
    /// `http` or `https` origin; empty admits none.
    pub allowed_origins: Vec<String>,
}

/// SMART App State (SMART App Launch 2.2.0, Persisting App State), served as
/// `Basic` at the facade's base.
///
/// Off, `Basic` is an ordinary resource type a program may map. On, it is the
/// App State endpoint, its resources kept in the state store, and the
/// discovery document lists `smart-app-state` (no specification governs the
/// store: our own design).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FacadeAppState {
    /// Whether `Basic` is the App State endpoint.
    pub enabled: bool,
}

/// SMART launch `Task`s (SMART App Launch 2.2.0, Task profile for requesting
/// a SMART launch), served as `Task` at the facade's base.
///
/// Off, `Task` is an ordinary resource type a program may map. On, it serves
/// the `task-ehr-launch` and `task-standalone-launch` profiles, its resources
/// kept in the state store (no specification governs the store: our own
/// design).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FacadeLaunchTask {
    /// Whether `Task` is the launch endpoint.
    pub enabled: bool,
}

/// The patient summary (HL7 IPS 2.0.1 `OperationDefinition/summary`), served
/// as `Patient/[id]/$summary`.
///
/// On, the operation reads the `Patient` party and the patient summary
/// compositions of the patient's EHR, and validates each document over
/// `packages` before it is served.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FacadeSummary {
    /// Whether `Patient/[id]/$summary` is served.
    pub enabled: bool,
    /// The FHIR packages each document is validated over: `hl7.fhir.r4.core`
    /// and the dependency closure of IPS 2.0.1 and of the HL7 Europe Patient
    /// Summary, each a `.tgz` or an unpacked directory.
    pub packages: Vec<PathBuf>,
    /// The name of the organisation that operates this server, which each
    /// document names as an author.
    pub operator: String,
}
