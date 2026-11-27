// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `/operator/sources`: a source and its tenants created, changed, disabled
//! and deleted with `If-Match`, a stale write answered `412` and a missing
//! precondition `428`, every version kept with its committer, a document
//! naming a missing secret refused, and no secret value in a row or an
//! answer. The sources live in the state store, so the cases run only when
//! `FERROBRIDGE_E2E=1` admits the harness.
//!
//! Every source is synthetic: names, tenants and identifier systems invented
//! for the test.

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrobridge_operator_api::sources::{Source, Sources as Listing, Versions};
use ferrobridge_server::sources::Sources;
use ferrobridge_server::store::Store;
use ferrobridge_testkit::containers;
use http::{Method, StatusCode};
use serde_json::json;

use super::{Case, OPERATOR, bytes};

/// The value of the synthetic HMAC key, which no row or answer may carry.
const KEY_VALUE: &str = "synthetic-hmac-key-value-7f3a";

/// A case over a state store and a `[secrets]` directory holding
/// `webhook-key`, with the container and the directory kept for it.
struct Sourced {
    /// The operator API.
    case: Case,
    /// The store.
    store: Store,
    /// The container and the secrets directory.
    _kept: (containers::Postgres, tempfile::TempDir),
}

/// Returns a case, `None` unless `FERROBRIDGE_E2E=1`.
async fn sourced() -> Result<Option<Sourced>, Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(None);
    }
    let postgres = containers::postgres().await?;
    let store = Store::open(&ferrobridge_server::store::Settings {
        connection: omop_cdm::connection::CdmConnection::new(postgres.url(), None)?,
        max_connections: 4,
        acquire_timeout: Duration::from_secs(30),
        identity_copy_entries: 0,
        seal: None,
    })
    .await?;
    let secrets = tempfile::tempdir()?;
    std::fs::write(secrets.path().join("webhook-key"), format!("{KEY_VALUE}\n"))?;
    let sources = Sources::open(
        &store,
        Some(&ferrohealth_secret::dir::SecretDirConfig {
            dir: secrets.path().to_path_buf(),
            poll_seconds: ferrohealth_secret::dir::PollSeconds::default(),
        }),
    )?;
    let case = Case::sourcing(Arc::new(sources)).await?;
    Ok(Some(Sourced {
        case,
        store,
        _kept: (postgres, secrets),
    }))
}

/// A webhook source named `name` with one tenant per entry of `tenants`.
fn webhook(name: &str, enabled: bool, tenants: &[&str]) -> serde_json::Value {
    json!({
        "name": name,
        "kind": "webhook",
        "enabled": enabled,
        "document": {
            "verify": {"kind": "hmac", "header": "X-Signature"},
            "keys": [{"secret": "webhook-key"}],
            "events": {"tenant": "/tenant", "entity": "/id", "dedupe": {"pointers": ["/id"]}},
            "tenants": tenants.iter().map(|tenant| json!({
                "tenant": tenant,
                "identifier_system": format!("urn:example:{tenant}")
            })).collect::<Vec<_>>()
        }
    })
}

/// Returns the answer's `ETag` and its body as a source.
async fn read(
    response: http::Response<axum::body::Body>,
) -> Result<(String, Source), Box<dyn Error>> {
    let tag = response
        .headers()
        .get(http::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .ok_or("an ETag")?
        .to_owned();
    let body = bytes(response).await?;
    assert!(
        !String::from_utf8_lossy(&body).contains(KEY_VALUE),
        "no secret value in an answer"
    );
    Ok((tag, serde_json::from_slice(&body)?))
}

/// Returns the tenants the store holds for `id`, sorted.
async fn tenants(store: &Store, id: &str) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(sqlx::query_scalar(
        "SELECT tenant FROM inbound_tenant WHERE source_id = $1::uuid ORDER BY tenant",
    )
    .bind(id)
    .fetch_all(store.pool())
    .await?)
}

