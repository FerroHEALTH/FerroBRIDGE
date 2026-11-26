// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one inbound handler: an upsert resolves, minimises and feeds the
//! patient over ITI-104, asks ITI-83 for its pseudonym and ensures exactly
//! one EHR; a source's merge is forwarded as Resolve Duplicate; a remove only
//! deactivates the patient at the identity source.

use std::error::Error;
use std::sync::Arc;

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::{MemoryStore, Store};
use ferrobridge_server::inbound::apply::{Apply, Resolve, Resolved, Step};
use ferrobridge_server::inbound::event::Change;
use ferrobridge_server::inbound::store::{Claimed, WorkKey};
use ferrobridge_server::inbound::worker::Handler;
use ferrobridge_server::sources::Source;
use ferrobridge_server::sources::document::Tenant;
use ferrobridge_testkit::stubs::its_rest;
use fhir_types::codec::Value;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

use crate::facade::{EHR_ID, client};
use crate::support::{PIX_PATH, identity_source_accepting, pseudonym_answer};

/// The domain the stub identity source answers pseudonyms in.
const DOMAIN: &str = "urn:oid:2.999.9";

/// The pseudonym the stub identity source answers.
const PSEUDONYM: &str = "synthetic-pseudonym-7";

/// The tenant of the source and its local identifier system.
const TENANT: &str = "t1";

/// The local identifier system of [`TENANT`].
const LOCAL: &str = "urn:example:t1";

/// The path the identity source's `Patient` type is fed at (ITI-104).
const FEED_PATH: &str = "/pix/Patient";

/// A source holding one synthetic patient with more than the feed carries.
#[derive(Debug, Default)]
struct Holding;

impl Resolve for Holding {
    fn patient<'a>(&'a self, _: &'a Source, _: &'a Tenant, entity: &'a str) -> Step<'a, Resolved> {
        Box::pin(std::future::ready(Ok(Resolved::Patient(held(entity)))))
    }
}

/// A source that holds its patient no more.
#[derive(Debug, Default)]
struct Gone;

impl Resolve for Gone {
    fn patient<'a>(&'a self, _: &'a Source, _: &'a Tenant, _: &'a str) -> Step<'a, Resolved> {
        Box::pin(std::future::ready(Ok(Resolved::Removed)))
    }
}

/// A source that merged its patient into `local-0002`, stating it with a
/// `replaced-by` link.
#[derive(Debug, Default)]
struct Merged;

impl Resolve for Merged {
    fn patient<'a>(&'a self, _: &'a Source, _: &'a Tenant, entity: &'a str) -> Step<'a, Resolved> {
        let mut patient = serde_json::to_value(held(entity)).expect("a JSON Patient");
        patient["link"] = serde_json::json!([{
            "other": {"identifier": {"system": LOCAL, "value": "local-0002"}},
            "type": "replaced-by"
        }]);
        Box::pin(std::future::ready(Ok(Resolved::Patient(
            serde_json::from_value::<Value>(patient).expect("a legal Patient"),
        ))))
    }
}

/// Returns the synthetic patient the source holds for `entity`.
fn held(entity: &str) -> Value {
    let patient = serde_json::json!({
        "resourceType": "Patient",
        "id": "source-side-id",
        "identifier": [{"system": "http://example.org/fhir/sid/other", "value": "SYN-OTHER-1"}],
        "name": [{"family": "Synthetic", "given": ["Sam"]}],
        "birthDate": "1980-01-01",
        "gender": "unknown",
        "telecom": [{"system": "phone", "value": "+31000000000"}],
        "address": [{"city": "Nowhere"}],
        "text": {"status": "generated", "div": format!("<div>{entity}</div>")}
    });
    serde_json::from_value::<Value>(patient).expect("a legal Patient")
}

/// Mounts on `server` an identity source that takes every ITI-104 update
/// with `status`.
pub(crate) async fn mount_feed(server: &MockServer, status: u16) {
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(FEED_PATH))
        .respond_with(ResponseTemplate::new(status))
        .mount(server)
        .await;
}

/// The webhook source serving [`TENANT`].
fn source() -> Source {
    Source::fixed(&crate::inbound::store::webhook("ons", &[TENANT], 1)).expect("a legal source")
}

/// The item of `entity` asking for `change`.
fn item(entity: &str, change: Change) -> Claimed {
    Claimed {
        key: WorkKey {
            source_id: source().id,
            tenant: TENANT.to_owned(),
            entity: entity.to_owned(),
        },
        generation: 1,
        change,
        attempts: 0,
    }
}

/// The upstreams and the identity map one case runs against.
struct Upstreams {
    cdr: MockServer,
    store: Arc<MemoryStore>,
}

