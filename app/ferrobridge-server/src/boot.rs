// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Waiting for the CDR at boot: the start is retried with a bounded backoff
//! while the CDR does not answer, and the health routes are served meanwhile.
//!
//! The lanes fetch the templates their mappings name from the CDR before they
//! serve. The template operations document `200`, `400`, `404` and `406`
//! (`definition-codegen.openapi.yaml`, `definition_template_adl1.4_get`), and
//! each of those settles the start. A call that reaches no answer, or one the
//! generated client counts retryable (a `5xx` other than `501`), means the CDR
//! is not ready yet, so the start is tried again after a delay that doubles up
//! to a ceiling, and `/health/readiness` answers `503` with a `startup`
//! indicator saying why. No specification governs this: our own design.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Duration;

use axum::Router;
use axum::routing::get;
use http::StatusCode;
use tokio::net::TcpListener;

use crate::config::Settings;

use crate::health::{Check, HealthIndicator};
use ferrobridge_operator_api::health::IndicatorState;

/// How long the start waits between two attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    /// The wait after the first attempt that found the CDR not ready.
    pub initial: Duration,
    /// The longest wait between two attempts.
    pub max: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(30),
        }
    }
}

impl Backoff {
    /// Returns the wait that follows `delay`: twice as long, up to the ceiling.
    #[must_use]
    pub fn next(&self, delay: Duration) -> Duration {
        delay.saturating_mul(2).min(self.max)
    }
}

/// Where the start stands, shared between the attempts and the readiness
/// indicator.
#[derive(Debug, Clone, Default)]
pub struct Status {
    /// Why the start is not finished, or `None` once it is.
    waiting: Arc<Mutex<Option<String>>>,
}

impl Status {
    /// Returns a status that reads not ready until [`Status::ready`].
    #[must_use]
    pub fn starting() -> Self {
        Self {
            waiting: Arc::new(Mutex::new(Some(String::from(
                "the lanes are loading their mappings",
            )))),
        }
    }

    /// Records why the start is still waiting.
    pub fn waiting(&self, reason: String) {
        *self.slot() = Some(reason);
    }

    /// Records that the start finished.
    pub fn ready(&self) {
        *self.slot() = None;
    }

    /// Returns why the start is still waiting, or `None` once it finished.
    #[must_use]
    pub fn reason(&self) -> Option<String> {
        self.slot().clone()
    }

    /// Returns the guard of the shared slot.
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        // NOTE: no specification governs this: our own design; every write
        // replaces the slot whole, so a poisoned slot still holds a usable value.
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The readiness indicator of the start itself.
#[derive(Debug, Clone)]
pub struct Startup {
    /// The status the attempts write.
    status: Status,
}

impl Startup {
    /// Returns an indicator over `status`.
    #[must_use]
    pub const fn new(status: Status) -> Self {
        Self { status }
    }
}

impl HealthIndicator for Startup {
    fn name(&self) -> &'static str {
        "startup"
    }

    fn check(&self) -> Check<'_> {
        Box::pin(async move {
            self.status
                .reason()
                .map_or_else(IndicatorState::up, IndicatorState::down)
        })
    }
}

/// Returns whether `error` means the CDR is not ready yet.
///
/// It does when, somewhere in its cause chain, a CDR call reached no answer
/// or one worth asking again ([`crate::cdr::error::CdrError::is_retryable`]).
#[must_use]
pub fn not_ready(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<crate::cdr::error::CdrError>()
            .or_else(|| {
                // A boxed `#[source]` is its own hop in the chain.
                cause
                    .downcast_ref::<Box<crate::cdr::error::CdrError>>()
                    .map(AsRef::as_ref)
            })
            .is_some_and(crate::cdr::error::CdrError::is_retryable)
    })
}

