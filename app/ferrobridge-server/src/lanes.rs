// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Starting every lane `serve` runs: the upstream clients, the FHIR facade
//! with its mapping set and identity store, the HL7 v2 face, and the boot
//! summary they report.
//!
//! [`crate::boot`] repeats [`start`] while the CDR is not ready, and serves
//! the started lanes through [`serve_with_face`] when the HL7 v2 face runs.

use std::future::Future;
use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;

use crate::config::{self, OperatorSettings, Settings};
use crate::serve_until;
use crate::state::{self, AppState};
use crate::terminology::Translator;
use crate::{cdr, change_feed, check, events, facade, hl7v2, operator, smart, startup};
use ferrohealth_health::lifecycle::Lifecycle;
use ferrohealth_server::section::ServerSection;
use std::collections::BTreeMap;

/// Every lane of one start, built and ready to serve.
pub(crate) struct Started {
    /// The state the router is built over, with no lanes recorded yet.
    pub(crate) state: AppState,
    /// The lanes the boot summary reports.
    pub(crate) lanes: Vec<startup::Lane>,
    /// The HL7 v2 face, when it runs.
    pub(crate) face: Option<StartedFace>,
    /// The change-feed consumers that run.
    pub(crate) feeds: Vec<StartedFeed>,
}

impl Started {
    /// Logs the boot summary and the indicators readiness runs.
    pub(crate) fn announce(&self) {
        startup::log(&self.lanes);
        tracing::info!(
            version = crate::build_info::VERSION,
            indicators = self.state.health().names().join(","),
            "ferrobridge starting"
        );
    }
}

/// Builds the upstream clients and every lane `settings` configures.
///
/// The operations lane and the facade fetch the templates their mappings
/// name from the CDR here, so this is the step [`crate::boot::retry`] repeats.
pub(crate) async fn start(settings: &Settings) -> anyhow::Result<Started> {
    use anyhow::Context;

    let mut state = AppState::build(settings)
        .await
        .context("building the upstream clients")?;
    if let Some(auth) = settings.auth.clone() {
        let smart = smart::Smart::discover(auth)
            .await
            .context("reading the authorization server")?;
        state = state.with_auth(Arc::new(smart));
    } else {
        let open = guarded_surfaces(
            settings.facade.is_some(),
            settings.operator.is_some(),
            settings.operations.enabled && settings.mapping_directory.is_some(),
        );
        if !open.is_empty() {
            tracing::warn!(
                surfaces = open.join(","),
                "[auth] is off: these surfaces authenticate no caller and serve every request"
            );
        }
    }
    let mut lanes = startup::lanes(settings);
    if let Some(lane) = settings.facade.as_ref() {
        let (mounted, counts) = build_facade(settings, lane, state.terminology().cloned())
            .await
            .context("starting the FHIR facade")?;
        let mounted = mounted.with_usage(Arc::clone(state.usage_window()));
        state = state.with_facade(Arc::new(mounted));
        set_counts(&mut lanes, "facade", counts);
    }
    if let (Some(audit), Some(mounted)) = (settings.audit.as_ref(), state.facade()) {
        let base = url::Url::parse(&mounted.settings().base_url)
            .context("reading [facade] base_url as the address the access records name")?;
        let log = crate::audit::AccessLog::open(audit, &base, mounted.client().base_url())
            .context("opening the access-record spool")?;
        tracing::info!(
            lane = "audit",
            repository = %startup::host_of(audit.feed.endpoint()).unwrap_or_default(),
            "the facade records every access in the audit trail"
        );
        state = state.with_access_log(Arc::new(log));
    }
    if let Some(operator) = settings.operator.as_ref() {
        let operator = build_operator(
            settings,
            operator,
            state.facade().cloned(),
            state.operations_handle().cloned(),
        )
        .await
        .context("starting the operator API")?;
        state = state.with_operator(Arc::new(operator));
        if let Some(cdm) = settings.cdm.as_ref() {
            let history = crate::etl::history::Store::new(cdm)
                .context("preparing the connection the run history is read through")?;
            state = state.with_etl_history(Arc::new(history));
        }
    }
    if let Some(operations) = state.operations() {
        set_counts(&mut lanes, "operations", operation_counts(operations));
    }
    let mut face = None;
    if let Some(lane) = settings.hl7v2.as_ref() {
        let (started, listening) = start_hl7v2(settings, lane, &state)
            .await
            .context("starting the HL7 v2 face")?;
        state = state
            .with_hl7v2(listening)
            .with_concept_maps(Arc::clone(started.face.directory()));
        face = Some(started);
    }
    let mut feeds = Vec::new();
    if let Some(feed_settings) = settings.change_feed.as_ref() {
        let (started, report) = start_change_feed(settings, feed_settings, &state)
            .await
            .context("starting the change-feed consumers")?;
        for consumer in &started {
            state = state.with_following(consumer.following.clone());
        }
        state = state.with_change_feed(report);
        feeds = started;
    }
    Ok(Started {
        state,
        lanes,
        face,
        feeds,
    })
}

