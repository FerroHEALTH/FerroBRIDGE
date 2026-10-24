// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[facade]` section and the tables under it.

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
    /// The file the identity map is kept in.
    pub identity_store: PathBuf,
    /// The namespace a literal subject reference is read in, the
    /// `PARTY_REF.namespace` of `EHR_STATUS.subject`.
    pub subject_namespace: String,
    /// How the id of a literal subject reference is written:
    /// `reference`, `logical_id` or `uuid`.
    pub subject_id_form: String,
    /// The `PARTY_REF.type` of the subject of an EHR the facade creates.
    pub subject_type: String,
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
}

impl Default for Facade {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::from("http://127.0.0.1:8080/fhir"),
            ehr_policy: String::from("existing"),
            identity_store: PathBuf::from("identity.redb"),
            subject_namespace: String::from("ferrobridge"),
            subject_id_form: String::from("reference"),
            subject_type: String::from("PERSON"),
            ehr_status_identifier_system: String::new(),
            system_id: String::from("FerroBRIDGE"),
            composition_language: String::new(),
            composition_territory: String::new(),
            search_count: crate::facade::search::Limits::default().default_count,
            search_count_max: crate::facade::search::Limits::default().max_count,
            references: FacadeReferences::default(),
            identifier_search: FacadeIdentifierSearch::default(),
            party_search: FacadePartySearch::default(),
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
    pub timeout_ms: u64,
}

impl Default for FacadeReferences {
    fn default() -> Self {
        Self {
            fetch_bases: Vec::new(),
            max_depth: crate::facade::references::DEFAULT_MAX_DEPTH,
            timeout_ms: crate::facade::references::DEFAULT_TIMEOUT_MS,
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
