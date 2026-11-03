// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every handler shares: the health registry, the lifecycle and the log
//! allowlist.

use std::sync::Arc;

use fhirconnect::engine::demographics::Demographics;
use fhirconnect::engine::demographics::IssuerDemographics;
use fhirconnect::engine::demographics::NoDemographics;

use crate::config::Settings;
use crate::indicators;
use crate::operations::{OperationsLane, OperationsOff};
use crate::terminology::Translator;
use ferrohealth_health::lifecycle::Lifecycle;
use ferrohealth_health::registry::Registry;

/// The product name the root document reports.
pub const PRODUCT: &str = "FerroBRIDGE";

/// The product version the root document reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The state the router is built over.
#[derive(Debug)]
pub struct AppState {
    /// The indicators readiness runs.
    health: Registry,
    /// Where the process is in its life: booting, serving or draining.
    lifecycle: Lifecycle,
    /// The query parameter names whose values may reach the request log.
    logged_query_parameters: Vec<String>,
    /// The FHIR facade, when its lane is on.
    ///
    /// A disabled facade is `None` and mounts no route, so a request answers
    /// `404` rather than `403` (no specification governs this: our own design).
    facade: Option<Arc<crate::facade::Facade>>,
    /// The FHIRconnect operations lane, when a mapping set is configured.
    operations: Option<Arc<OperationsLane>>,
    /// Why the operations are not served, when [`Self::operations`] is
    /// `None`.
    operations_off: OperationsOff,
    /// The lanes the boot summary reported, for `GET /health/info`.
    lanes: Vec<crate::startup::Lane>,
    /// Whether the HL7 v2 listener accepts, when the face runs.
    hl7v2: Option<crate::hl7v2::Listening>,
    /// The OMOP ETL run history the operator API reads, when `[cdm]` is set.
    etl_history: Option<Arc<crate::etl::history::Store>>,
    /// The HL7 v2 face's `ConceptMap` directory, when the face runs.
    concept_maps: Option<Arc<crate::hl7v2::directory::Directory>>,
    /// The resource server, when inbound authentication is on.
    auth: Option<Arc<crate::smart::Smart>>,
    /// The requests the usage report counts, recorded by the request log.
    usage: Arc<ferrohealth_usage_report::window::Window>,
    /// The operator API, when `[operator] enabled` mounts it.
    operator: Option<Arc<crate::operator::Operator>>,
    /// The one client of the `[terminology]` server, when one is configured,
    /// shared by readiness and every lane that translates.
    terminology: Option<ferrobridge_term::client::Client>,
    /// The access-record lane, when `[audit]` turns it on.
    access_log: Option<Arc<crate::audit::AccessLog>>,
    /// The change-feed consumers the operator API reports, when any runs.
    change_feed: Option<Arc<crate::change_feed::report::Report>>,
}