/// One change-feed consumer, ready to follow its source.
pub(crate) struct StartedFeed {
    /// The source.
    feed: Arc<dyn change_feed::ChangeFeed>,
    /// The reader the consumer follows the source under.
    reader: change_feed::ReaderName,
    /// The consumer.
    consumer: Arc<dyn change_feed::follow::Consumer>,
    /// How the follower paces its reads.
    polling: change_feed::follow::Polling,
    /// Where a poisoned event is parked.
    parking: change_feed::follow::Parking,
    /// What readiness reports for the consumer.
    following: change_feed::health::Following,
}

impl StartedFeed {
    /// Follows the source on its own task until `stop` reads `true`.
    pub(crate) fn spawn(
        self,
        stop: &tokio::sync::watch::Sender<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let stopped = stop.subscribe();
        tokio::spawn(async move {
            change_feed::follow::follow(
                self.feed.as_ref(),
                &self.reader,
                self.consumer.as_ref(),
                self.polling,
                Some(self.parking),
                &self.following,
                stopped,
            )
            .await;
        })
    }
}

/// Opens the change-feed source and its dead-letter store once, and starts
/// every consumer section that is on over them, each under its own reader.
async fn start_change_feed(
    settings: &Settings,
    feed_settings: &change_feed::Settings,
    state: &AppState,
) -> anyhow::Result<(Vec<StartedFeed>, change_feed::report::Report)> {
    use anyhow::Context;

    let cdr = settings
        .cdr
        .as_ref()
        .context("the change feed reads the CDR, and [cdr] is not set")?;
    let client = cdr::CdrClient::new(cdr).context("building the CDR client the feed reads")?;
    let feed =
        change_feed::open(feed_settings, client.clone()).context("opening the change feed")?;
    let dead_letters = Arc::new(
        change_feed::dead_letter::DeadLetters::open(&feed_settings.dead_letters)
            .context("opening the dead-letter store of the change feed")?,
    );
    let parking = change_feed::follow::Parking {
        store: Arc::clone(&dead_letters),
        max_attempts: feed_settings.max_attempts,
    };
    let mut report = change_feed::report::Report::new(feed_settings.source.clone(), dead_letters);
    let mut started = Vec::new();
    if let Some(lane) = settings.events.as_ref() {
        let following =
            follower(&feed, feed_settings, &lane.reader, events::NAME, "events").await?;
        let publisher =
            events::Publisher::new(Arc::new(events::amqp::AmqpBroker::new(lane.clone())))
                .with_usage(Arc::clone(state.usage_window()))
                .with_following(following.clone());
        tracing::info!(
            lane = events::NAME,
            reader = %lane.reader,
            exchange = %lane.exchange,
            queues = lane.queues.len(),
            "the events lane publishes the change feed to the AMQP broker"
        );
        report = report
            .with_follower(change_feed::report::Follower {
                name: events::NAME,
                reader: lane.reader.clone(),
                following: following.clone(),
            })
            .with_events(lane.clone());
        started.push(StartedFeed {
            feed: Arc::clone(&feed),
            reader: lane.reader.clone(),
            consumer: Arc::new(publisher),
            polling: feed_settings.polling,
            parking: parking.clone(),
            following,
        });
    }
    if let Some(load) = settings.omop_incremental.as_ref() {
        let name = crate::etl::incremental::NAME;
        let following =
            follower(&feed, feed_settings, &load.reader, name, "omop_incremental").await?;
        let consumer = crate::etl::incremental::Incremental::start(settings, client.clone())
            .await
            .context("starting the incremental OMOP load")?
            .with_following(following.clone());
        tracing::info!(
            lane = name,
            reader = %load.reader,
            "the incremental OMOP load commits each changed composition to the CDM database"
        );
        report = report.with_follower(change_feed::report::Follower {
            name,
            reader: load.reader.clone(),
            following: following.clone(),
        });
        started.push(StartedFeed {
            feed: Arc::clone(&feed),
            reader: load.reader.clone(),
            consumer: Arc::new(consumer),
            polling: feed_settings.polling,
            parking: parking.clone(),
            following,
        });
    }
    if let Some(lane) = settings.fhir_outbound.as_ref() {
        let outbound = start_outbound(lane, state, &feed, feed_settings, client, parking).await?;
        report = report.with_follower(change_feed::report::Follower {
            name: crate::outbound::NAME,
            reader: lane.reader.clone(),
            following: outbound.following.clone(),
        });
        started.push(outbound);
    }
    Ok((started, report))
}

