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

use crate::config::{self, ServerSettings, Settings};
use crate::serve_until;
use crate::state::{self, AppState};
use crate::terminology::Translator;
use crate::{auth, cdr, check, facade, hl7v2, operator, startup};
use std::collections::BTreeMap;

/// Every lane of one start, built and ready to serve.
pub(crate) struct Started {
    /// The state the router is built over, with no lanes recorded yet.
    pub(crate) state: AppState,
    /// The lanes the boot summary reports.
    pub(crate) lanes: Vec<startup::Lane>,
    /// The HL7 v2 face, when it runs.
    pub(crate) face: Option<StartedFace>,
}

impl Started {
    /// Logs the boot summary and the indicators readiness runs.
    pub(crate) fn announce(&self) {
        startup::log(&self.lanes);
        tracing::info!(
            version = state::VERSION,
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
        let authority = auth::Authority::discover(auth)
            .await
            .context("reading the authorization server")?;
        state = state.with_auth(Arc::new(authority));
    } else {
        tracing::warn!(
            "[auth] is off: the FHIR facade authenticates no caller and serves every request"
        );
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
    }
    if let Some(operations) = state.operations() {
        set_counts(&mut lanes, "operations", operation_counts(operations));
    }
    let mut face = None;
    if let Some(lane) = settings.hl7v2.as_ref() {
        let (started, listening) = start_hl7v2(settings, lane, &state)
            .await
            .context("starting the HL7 v2 face")?;
        state = state.with_hl7v2(listening);
        face = Some(started);
    }
    Ok(Started { state, lanes, face })
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
                .with_usage(Arc::clone(state.usage_window())),
        ),
        listener: hl7v2::Listener::of(lane, settings.server.shutdown_timeout),
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
    server: &ServerSettings,
    face: StartedFace,
    stop: &tokio::sync::watch::Sender<bool>,
) -> anyhow::Result<()> {
    use anyhow::Context;

    let http = async {
        let ended = serve_until(listener, app, server.shutdown_timeout, stopped(stop)).await;
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
    operator_settings: &operator::OperatorSettings,
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
        .with_translator(Translator::new(terminology));
    Ok((
        built
            .with_search(search, lane.search)
            .with_references(lane.references.clone()),
        counts,
    ))
}
