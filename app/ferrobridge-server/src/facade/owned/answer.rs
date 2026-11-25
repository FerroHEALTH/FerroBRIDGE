// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR answers of the resources the facade keeps itself: a created or
//! updated resource with its `ETag` and `Location`, and a `searchset`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::response::Response;
use ferrohealth_smart::resource::Access;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;

use crate::facade::Facade;
use crate::facade::handlers::Body;
use crate::facade::handlers::guard;
use crate::facade::media;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::owned::OwnedError;
use crate::facade::owned::OwnedStore;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// Returns the `id` the store wrote into `resource`, empty when it holds
/// none.
#[must_use]
pub fn id_of(resource: &Object) -> &str {
    match resource.get("id") {
        Some(Value::String(id)) => id,
        _ => "",
    }
}

/// Returns `resource` as bytes, for a reader that checks a stored resource
/// again.
///
/// # Errors
/// Returns a `500` when the resource cannot be written as JSON.
pub fn encoded(resource: &Object) -> Result<Vec<u8>, Refusal> {
    serde_json::to_vec(resource).map_err(|error| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception)
                .diagnosing(format!("a stored resource could not be read: {error}")),
        )
    })
}

/// Returns the refusal a store failure renders as, logging its cause chain.
#[must_use]
pub fn store_refusal(error: &OwnedError) -> Refusal {
    tracing::error!(
        error = ferrohealth_server::run::chain(error),
        "a facade resource store did not answer"
    );
    reply::refusal(
        StatusCode::INTERNAL_SERVER_ERROR,
        Issue::error(IssueType::Exception).diagnosing("the resource store did not answer"),
    )
}

/// What a write answers about the resource it stored.
#[derive(Debug, Clone, Copy)]
pub struct Written<'a> {
    /// The resource type.
    pub resource_type: &'a str,
    /// The resource as stored.
    pub resource: &'a Object,
    /// Its version.
    pub version: u64,
    /// Whether the write created it, so the answer carries its `Location`.
    pub created: bool,
}

/// Returns the answer of a create, an update or a read: the `ETag` of the
/// version, the `Location` of a create, and the body `Prefer` asks for
/// (<https://hl7.org/fhir/R4/http.html#create>).
#[must_use]
pub fn answer(
    facade: &Facade,
    status: StatusCode,
    headers: &HeaderMap,
    written: Written<'_>,
) -> Response {
    let version = written.version.to_string();
    let id = id_of(written.resource);
    let mut out = vec![reply::Header::entity_tag(&version)];
    if written.created {
        out.push(reply::Header::location(&format!(
            "{}/{}/{id}/_history/{version}",
            facade.settings().base_url.trim_end_matches('/'),
            written.resource_type,
        )));
    }
    match media::Prefer::of(headers) {
        media::Prefer::Minimal => reply::minimal(status, &out),
        media::Prefer::Outcome => reply::issues(
            status,
            &[
                Issue::information(IssueType::Informational).diagnosing(format!(
                    "{}/{id} is at version {version}",
                    written.resource_type
                )),
            ],
            &out,
        ),
        media::Prefer::Representation => reply::resource(status, written.resource, &out),
    }
}

/// Returns the `searchset` Bundle of `matches` of `resource_type`, every
/// match on one page (<https://hl7.org/fhir/R4/bundle.html>).
///
/// # Errors
/// Returns a `500` when the Bundle cannot be written.
pub fn searchset(
    facade: &Facade,
    resource_type: &str,
    uri: &Uri,
    matches: Vec<Object>,
) -> Result<Response, Refusal> {
    let base = facade.settings().base_url.trim_end_matches('/');
    let entries: Vec<serde_json::Value> = matches
        .into_iter()
        .map(|resource| {
            serde_json::json!({
                "fullUrl": format!("{base}/{resource_type}/{}", id_of(&resource)),
                "resource": resource,
                "search": {"mode": "match"},
            })
        })
        .collect();
    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "searchset",
        "total": entries.len(),
        "link": [{
            "relation": "self",
            "url": format!("{base}/{resource_type}?{}", uri.query().unwrap_or_default()),
        }],
        "entry": entries,
    });
    let object: Object = serde_json::from_value(bundle).map_err(|error| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception)
                .diagnosing(format!("the searchset could not be written: {error}")),
        )
    })?;
    Ok(reply::resource(StatusCode::OK, &object, &[]))
}

/// The future a search answers with.
pub type SearchFuture<'a> =
    std::pin::Pin<Box<dyn Future<Output = Result<Response, Refusal>> + Send + 'a>>;

/// The search one endpoint answers over the parameters of a request.
pub type Searched = for<'a> fn(&'a Search<'a>) -> SearchFuture<'a>;

/// One search request, its media types already checked.
#[derive(Debug, Clone, Copy)]
pub struct Search<'a> {
    /// The facade.
    pub facade: &'a Facade,
    /// The caller.
    pub access: &'a Access,
    /// The request URI.
    pub uri: &'a Uri,
    /// The parameters, from the query and a form body.
    pub parameters: &'a [(String, String)],
}

/// What the search handler answers with.
pub type SearchAnswer = SearchFuture<'static>;

/// Returns the search handler of one endpoint over `searched`.
///
/// It serves `GET [base]/[type]` and `POST [base]/[type]/_search`, its media
/// types checked and its parameters the query and, for the `POST`, the form
/// body beside it (<https://hl7.org/fhir/R4/http.html#search>).
pub fn search(
    searched: Searched,
) -> impl Fn(State<Arc<Facade>>, Access, HeaderMap, Uri, Bytes) -> SearchAnswer
+ Clone
+ Send
+ Sync
+ 'static {
    move |State(facade), access, headers, uri, body| {
        Box::pin(async move {
            let mut parameters = media::query_pairs(uri.query().unwrap_or_default().as_bytes());
            parameters.extend(media::query_pairs(&body));
            guard(&headers, &uri, Body::Absent)?;
            searched(&Search {
                facade: &facade,
                access: &access,
                uri: &uri,
                parameters: &parameters,
            })
            .await
        })
    }
}

/// Returns every live resource of `resource_type` in `store` beside what
/// `read` makes of it, for a search that checks each one again.
///
/// # Errors
/// Returns a `500` when the store fails, and the refusal of `read`.
pub async fn scanned<T>(
    store: &OwnedStore,
    resource_type: &str,
    read: impl Fn(&Object, &[u8]) -> Result<T, Refusal>,
) -> Result<Vec<(Object, T)>, Refusal> {
    store
        .scan(resource_type)
        .await
        .map_err(|error| store_refusal(&error))?
        .into_iter()
        .map(|resource| {
            let bytes = encoded(&resource)?;
            let held = read(&resource, &bytes)?;
            Ok((resource, held))
        })
        .collect()
}

/// Returns the `400` of a body refused at `element`.
#[must_use]
pub fn invalid(kind: IssueType, element: &str, text: impl Into<String>) -> Refusal {
    reply::refusal(
        StatusCode::BAD_REQUEST,
        Issue::error(kind).diagnosing(text).at(element.to_owned()),
    )
}
