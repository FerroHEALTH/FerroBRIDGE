// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search interaction on a party type, through Cadasto's experimental
//! AQL demographic extension (`crate::facade::search::party`).
//!
//! `identifier` and `name` run the type's configured query: one page with
//! `offset` and one row more than it answers in `fetch`, so a `next` link is
//! written only when a row is left. Each party uid a row names is read over
//! the ITS-REST 1.1.0 Demographic API, DEVELOPMENT, and mapped back as a read
//! maps it. `_id` reads the one party it names, compared case-sensitively as
//! R4 compares `_id` (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.1).
//! `_count` is capped at the configured largest page, since "Servers SHALL
//! NOT return more resources than requested", and `_count=0` "shall be
//! treated the same as `_summary=count`" (§3.1.1.5.3): the matches are
//! counted and the searchset carries `total` and no entry. A CDR `400` on a
//! query is the extension's refusal row, a `501`, never an empty searchset.
//! No specification governs a party search beyond those rules: our own
//! design.

mod request;

use axum::response::Response;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_its::rest::client::ErrorBody;
use openehr_its::rest::generated::query::AdhocQueryExecute;
use openehr_its::rest::generated::query::ResultSetRow;
use openehr_its::rest::generated::query::client::QueryExecuteAdhocQueryBodyOutcome;

use crate::auth::Access;
use crate::auth::Compartment;
use crate::auth::scope::Permission;
use crate::cdr::CdrClient;
use crate::facade::Facade;
use crate::facade::handlers::Refusal;
use crate::facade::handlers::write;
use crate::facade::identity::FhirResourceId;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::party;
use crate::facade::party::Held;
use crate::facade::programs::Loaded;
use crate::facade::reply;
use crate::facade::search::bundle;
use crate::facade::search::bundle::Match;
use crate::facade::search::criteria::Handling;
use crate::facade::search::cursor;
use crate::facade::search::cursor::Position;
use crate::facade::search::identifier;
use crate::facade::search::party::IDENTIFIER;
use crate::facade::search::party::NAME;
use crate::facade::search::party::PartyQueries;
use crate::facade::status;

use request::Criterion;
use request::Request;

/// What one search found: the matches of the page and, for `_count=0`, the
/// count.
struct Found {
    /// The matches of the page.
    matches: Vec<Match>,
    /// Whether a row is left after the page.
    more: bool,
    /// The number of matches, when the search asked for the count.
    total: Option<u64>,
}

/// Runs one search on the party type `loaded` serves.
pub(super) async fn search(
    facade: &Facade,
    access: &Access,
    loaded: &Loaded,
    resource_type: &str,
    headers: &http::HeaderMap,
    pairs: &[(String, String)],
) -> Result<Response, Refusal> {
    let compartment = access
        .reach(Permission::Search, resource_type)
        .ok_or_else(crate::auth::guard::forbidden)?;
    let queries = facade
        .party_search()
        .and_then(|search| search.of_type(resource_type))
        .ok_or_else(|| {
            reply::refusal(
                StatusCode::NOT_FOUND,
                Issue::error(IssueType::NotSupported).diagnosing(format!(
                    "no search is declared for {resource_type}; GET {}/metadata names the interactions each type supports",
                    crate::facade::BASE_PATH
                )),
            )
        })?;
    let request = request::read(facade, resource_type, queries, pairs, Handling::of(headers))?;
    let client = facade.client_for(headers);
    let keep = |uid: &str| match compartment {
        Compartment::Any => true,
        // NOTE: SMART App Launch 2.x scopes-and-launch-context.html, a `patient/` token
        // reaches the one `Patient` its launch context names, so a search keeps only it.
        Compartment::Patient(ref patient) => resource_type == "Patient" && uid == patient.as_str(),
    };
    if matches!(compartment, Compartment::Patient(_)) && resource_type != "Patient" {
        return Err(crate::auth::guard::forbidden());
    }
    let found = match request.criterion {
        Criterion::Id(ref id) => {
            by_id(
                facade,
                &client,
                loaded,
                (resource_type, id),
                &request,
                &keep,
            )
            .await?
        }
        Criterion::Identifier(_) | Criterion::Name(_) => {
            let aql = query_of(queries, &request)?;
            by_query(
                facade,
                &client,
                loaded,
                resource_type,
                &aql,
                &request,
                &keep,
            )
            .await?
        }
    };
    answer(facade, resource_type, &request, &found)
}

