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

use crate::config::{self, Settings};
use crate::serve_until;
use crate::state::AppState;
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
    state = open_sources(settings, state)?;
    let mut lanes = startup::lanes(settings);
    if let Some(lane) = settings.facade.as_ref() {
        let (mounted, counts) =
            build_facade(settings, lane, state.terminology().cloned(), state.store())
                .await
                .context("starting the FHIR facade")?;
        let mounted = mounted.with_usage(Arc::clone(state.usage_window()));
        state = state.with_facade(Arc::new(mounted));
        set_counts(&mut lanes, "facade", counts);
    }
    if let (Some(audit), Some(mounted)) = (settings.audit.as_ref(), state.facade()) {
        let log = start_audit(audit, mounted, &state).await?;
        state = state.with_access_log(Arc::new(log));
    }
    state = start_links(settings, state)?;
    if settings.operator.is_some() {
        let operator = build_operator(
            settings,
            state.store(),
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
        let started = start_hl7v2(settings, lane, &state)
            .await
            .context("starting the HL7 v2 face")?;
        state = state.with_concept_maps(Arc::clone(started.face.directory()));
        if let Some((_, listening)) = started.tls.as_ref() {
            state = state.with_hl7v2(listening.clone());
        }
        if let Some(plain) = started.plain.as_ref() {
            state = state.with_hl7v2_plain(plain.listening.clone());
        }
        face = Some(started);
    }
    let mut feeds = Vec::new();
    if let Some(feed_settings) = settings.change_feed.as_ref() {
        let (started, report) = start_change_feed(settings, feed_settings, &state)
            .await
            .context("starting the change-feed consumers")?;
        for consumer in &started {
            state = state.with_following(consumer.lane.following.clone());
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

/// Returns `state` serving the inbound sources of its state store, the
/// `[secrets]` directory they name opened.
fn open_sources(settings: &Settings, state: AppState) -> anyhow::Result<AppState> {
    use anyhow::Context;

    let Some(store) = state.store() else {
        return Ok(state);
    };
    let sources = Arc::new(
        crate::sources::Sources::open(store, settings.secrets.as_ref())
            .context("opening the [secrets] directory the inbound sources name")?,
    );
    let intake = settings.inbound.map(|inbound| {
        // TODO(#2048): start the worker pool with the identity-feed handler.
        crate::inbound::intake::Intake::new(
            Arc::clone(&sources),
            crate::inbound::store::InboxStore::new(store.pool().clone()),
            inbound.body_limit,
        )
        .with_recoverer(Arc::new(crate::inbound::recover::Recoverer::new(
            store.clone(),
            Arc::clone(&sources),
            inbound.workers.lease,
        )))
    });
    let state = state.with_sources(sources);
    Ok(match intake {
        Some(intake) => state.with_intake(Arc::new(intake)),
        None => state,
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
    /// The lease of the reader's lane.
    lease: crate::store::lease::Lease,
    /// How the follower paces, parks and reports.
    lane: change_feed::follow::Lane,
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
                Arc::new(self.lease),
                self.lane,
                stopped,
            )
            .await;
        })
    }
}

/// Spawns every change-feed follower, the inbound recovery and the link
/// sweeper of `state`, each on its own task until `stop` reads `true`.
pub(crate) fn spawn_followers(
    feeds: Vec<StartedFeed>,
    state: &AppState,
    settings: &Settings,
    stop: &tokio::sync::watch::Sender<bool>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut followers: Vec<_> = feeds.into_iter().map(|feed| feed.spawn(stop)).collect();
    if let (Some(recoverer), Some(inbound)) = (
        state.intake().and_then(|intake| intake.recoverer()),
        settings.inbound,
    ) {
        followers.push(recoverer.spawn(inbound.recovery_tick, &stop.subscribe()));
    }
    if let Some(links) = state.links() {
        followers.push(links.spawn_sweeper(&stop.subscribe()));
    }
    followers
}

/// One change-feed consumer section that is on, before it starts.
#[derive(Debug, Clone, Copy)]
enum FeedConsumer<'a> {
    /// The events lane.
    Events(&'a events::Settings),
    /// The incremental OMOP load.
    OmopIncremental(&'a config::OmopIncrementalSettings),
    /// The outbound FHIR lane.
    FhirOutbound(&'a crate::outbound::Settings),
}

impl<'a> FeedConsumer<'a> {
    /// Returns every consumer section `settings` turns on, in the order they
    /// start.
    fn all(settings: &'a Settings) -> Vec<Self> {
        [
            settings.events.as_ref().map(Self::Events),
            settings
                .omop_incremental
                .as_ref()
                .map(Self::OmopIncremental),
            settings.fhir_outbound.as_ref().map(Self::FhirOutbound),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Returns the consumer's name, the reader it follows and its section.
    fn named(self) -> (&'static str, &'a change_feed::ReaderName, &'static str) {
        match self {
            Self::Events(lane) => (events::NAME, &lane.reader, "events"),
            Self::OmopIncremental(load) => (
                crate::etl::incremental::NAME,
                &load.reader,
                "omop_incremental",
            ),
            Self::FhirOutbound(lane) => (crate::outbound::NAME, &lane.reader, "fhir_outbound"),
        }
    }

    /// Starts the consumer, reporting where it stands on `following`.
    async fn start(
        self,
        settings: &Settings,
        state: &AppState,
        client: &cdr::CdrClient,
        following: change_feed::health::Following,
    ) -> anyhow::Result<Arc<dyn change_feed::follow::Consumer>> {
        use anyhow::Context;

        Ok(match self {
            Self::Events(lane) => {
                tracing::info!(
                    lane = events::NAME,
                    reader = %lane.reader,
                    exchange = %lane.exchange,
                    queues = lane.queues.len(),
                    "the events lane publishes the change feed to the AMQP broker"
                );
                Arc::new(
                    events::Publisher::new(Arc::new(events::amqp::AmqpBroker::new(lane.clone())))
                        .with_usage(Arc::clone(state.usage_window()))
                        .with_following(following),
                )
            }
            Self::OmopIncremental(load) => {
                let consumer = crate::etl::incremental::Incremental::start(
                    settings,
                    state.store(),
                    client.clone(),
                )
                .await
                .context("starting the incremental OMOP load")?
                .with_following(following);
                tracing::info!(
                    lane = crate::etl::incremental::NAME,
                    reader = %load.reader,
                    "the incremental OMOP load commits each changed composition to the CDM database"
                );
                Arc::new(consumer)
            }
            Self::FhirOutbound(lane) => Arc::new(start_outbound(lane, state, client, following)?),
        })
    }
}

/// Opens the change-feed source over the state store once, and starts every
/// consumer section that is on over it, each under its own reader and lane
/// lease.
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
    let store = state.store().cloned().context(
        "the change feed keeps its cursors and dead letters in the state store, and [state] is not set",
    )?;
    let feed = change_feed::open(feed_settings, client.clone(), &store);
    let dead_letters = change_feed::dead_letter::DeadLetters::new(store.pool().clone());
    let parking = change_feed::follow::Parking {
        store: dead_letters.clone(),
        max_attempts: feed_settings.max_attempts,
    };
    let mut report = change_feed::report::Report::new(feed_settings.source.clone(), dead_letters);
    let mut started = Vec::new();
    for consumer in FeedConsumer::all(settings) {
        let (name, reader, section) = consumer.named();
        let following = follower(&feed, feed_settings, reader, name, section).await?;
        let (lease, lane) =
            follower_lane(&store, feed_settings, &parking, reader, following.clone())?;
        let started_consumer = consumer
            .start(settings, state, &client, following.clone())
            .await?;
        report = report.with_follower(change_feed::report::Follower {
            name,
            reader: reader.clone(),
            following,
        });
        if let FeedConsumer::Events(lane) = consumer {
            report = report.with_topology(lane.topology());
        }
        started.push(StartedFeed {
            feed: Arc::clone(&feed),
            reader: reader.clone(),
            consumer: started_consumer,
            lease,
            lane,
        });
    }
    Ok((started, report))
}

/// Returns the lease of `reader`'s lane on `store` and how its follower
/// paces, parks and reports.
fn follower_lane(
    store: &crate::store::Store,
    feed_settings: &change_feed::Settings,
    parking: &change_feed::follow::Parking,
    reader: &change_feed::ReaderName,
    following: change_feed::health::Following,
) -> anyhow::Result<(crate::store::lease::Lease, change_feed::follow::Lane)> {
    let name = crate::store::lease::Lane::new(format!("change-feed:{reader}"))?;
    Ok((
        store.lease(name, feed_settings.lease),
        change_feed::follow::Lane {
            polling: feed_settings.polling,
            parking: Some(parking.clone()),
            following,
        },
    ))
}

/// Starts the outbound FHIR lane, mapping through the facade of `state`, and
/// warns that it delivers identifiable clinical data.
fn start_outbound(
    lane: &crate::outbound::Settings,
    state: &AppState,
    client: &cdr::CdrClient,
    following: change_feed::health::Following,
) -> anyhow::Result<crate::outbound::Outbound> {
    use anyhow::Context;

    let facade = state
        .facade()
        .cloned()
        .context("the outbound FHIR lane maps through the facade, and the facade is off")?;
    let store = state.store().context(
        "the outbound FHIR lane keeps its delivery record in the state store, and [state] is not set",
    )?;
    let consumer = crate::outbound::Outbound::start(lane, facade, client.clone(), store)
        .with_following(following);
    tracing::warn!(
        lane = crate::outbound::NAME,
        reader = %lane.reader,
        endpoint = startup::host_of(lane.sink.base()).unwrap_or_default(),
        "the outbound FHIR lane delivers identifiable clinical data to this endpoint"
    );
    Ok(consumer)
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
    /// The bound TLS socket and whether it accepts, for the readiness probe,
    /// when a sender sends over TLS.
    tls: Option<(TcpListener, hl7v2::Listening)>,
    /// The face its frames are answered by.
    face: Arc<hl7v2::Face>,
    /// How the listeners run.
    listener: hl7v2::Listener,
    /// The plain listener for the devices that cannot speak TLS, when one is
    /// configured.
    plain: Option<hl7v2::plain::StartedPlain>,
}

/// Binds the HL7 v2 face's listeners: the TLS one when a sender sends over
/// TLS, and the plain one when it is configured.
///
/// A port that cannot be bound refuses the start.
async fn start_hl7v2(
    settings: &Settings,
    lane: &config::Hl7v2Settings,
    state: &AppState,
) -> anyhow::Result<StartedFace> {
    use anyhow::Context;

    let facade = state
        .facade()
        .cloned()
        .context("the HL7 v2 face writes through the facade, and the facade is off")?;
    let maps = ferrobridge_hl7v2::map::maps::Maps::corrected()
        .maps()
        .count();
    let terminology = state.terminology().cloned();
    let tls = match lane.tls.as_ref() {
        Some(_) => {
            let socket = TcpListener::bind(lane.listen)
                .await
                .with_context(|| format!("binding {}", lane.listen))?;
            tracing::info!(
                lane = "hl7v2",
                listen = %lane.listen,
                concept_maps = maps,
                "the HL7 v2 face listens for MLLP over TLS"
            );
            Some((socket, hl7v2::Listening::default()))
        }
        None => None,
    };
    let routes: Arc<dyn crate::sources::Routes> = state.sources().cloned().context(
        "the HL7 v2 face routes its senders from the state store, and [state] is not set",
    )?;
    let face = hl7v2::Face::new(facade, terminology, routes)
        .with_usage(Arc::clone(state.usage_window()))
        .with_access_log(state.access_log().cloned(), lane.listen);
    let plain = match lane.plain {
        Some(bind) => Some(hl7v2::plain::bind(bind, &face).await?),
        None => None,
    };
    Ok(StartedFace {
        tls,
        face: Arc::new(face),
        listener: hl7v2::Listener::of(lane, settings.server.shutdown_timeout.get()),
        plain,
    })
}

/// Serves HTTP and the HL7 v2 face, with its plain listener when one is
/// configured, until `stop` reads `true`, each draining within the same bound.
///
/// The one signal stops them all, and a listener that fails stops the others, so
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
    let StartedFace {
        tls,
        face,
        listener: mllp_listener,
        plain,
    } = face;
    let plain_listener = mllp_listener.clone();
    let mllp = async {
        let Some((socket, listening)) = tls else {
            return Ok(());
        };
        let ended = hl7v2::serve_until(socket, face, mllp_listener, listening, stopped(stop)).await;
        stop.send_replace(true);
        ended
    };
    let plain = async {
        let Some(plain) = plain else {
            return Ok(());
        };
        let ended = hl7v2::serve_until(
            plain.socket,
            plain.face,
            plain_listener,
            plain.listening,
            stopped(stop),
        )
        .await;
        stop.send_replace(true);
        ended
    };
    let (http, mllp, plain) = tokio::join!(http, mllp, plain);
    http.context("serving HTTP")?;
    mllp.context("serving MLLP")?;
    plain.context("serving plain MLLP")?;
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
fn operation_counts(lane: &crate::operations::lane::OperationsLane) -> startup::MappingCounts {
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

/// Opens the access-record lane `audit` describes for the facade `mounted`,
/// its outbox and its category register in the state store of `state`.
async fn start_audit(
    audit: &crate::audit::Settings,
    mounted: &facade::Facade,
    state: &AppState,
) -> anyhow::Result<crate::audit::AccessLog> {
    use anyhow::Context;

    let base = url::Url::parse(&mounted.settings().base_url)
        .context("reading [facade] base_url as the address the access records name")?;
    let store = state.store().context(
        "the access-record lane keeps its outbox in the state store, and [state] is not set",
    )?;
    let registers = state.registers().cloned().context(
        "the access records are classified by a settings register, and [state] is not set",
    )?;
    let log = crate::audit::AccessLog::open(
        audit,
        (&base, mounted.client().base_url()),
        store,
        registers,
    )
    .await
    .context("reading the audit-categories register the access records are classified by")?;
    tracing::info!(
        lane = "audit",
        repository = %startup::host_of(audit.feed.endpoint()).unwrap_or_default(),
        "the facade records every access in the audit trail"
    );
    Ok(log)
}

/// Builds the operator API over the configured mapping directories, with the
/// running load as the checker sees it.
///
/// With the state store, the running revision's console files join the
/// provisioned ones here and the start is refused when they do not compile;
/// without it, the check runs once over the inputs `mapping check` reads.
/// The search declaration of each context comes from the facade when it
/// runs (no specification governs the operator surface: our own design).
async fn build_operator(
    settings: &Settings,
    store: Option<&crate::store::Store>,
    facade: Option<Arc<facade::Facade>>,
    operations: Option<Arc<crate::operations::lane::OperationsLane>>,
) -> anyhow::Result<operator::Operator> {
    use anyhow::Context;

    let needs_cdr =
        settings.mappings.is_none() && (settings.mapping_directory.is_some() || store.is_some());
    let cdr = match settings.cdr.as_ref() {
        Some(config) if needs_cdr => Some(
            cdr::CdrClient::new(config)
                .context("building the CDR client the templates are fetched through")?,
        ),
        _ => None,
    };
    let mut roots = Vec::new();
    if let Some(directory) = settings.mapping_directory.as_ref() {
        roots.push(crate::mappings::files::Root::new(
            crate::mappings::files::Language::Fhirconnect,
            directory.clone(),
        ));
    }
    if let Some(directory) = settings.omocl_directory.as_ref() {
        roots.push(crate::mappings::files::Root::new(
            crate::mappings::files::Language::Omocl,
            directory.clone(),
        ));
    }
    let searches = facade.as_ref().map_or_else(BTreeMap::new, |facade| {
        let mappings = facade.mappings();
        operator::reload::searches(mappings.programs(), mappings.search())
    });
    let built = if let Some(store) = store {
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
            structuremaps: Some(Arc::default()),
        };
        let revisions = crate::mappings::revisions::Revisions::new(store);
        revisions
            .baseline()
            .await
            .context("recording the baseline revision of the mapping set")?;
        let operator = Arc::new(
            operator::Operator::new(
                roots,
                operator::snapshot::Snapshot::new(check::Report::default(), searches),
            )
            .with_saving(revisions, reloader),
        );
        operator
            .converge(operator::watch::When::Always)
            .await
            .context("loading the running revision of the console's mapping files")?;
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
        saving = store.is_some(),
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
/// identity map is opened here too, so a store without `[state.seal]` or of
/// another derivation is a boot error.
async fn build_facade(
    settings: &Settings,
    lane: &config::FacadeSettings,
    terminology: Option<ferrobridge_term::client::Client>,
    state: Option<&crate::store::Store>,
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
    let owned: Vec<&str> = [
        (lane.app_state, facade::app_state::BASIC),
        (lane.launch_task, facade::launch_task::TASK),
        (lane.endpoints.is_some(), facade::endpoint::ENDPOINT),
        (lane.mhd, facade::mhd::DOCUMENT_REFERENCE),
        (lane.mhd, facade::mhd::LIST),
    ]
    .into_iter()
    .filter_map(|(on, resource_type)| on.then_some(resource_type))
    .collect();
    facade::programs::check_owned(&programs, &owned)
        .context("checking the contexts against [facade.app_state], [facade.launch_task], [facade.endpoint] and [facade.mhd]")?;
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
    let state = state.context(
        "the facade needs a [state] section: every replica resolves its identities in the state store",
    )?;
    let store = facade::identity::state::StateStore::open(state)
        .await
        .context("opening the identity map in the state store")?;
    let owned = |on: bool| on.then(|| Arc::new(facade::owned::OwnedStore::open(state)));
    let app_state = owned(lane.app_state);
    let launch_tasks = owned(lane.launch_task);
    let summary = match lane.summary.as_ref() {
        Some(summary) => Some(
            build_summary(summary, &programs, &client)
                .await
                .context("building the patient summary of [facade.summary]")?,
        ),
        None => None,
    };
    let built = facade::Facade::new(programs, Arc::new(store), client, lane.settings.clone())
        .with_translator(
            Translator::new(terminology).with_display_fallback(settings.display_fallback),
        );
    Ok((
        built
            .with_search(search, lane.search)
            .with_references(lane.references.clone())
            .with_identifier_search(lane.identifier.clone())
            .with_party_search(lane.party_search.clone())
            .with_binding_issuers(lane.binding_issuers.clone())
            .with_consent(lane.consent.clone())
            .with_app_state(app_state)
            .with_launch_tasks(launch_tasks)
            .with_summary(summary)
            .with_endpoints(lane.endpoints.clone())
            .with_mhd(lane.mhd)
            .with_cors(lane.cors.clone()),
        counts,
    ))
}

/// Builds the patient summary `summary` configures: the EPS project compiled
/// against the patient summary template the CDR serves, over the profile
/// packages read once at boot, so a template or a package set that cannot
/// serve refuses the start.
async fn build_summary(
    summary: &config::SummarySettings,
    programs: &facade::programs::Programs,
    client: &cdr::CdrClient,
) -> anyhow::Result<facade::summary::Summary> {
    use anyhow::Context;

    if programs.party(facade::summary::PATIENT).is_none() {
        anyhow::bail!(
            "the patient summary reads its patient through a Patient party context, and the mapping set loads none"
        );
    }
    let id = facade::summary::Summary::template_id();
    let template_id =
        cdr::ids::template_id(id).with_context(|| format!("reading {id} as a template id"))?;
    let template = match client
        .template(&template_id)
        .await
        .with_context(|| format!("fetching the template {id}"))?
    {
        cdr::template::TemplateOutcome::Found(template) => template,
        cdr::template::TemplateOutcome::UnknownTemplate => {
            anyhow::bail!("the CDR holds no template {id}")
        }
        cdr::template::TemplateOutcome::BadRequest(upstream) => {
            anyhow::bail!(
                "the CDR refused the template {id} with {}",
                upstream.status()
            )
        }
    };
    let paths = summary.packages.clone();
    let packages =
        tokio::task::spawn_blocking(move || ferrobridge_validator::set::PackageSet::load(paths))
            .await
            .context("reading the profile packages")?
            .context("reading the profile packages of [facade.summary] packages")?;
    facade::summary::Summary::new(&template, packages, summary.operator.clone())
        .with_context(|| format!("compiling the patient summary project against {id}"))
}

/// Returns `state` serving the person's SMART Health Links when `[shl]` is
/// on, with the sweep of its store started.
fn start_links(settings: &Settings, state: AppState) -> anyhow::Result<AppState> {
    use anyhow::Context;

    let (Some(shl), Some(mounted)) = (settings.shl.as_ref(), state.facade()) else {
        return Ok(state);
    };
    let store = state.store().context(
        "the links need a [state] section: every replica serves them from the state store",
    )?;
    let links = crate::shl::Links::open(
        store,
        shl.clone(),
        Arc::clone(mounted),
        state.access_log().cloned(),
    )
    .context("opening the links in the state store")?;
    let links = Arc::new(links);
    tracing::info!(
        lane = "shl",
        "the person's SMART Health Links are served under /shl"
    );
    Ok(state.with_links(links))
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