impl AppState {
    /// Returns the state `settings` describes, with one indicator per
    /// configured upstream.
    ///
    /// The operations lane compiles its mapping set here, against the
    /// templates [`crate::mappings::load_configured`] takes from the template
    /// directory or the CDR, so a template the CDR does not serve refuses the
    /// start.
    ///
    /// # Errors
    /// Returns [`crate::config::error::Error::Client`] when an upstream client
    /// refuses the configuration it was handed, and
    /// [`crate::config::error::Error::Mappings`] when the mapping set does not load.
    pub async fn build(settings: &Settings) -> Result<Self, crate::config::error::Error> {
        let mut indicators: Vec<Arc<dyn ferrohealth_health::registry::HealthIndicator>> =
            Vec::new();
        let mut cdr = None;
        if let Some(config) = settings.cdr.as_ref() {
            let client = crate::cdr::CdrClient::new(config).map_err(|source| {
                crate::config::error::Error::Client {
                    upstream: "cdr",
                    source: Box::new(source),
                }
            })?;
            indicators.push(Arc::new(indicators::Cdr::new(client.clone())));
            cdr = Some(client);
        }
        let mut terminology = None;
        if let Some(config) = settings.terminology.as_ref() {
            let client =
                ferrobridge_term::client::Client::new(config.clone()).map_err(|source| {
                    crate::config::error::Error::Client {
                        upstream: "terminology",
                        source: Box::new(source),
                    }
                })?;
            indicators.push(Arc::new(indicators::Terminology::new(client.clone())));
            terminology = Some(client);
        }
        if let Some(cdm) = settings.cdm.as_ref() {
            let probe = indicators::Cdm::new(&cdm.connection).map_err(|source| {
                crate::config::error::Error::CdmConnection {
                    key: String::from("cdm.url"),
                    source,
                }
            })?;
            indicators.push(Arc::new(probe));
        }
        let programs = crate::mappings::load_configured(settings, cdr.as_ref())
            .await
            .map_err(|source| crate::config::error::Error::Mappings {
                source: Box::new(source),
            })?;
        // NOTE: engine/demographics.adoc falls back to "the IDs maintained by the FHIRconnect
        // engine's FHIR-server"; with no facade there is none, so a party reference refuses.
        let demographics: Arc<dyn Demographics> = match settings.facade.as_ref() {
            Some(facade) => Arc::new(IssuerDemographics::new(&facade.settings.base_url).map_err(
                |source| crate::config::error::Error::DemographicsIssuer {
                    key: String::from("facade.base_url"),
                    source: Box::new(source),
                },
            )?),
            None => Arc::new(NoDemographics),
        };
        let operations = programs.map(|programs| {
            Arc::new(
                OperationsLane::new(programs, settings.operations.device_reference.as_str())
                    .with_composition_defaults(
                        settings.operations.composer.as_str(),
                        settings.operations.composition_language.clone(),
                        settings.operations.composition_territory.clone(),
                    )
                    .with_translator(
                        Translator::new(terminology.clone())
                            .with_display_fallback(settings.display_fallback),
                    )
                    .with_demographics(Arc::clone(&demographics)),
            )
        });
        Ok(Self {
            health: Registry::new(indicators),
            lifecycle: Lifecycle::default(),
            logged_query_parameters: settings.telemetry.logged_query_parameters.clone(),
            facade: None,
            operations,
            operations_off: if settings.operations.enabled {
                OperationsOff::NoMappingSet
            } else {
                OperationsOff::Disabled
            },
            lanes: Vec::new(),
            hl7v2: None,
            etl_history: None,
            concept_maps: None,
            auth: None,
            usage: Arc::new(crate::usage_report::window()),
            operator: None,
            terminology,
            change_feed: None,
            access_log: None,
        })
    }

    /// Returns a state with `health` as its registry and no logged query
    /// parameter.
    #[must_use]
    pub fn with_health(health: Registry) -> Self {
        Self {
            health,
            lifecycle: Lifecycle::default(),
            logged_query_parameters: Vec::new(),
            facade: None,
            operations: None,
            operations_off: OperationsOff::default(),
            lanes: Vec::new(),
            hl7v2: None,
            etl_history: None,
            concept_maps: None,
            auth: None,
            usage: Arc::new(crate::usage_report::window()),
            operator: None,
            terminology: None,
            change_feed: None,
            access_log: None,
        }
    }

    /// Returns this state recording every facade access and every committed
    /// HL7 v2 message through `log`.
    #[must_use]
    pub fn with_access_log(mut self, log: Arc<crate::audit::AccessLog>) -> Self {
        self.access_log = Some(log);
        self
    }

    /// Returns the access-record lane, when it is on.
    #[must_use]
    pub const fn access_log(&self) -> Option<&Arc<crate::audit::AccessLog>> {
        self.access_log.as_ref()
    }

    /// Returns this state with `facade` mounted and its store probed.
    ///
    /// The identity-store indicator joins readiness with the facade, so a
    /// deployment that does not run the facade does not report on a store it
    /// has no use for.
    #[must_use]
    pub fn with_facade(mut self, facade: Arc<crate::facade::Facade>) -> Self {
        self.health = self.health.and(Arc::new(indicators::IdentityStore::new(
            facade.store_handle(),
        )));
        self.facade = Some(facade);
        self
    }