/// Returns the configured query the criterion of `request` runs.
fn query_of(queries: &PartyQueries, request: &Request) -> Result<AdhocQueryExecute, Refusal> {
    match request.criterion {
        Criterion::Identifier(ref token) => queries
            .identifier()
            .ok_or_else(|| bad_request(&format!("{IDENTIFIER} is not configured")))?
            .request(token)
            .map_err(|error| bad_request(&format!("{IDENTIFIER}: {error}"))),
        Criterion::Name(ref value) => queries
            .name()
            .ok_or_else(|| bad_request(&format!("{NAME} is not configured")))?
            .request(value)
            .map_err(|error| bad_request(&format!("{NAME}: {error}"))),
        Criterion::Id(_) => Err(bad_request("_id runs no query")),
    }
}

/// Finds the parties a configured query names.
async fn by_query(
    facade: &Facade,
    client: &CdrClient,
    loaded: &Loaded,
    resource_type: &str,
    aql: &AdhocQueryExecute,
    request: &Request,
    keep: &(dyn Fn(&str) -> bool + Sync),
) -> Result<Found, Refusal> {
    let page = usize::try_from(request.count).unwrap_or(usize::MAX);
    if request.count == 0 {
        let rows = run(client, aql).await?;
        let total = uids(&rows)?.iter().filter(|uid| keep(uid)).count();
        return Ok(Found {
            matches: Vec::new(),
            more: false,
            total: Some(u64::try_from(total).unwrap_or(u64::MAX)),
        });
    }
    let fetch = i64::from(request.count).saturating_add(1);
    let paged = crate::facade::search::party::page(aql.clone(), request.offset, fetch);
    let rows = run(client, &paged).await?;
    let more = rows.len() > page;
    let mut found = uids(&rows)?;
    found.truncate(page);
    found.retain(|uid| keep(uid));
    let demographic = party::demographic(client)?;
    let class = party::read::class_of(loaded)?;
    let mut matches = Vec::with_capacity(found.len());
    for uid in &found {
        let held = party::get(demographic, class, resource_type, uid).await?;
        matches.push(rendered(facade, loaded, (resource_type, uid), &held).await?);
    }
    Ok(Found {
        matches,
        more,
        total: None,
    })
}

/// Finds the one party `_id` names, or none.
///
/// An id that is no `uid`, or a party of another template, names nothing
/// this type serves, so it matches nothing.
async fn by_id(
    facade: &Facade,
    client: &CdrClient,
    loaded: &Loaded,
    (resource_type, id): (&str, &str),
    request: &Request,
    keep: &(dyn Fn(&str) -> bool + Sync),
) -> Result<Found, Refusal> {
    let mut held = None;
    if request.offset == 0 && keep(id) && HierObjectId::new(id).is_ok() {
        let demographic = party::demographic(client)?;
        let class = party::read::class_of(loaded)?;
        held = party::find(demographic, class, id)
            .await?
            .filter(|found| party::of_template(loaded, found));
    }
    if request.count == 0 {
        return Ok(Found {
            matches: Vec::new(),
            more: false,
            total: Some(u64::from(held.is_some())),
        });
    }
    let mut matches = Vec::new();
    if let Some(ref found) = held {
        matches.push(rendered(facade, loaded, (resource_type, id), found).await?);
    }
    Ok(Found {
        matches,
        more: false,
        total: None,
    })
}

/// Maps one party back as a read maps it.
async fn rendered(
    facade: &Facade,
    loaded: &Loaded,
    (resource_type, uid): (&str, &str),
    held: &Held,
) -> Result<Match, Refusal> {
    let canonical = party::accept(loaded, resource_type, uid, held)?;
    let resource = party::render(facade, loaded, &canonical, &held.version).await?;
    let id = FhirResourceId::new(uid).map_err(|error| {
        write::refuse(&status::Answer::new(
            status::UPSTREAM,
            format!("the party uid {uid} is no FHIR id: {error}"),
        ))
    })?;
    Ok(Match { id, resource })
}

