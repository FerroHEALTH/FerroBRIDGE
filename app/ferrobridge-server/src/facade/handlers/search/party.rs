// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search interaction on a party type, by `identifier`, through Cadasto's
//! experimental AQL demographic extension (`crate::facade::search::party`).
//!
//! One page runs the type's configured query with `offset` and one row more
//! than it answers in `fetch`, so a `next` link is written only when a row is
//! left. Each party uid a row names is read over the ITS-REST 1.1.0
//! Demographic API, DEVELOPMENT, and mapped back as a read maps it. A CDR
//! `400` on the query is the extension's refusal row, a `501`, never an empty
//! searchset. Only `identifier`, `_count` and the paging cursor are taken; any
//! other parameter is refused, or ignored and reported when the client sends
//! `Prefer: handling=lenient` (<https://hl7.org/fhir/R4/search.html>,
//! §3.1.1.3). No specification governs a party search: our own design.

use axum::response::Response;
use http::StatusCode;
use openehr_its::rest::client::ErrorBody;
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
use crate::facade::programs::Loaded;
use crate::facade::reply;
use crate::facade::search::bundle;
use crate::facade::search::bundle::Match;
use crate::facade::search::criteria::CURSOR;
use crate::facade::search::criteria::Handling;
use crate::facade::search::cursor;
use crate::facade::search::cursor::Position;
use crate::facade::search::identifier;
use crate::facade::search::party::PARAMETER;
use crate::facade::search::value;
use crate::facade::search::value::Token;
use crate::facade::status;

/// The page-size parameter (<https://hl7.org/fhir/R4/search.html>,
/// §3.1.1.5.3).
const COUNT: &str = "_count";

/// What one party search asks.
struct Request {
    /// The identifier searched.
    token: Token,
    /// The parameters the search used, for the links and the cursor.
    used: Vec<(String, String)>,
    /// The page size.
    count: u32,
    /// The first row of the page.
    offset: i64,
    /// The parameters ignored as `Prefer: handling=lenient` asks.
    ignored: Vec<String>,
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
    let query = facade
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
    let request = read(facade, resource_type, pairs, Handling::of(headers))?;
    let fetch = i64::from(request.count).saturating_add(1);
    let aql = crate::facade::search::party::page(query, &request.token, request.offset, fetch)
        .map_err(|error| bad_request(&format!("{PARAMETER}: {error}")))?;
    let client = facade.client_for(headers);
    let rows = run(&client, &aql).await?;
    let more = rows.len() > usize::try_from(request.count).unwrap_or(usize::MAX);
    let mut uids = identifier::uids(&rows).map_err(|error| {
        write::refuse(&status::Answer::new(status::UPSTREAM, error.to_string()))
    })?;
    uids.truncate(usize::try_from(request.count).unwrap_or(usize::MAX));
    match compartment {
        Compartment::Any => {}
        // NOTE: SMART App Launch 2.x scopes-and-launch-context.html, a `patient/` token
        // reaches the one `Patient` its launch context names, so the page keeps only it.
        Compartment::Patient(ref patient) if resource_type == "Patient" => {
            uids.retain(|uid| uid == patient.as_str());
        }
        Compartment::Patient(_) => return Err(crate::auth::guard::forbidden()),
    }
    let matches = matches_of(facade, &client, loaded, resource_type, &uids).await?;
    let base = facade.settings().base_url.as_str();
    let fingerprint = cursor::fingerprint(resource_type, &request.used);
    let page_url = |cursor: Option<&str>| {
        bundle::page_url(base, resource_type, &request.used, request.count, cursor)
    };
    let next = more.then(|| {
        cursor::encode(
            Position {
                declaration: 0,
                offset: request.offset.saturating_add(i64::from(request.count)),
                skip: 0,
            },
            &fingerprint,
        )
    });
    let current = (request.offset > 0).then(|| {
        cursor::encode(
            Position {
                declaration: 0,
                offset: request.offset,
                skip: 0,
            },
            &fingerprint,
        )
    });
    let envelope = bundle::Envelope {
        id: &uuid::Uuid::new_v4().to_string(),
        last_updated: &jiff::Timestamp::now().to_string(),
        base,
        resource_type,
        total: None,
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
    let body = bundle::searchset(&envelope, &matches).map_err(|error| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(crate::facade::outcome::chain(&error)),
        )
    })?;
    tracing::info!(
        resource_type,
        matches = matches.len(),
        paged = more,
        "the party search answered"
    );
    Ok(reply::resource(StatusCode::OK, &body, &[]))
}