    /// Returns this state with the change-feed consumer `following` reported
    /// to readiness, degraded and never down while it cannot follow.
    #[must_use]
    pub fn with_following(mut self, following: crate::change_feed::health::Following) -> Self {
        self.health = self.health.and(Arc::new(following));
        self
    }

    /// Returns this state with the change-feed consumers `report` reported
    /// by the operator API.
    #[must_use]
    pub fn with_change_feed(mut self, report: crate::change_feed::report::Report) -> Self {
        self.change_feed = Some(Arc::new(report));
        self
    }

    /// Returns the change-feed consumers, when any runs.
    #[must_use]
    pub const fn change_feed(&self) -> Option<&Arc<crate::change_feed::report::Report>> {
        self.change_feed.as_ref()
    }

    /// Returns this state with the HL7 v2 listener probed and reported.
    #[must_use]
    pub fn with_hl7v2(mut self, listening: crate::hl7v2::Listening) -> Self {
        self.health = self
            .health
            .and(Arc::new(indicators::Hl7v2Listener::new(listening.clone())));
        self.hl7v2 = Some(listening);
        self
    }

    /// Returns this state answering the OMOP ETL run history `history` on
    /// the operator API.
    #[must_use]
    pub fn with_etl_history(mut self, history: Arc<crate::etl::history::Store>) -> Self {
        self.etl_history = Some(history);
        self
    }

    /// Returns the OMOP ETL run history, when the bridge has a CDM database.
    #[must_use]
    pub const fn etl_history(&self) -> Option<&Arc<crate::etl::history::Store>> {
        self.etl_history.as_ref()
    }

    /// Returns this state answering the HL7 v2 face's `ConceptMap`
    /// directory `directory` on the operator API.
    #[must_use]
    pub fn with_concept_maps(mut self, directory: Arc<crate::hl7v2::directory::Directory>) -> Self {
        self.concept_maps = Some(directory);
        self
    }

    /// Returns the HL7 v2 face's `ConceptMap` directory, when the face runs.
    #[must_use]
    pub const fn concept_maps(&self) -> Option<&Arc<crate::hl7v2::directory::Directory>> {
        self.concept_maps.as_ref()
    }

    /// Returns this state with `smart` checking the credential of every guarded
    /// request.
    #[must_use]
    pub fn with_auth(mut self, smart: Arc<crate::smart::Smart>) -> Self {
        self.auth = Some(smart);
        self
    }

    /// Returns this state with `operator` serving the operator API.
    #[must_use]
    pub fn with_operator(mut self, operator: Arc<crate::operator::Operator>) -> Self {
        self.operator = Some(operator);
        self
    }

    /// Returns the operator API, when it is mounted.
    #[must_use]
    pub const fn operator(&self) -> Option<&Arc<crate::operator::Operator>> {
        self.operator.as_ref()
    }

    /// Returns the resource server, when inbound authentication is on.
    #[must_use]
    pub const fn auth(&self) -> Option<&Arc<crate::smart::Smart>> {
        self.auth.as_ref()
    }

    /// Returns whether the HL7 v2 listener accepts, when the face runs.
    #[must_use]
    pub const fn hl7v2(&self) -> Option<&crate::hl7v2::Listening> {
        self.hl7v2.as_ref()
    }

    /// Returns this state with `lanes` as the lanes `GET /health/info`
    /// reports.
    #[must_use]
    pub fn with_lanes(mut self, lanes: Vec<crate::startup::Lane>) -> Self {
        self.lanes = lanes;
        self
    }

    /// Returns the document `GET /health/info` answers: the build facts, the
    /// pins and every lane, the HL7 v2 lane with its listen address and
    /// whether its listener accepts now.
    #[must_use]
    pub fn info(&self) -> ferrobridge_operator_api::health::Info {
        let lanes = self
            .lanes
            .iter()
            .map(|lane| ferrobridge_operator_api::health::Lane {
                name: lane.name.to_owned(),
                enabled: lane.enabled,
                listen: lane.listen.clone(),
                up: lane.listen.as_ref().map(|_| {
                    self.hl7v2
                        .as_ref()
                        .is_some_and(crate::hl7v2::Listening::is_up)
                }),
            })
            .collect();
        crate::build_info::info(lanes)
    }

