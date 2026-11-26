// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-request budget of the hot paths (#2249): the allocations of the
//! thread that polls one request and the statements it sends to the state
//! store, held to the ceilings [`BUDGETS`] names.
//!
//! The paths are the facade's create, read and search of the synthetic
//! Condition over a `wiremock` CDR, and the outbound lane's delivery of the
//! changed composition to a `wiremock` FHIR endpoint, each over a real
//! PostgreSQL state store, so the cases run only when `FERROBRIDGE_E2E=1`
//! admits the harness. The allocations are counted by `allocation-counter`,
//! the global allocator of this test binary, on the polling thread alone; a
//! statement is a `sqlx::query` event sent inside the request's span. Of
//! [`MEASURED`] requests after [`WARM`] unmeasured ones, the least
//! allocations and the most statements are judged. The CPU instructions of the paths' in-process work are counted
//! apart by callgrind (`scripts/perf/instructions.sh`). No specification
//! governs the cost of a request: our own design.

use std::cell::Cell;
use std::error::Error;
use std::fmt::Write as _;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context as TaskContext, Poll};

use axum::body::Body;
use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::change_feed::follow::Consumer;
use ferrobridge_server::change_feed::{ChangedVersion, Domain, Event, Position, Seq};
use ferrobridge_server::facade::search::Limits;
use ferrobridge_server::identity::state::StateStore;
use ferrobridge_server::identity::store::{MemoryStore, Store};
use ferrobridge_server::outbound::sink::Sink;
use ferrobridge_server::outbound::{Outbound, Settings};
use ferrobridge_server::terminology::Translator;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;
use ferrohealth_smart::config::endpoint::EndpointSection;
use http::{Request, StatusCode};
use tracing::Instrument as _;
use tracing_subscriber::layer::{Context, SubscriberExt as _};
use tracing_subscriber::registry::LookupSpan;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

use super::search::{mount_rows, row};
use super::stub::{mount_create, mount_ehr, mount_read};
use super::{
    CONTAINER, EHR_ID, FIXTURES, Harness, VERSION_ONE, call, condition, facade_over,
    post_condition, settings,
};

/// The requests of each path sent before the measured ones.
const WARM: usize = 2;

/// The measured requests of each path.
const MEASURED: usize = 5;

/// The span each measured request runs in.
const REQUEST_SPAN: &str = "hot_path_budget_request";

/// A hot path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Path {
    /// `POST [base]/Condition`.
    Create,
    /// `GET [base]/Condition/[id]`.
    Read,
    /// `GET [base]/Condition`.
    Search,
    /// The outbound lane's delivery of one changed composition.
    Outbound,
}

/// The most one request of a path may cost.
struct Budget {
    path: Path,
    /// The allocations of the least measured request.
    allocations: u64,
    /// The statements one request sends to the state store.
    statements: u64,
}

// NOTE: no specification governs these ceilings: our own design (#2249).
const BUDGETS: [Budget; 4] = [
    Budget {
        path: Path::Create,
        allocations: 0,
        statements: 0,
    },
    Budget {
        path: Path::Read,
        allocations: 0,
        statements: 0,
    },
    Budget {
        path: Path::Search,
        allocations: 0,
        statements: 0,
    },
    Budget {
        path: Path::Outbound,
        allocations: 0,
        statements: 0,
    },
];

/// What one request cost.
#[derive(Clone, Copy, Debug, Default)]
struct Cost {
    allocations: u64,
    statements: u64,
}

/// Counts the `sqlx::query` events sent inside a [`REQUEST_SPAN`] span.
#[derive(Clone, Default)]
struct Statements(Arc<AtomicU64>);

impl<S> tracing_subscriber::Layer<S> for Statements
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }
        if ctx
            .event_scope(event)
            .is_some_and(|mut scope| scope.any(|span| span.name() == REQUEST_SPAN))
        {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// A future whose polls count their allocations into a shared cell.
struct Counted<F> {
    inner: Pin<Box<F>>,
    allocations: Rc<Cell<u64>>,
}

impl<F: Future> Future for Counted<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<F::Output> {
        let this = &mut *self;
        let mut out = Poll::Pending;
        let info = allocation_counter::measure(|| out = this.inner.as_mut().poll(cx));
        this.allocations
            .set(this.allocations.get().saturating_add(info.count_total));
        out
    }
}