/// Starts the outbound FHIR lane over `feed`, mapping through the facade of
/// `state`, and warns that it delivers identifiable clinical data.
async fn start_outbound(
    lane: &crate::outbound::Settings,
    state: &AppState,
    feed: &Arc<dyn change_feed::ChangeFeed>,
    feed_settings: &change_feed::Settings,
    client: cdr::CdrClient,
    parking: change_feed::follow::Parking,
) -> anyhow::Result<StartedFeed> {
    use anyhow::Context;

    let name = crate::outbound::NAME;
    let facade = state
        .facade()
        .cloned()
        .context("the outbound FHIR lane maps through the facade, and the facade is off")?;
    let following = follower(feed, feed_settings, &lane.reader, name, "fhir_outbound").await?;
    let consumer = crate::outbound::Outbound::start(lane, facade, client)
        .with_context(|| format!("opening {}", lane.emitted.display()))?
        .with_following(following.clone());
    tracing::warn!(
        lane = name,
        reader = %lane.reader,
        endpoint = startup::host_of(lane.sink.base()).unwrap_or_default(),
        "the outbound FHIR lane delivers identifiable clinical data to this endpoint"
    );
    Ok(StartedFeed {
        feed: Arc::clone(feed),
        reader: lane.reader.clone(),
        consumer: Arc::new(consumer),
        polling: feed_settings.polling,
        parking,
        following,
    })
}

/// Checks the reader `reader` of the consumer `name`, configured under
/// `[section]`, and returns its status with the positions kept for it.
///
/// Over the reference CDR's feed a first read checks the reader: a `404`
/// means the CDR serves no feed for it, and the start is refused. Any other
/// failure leaves the consumer to report degraded, since the events wait at
/// the source (no specification governs this: our own design).
async fn follower(
    feed: &Arc<dyn change_feed::ChangeFeed>,
    feed_settings: &change_feed::Settings,
    reader: &change_feed::ReaderName,
    name: &'static str,
    section: &str,
) -> anyhow::Result<change_feed::health::Following> {
    use anyhow::Context;

    if matches!(feed_settings.source, change_feed::Source::FerroEhr { .. }) {
        let probe = change_feed::PageSize::new(1).context("a page of one event")?;
        match feed.page(reader, probe).await {
            Err(error @ change_feed::FeedError::UnknownReader { .. }) => {
                return Err(error).with_context(|| {
                    format!(
                        "the CDR serves no outbox feed for [{section}] reader; name the reader in the CDR's configuration"
                    )
                });
            }
            Err(error) => tracing::warn!(
                lane = name,
                reason = %crate::indicators::reason(&error),
                "the change feed did not answer at boot; the consumer reports degraded until it does"
            ),
            Ok(_page) => {}
        }
    }
    let following = change_feed::health::Following::new(name);
    match feed.positions(reader).await {
        Ok(positions) => following.acknowledged(&positions),
        Err(error) => tracing::warn!(
            lane = name,
            reason = %crate::indicators::reason(&error),
            "the kept watermark could not be read; the position shows after the first pass"
        ),
    }
    Ok(following)
}