    /// Returns the facade, when its lane is on.
    #[must_use]
    pub fn facade(&self) -> Option<&Arc<crate::facade::Facade>> {
        self.facade.as_ref()
    }

    /// Returns this state with `names` as the query parameters it may log.
    #[must_use]
    pub fn logging_query_parameters(mut self, names: Vec<String>) -> Self {
        self.logged_query_parameters = names;
        self
    }

    /// Returns this state with `lane` serving the FHIRconnect operations.
    #[must_use]
    pub fn serving_operations(mut self, lane: OperationsLane) -> Self {
        self.operations = Some(Arc::new(lane));
        self
    }

    /// Returns the FHIRconnect operations lane, when one is configured.
    #[must_use]
    pub fn operations(&self) -> Option<&OperationsLane> {
        self.operations.as_deref()
    }

    /// Returns why the operations are not served, which matters only while
    /// [`Self::operations`] is `None`.
    #[must_use]
    pub const fn operations_off(&self) -> OperationsOff {
        self.operations_off
    }

    /// Returns a shared handle on the operations lane, for the reload that
    /// swaps its programs.
    #[must_use]
    pub const fn operations_handle(&self) -> Option<&Arc<OperationsLane>> {
        self.operations.as_ref()
    }

    /// Returns the client of the `[terminology]` server, `None` when none is
    /// configured.
    #[must_use]
    pub const fn terminology(&self) -> Option<&ferrobridge_term::client::Client> {
        self.terminology.as_ref()
    }

    /// Returns the indicators readiness runs.
    #[must_use]
    pub const fn health(&self) -> &Registry {
        &self.health
    }

    /// Returns the lifecycle the health routes and the drain share.
    #[must_use]
    pub const fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    /// Returns the query parameter names whose values may reach the log.
    #[must_use]
    pub fn logged_query_parameters(&self) -> &[String] {
        &self.logged_query_parameters
    }

    /// Returns the lanes the boot summary reported.
    #[must_use]
    pub fn lanes(&self) -> &[crate::startup::Lane] {
        &self.lanes
    }

    /// Returns the window the request log records the usage report's
    /// request counts into.
    #[must_use]
    pub const fn usage_window(&self) -> &Arc<ferrohealth_usage_report::window::Window> {
        &self.usage
    }
}

#[cfg(test)]
mod tests {
    use super::AppState;
    use crate::hl7v2::Listening;
    use crate::startup::Lane;
    use ferrohealth_health::registry::Registry;

    fn lanes() -> Vec<Lane> {
        vec![
            Lane {
                name: "facade",
                enabled: true,
                ..Lane::default()
            },
            Lane {
                name: "hl7v2",
                enabled: true,
                listen: Some(String::from("127.0.0.1:2575")),
                ..Lane::default()
            },
        ]
    }

    #[test]
    fn the_info_names_every_lane_and_the_hl7v2_listener_state() {
        let state = AppState::with_health(Registry::default())
            .with_hl7v2(Listening::default())
            .with_lanes(lanes());
        let document = serde_json::to_value(state.info()).expect("the info serializes");
        let lanes = document["lanes"].as_array().expect("a lanes array");
        assert_eq!(2, lanes.len());
        assert_eq!(Some("facade"), lanes[0]["name"].as_str());
        assert!(lanes[0].get("listen").is_none(), "a lane with no listener");
        assert_eq!(Some("hl7v2"), lanes[1]["name"].as_str());
        assert_eq!(Some("127.0.0.1:2575"), lanes[1]["listen"].as_str());
        assert_eq!(Some(false), lanes[1]["up"].as_bool(), "not accepting yet");
    }

    #[test]
    fn a_listener_that_accepts_reads_up_on_the_info() {
        let listening = Listening::default();
        let state = AppState::with_health(Registry::default())
            .with_hl7v2(listening.clone())
            .with_lanes(lanes());
        listening.set(true);
        let info = state.info();
        let lane = info.lanes.iter().find(|lane| lane.name == "hl7v2");
        assert_eq!(Some(Some(true)), lane.map(|lane| lane.up));
    }
}
