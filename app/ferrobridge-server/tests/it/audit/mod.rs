// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access-record lane: the crate boundary of its two audit crates, the
//! HL7 v2 face's import record, and the Audit Record Repository stub every
//! recording case sends to over the ITI-20 ATX: FHIR Feed. The records wait in
//! the state store's outbox, so the recording cases run only when
//! `FERROBRIDGE_E2E=1` admits the harness.

mod boundary;
mod hl7v2;
mod outbox;

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ehds_logging::map::CategoryMap;
use ferrobridge_server::audit::AccessLog;
use ferrobridge_server::audit::Settings;
use ferrobridge_server::store::Store;
use ferrobridge_testkit::containers;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::ServerTls;
use ferrohealth_smart::config::endpoint::{EndpointSection, RetrySection};
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

/// The FHIR base the facade of these cases answers under.
const BASE_URL: &str = "http://ferrobridge.invalid/fhir";

/// Starts an Audit Record Repository that creates every `AuditEvent` it is
/// sent (ITI TF-2 §3.20.4.3.2).
pub(crate) async fn repository() -> MockServer {
    let arr = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/AuditEvent"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&arr)
        .await;
    arr
}

/// Returns a migrated state store in a PostgreSQL container, `None` unless
/// `FERROBRIDGE_E2E=1` admits the harness.
pub(crate) async fn state() -> Result<Option<(containers::Postgres, Store)>, Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(None);
    }
    let postgres = containers::postgres().await?;
    let store = Store::open(&ferrobridge_server::store::Settings {
        connection: omop_cdm::connection::CdmConnection::new(postgres.url(), None)?,
        max_connections: 4,
        acquire_timeout: Duration::from_secs(30),
        seal: None,
    })
    .await?;
    Ok(Some((postgres, store)))
}

/// Opens an access-record lane that delivers to `arr` from the outbox of
/// `store`, classifying by `categories`, for a facade over the CDR at `cdr`,
/// and starts its delivery.
pub(crate) fn access_log(
    arr: &MockServer,
    store: &Store,
    cdr: &url::Url,
    categories: CategoryMap,
) -> Result<Arc<AccessLog>, Box<dyn Error>> {
    let client = OutboundClient::build(
        &EndpointSection {
            url: SecretUrl::new(arr.uri()),
            timeout: ferrohealth_config::units::Duration::from_secs(5),
            retry: RetrySection {
                max_attempts: 1,
                initial_backoff: ferrohealth_config::units::Duration::from_millis(100),
                max_backoff: ferrohealth_config::units::Duration::from_secs(1),
            },
            ..EndpointSection::default()
        },
        "audit.endpoint",
        &ServerTls,
    )?;
    let settings = Settings {
        feed: ferrobridge_server::audit::feed::repository(client, true)?,
        source_id: String::from("ferrobridge.test"),
        site: None,
        categories,
    };
    let log = Arc::new(AccessLog::open(&settings, &BASE_URL.parse()?, cdr, store));
    drop(log.spawn());
    Ok(log)
}

/// Returns the `AuditEvent`s `arr` received, waiting until it holds `count`.
pub(crate) async fn records(
    arr: &MockServer,
    count: usize,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    for _ in 0..200 {
        let records: Vec<serde_json::Value> = arr
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| request.url.path() == "/AuditEvent")
            .filter_map(|request| serde_json::from_slice(&request.body).ok())
            .collect();
        if records.len() >= count {
            return Ok(records);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Err(format!("the repository received fewer than {count} records").into())
}
