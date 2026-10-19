// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 REST facade over the openEHR CDR.
//!
//! FerroBRIDGE stores no clinical data of its own: every interaction here maps
//! onto CDR operations through a compiled FHIRconnect program
//! (no specification governs this: our own design). What the facade keeps is identity, in the
//! store of [`identity`], so a re-sent resource updates the composition it
//! already produced and a read resolves a FHIR id back to one entry.
//!
//! Two rules run through every module. A status is a `StatusCode`, mapped from
//! the CDR's own outcome by the one table in [`status`]. Everything the facade
//! authors is an `OperationOutcome`, and an upstream openEHR error body
//! travels inside `issue.diagnostics` rather than reaching the wire raw.
//!
//! A disabled facade mounts no route, so a request answers `404` rather than
//! `403`: capability is not authorisation.

pub mod capability;
pub mod commit;
pub mod edge;
pub mod ehr;
pub mod engine;
pub mod handlers;
pub mod identity;
pub mod ingest;
pub mod media;
pub mod outcome;
pub mod party;
pub mod programs;
pub mod references;
pub mod reply;
pub mod request;
pub mod search;
pub mod status;
pub mod subject;

use std::sync::Arc;

use fhirconnect::engine::demographics::Demographics;
use fhirconnect::engine::demographics::IssuerDemographics;
use fhirconnect::engine::demographics::NoDemographics;

use axum::Router;
use axum::routing::get;
use axum::routing::post;

use crate::cdr::CdrClient;
use crate::facade::identity::claims::Claims;
use crate::facade::identity::store::Store;
use crate::facade::programs::Programs;
use crate::terminology::Translator;
use crate::usage_report::window::Window;

/// The path every facade route sits under.
///
/// The FHIR service base is `[base]` of the R4 HTTP chapter
/// (<https://hl7.org/fhir/R4/http.html>), and this deployment mounts it at
/// `/fhir` so the health routes and the FHIRconnect operations keep their own
/// space (no specification governs the mount point: our own design).
pub const BASE_PATH: &str = "/fhir";

/// What one facade deployment was configured with.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The absolute FHIR service base, as a client sees it.
    ///
    /// `Location` is written under this.
    pub base_url: String,
    /// Whether an unknown subject gets an EHR on its first write.
    pub ehr_policy: ehr::Policy,
    /// The link between a FHIR subject and its EHR: the namespace, the id
    /// form and the `PARTY_REF.type`.
    pub subject: subject::SubjectLink,
    /// The `AUDIT_DETAILS.system_id` every commit records.
    pub system_id: String,
    /// The `COMPOSITION.language` the facade writes, an ISO 639-1 code.
    ///
    /// `engine/defaults-for-fields.adoc` puts the composer and the context
    /// start time on the engine and every other field on "the project
    /// performing the mapping", so this deployment states it. There is no
    /// correct default for the language of a clinical record, so the
    /// configuration has none.
    pub language: String,
    /// The `COMPOSITION.territory` the facade writes, an ISO 3166-1 code.
    pub territory: String,
}

/// The programs and search declarations of one load, which a reload swaps
/// whole (no specification governs this: our own design).
#[derive(Debug)]
pub struct Mappings {
    /// The compiled programs.
    programs: Arc<Programs>,
    /// The search declarations, by resource type.
    search: Arc<search::Catalog>,
}

impl Mappings {
    /// Returns the compiled programs.
    #[must_use]
    pub fn programs(&self) -> &Programs {
        &self.programs
    }

    /// Returns the search declarations.
    #[must_use]
    pub fn search(&self) -> &search::Catalog {
        &self.search
    }
}

/// Everything a facade handler reaches.
#[derive(Debug)]
pub struct Facade {
    /// The programs and search declarations, swapped whole on a reload.
    mappings: std::sync::RwLock<Arc<Mappings>>,
    /// The identity map.
    store: Arc<dyn Store>,
    /// The source keys the in-flight deliveries into that map hold.
    claims: Claims,
    /// The CDR client.
    client: CdrClient,
    /// What the deployment configured.
    settings: Settings,
    /// The page sizes a search answers with.
    limits: search::Limits,
    /// The identifier resolve a chained `patient.identifier` runs, when one is
    /// configured.
    identifier: Option<search::identifier::IdentifierSearch>,
    /// The terminology server every `conceptmap` code translates through.
    translator: Translator,
    /// What may be fetched to resolve a reference the request does not carry.
    references: references::Policy,
    /// The usage report's window the ingest and the renders count mapping
    /// failures in.
    usage: Arc<Window>,
    /// The demographics endpoint every run resolves a party reference
    /// through: the ids of this FHIR server.
    demographics: Arc<dyn Demographics>,
}