/// Runs `request` in a [`REQUEST_SPAN`] span and returns its output with
/// what it cost.
async fn costed<F: Future>(statements: &Statements, request: F) -> (F::Output, Cost) {
    let allocations = Rc::new(Cell::new(0));
    let before = statements.0.load(Ordering::Relaxed);
    let output = Counted {
        inner: Box::pin(request.instrument(tracing::info_span!(REQUEST_SPAN))),
        allocations: Rc::clone(&allocations),
    }
    .await;
    let cost = Cost {
        allocations: allocations.get(),
        statements: statements.0.load(Ordering::Relaxed) - before,
    };
    (output, cost)
}

/// The facade over the stub CDR with its identity map in a real state store,
/// its subject bound to [`EHR_ID`], and the store and its container kept.
struct Bench {
    harness: Harness,
    store: ferrobridge_server::store::Store,
    _postgres: ferrobridge_testkit::containers::Postgres,
}

/// Returns the bench, `None` unless `FERROBRIDGE_E2E=1` admits the harness.
async fn bench() -> Result<Option<Bench>, Box<dyn Error>> {
    let Some((postgres, store)) = crate::store::sealed_store().await? else {
        return Ok(None);
    };
    let identities: Arc<dyn Store> = Arc::new(StateStore::open(&store).await?);
    let (cdr, facade) = facade_over(
        FIXTURES,
        settings(),
        Some(Limits::default()),
        identities,
        Translator::default(),
    )
    .await;
    let harness = Harness {
        cdr,
        store: Arc::new(MemoryStore::default()),
        facade,
    };
    Ok(Some(Bench {
        harness,
        store,
        _postgres: postgres,
    }))
}

/// Returns the event that changed [`CONTAINER`] in [`EHR_ID`].
fn changed(seq: u64) -> Result<Event, Box<dyn Error>> {
    Ok(Event {
        position: Position::Outbox {
            domain: Domain::Clinical,
            seq: Seq::new(seq),
        },
        erasure: false,
        contribution_id: None,
        ehr_id: Some(EhrId::new(EHR_ID)?),
        committed_at: None,
        versions: vec![ChangedVersion {
            vo_id: Some(CONTAINER.parse()?),
            kind: String::from("COMPOSITION"),
            sys_version: None,
            version_tree_id: None,
            change_type: String::from("249"),
            template_id: None,
        }],
        envelope: serde_json::Map::new(),
    })
}

/// Returns the synthetic Condition under an identifier of its own, so each
/// measured create is a new delivery.
fn identified(attempt: usize) -> serde_json::Value {
    let mut resource = condition();
    resource["identifier"] = serde_json::json!([{
        "system": "http://example.org/fhir/sid/ferrobridge-condition",
        "value": format!("synthetic-condition-budget-{attempt}")
    }]);
    resource
}

/// Answers every later composition create `201` under a version container
/// of its own, so each measured create commits a new composition.
async fn mount_fresh_creates(cdr: &MockServer, body: &serde_json::Value) {
    let body = body.to_string();
    let next = AtomicU64::new(1);
    Mock::given(matchers::method("POST"))
        .and(matchers::path(format!("/ehr/{EHR_ID}/composition")))
        .respond_with(move |_: &wiremock::Request| {
            let n = next.fetch_add(1, Ordering::Relaxed);
            ResponseTemplate::new(201)
                .insert_header(
                    "ETag",
                    format!("W/\"8849182c-82ad-4088-a07f-{n:012x}::ferrobridge.test::1\""),
                )
                .insert_header("Content-Type", "application/json")
                .set_body_string(body.clone())
        })
        .with_priority(1)
        .mount(cdr)
        .await;
}

/// Sends `request` through the facade of `bench` and returns what it cost,
/// refusing an answer other than `expected`.
async fn facade_cost(
    bench: &Bench,
    statements: &Statements,
    expected: StatusCode,
    request: impl Fn() -> Result<Request<Body>, Box<dyn Error>>,
) -> Result<Cost, Box<dyn Error>> {
    let app = bench.harness.app();
    let (answer, cost) = costed(statements, call(app, request()?)).await;
    let (status, body) = answer?;
    assert_eq!(expected, status, "{body}");
    Ok(cost)
}