impl Upstreams {
    /// Starts a CDR holding no EHR for the subject, creating [`EHR_ID`] on
    /// the first `POST /ehr`, and an identity source taking every feed and
    /// answering [`PSEUDONYM`].
    async fn start() -> Self {
        Self::feeding(201).await
    }

    /// Starts the upstreams of [`Self::start`] with an identity source that
    /// answers every feed with `status`.
    async fn feeding(status: u16) -> Self {
        let cdr = MockServer::start().await;
        mount_feed(&cdr, status).await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path(PIX_PATH))
            .respond_with(pseudonym_answer(DOMAIN, PSEUDONYM))
            .mount(&cdr)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/ehr"))
            .respond_with(its_rest::not_found("no EHR for the subject"))
            .mount(&cdr)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/ehr"))
            .respond_with(ResponseTemplate::new(201).insert_header("ETag", format!("\"{EHR_ID}\"")))
            .mount(&cdr)
            .await;
        Self {
            cdr,
            store: Arc::new(MemoryStore::default()),
        }
    }

    /// Returns the handler over `resolve`.
    pub(crate) fn handler(&self, resolve: Arc<dyn Resolve>) -> Apply {
        let identity =
            identity_source_accepting(&format!("{}/pix", self.cdr.uri()), DOMAIN, &[LOCAL]);
        let store: Arc<dyn Store> = crate::facade::handle(&self.store);
        Apply::new(resolve, identity, client(&self.cdr), store)
    }

    /// Returns the bodies of the ITI-104 updates the identity source took,
    /// with the condition each named.
    async fn fed(&self) -> Result<Vec<(String, serde_json::Value)>, Box<dyn Error>> {
        self.cdr
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.method.as_str() == "PUT" && request.url.path() == FEED_PATH)
            .map(|request| {
                Ok((
                    request.url.query().unwrap_or_default().to_owned(),
                    serde_json::from_slice(&request.body)?,
                ))
            })
            .collect()
    }

    /// Returns how many requests of `method` reached `path`.
    async fn count(&self, method: &str, path: &str) -> usize {
        self.cdr
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.method.as_str() == method && request.url.path() == path)
            .count()
    }
}

#[tokio::test]
async fn an_upsert_feeds_the_minimised_patient_and_ensures_its_one_ehr()
-> Result<(), Box<dyn Error>> {
    let upstreams = Upstreams::start().await;
    let handler = upstreams.handler(Arc::new(Holding));
    handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .map_err(|failed| failed.reason)?;
    let fed = upstreams.fed().await?;
    let (condition, patient) = fed.first().ok_or("one patient was fed")?;
    assert_eq!(
        format!(
            "identifier={}",
            url::form_urlencoded::byte_serialize(format!("{LOCAL}|local-0001").as_bytes())
                .collect::<String>()
        ),
        *condition,
        "PIXm 3.1.0 §2:3.104.4.1: the update is conditional on the local identifier"
    );
    assert_eq!(
        serde_json::json!({
            "resourceType": "Patient",
            "identifier": [
                {"system": LOCAL, "value": "local-0001"},
                {"system": "http://example.org/fhir/sid/other", "value": "SYN-OTHER-1"}
            ],
            "name": [{"family": "Synthetic", "given": ["Sam"]}],
            "birthDate": "1980-01-01",
            "gender": "unknown"
        }),
        *patient,
        "only the minimised elements and the local identifier are fed"
    );
    let asked = upstreams
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|request| request.url.path() == PIX_PATH)
        .ok_or("the identity source was asked")?;
    let asked = String::from_utf8(asked.body)?;
    assert!(
        asked.contains(LOCAL) && asked.contains("local-0001") && asked.contains(DOMAIN),
        "ITI-83 asks by the local identifier for the pseudonym domain: {asked}"
    );
    assert_eq!(
        1,
        upstreams.count("POST", "/ehr").await,
        "one EHR is created"
    );
    let person = PersonId::new(DOMAIN, PSEUDONYM)?;
    assert_eq!(
        Some(EhrId::new(EHR_ID)?),
        upstreams.store.ehr_of(&person).await?,
        "the identity map binds the pseudonym to the EHR"
    );
    Ok(())
}

#[tokio::test]
async fn a_second_upsert_of_the_patient_creates_no_second_ehr() -> Result<(), Box<dyn Error>> {
    let upstreams = Upstreams::start().await;
    let handler = upstreams.handler(Arc::new(Holding));
    for _ in 0..2 {
        handler
            .apply(&source(), &item("local-0001", Change::Upsert))
            .await
            .map_err(|failed| failed.reason)?;
    }
    assert_eq!(1, upstreams.count("POST", "/ehr").await);
    Ok(())
}