/// Returns the searchset of what one search found.
fn answer(
    facade: &Facade,
    resource_type: &str,
    request: &Request,
    found: &Found,
) -> Result<Response, Refusal> {
    let base = facade.settings().base_url.as_str();
    let fingerprint = cursor::fingerprint(resource_type, &request.used);
    let cursor_at = |offset: i64| {
        cursor::encode(
            Position {
                declaration: 0,
                offset,
                skip: 0,
            },
            &fingerprint,
        )
    };
    let page_url = |cursor: Option<&str>| {
        bundle::page_url(base, resource_type, &request.used, request.count, cursor)
    };
    let next = found
        .more
        .then(|| cursor_at(request.offset.saturating_add(i64::from(request.count))));
    let current = (request.offset > 0).then(|| cursor_at(request.offset));
    let envelope = bundle::Envelope {
        id: &uuid::Uuid::new_v4().to_string(),
        last_updated: &jiff::Timestamp::now().to_string(),
        base,
        resource_type,
        total: found.total,
        self_link: page_url(current.as_deref()),
        next_link: next.as_deref().map(|cursor| page_url(Some(cursor))),
        outcome: request
            .ignored
            .iter()
            .map(|name| {
                Issue::warning(IssueType::NotSupported)
                    .diagnosing(format!(
                        "{name} is not supported here and was ignored, as Prefer: handling=lenient asks"
                    ))
                    .at(name.clone())
            })
            .collect(),
    };
    let body = bundle::searchset(&envelope, &found.matches).map_err(|error| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(crate::facade::outcome::chain(&error)),
        )
    })?;
    tracing::info!(
        resource_type,
        matches = found.matches.len(),
        paged = found.more,
        counted = found.total.is_some(),
        "the party search answered"
    );
    Ok(reply::resource(StatusCode::OK, &body, &[]))
}

/// Returns the party uids `rows` name.
fn uids(rows: &[ResultSetRow]) -> Result<Vec<String>, Refusal> {
    identifier::uids(rows)
        .map_err(|error| write::refuse(&status::Answer::new(status::UPSTREAM, error.to_string())))
}

/// Runs one query and returns its rows.
async fn run(client: &CdrClient, aql: &AdhocQueryExecute) -> Result<Vec<ResultSetRow>, Refusal> {
    let answered = client
        .query_aql(aql)
        .await
        .map_err(|error| write::refuse(&status::of_client_error(&error)))?;
    match answered.outcome {
        QueryExecuteAdhocQueryBodyOutcome::Ok { body, .. } => Ok(body.rows),
        QueryExecuteAdhocQueryBodyOutcome::BadRequest { .. }
        | QueryExecuteAdhocQueryBodyOutcome::RequestTimeout { .. } => {
            Err(write::refuse(&refused(&answered.upstream)))
        }
    }
}

/// Returns the answer a CDR refusal of a party query renders as: the
/// extension's row for a `400`, the query timeout row for a `408`, the
/// diagnostics naming the extension and carrying the CDR's words.
fn refused(upstream: &crate::cdr::error::Upstream) -> status::Answer {
    let body = ErrorBody::from_bytes(upstream.body().as_bytes().to_vec());
    let words = status::diagnostics(upstream.status(), &body);
    if upstream.status() == StatusCode::REQUEST_TIMEOUT {
        return status::Answer::new(
            status::QUERY_TIMEOUT,
            format!(
                "the {} party query did not finish: {words}",
                identifier::EXTENSION
            ),
        );
    }
    status::Answer::new(
        status::EXTENSION_REFUSED,
        format!(
            "the CDR refused the {} party query, so it does not serve the extension or this \
             query of it: {words}",
            identifier::EXTENSION
        ),
    )
}

/// Returns a `400` with `not-supported` diagnosing `text`.
fn bad_request(text: &str) -> Refusal {
    reply::refusal(
        StatusCode::BAD_REQUEST,
        Issue::error(IssueType::NotSupported).diagnosing(text.to_owned()),
    )
}