/// Runs `attempt` again after `first` failed, until it succeeds or fails with
/// an error that is not [`not_ready`], waiting `backoff` before each attempt
/// and recording each wait on `status`.
///
/// # Errors
/// Returns the first error, `first` included, that does not mean the CDR is
/// not ready yet.
pub async fn retry<T, F, Fut>(
    status: &Status,
    backoff: Backoff,
    first: anyhow::Error,
    mut attempt: F,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let mut error = first;
    let mut delay = backoff.initial;
    let mut attempts: u32 = 1;
    loop {
        if !not_ready(&error) {
            return Err(error);
        }
        tracing::warn!(
            attempt = attempts,
            retry_in_ms = delay.as_millis(),
            error = format!("{error:#}"),
            "the CDR is not ready; the start is tried again"
        );
        status.waiting(format!(
            "attempt {attempts} found the CDR not ready, the next one runs in {} ms: {error:#}",
            delay.as_millis()
        ));
        tokio::time::sleep(delay).await;
        delay = backoff.next(delay);
        attempts = attempts.saturating_add(1);
        match attempt().await {
            Ok(started) => {
                status.ready();
                return Ok(started);
            }
            Err(next) => error = next,
        }
    }
}

/// Returns the indicators readiness runs while the start waits: the start
/// itself, and the CDR when one is configured.
///
/// # Errors
/// Returns the [`crate::cdr::error::CdrError`] of a CDR client the
/// configuration does not build.
pub fn registry(
    settings: &Settings,
    status: &Status,
) -> Result<crate::health::Registry, crate::cdr::error::CdrError> {
    let mut indicators: Vec<Arc<dyn HealthIndicator>> =
        vec![Arc::new(Startup::new(status.clone()))];
    if let Some(cdr) = settings.cdr.as_ref() {
        indicators.push(Arc::new(crate::indicators::Cdr::new(
            crate::cdr::CdrClient::new(cdr)?,
        )));
    }
    Ok(crate::health::Registry::new(indicators))
}

/// Starts every lane `settings` configures and serves them on the listener
/// `bind` returns until `shutdown` completes, then drains.
///
/// The listener is bound once the lanes have started. When the first start
/// finds the CDR not ready ([`not_ready`]), the listener is bound at
/// once instead: liveness answers `200` and readiness `503` with a `startup`
/// indicator while the start is tried again after each wait `backoff` sets,
/// and the lanes join the same listener once they start. A `shutdown` during
/// the wait ends the function without an error. Once the lanes start, the
/// usage report logs its boot line and, when it is on, runs in the background
/// until serving ends ([`crate::usage_report::reporter::start`]).
///
/// # Errors
/// Returns the error of a start that fails for any reason other than a CDR
/// that is not ready yet, the error `bind` returns, and the I/O error from
/// serving.
pub async fn serve_settings<B, BindFuture, F>(
    settings: &Settings,
    bind: B,
    backoff: Backoff,
    shutdown: F,
) -> anyhow::Result<()>
where
    B: FnOnce() -> BindFuture,
    BindFuture: Future<Output = anyhow::Result<TcpListener>>,
    F: Future<Output = ()> + Send + 'static,
{
    use anyhow::Context;

    let began = std::time::Instant::now();
    let (stop, _) = tokio::sync::watch::channel(false);
    let stop = Arc::new(stop);
    let signal = {
        let stop = Arc::clone(&stop);
        tokio::spawn(async move {
            shutdown.await;
            stop.send_replace(true);
        })
    };
    let (started, listener) = match crate::lanes::start(settings).await {
        Ok(started) => {
            started.announce();
            let listener = bind().await?;
            tracing::info!(listen = %settings.server.listen, "listening");
            (started, listener)
        }
        Err(error) if not_ready(&error) => {
            let listener = bind().await?;
            tracing::info!(
                listen = %settings.server.listen,
                "listening for the health probes while the CDR is not ready"
            );
            let Some((started, listener)) =
                wait_for_cdr(settings, listener, backoff, error, &stop).await?
            else {
                signal.abort();
                return Ok(());
            };
            started.announce();
            (started, listener)
        }
        Err(error) => return Err(error),
    };
    let state = Arc::new(started.state.with_lanes(started.lanes));
    let ci = crate::usage_report::config::ci_of(|name| std::env::var(name).ok());
    let report =
        crate::usage_report::reporter::start(&settings.usage_report, Arc::clone(&state), began, ci)
            .await;
    let app = crate::router(Arc::clone(&state), &settings.server);
    match started.face {
        None => crate::serve_until(
            listener,
            app,
            settings.server.shutdown_timeout,
            crate::lanes::stopped(&stop),
        )
        .await
        .context("serving HTTP")?,
        Some(face) => {
            crate::lanes::serve_with_face(listener, app, &settings.server, face, &stop).await?;
        }
    }
    if let Some(report) = report {
        report.abort();
    }
    signal.abort();
    Ok(())
}