/// Returns the demographics endpoint of a facade at `settings.base_url`: the
/// ids of this FHIR server, the fallback `engine/demographics.adoc` names when
/// no demographics service is implemented.
fn demographics_of(settings: &Settings) -> Arc<dyn Demographics> {
    match IssuerDemographics::new(&settings.base_url) {
        Ok(port) => Arc::new(port),
        // NOTE: no specification governs this: our own design; `[facade] base_url` is checked
        // at boot, and a base that still cannot issue refuses every reference rather than drop one.
        Err(_) => Arc::new(NoDemographics),
    }
}

impl Facade {
    /// Returns a facade over `programs`, `store` and `client`.
    #[must_use]
    pub fn new(
        programs: Programs,
        store: Arc<dyn Store>,
        client: CdrClient,
        settings: Settings,
    ) -> Self {
        Self {
            mappings: std::sync::RwLock::new(Arc::new(Mappings {
                programs: Arc::new(programs),
                search: Arc::new(search::Catalog::default()),
            })),
            store,
            claims: Claims::new(),
            client,
            demographics: demographics_of(&settings),
            settings,
            limits: search::Limits::default(),
            identifier: None,
            translator: Translator::default(),
            references: references::Policy::default(),
            usage: Arc::default(),
        }
    }

    /// Returns this facade counting every unit its programs refuse to map,
    /// on ingest through any face and on a read or search, in `usage`, the
    /// window the server's usage report takes.
    #[must_use]
    pub fn with_usage(mut self, usage: Arc<Window>) -> Self {
        self.usage = usage;
        self
    }

    /// Returns this facade translating every `conceptmap` code through
    /// `translator`, on every read, write and `$validate` and on the HL7 v2
    /// face that writes through it.
    ///
    /// A facade built without it translates through no server, so a
    /// `conceptmap` refuses.
    #[must_use]
    pub fn with_translator(mut self, translator: Translator) -> Self {
        self.translator = translator;
        self
    }

    /// Returns the usage report's window the facade counts mapping failures
    /// in.
    #[must_use]
    pub fn usage(&self) -> &Window {
        &self.usage
    }

    /// Returns the demographics endpoint every run resolves a party reference
    /// through.
    #[must_use]
    pub fn demographics(&self) -> &dyn Demographics {
        self.demographics.as_ref()
    }

    /// Returns the terminology server the engine runs translate through.
    #[must_use]
    pub const fn translator(&self) -> &Translator {
        &self.translator
    }

    /// Returns this facade resolving a reference the request does not carry
    /// under `policy`.
    ///
    /// A facade built without it reads a relative reference to a resource it
    /// committed and fetches no absolute one.
    #[must_use]
    pub fn with_references(mut self, policy: references::Policy) -> Self {
        self.references = policy;
        self
    }

    /// Returns what may be fetched to resolve a reference.
    #[must_use]
    pub const fn references(&self) -> &references::Policy {
        &self.references
    }

    /// Returns this facade answering search as `catalog` declares, with the
    /// page sizes of `limits`.
    ///
    /// A facade built without it answers no search, and its
    /// `CapabilityStatement` declares none.
    #[must_use]
    pub fn with_search(mut self, catalog: search::Catalog, limits: search::Limits) -> Self {
        let programs = Arc::clone(&self.mappings().programs);
        self.mappings = std::sync::RwLock::new(Arc::new(Mappings {
            programs,
            search: Arc::new(catalog),
        }));
        self.limits = limits;
        self
    }