/// The HL7 v2 face, bound and ready to serve.
pub(crate) struct StartedFace {
    /// The bound MLLP socket.
    socket: TcpListener,
    /// The face its frames are answered by.
    face: Arc<hl7v2::Face>,
    /// How the listener runs.
    listener: hl7v2::Listener,
    /// Whether the listener accepts, for the readiness probe.
    listening: hl7v2::Listening,
}

/// Loads the HL7 v2 face's `ConceptMaps` and binds its listener.
///
/// A corpus that does not load and a port that cannot be bound both refuse
/// the start, as a facade mapping that does not compile does.
async fn start_hl7v2(
    settings: &Settings,
    lane: &config::Hl7v2Settings,
    state: &AppState,
) -> anyhow::Result<(StartedFace, hl7v2::Listening)> {
    use anyhow::Context;

    let facade = state
        .facade()
        .cloned()
        .context("the HL7 v2 face writes through the facade, and the facade is off")?;
    let corpus = hl7v2::load_corpus(lane).with_context(|| {
        format!(
            "loading the v2-to-FHIR ConceptMaps from {}",
            lane.concept_maps.display()
        )
    })?;
    let maps = corpus.maps().count();
    let terminology = state.terminology().cloned();
    let socket = TcpListener::bind(lane.listen)
        .await
        .with_context(|| format!("binding {}", lane.listen))?;
    tracing::info!(
        lane = "hl7v2",
        listen = %lane.listen,
        concept_maps = maps,
        profiles = lane.profiles.len(),
        "the HL7 v2 face listens for MLLP"
    );
    let listening = hl7v2::Listening::default();
    let face = StartedFace {
        socket,
        face: Arc::new(
            hl7v2::Face::new(facade, corpus, terminology, lane)
                .with_usage(Arc::clone(state.usage_window()))
                .with_access_log(state.access_log().cloned(), lane.listen),
        ),
        listener: hl7v2::Listener::of(lane, settings.server.shutdown_timeout()),
        listening: listening.clone(),
    };
    Ok((face, listening))
}

/// Serves HTTP and the HL7 v2 face until `stop` reads `true`, each draining
/// within the same bound.
///
/// The one signal stops both, and a listener that fails stops the other, so
/// the process never keeps half its surface.
pub(crate) async fn serve_with_face(
    listener: TcpListener,
    app: Router,
    server: &ServerSection,
    lifecycle: Lifecycle,
    face: StartedFace,
    stop: &tokio::sync::watch::Sender<bool>,
) -> anyhow::Result<()> {
    use anyhow::Context;

    let http = async {
        let ended = serve_until(listener, app, server, lifecycle, stopped(stop)).await;
        stop.send_replace(true);
        ended
    };
    let mllp = async {
        let ended = hl7v2::serve_until(
            face.socket,
            face.face,
            face.listener,
            face.listening,
            stopped(stop),
        )
        .await;
        stop.send_replace(true);
        ended
    };
    let (http, mllp) = tokio::join!(http, mllp);
    http.context("serving HTTP")?;
    mllp.context("serving MLLP")?;
    Ok(())
}

/// Completes once `stop` reads `true`.
pub(crate) fn stopped(
    stop: &tokio::sync::watch::Sender<bool>,
) -> impl Future<Output = ()> + Send + 'static {
    let mut receiver = stop.subscribe();
    async move {
        let _settled = receiver.wait_for(|stopped| *stopped).await.is_ok();
    }
}

