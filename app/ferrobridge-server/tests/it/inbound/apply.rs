// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one inbound handler: an upsert resolves, minimises and feeds the
//! patient, asks ITI-83 for its pseudonym and ensures exactly one EHR; a
//! remove only deactivates the patient at the identity source.

use std::error::Error;
use std::sync::{Arc, Mutex};

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::{MemoryStore, Store};
use ferrobridge_server::inbound::apply::{Apply, Feed, Resolve, Step, Unavailable, Undeclared};
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

/// A source holding one synthetic patient with more than the feed carries.
#[derive(Debug, Default)]
struct Holding;

impl Resolve for Holding {
    fn patient<'a>(&'a self, _: &'a Source, _: &'a Tenant, entity: &'a str) -> Step<'a, Value> {
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
        Box::pin(std::future::ready(Ok(serde_json::from_value::<Value>(
            patient,
        )
        .expect("a legal Patient"))))
    }
}

/// A feed that records what it was given and accepts it.
#[derive(Debug, Default)]
struct Recording {
    upserted: Mutex<Vec<Value>>,
    deactivated: Mutex<Vec<String>>,
}

impl Feed for Recording {
    fn upsert<'a>(&'a self, _: &'a Tenant, _: &'a str, patient: &'a Value) -> Step<'a, ()> {
        self.upserted
            .lock()
            .expect("the lock")
            .push(patient.clone());
        Box::pin(std::future::ready(Ok(())))
    }

    fn deactivate<'a>(&'a self, _: &'a Tenant, entity: &'a str) -> Step<'a, ()> {
        self.deactivated
            .lock()
            .expect("the lock")
            .push(entity.to_owned());
        Box::pin(std::future::ready(Ok(())))
    }
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
    /// the first `POST /ehr`, and an identity source answering
    /// [`PSEUDONYM`].
    async fn start() -> Self {
        let cdr = MockServer::start().await;
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

    /// Returns the handler over `resolve` and `feed`.
    fn handler(&self, resolve: Arc<dyn Resolve>, feed: Arc<dyn Feed>) -> Apply {
        let identity =
            identity_source_accepting(&format!("{}/pix", self.cdr.uri()), DOMAIN, &[LOCAL]);
        let store: Arc<dyn Store> = crate::facade::handle(&self.store);
        Apply::new((resolve, feed), identity, client(&self.cdr), store)
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
    let feed = Arc::new(Recording::default());
    let handler = upstreams.handler(Arc::new(Holding), feed.clone());
    handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .map_err(|failed| failed.reason)?;
    let upserted = feed.upserted.lock().expect("the lock").clone();
    let patient = serde_json::to_value(upserted.first().ok_or("one patient was fed")?)?;
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
        patient,
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
    let handler = upstreams.handler(Arc::new(Holding), Arc::new(Recording::default()));
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
    let first = upstreams.handler(Arc::new(Holding), Arc::new(Recording::default()));
    let second = upstreams.handler(Arc::new(Holding), Arc::new(Recording::default()));
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
    let feed = Arc::new(Recording::default());
    let handler = upstreams.handler(Arc::new(Holding), feed.clone());
    handler
        .apply(&source(), &item("local-0001", Change::Remove))
        .await
        .map_err(|failed| failed.reason)?;
    assert_eq!(
        vec![String::from("local-0001")],
        *feed.deactivated.lock().expect("the lock")
    );
    assert!(
        upstreams
            .cdr
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "a remove reaches neither the identity source's ITI-83 nor the CDR"
    );
    Ok(())
}

#[tokio::test]
async fn a_source_that_declares_no_resolution_parks_its_items() -> Result<(), Box<dyn Error>> {
    let upstreams = Upstreams::start().await;
    let handler = upstreams.handler(Arc::new(Undeclared), Arc::new(Recording::default()));
    let failed = handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .expect_err("an undeclared resolution fails for good");
    assert_eq!("resolution-undeclared", failed.reason);
    Ok(())
}

#[tokio::test]
async fn with_no_feed_client_an_item_parks_before_any_pseudonym() -> Result<(), Box<dyn Error>> {
    let upstreams = Upstreams::start().await;
    let handler = upstreams.handler(Arc::new(Holding), Arc::new(Unavailable));
    let failed = handler
        .apply(&source(), &item("local-0001", Change::Upsert))
        .await
        .expect_err("no feed client fails for good");
    assert_eq!("feed-client-unavailable", failed.reason);
    assert_eq!(0, upstreams.count("POST", PIX_PATH).await);
    Ok(())
}

#[tokio::test]
async fn an_item_of_a_tenant_the_source_does_not_serve_parks() -> Result<(), Box<dyn Error>> {
    let upstreams = Upstreams::start().await;
    let handler = upstreams.handler(Arc::new(Holding), Arc::new(Recording::default()));
    let mut unknown = item("local-0001", Change::Upsert);
    unknown.key.tenant = String::from("t9");
    let failed = handler
        .apply(&source(), &unknown)
        .await
        .expect_err("an unknown tenant fails for good");
    assert_eq!("tenant-unknown", failed.reason);
    Ok(())
}