#[tokio::test]
async fn two_workers_ensuring_one_patient_at_once_end_with_one_ehr() -> Result<(), Box<dyn Error>> {
    let cdr = MockServer::start().await;
    mount_feed(&cdr, 200).await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(PIX_PATH))
        .respond_with(pseudonym_answer(DOMAIN, PSEUDONYM))
        .mount(&cdr)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .respond_with(its_rest::not_found("no EHR for the subject"))
        .up_to_n_times(2)
        .with_priority(1)
        .mount(&cdr)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .respond_with(ResponseTemplate::new(200).set_body_string(crate::facade::ehr_body()))
        .with_priority(2)
        .mount(&cdr)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/ehr"))
        .respond_with(ResponseTemplate::new(201).insert_header("ETag", format!("\"{EHR_ID}\"")))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&cdr)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/ehr"))
        .respond_with(ResponseTemplate::new(409))
        .with_priority(2)
        .mount(&cdr)
        .await;
    let upstreams = Upstreams {
        cdr,
        store: Arc::new(MemoryStore::default()),
    };
    let first = upstreams.handler(Arc::new(Holding));
    let second = upstreams.handler(Arc::new(Holding));
    let (source, item) = (source(), item("local-0001", Change::Upsert));
    let (one, two) = tokio::join!(first.apply(&source, &item), second.apply(&source, &item));
    one.map_err(|failed| failed.reason)?;
    two.map_err(|failed| failed.reason)?;
    let person = PersonId::new(DOMAIN, PSEUDONYM)?;
    assert_eq!(
        Some(EhrId::new(EHR_ID)?),
        upstreams.store.ehr_of(&person).await?
    );
    Ok(())
}

#[tokio::test]
async fn a_remove_deactivates_the_patient_and_never_touches_an_ehr() -> Result<(), Box<dyn Error>> {
    let upstreams = Upstreams::start().await;
    let handler = upstreams.handler(Arc::new(Holding));
    handler
        .apply(&source(), &item("local-0001", Change::Remove))
        .await
        .map_err(|failed| failed.reason)?;
    let fed = upstreams.fed().await?;
    assert_eq!(1, fed.len());
    assert_eq!(
        serde_json::json!({
            "resourceType": "Patient",
            "identifier": [{"system": LOCAL, "value": "local-0001"}],
            "active": false
        }),
        fed[0].1
    );
    assert_eq!(
        1,
        upstreams
            .cdr
            .received_requests()
            .await
            .unwrap_or_default()
            .len(),
        "a remove reaches neither the identity source's ITI-83 nor the CDR"
    );
    Ok(())
}

#[tokio::test]
async fn an_upsert_of_an_entity_the_source_removed_deactivates_it() -> Result<(), Box<dyn Error>> {
    let upstreams = Upstreams::start().await;
    let handler = upstreams.handler(Arc::new(Gone));
    handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .map_err(|failed| failed.reason)?;
    let fed = upstreams.fed().await?;
    assert_eq!(1, fed.len());
    assert_eq!(Some(false), fed[0].1["active"].as_bool());
    assert_eq!(0, upstreams.count("POST", "/ehr").await);
    Ok(())
}

#[tokio::test]
async fn a_source_that_declares_no_resolution_parks_its_items() -> Result<(), Box<dyn Error>> {
    let upstreams = Upstreams::start().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://nobody:nothing@127.0.0.1:1/none?sslmode=disable")?;
    let sources = Arc::new(ferrobridge_server::sources::Sources::new(pool, None));
    let handler = upstreams.handler(Arc::new(
        ferrobridge_server::inbound::resolve::Declared::new(sources),
    ));
    let failed = handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .expect_err("an undeclared resolution fails for good");
    assert_eq!("resolution-undeclared", failed.reason);
    Ok(())
}

#[tokio::test]
async fn a_refused_feed_parks_before_any_pseudonym() -> Result<(), Box<dyn Error>> {
    // #2048: a 400, 403 or 422 parks the item; nothing reaches ITI-83 or the CDR.
    let upstreams = Upstreams::feeding(422).await;
    let handler = upstreams.handler(Arc::new(Holding));
    let failed = handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .expect_err("a refused feed fails");
    assert_eq!("feed-refused", failed.reason);
    assert_eq!(ferrobridge_server::retry::Failure::Poisoned, failed.failure);
    assert_eq!(0, upstreams.count("POST", PIX_PATH).await);
    assert_eq!(0, upstreams.count("POST", "/ehr").await);
    Ok(())
}