/// Records `counts` on the lane called `name`.
fn set_counts(lanes: &mut [startup::Lane], name: &str, counts: startup::MappingCounts) {
    if let Some(lane) = lanes.iter_mut().find(|lane| lane.name == name) {
        lane.mappings = Some(counts);
    }
}

/// Returns what the operations lane compiled: one program per context, over
/// the distinct templates the programs name.
fn operation_counts(lane: &state::OperationsLane) -> startup::MappingCounts {
    let set = lane.programs();
    let programs = set.programs();
    let contexts: std::collections::BTreeSet<&str> = programs
        .iter()
        .map(|program| program.context().as_str())
        .collect();
    let templates: std::collections::BTreeSet<&str> = programs
        .iter()
        .map(|program| program.template().id().as_str())
        .collect();
    startup::MappingCounts {
        contexts: contexts.len(),
        programs: programs.len(),
        templates: templates.len(),
    }
}

/// Builds the operator API over the configured mapping directories, with the
/// running load as the checker sees it.
///
/// The check runs once here, over the inputs `mapping check` reads, and the
/// search declaration of each context comes from the facade when it runs
/// (no specification governs the operator surface: our own design).
async fn build_operator(
    settings: &Settings,
    operator_settings: &OperatorSettings,
    facade: Option<Arc<facade::Facade>>,
    operations: Option<Arc<state::OperationsLane>>,
) -> anyhow::Result<operator::Operator> {
    use anyhow::Context;

    let needs_cdr = settings.mappings.is_none()
        && (settings.mapping_directory.is_some() || operator_settings.mapping_dir.is_some());
    let cdr = match settings.cdr.as_ref() {
        Some(config) if needs_cdr => Some(
            cdr::CdrClient::new(config)
                .context("building the CDR client the templates are fetched through")?,
        ),
        _ => None,
    };
    let mut roots = Vec::new();
    if let Some(directory) = settings.mapping_directory.as_ref() {
        roots.push(operator::files::Root::new(
            operator::files::Language::Fhirconnect,
            directory.clone(),
        ));
    }
    if let Some(directory) = settings.omocl_directory.as_ref() {
        roots.push(operator::files::Root::new(
            operator::files::Language::Omocl,
            directory.clone(),
        ));
    }
    let searches = facade.as_ref().map_or_else(BTreeMap::new, |facade| {
        let mappings = facade.mappings();
        operator::reload::searches(mappings.programs(), mappings.search())
    });
    let built = if let Some(mapping_dir) = operator_settings.mapping_dir.clone() {
        let reloader = operator::reload::Reloader {
            fhirconnect: settings.mapping_directory.clone(),
            templates: settings
                .mappings
                .as_ref()
                .map(|mappings| mappings.templates.clone()),
            cdr,
            omocl: settings.omocl_directory.is_some(),
            facade,
            operations,
        };
        let operator = Arc::new(
            operator::Operator::new(
                roots,
                operator::snapshot::Snapshot::new(check::Report::default(), searches),
            )
            .with_saving(mapping_dir, reloader),
        );
        // NOTE: no specification governs this: our own design; the console's
        // saved files join the running set at boot, or the start is refused.
        let worker = Arc::clone(&operator);
        tokio::task::spawn_blocking(move || {
            worker.prepare(None).map(|prepared| worker.apply(prepared))
        })
        .await
        .context("loading the console's mapping files")?
        .context("loading the console's mapping files")?;
        let worker = Arc::clone(&operator);
        tokio::task::spawn_blocking(move || worker.settle(operator::save::on_disk()))
            .await
            .context("recording the console's mapping files as a revision")?
            .context("recording the console's mapping files as a revision")?;
        Arc::try_unwrap(operator).map_err(|_shared| {
            anyhow::anyhow!("the operator API is still shared after its boot load")
        })?
    } else {
        let report = match check::inputs(settings, cdr.as_ref()) {
            Ok(inputs) => check::check(inputs)
                .await
                .context("checking the mapping set")?,
            Err(check::Error::NothingConfigured) => check::Report::default(),
            Err(error) => return Err(error).context("reading the mapping inputs"),
        };
        operator::Operator::new(roots, operator::snapshot::Snapshot::new(report, searches))
    };
    let built = match settings.configuration.clone() {
        Some(configuration) => built.with_configuration(configuration),
        None => built,
    };
    tracing::info!(
        lane = "operator",
        roots = built.roots().len(),
        diagnostics = built.snapshot().report().diagnostics().len(),
        saving = operator_settings.mapping_dir.is_some(),
        "the operator API is mounted under /operator"
    );
    Ok(built)
}