/// Returns the least cost of each path, warm.
async fn measure(
    bench: &Bench,
    statements: &Statements,
) -> Result<Vec<(Path, Cost)>, Box<dyn Error>> {
    let harness = &bench.harness;
    mount_ehr(&harness.cdr).await;
    mount_create(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    mount_read(&harness.cdr, VERSION_ONE, &harness.composition()).await;
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &harness.composition())]).await;
    let (status, created) = call(harness.app(), post_condition(&condition())).await?;
    assert_eq!(StatusCode::CREATED, status, "{created}");
    let id = created["id"]
        .as_str()
        .ok_or("the created resource carries an id")?
        .to_owned();
    mount_fresh_creates(&harness.cdr, &harness.composition()).await;
    let endpoint = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/fhir"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&endpoint)
        .await;
    let outbound = Outbound::start(
        &Settings {
            reader: ferrobridge_server::change_feed::ReaderName::new("fhir-outbound")?,
            sink: Sink::new(OutboundClient::build(
                &EndpointSection {
                    url: SecretUrl::new(format!("{}/fhir", endpoint.uri())),
                    timeout: ferrohealth_config::units::Duration::from_secs(5),
                    ..EndpointSection::default()
                },
                "fhir_outbound.endpoint",
                &ServerTls,
            )?),
        },
        Arc::clone(&harness.facade),
        harness.facade.client().clone(),
        &bench.store,
    );
    let mut costs = Vec::new();
    for path in BUDGETS.iter().map(|budget| budget.path) {
        let mut least: Option<Cost> = None;
        for attempt in 0..WARM + MEASURED {
            let cost = match path {
                Path::Create => {
                    facade_cost(bench, statements, StatusCode::CREATED, || {
                        Ok(post_condition(&identified(attempt)))
                    })
                    .await?
                }
                Path::Read => {
                    facade_cost(bench, statements, StatusCode::OK, || {
                        Ok(Request::get(format!("/fhir/Condition/{id}")).body(Body::empty())?)
                    })
                    .await?
                }
                Path::Search => {
                    facade_cost(bench, statements, StatusCode::OK, || {
                        Ok(Request::get("/fhir/Condition").body(Body::empty())?)
                    })
                    .await?
                }
                Path::Outbound => {
                    let event = changed(u64::try_from(attempt)?.saturating_add(1))?;
                    let (processed, cost) = costed(statements, outbound.process(&event)).await;
                    processed?;
                    cost
                }
            };
            if attempt < WARM {
                continue;
            }
            least = Some(match least {
                Some(seen) => Cost {
                    allocations: seen.allocations.min(cost.allocations),
                    statements: seen.statements.max(cost.statements),
                },
                None => cost,
            });
        }
        costs.push((path, least.ok_or("each path is measured")?));
    }
    Ok(costs)
}

#[tokio::test]
async fn every_hot_path_holds_its_allocation_and_statement_budget() -> Result<(), Box<dyn Error>> {
    let Some(bench) = bench().await? else {
        return Ok(());
    };
    let statements = Statements::default();
    let subscriber = tracing_subscriber::registry().with(statements.clone());
    let _default = tracing::subscriber::set_default(subscriber);
    let costs = measure(&bench, &statements).await?;
    let mut report = String::from("path        allocations (budget)   statements (budget)\n");
    let mut over = Vec::new();
    for (path, cost) in &costs {
        let budget = BUDGETS
            .iter()
            .find(|budget| budget.path == *path)
            .ok_or("each path has a budget")?;
        writeln!(
            report,
            "{path:<10?} {:>12} ({:>8}) {:>12} ({:>6})",
            cost.allocations, budget.allocations, cost.statements, budget.statements
        )?;
        if cost.allocations > budget.allocations || cost.statements > budget.statements {
            over.push(*path);
        }
    }
    println!("{report}");
    assert!(over.is_empty(), "over budget: {over:?}\n{report}");
    Ok(())
}