#[tokio::test]
async fn an_unavailable_identity_source_runs_the_feed_again() -> Result<(), Box<dyn Error>> {
    let upstreams = Upstreams::feeding(503).await;
    let handler = upstreams.handler(Arc::new(Holding));
    let failed = handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .expect_err("an unavailable identity source fails the attempt");
    assert_eq!("feed-unavailable", failed.reason);
    assert_eq!(
        ferrobridge_server::retry::Failure::Transient,
        failed.failure
    );
    Ok(())
}

#[tokio::test]
async fn a_source_merge_reaches_the_identity_source_as_resolve_duplicate()
-> Result<(), Box<dyn Error>> {
    // PIXm 3.1.0 §2:3.104.4.2.2: the subsumed patient, with a replaced-by link to
    // the survivor, updated on the subsumed identifier; the bridge decides nothing.
    let upstreams = Upstreams::start().await;
    let handler = upstreams.handler(Arc::new(Merged));
    handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .map_err(|failed| failed.reason)?;
    let fed = upstreams.fed().await?;
    assert_eq!(1, fed.len());
    let (condition, patient) = &fed[0];
    assert!(condition.contains("local-0001"), "{condition}");
    assert_eq!(
        serde_json::json!([{
            "other": {"identifier": {"system": LOCAL, "value": "local-0002"}},
            "type": "replaced-by"
        }]),
        patient["link"]
    );
    assert_eq!(0, upstreams.count("POST", PIX_PATH).await);
    assert_eq!(0, upstreams.count("POST", "/ehr").await);
    Ok(())
}

#[tokio::test]
async fn an_item_of_a_tenant_the_source_does_not_serve_parks() -> Result<(), Box<dyn Error>> {
    let upstreams = Upstreams::start().await;
    let handler = upstreams.handler(Arc::new(Holding));
    let mut unknown = item("local-0001", Change::Upsert);
    unknown.key.tenant = String::from("t9");
    let failed = handler
        .apply(&source(), &unknown)
        .await
        .expect_err("an unknown tenant fails for good");
    assert_eq!("tenant-unknown", failed.reason);
    Ok(())
}

/// The event type code and the subtype codes of one record.
type Kind = (String, Vec<String>);

/// Returns the event type code and the subtype codes of each record.
fn kinds(records: &[serde_json::Value]) -> Vec<Kind> {
    records
        .iter()
        .map(|record| {
            let subtypes = record["subtype"]
                .as_array()
                .map(|subtypes| {
                    subtypes
                        .iter()
                        .filter_map(|subtype| subtype["code"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            (
                record["type"]["code"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                subtypes,
            )
        })
        .collect()
}

#[tokio::test]
async fn each_feed_and_ehr_create_is_recorded_in_the_outbox() -> Result<(), Box<dyn Error>> {
    // #2048: the system's import record and the BALP ITI-104 Source record of the
    // feed, the ITI-83 Consumer record of the query and the record of the EHR create.
    let Some(state) = crate::audit::state().await? else {
        return Ok(());
    };
    let arr = crate::audit::repository().await;
    let upstreams = Upstreams::start().await;
    let log = crate::audit::access_log(&arr, &state.1, &upstreams.cdr.uri().parse()?, None).await?;
    let identity =
        identity_source_accepting(&format!("{}/pix", upstreams.cdr.uri()), DOMAIN, &[LOCAL]);
    let identity = Arc::new((*identity).clone().audited(Arc::clone(log.recorder())));
    let store: Arc<dyn Store> = crate::facade::handle(&upstreams.store);
    let handler = Apply::new(Arc::new(Holding), identity, client(&upstreams.cdr), store)
        .with_access_log(Some(log));
    handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .map_err(|failed| failed.reason)?;
    let first = kinds(&crate::audit::records(&arr, 4).await?);
    assert_eq!(4, first.len(), "{first:?}");
    let count = |found: &dyn Fn(&Kind) -> bool| first.iter().filter(|kind| found(kind)).count();
    let names = |kind: &Kind, code: &str| kind.1.iter().any(|subtype| subtype == code);
    assert_eq!(
        1,
        count(&|kind| kind.0 == "110107"),
        "the import: {first:?}"
    );
    assert_eq!(
        1,
        count(&|kind| names(kind, "ITI-104")),
        "the Source: {first:?}"
    );
    assert_eq!(
        1,
        count(&|kind| names(kind, "ITI-83")),
        "the Consumer: {first:?}"
    );
    assert_eq!(
        1,
        count(&|kind| kind.0 == "rest" && kind.1 == ["create"]),
        "the EHR create: {first:?}"
    );
    handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .map_err(|failed| failed.reason)?;
    let second = crate::audit::records(&arr, 7).await?;
    assert_eq!(
        7,
        second.len(),
        "an EHR found again is no create: {second:?}"
    );
    Ok(())
}