/// Reads each party `uids` names over the Demographic API and maps it back
/// as a read maps it.
async fn matches_of(
    facade: &Facade,
    client: &CdrClient,
    loaded: &Loaded,
    resource_type: &str,
    uids: &[String],
) -> Result<Vec<Match>, Refusal> {
    let demographic = party::demographic(client)?;
    let class = party::read::class_of(loaded)?;
    let mut matches = Vec::with_capacity(uids.len());
    for uid in uids {
        let held = party::get(demographic, class, resource_type, uid).await?;
        let canonical = party::accept(loaded, resource_type, uid, &held)?;
        let resource = party::render(facade, loaded, &canonical, &held.version).await?;
        let id = FhirResourceId::new(uid).map_err(|error| {
            write::refuse(&status::Answer::new(
                status::UPSTREAM,
                format!("the party uid {uid} is no FHIR id: {error}"),
            ))
        })?;
        matches.push(Match { id, resource });
    }
    Ok(matches)
}

/// Reads the parameters of one party search.
fn read(
    facade: &Facade,
    resource_type: &str,
    pairs: &[(String, String)],
    handling: Handling,
) -> Result<Request, Refusal> {
    let mut token = None;
    let mut used = Vec::new();
    let mut count = None;
    let mut cursor_text = None;
    let mut ignored = Vec::new();
    for (name, text) in pairs {
        match name.as_str() {
            PARAMETER if token.is_some() => {
                return Err(bad_request(&format!(
                    "{PARAMETER} is given more than once, and a {resource_type} is searched by one identifier"
                )));
            }
            PARAMETER => {
                if value::split(text, ',').len() > 1 {
                    return Err(bad_request(&format!(
                        "{PARAMETER} names more than one identifier, and a {resource_type} is searched by one"
                    )));
                }
                token = Some(
                    value::token(text)
                        .map_err(|error| bad_request(&format!("{PARAMETER}: {error}")))?,
                );
                used.push((name.clone(), text.clone()));
            }
            COUNT => {
                let parsed = text
                    .parse::<u32>()
                    .ok()
                    .filter(|count| *count > 0)
                    .ok_or_else(|| {
                        bad_request(&format!(
                            "{COUNT} is {text}, and a {resource_type} search takes a page size of 1 or more"
                        ))
                    })?;
                count = Some(parsed);
            }
            CURSOR => cursor_text = Some(text.clone()),
            other if handling == Handling::Lenient => ignored.push(other.to_owned()),
            other => {
                return Err(bad_request(&format!(
                    "{other} is no search parameter of {resource_type}, which is searched by {PARAMETER} only"
                )));
            }
        }
    }
    let token = token.ok_or_else(|| {
        bad_request(&format!(
            "a {resource_type} is searched by {PARAMETER}, and the search names none"
        ))
    })?;
    let limits = facade.search_limits();
    let count = count.unwrap_or(limits.default_count).min(limits.max_count);
    let fingerprint = cursor::fingerprint(resource_type, &used);
    let offset = match cursor_text {
        None => 0,
        Some(text) => {
            let position = cursor::decode(&text, &fingerprint)
                .map_err(|error| bad_request(&format!("{CURSOR}: {error}")))?;
            if position.declaration != 0 || position.skip != 0 {
                return Err(bad_request(&format!(
                    "{CURSOR}: `{text}` is no cursor of a {resource_type} search"
                )));
            }
            position.offset
        }
    };
    Ok(Request {
        token,
        used,
        count,
        offset,
        ignored,
    })
}

/// Runs the page's query and returns its rows.
async fn run(
    client: &CdrClient,
    aql: &openehr_its::rest::generated::query::AdhocQueryExecute,
) -> Result<Vec<openehr_its::rest::generated::query::ResultSetRow>, Refusal> {
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