/// Builds the FHIR facade this deployment mounts and returns it with what its
/// mapping set loaded.
///
/// The mapping set is read and compiled once, at boot, against the templates
/// the CDR holds, so a mapping that does not compile refuses the start rather
/// than the first request that touches it (no specification governs this: our own design). The
/// identity store is opened here too, so a path that cannot be written is a
/// boot error.
async fn build_facade(
    settings: &Settings,
    lane: &config::FacadeSettings,
    terminology: Option<ferrobridge_term::client::Client>,
) -> anyhow::Result<(facade::Facade, startup::MappingCounts)> {
    use anyhow::Context;

    let cdr = settings
        .cdr
        .as_ref()
        .context("the facade needs a [cdr] section: it maps every request onto CDR operations")?;
    let client =
        cdr::CdrClient::new(cdr).context("building the CDR client the facade calls through")?;
    let directory = settings
        .mapping_directory
        .as_ref()
        .context("the facade needs [mappings] directory: it runs the mappings it finds there")?;
    let set = facade::programs::read_set(directory).context("reading the mapping set")?;
    let templates = facade::programs::fetch_templates(&set, &client)
        .await
        .context("fetching the templates the mappings name")?;
    let programs =
        facade::programs::compile_set(&set, &templates).context("compiling the mapping set")?;
    facade::programs::check_parties(&programs, client.demographic().is_some())
        .context("checking the party contexts against [cdr] demographic")?;
    facade::programs::check_party_search(&programs, lane.party_search.as_ref())
        .context("checking [facade.party_search] against the party contexts")?;
    tracing::info!(
        lane = "facade",
        types = programs
            .resource_types()
            .iter()
            .cloned()
            .collect::<Vec<String>>()
            .join(","),
        "the FHIR facade serves these resource types"
    );
    let counts = startup::MappingCounts {
        contexts: set.contexts().count(),
        programs: programs.len(),
        templates: templates.len(),
    };
    let search = facade::search::load(directory, &programs).context("reading the searches")?;
    let store = facade::identity::redb_store::RedbStore::open(&lane.identity_store)
        .with_context(|| format!("opening {}", lane.identity_store.display()))?;
    let built = facade::Facade::new(programs, Arc::new(store), client, lane.settings.clone())
        .with_translator(
            Translator::new(terminology).with_display_fallback(settings.display_fallback),
        );
    Ok((
        built
            .with_search(search, lane.search)
            .with_references(lane.references.clone())
            .with_identifier_search(lane.identifier.clone())
            .with_party_search(lane.party_search.clone()),
        counts,
    ))
}

/// Returns the names of the surfaces `[auth]` guards that run: the facade,
/// the operator API, and the FHIRconnect operations over a mapping set.
///
/// A server with none of them serves only `/` and the health routes, so an
/// `[auth]` that is off leaves nothing open and warrants no warning (no
/// specification governs this: our own design).
fn guarded_surfaces(facade: bool, operator: bool, operations: bool) -> Vec<&'static str> {
    [
        (facade, "facade"),
        (operator, "operator"),
        (operations, "operations"),
    ]
    .into_iter()
    .filter_map(|(runs, name)| runs.then_some(name))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::guarded_surfaces;

    #[test]
    fn only_a_running_guarded_surface_is_named() {
        assert_eq!(guarded_surfaces(false, false, false), Vec::<&str>::new());
        assert_eq!(vec!["facade"], guarded_surfaces(true, false, false));
        assert_eq!(
            vec!["operator", "operations"],
            guarded_surfaces(false, true, true)
        );
    }
}