/// Serves the health routes on `listener` and retries the start after
/// `first` found the CDR not ready, until it starts or `stop` reads `true`.
///
/// The health routes accept on a duplicate of the socket, so the lanes take
/// over the same listener with no moment the port is closed.
async fn wait_for_cdr(
    settings: &Settings,
    listener: TcpListener,
    backoff: Backoff,
    first: anyhow::Error,
    stop: &tokio::sync::watch::Sender<bool>,
) -> anyhow::Result<Option<(crate::lanes::Started, TcpListener)>> {
    use anyhow::Context;

    let shared = listener
        .into_std()
        .context("taking the listener for the health probes")?;
    let probes = TcpListener::from_std(
        shared
            .try_clone()
            .context("duplicating the listener for the health probes")?,
    )?;
    let listener = TcpListener::from_std(shared)?;
    let status = Status::starting();
    let state = Arc::new(
        crate::state::AppState::with_health(registry(settings, &status)?)
            .logging_query_parameters(settings.telemetry.logged_query_parameters.clone()),
    );
    let app = crate::with_middleware(
        routes().with_state(Arc::clone(&state)),
        state,
        &settings.server,
    );
    let (done, _) = tokio::sync::watch::channel(false);
    let probing = tokio::spawn(crate::serve_until(
        probes,
        app,
        settings.server.shutdown_timeout,
        crate::lanes::stopped(&done),
    ));
    let started = tokio::select! {
        started = retry(&status, backoff, first, || crate::lanes::start(settings)) => Some(started),
        () = crate::lanes::stopped(stop) => None,
    };
    done.send_replace(true);
    probing
        .await
        .context("the health probe server stopped")?
        .context("serving the health probes")?;
    started
        .map(|started| started.map(|started| (started, listener)))
        .transpose()
}

/// Returns the routes served while the start waits: the liveness and
/// readiness probes, the root document, and `503` for everything else.
pub(crate) fn routes() -> Router<Arc<crate::state::AppState>> {
    Router::new()
        .route("/", get(crate::root))
        .route("/health/liveness", get(crate::liveness))
        .route("/health/readiness", get(crate::readiness))
        .fallback(starting)
}

/// Answers a route the start has not mounted yet.
async fn starting() -> StatusCode {
    StatusCode::SERVICE_UNAVAILABLE
}

#[cfg(test)]
mod tests {
    use super::{Backoff, Status, not_ready};
    use std::time::Duration;

    #[test]
    fn the_backoff_doubles_up_to_its_ceiling() {
        let backoff = Backoff {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(5),
        };
        assert_eq!(Duration::from_secs(2), backoff.next(Duration::from_secs(1)));
        assert_eq!(Duration::from_secs(5), backoff.next(Duration::from_secs(4)));
        assert_eq!(Duration::from_secs(5), backoff.next(Duration::from_secs(5)));
    }

    #[test]
    fn an_error_with_no_cdr_call_in_its_chain_is_final() {
        let error = anyhow::anyhow!("the mapping set did not compile");
        assert!(!not_ready(&error));
    }

    #[test]
    fn a_status_reads_waiting_until_ready() {
        let status = Status::starting();
        assert!(status.reason().is_some());
        status.waiting(String::from("the CDR answered 503"));
        assert_eq!(Some("the CDR answered 503"), status.reason().as_deref());
        status.ready();
        assert_eq!(None, status.reason());
    }
}