    /// Returns the programs and search declarations a request runs on, taken
    /// once at its entry so a reload never changes them under it.
    #[must_use]
    pub fn mappings(&self) -> Arc<Mappings> {
        // NOTE: no specification governs this: our own design; the lock only
        // guards an `Arc` swap, so a poisoned lock still holds a whole set.
        Arc::clone(
            &self
                .mappings
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Replaces the programs and search declarations every later request runs
    /// on; a request in flight finishes on the set it took.
    pub fn replace(&self, programs: Programs, catalog: search::Catalog) {
        let next = Arc::new(Mappings {
            programs: Arc::new(programs),
            search: Arc::new(catalog),
        });
        *self
            .mappings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
    }

    /// Returns the search declarations of the current set.
    #[must_use]
    pub fn search(&self) -> Arc<search::Catalog> {
        Arc::clone(&self.mappings().search)
    }

    /// Returns the page sizes a search answers with.
    #[must_use]
    pub const fn search_limits(&self) -> search::Limits {
        self.limits
    }

    /// Returns this facade resolving a chained `patient.identifier` or
    /// `subject.identifier` through `identifier`, the configured query of
    /// Cadasto's experimental AQL demographic extension.
    ///
    /// A facade built without it supports no chained parameter.
    #[must_use]
    pub fn with_identifier_search(
        mut self,
        identifier: Option<search::identifier::IdentifierSearch>,
    ) -> Self {
        self.identifier = identifier;
        self
    }

    /// Returns the identifier resolve a chained search runs, when one is
    /// configured.
    #[must_use]
    pub const fn identifier_search(&self) -> Option<&search::identifier::IdentifierSearch> {
        self.identifier.as_ref()
    }

    /// Returns the compiled programs of the current set.
    #[must_use]
    pub fn programs(&self) -> Arc<Programs> {
        Arc::clone(&self.mappings().programs)
    }

    /// Returns the identity map.
    #[must_use]
    pub fn store(&self) -> &dyn Store {
        self.store.as_ref()
    }

    /// Returns a handle on the identity map, for the readiness probe.
    #[must_use]
    pub fn store_handle(&self) -> Arc<dyn Store> {
        Arc::clone(&self.store)
    }

    /// Returns the CDR client.
    #[must_use]
    pub const fn client(&self) -> &CdrClient {
        &self.client
    }

    /// Returns the CDR client one request calls through.
    ///
    /// The inbound `X-Request-Id` travels onto every outbound call, so one
    /// correlation identifier covers the request and the CDR calls it makes.
    #[must_use]
    pub fn client_for(&self, headers: &http::HeaderMap) -> CdrClient {
        headers
            .get(crate::request_id::HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(|text| crate::cdr::ids::RequestId::new(text).ok())
            .map_or_else(|| self.client.clone(), |id| self.client.with_request_id(id))
    }

    /// Returns what the deployment configured.
    #[must_use]
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Returns the ingest service over this facade, calling the CDR through
    /// `client`.
    ///
    /// A handler passes [`Facade::client_for`] so one request's CDR calls
    /// carry its request id; another face passes [`Facade::client`] or a
    /// client of its own.
    #[must_use]
    pub fn ingest(&self, client: CdrClient) -> ingest::Ingest<'_> {
        self.ingest_under(client, &self.settings)
    }

    /// Returns the ingest service over this facade's programs, identity map
    /// and claims, calling the CDR through `client` under `settings`.
    ///
    /// A face whose deployment states its own EHR policy passes settings of
    /// its own; the identity map and the claims stay the facade's, so one
    /// source delivered through two faces is recognised by both.
    #[must_use]
    pub fn ingest_under<'a>(
        &'a self,
        client: CdrClient,
        settings: &'a Settings,
    ) -> ingest::Ingest<'a> {
        ingest::Ingest::new(
            self.programs(),
            self.store.as_ref(),
            &self.claims,
            client,
            settings,
        )
        .with_translator(&self.translator)
        .with_usage(&self.usage)
        .with_demographics(self.demographics.as_ref())
    }
}

/// Returns the facade's routes, mounted under [`BASE_PATH`].
///
/// Only what is implemented is mounted: the conformance statement, a
/// type-level create and search (`GET` and `POST _search`), `$validate`, an
/// instance read and update, a vread, and a system-level transaction. Batch has
/// no route, so a request for it answers `404`
/// (<https://hl7.org/fhir/R4/http.html>). `shape`
/// says whether the same router serves the FHIRconnect operations, whether
/// the HL7 v2 face runs and which authorization server guards the facade,
/// which the conformance statement declares only then.
pub fn routes(facade: Arc<Facade>, shape: capability::Shape) -> Router {
    let shape = Arc::new(shape);
    let router = Router::new()
        .route(
            &format!("{BASE_PATH}/metadata"),
            get(
                move |axum::extract::State(facade): axum::extract::State<Arc<Facade>>,
                      headers: http::HeaderMap,
                      uri: http::Uri| {
                    let shape = Arc::clone(&shape);
                    async move { handlers::metadata_route(&facade, &headers, &uri, &shape) }
                },
            ),
        )
        .route(BASE_PATH, post(handlers::transaction_route))
        .route(
            &format!("{BASE_PATH}/{{resource_type}}"),
            post(handlers::create_route).get(handlers::search_route),
        )
        .route(
            &format!("{BASE_PATH}/{{resource_type}}/_search"),
            post(handlers::search_form_route),
        )
        .route(
            &format!("{BASE_PATH}/{{resource_type}}/$validate"),
            post(handlers::validate_route),
        )
        .route(
            &format!("{BASE_PATH}/{{resource_type}}/{{id}}/_history/{{vid}}"),
            get(handlers::vread_route),
        )
        .route(
            &format!("{BASE_PATH}/{{resource_type}}/{{id}}"),
            get(handlers::read_route)
                .put(handlers::update_route)
                .delete(handlers::delete_route),
        )
        .route(
            &format!("{BASE_PATH}/{{resource_type}}/{{id}}/_history"),
            get(handlers::history_route),
        );
    router.with_state(facade)
}