#[tokio::test]
async fn a_source_and_its_tenants_are_created_changed_disabled_and_deleted_with_if_match()
-> Result<(), Box<dyn Error>> {
    let Some(Sourced { case, store, _kept }) = sourced().await? else {
        return Ok(());
    };
    let created = case
        .send(
            Method::POST,
            "/operator/sources",
            OPERATOR,
            None,
            Some(&webhook("ons-test", true, &["t1"])),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, created.status());
    let (tag, source) = read(created).await?;
    assert_eq!(("\"1\"", 1), (tag.as_str(), source.version));
    assert_eq!(vec!["t1"], tenants(&store, &source.id).await?);
    let path = format!("/operator/sources/{}", source.id);

    let changed = case
        .send(
            Method::PUT,
            &path,
            OPERATOR,
            Some(&tag),
            Some(&webhook("ons-test", true, &["t1", "t2"])),
        )
        .await?;
    assert_eq!(StatusCode::OK, changed.status());
    let (tag, source) = read(changed).await?;
    assert_eq!(("\"2\"", 2), (tag.as_str(), source.version));
    assert_eq!(vec!["t1", "t2"], tenants(&store, &source.id).await?);

    let stale = case
        .send(
            Method::PUT,
            &path,
            OPERATOR,
            Some("\"1\""),
            Some(&webhook("ons-test", false, &[])),
        )
        .await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, stale.status());
    let missing = case
        .send(
            Method::PUT,
            &path,
            OPERATOR,
            None,
            Some(&webhook("ons-test", false, &[])),
        )
        .await?;
    assert_eq!(StatusCode::PRECONDITION_REQUIRED, missing.status());

    let disabled = case
        .send(
            Method::PUT,
            &path,
            OPERATOR,
            Some(&tag),
            Some(&webhook("ons-test", false, &["t1", "t2"])),
        )
        .await?;
    assert_eq!(StatusCode::OK, disabled.status());
    let (tag, source) = read(disabled).await?;
    assert!(!source.enabled, "a PUT with enabled false disables it");

    let stale_delete = case
        .send(Method::DELETE, &path, OPERATOR, Some("\"2\""), None)
        .await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, stale_delete.status());
    let deleted = case
        .send(Method::DELETE, &path, OPERATOR, Some(&tag), None)
        .await?;
    assert_eq!(StatusCode::NO_CONTENT, deleted.status());
    let gone = case.get(&path, Some(OPERATOR)).await?;
    assert_eq!(StatusCode::NOT_FOUND, gone.status());
    assert!(
        tenants(&store, &source.id).await?.is_empty(),
        "the tenants go with the source"
    );

    let versions = case
        .get(&format!("{path}/versions"), Some(OPERATOR))
        .await?;
    assert_eq!(StatusCode::OK, versions.status());
    let versions: Versions = serde_json::from_slice(&bytes(versions).await?)?;
    let numbered: Vec<(u64, bool)> = versions
        .versions
        .iter()
        .map(|version| (version.version, version.deleted))
        .collect();
    assert_eq!(
        vec![(1, false), (2, false), (3, false), (4, true)],
        numbered
    );
    assert!(
        versions
            .versions
            .iter()
            .all(|version| !version.committer.is_empty() && version.committer != "-"),
        "every version names its committer: {versions:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_document_naming_a_missing_secret_or_a_taken_name_is_refused()
-> Result<(), Box<dyn Error>> {
    let Some(Sourced { case, store, _kept }) = sourced().await? else {
        return Ok(());
    };
    let mut missing = webhook("ons-missing", true, &[]);
    missing["document"]["verify"]["keys"] = json!([{"secret": "no-such-key"}]);
    let refused = case
        .send(
            Method::POST,
            "/operator/sources",
            OPERATOR,
            None,
            Some(&missing),
        )
        .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, refused.status());
    let mut malformed = webhook("ons-malformed", true, &[]);
    malformed["document"]["verify"]["scheme"] = json!("md5");
    let refused = case
        .send(
            Method::POST,
            "/operator/sources",
            OPERATOR,
            None,
            Some(&malformed),
        )
        .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, refused.status());
    let first = case
        .send(
            Method::POST,
            "/operator/sources",
            OPERATOR,
            None,
            Some(&webhook("ons-dup", true, &[])),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, first.status());
    let again = case
        .send(
            Method::POST,
            "/operator/sources",
            OPERATOR,
            None,
            Some(&webhook("ons-dup", true, &[])),
        )
        .await?;
    assert_eq!(StatusCode::CONFLICT, again.status());
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT document::text FROM inbound_source UNION ALL SELECT document::text FROM inbound_source_version WHERE document IS NOT NULL",
    )
    .fetch_all(store.pool())
    .await?;
    assert!(
        rows.iter().all(|row| !row.contains(KEY_VALUE)),
        "no secret value reaches a row"
    );
    let listing = case.get("/operator/sources", Some(OPERATOR)).await?;
    let listing: Listing = serde_json::from_slice(&bytes(listing).await?)?;
    assert_eq!(
        vec!["ons-dup"],
        listing
            .sources
            .iter()
            .map(|source| source.name.as_str())
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[tokio::test]
async fn the_sources_need_the_operator_scope() -> Result<(), Box<dyn Error>> {
    let Some(Sourced { case, _kept, .. }) = sourced().await? else {
        return Ok(());
    };
    let refused = case
        .send(
            Method::POST,
            "/operator/sources",
            "patient/*.read",
            None,
            Some(&webhook("ons-x", true, &[])),
        )
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, refused.status());
    Ok(())
}

#[tokio::test]
async fn the_nedap_ons_template_creates_a_source_once_its_map_runs() -> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let postgres = containers::postgres().await?;
    let store = Store::open(&ferrobridge_server::store::Settings {
        connection: omop_cdm::connection::CdmConnection::new(postgres.url(), None)?,
        max_connections: 4,
        acquire_timeout: Duration::from_secs(30),
        identity_copy_entries: 0,
        seal: None,
    })
    .await?;
    let secrets = tempfile::tempdir()?;
    for name in [
        "nedap-ons-connector-secret",
        "nedap-ons-environment-certificate",
    ] {
        std::fs::write(secrets.path().join(name), "synthetic\n")?;
    }
    let sources = Arc::new(Sources::open(
        &store,
        Some(&ferrohealth_secret::dir::SecretDirConfig {
            dir: secrets.path().to_path_buf(),
            poll_seconds: ferrohealth_secret::dir::PollSeconds::default(),
        }),
    )?);
    let case = Case::sourcing(Arc::clone(&sources)).await?;
    let template = serde_json::to_value(crate::inbound::nedap_ons::template()?)?;
    let refused = case
        .send(
            Method::POST,
            "/operator/sources",
            OPERATOR,
            None,
            Some(&template),
        )
        .await?;
    assert_eq!(
        StatusCode::UNPROCESSABLE_ENTITY,
        refused.status(),
        "no map runs yet"
    );
    sources
        .structuremaps()
        .replace(crate::inbound::nedap_ons::set()?);
    let created = case
        .send(
            Method::POST,
            "/operator/sources",
            OPERATOR,
            None,
            Some(&template),
        )
        .await?;
    assert_eq!(StatusCode::CREATED, created.status());
    let (_tag, source) = read(created).await?;
    assert!(!source.enabled);
    Ok(())
}
