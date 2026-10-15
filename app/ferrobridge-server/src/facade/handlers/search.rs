// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The search interaction: `GET [base]/{type}?…` and
//! `POST [base]/{type}/_search` (<https://hl7.org/fhir/R4/http.html#search>).
//!
//! The request is read against the type's declared parameters, the subject
//! parameters and `_id` resolve through the identity map, and each
//! declaration runs one AQL query paged by the CDR's `offset` and `fetch`.
//! A page reads one row more than it answers, so a `next` link is written
//! only when a row is left. Every row is mapped back through the context's
//! compiled program as a read maps it, and every CDR refusal reaches the wire
//! through the status table: a search never answers an empty Bundle for a
//! failed call (`docs/architecture.md` §9).

use std::collections::BTreeSet;

use axum::response::Response;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;
use http::header;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::generated::query::ResultSetRow;
use openehr_its::rest::generated::query::client::QueryExecuteAdhocQueryBodyOutcome;
use openehr_rm::v1_2::composition::composition::Composition;

use crate::cdr::CdrClient;
use crate::cdr::ids::EhrId;
use crate::facade::Facade;
use crate::facade::handlers::Body;
use crate::facade::handlers::Refusal;
use crate::facade::handlers::read;
use crate::facade::handlers::render;
use crate::facade::handlers::write;
use crate::facade::identity::FhirResourceId;
use crate::facade::identity::PersonId;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::search::Declared;
use crate::facade::search::aql;
use crate::facade::search::aql::Conditions;
use crate::facade::search::aql::OrderKey;
use crate::facade::search::bundle;
use crate::facade::search::bundle::Match;
use crate::facade::search::criteria;
use crate::facade::search::criteria::Criteria;
use crate::facade::search::criteria::Handling;
use crate::facade::search::criteria::Subject;
use crate::facade::search::criteria::Total;
use crate::facade::search::cursor;
use crate::facade::search::cursor::Position;
use crate::facade::search::plan;
use crate::facade::search::plan::Resolved;
use crate::facade::status;

/// The media type of a `POST _search` body.
///
/// The parameters travel "as an application/x-www-form-urlencoded submission
/// for a POST" (<https://hl7.org/fhir/R4/search.html>, §3.1.1.2).
const FORM: &str = "application/x-www-form-urlencoded";

/// `GET [base]/{type}?…`.
pub(crate) async fn search_get(
    facade: &Facade,
    resource_type: &str,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Response, Refusal> {
    crate::facade::handlers::guard(headers, uri, Body::Absent)?;
    let pairs = pairs_of(uri.query().unwrap_or_default().as_bytes());
    search(facade, resource_type, headers, &pairs).await
}

/// `POST [base]/{type}/_search`, the parameters in the URL and the form body.
pub(crate) async fn search_post(
    facade: &Facade,
    resource_type: &str,
    headers: &HeaderMap,
    uri: &Uri,
    body: &[u8],
) -> Result<Response, Refusal> {
    crate::facade::handlers::guard(headers, uri, Body::Absent)?;
    check_form(headers)?;
    let mut pairs = pairs_of(uri.query().unwrap_or_default().as_bytes());
    pairs.extend(pairs_of(body));
    search(facade, resource_type, headers, &pairs).await
}

/// Returns the `name=value` pairs of a query string or a form body.
fn pairs_of(bytes: &[u8]) -> Vec<(String, String)> {
    url::form_urlencoded::parse(bytes)
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect()
}

/// Checks that a `POST _search` body is a form.
fn check_form(headers: &HeaderMap) -> Result<(), Refusal> {
    let found = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|text| {
            text.split(';')
                .next()
                .unwrap_or(text)
                .trim()
                .to_ascii_lowercase()
        });
    if found.as_deref() == Some(FORM) {
        return Ok(());
    }
    Err(reply::refusal(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        Issue::error(IssueType::NotSupported).diagnosing(format!(
            "a POST _search carries its parameters as {FORM}, and this request sent {}",
            found.as_deref().unwrap_or("no Content-Type")
        )),
    ))
}

/// Runs one search.
async fn search(
    facade: &Facade,
    resource_type: &str,
    headers: &HeaderMap,
    pairs: &[(String, String)],
) -> Result<Response, Refusal> {
    crate::facade::handlers::supported(facade, resource_type)?;
    let declarations = facade.search().of_type(resource_type);
    let Some(first) = declarations.first() else {
        return Err(reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotSupported).diagnosing(format!(
                "no search is declared for {resource_type}; GET {}/metadata names the interactions each type supports",
                crate::facade::BASE_PATH
            )),
        ));
    };
    let base = facade.settings().base_url.as_str();
    let criteria = criteria::read(pairs, first, Handling::of(headers), base).map_err(|errors| {
        let issues: Vec<Issue> = errors.iter().map(criteria::CriteriaError::issue).collect();
        reply::refusals(StatusCode::BAD_REQUEST, &issues, &[])
    })?;
    let fingerprint = cursor::fingerprint(resource_type, &criteria.used);
    let start = start_of(&criteria, &fingerprint, declarations.len())?;
    let resolved = resolve(facade, resource_type, &criteria)?;
    let mut plans = Vec::with_capacity(declarations.len());
    for declared in declarations {
        let conditions = plan::conditions(declared, &criteria, &resolved)
            .map_err(|error| reply::refusal(StatusCode::BAD_REQUEST, error.issue()))?;
        plans.push(Plan {
            declared,
            conditions,
            order: plan::order(declared, &criteria),
        });
    }
    let limits = facade.search_limits();
    let client = facade.client_for(headers);
    let page_size = criteria
        .count
        .unwrap_or(limits.default_count)
        .min(limits.max_count);
    // NOTE: R4 search.html §3.1.1.5.3: "if _count has the value 0, this shall be
    // treated the same as _summary=count", a Bundle.total and no entries or links.
    let (matches, next, total) = if criteria.summary_count || page_size == 0 {
        (Vec::new(), None, Some(count_all(&client, &plans).await?))
    } else {
        let (matches, next) = page(facade, &client, &plans, start, page_size).await?;
        let total = total_of(&client, &plans, &criteria, start, next, matches.len()).await?;
        (matches, next, total)
    };
    let self_cursor = criteria.cursor.as_deref();
    let envelope = bundle::Envelope {
        id: &uuid::Uuid::new_v4().to_string(),
        last_updated: &jiff::Timestamp::now().to_string(),
        base,
        resource_type,
        total,
        self_link: bundle::page_url(base, resource_type, &criteria.used, page_size, self_cursor),
        next_link: next.map(|position| {
            bundle::page_url(
                base,
                resource_type,
                &criteria.used,
                page_size,
                Some(&cursor::encode(position, &fingerprint)),
            )
        }),
        outcome: ignored_issues(&criteria),
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
        declarations = declarations.len(),
        paged = next.is_some(),
        "the search answered"
    );
    Ok(reply::resource(StatusCode::OK, &body, &[]))
}

/// One declaration with the query the request makes of it.
struct Plan<'a> {
    /// The declaration.
    declared: &'a Declared,
    /// The conditions, or `None` when it can match nothing.
    conditions: Option<Conditions>,
    /// The `ORDER BY`.
    order: Vec<OrderKey>,
}

/// Returns where the asked page starts.
fn start_of(
    criteria: &Criteria,
    fingerprint: &str,
    declarations: usize,
) -> Result<Position, Refusal> {
    let Some(text) = criteria.cursor.as_deref() else {
        return Ok(Position::START);
    };
    let invalid = |reason: String| {
        reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::Invalid)
                .diagnosing(reason)
                .at(criteria::CURSOR),
        )
    };
    let position = cursor::decode(text, fingerprint).map_err(|error| invalid(error.to_string()))?;
    if position.declaration >= declarations {
        return Err(invalid(format!(
            "`{text}` names a page past the end of this search"
        )));
    }
    Ok(position)
}

/// Resolves the subject parameters and `_id` through the identity map.
///
/// A subject or an id the map does not know resolves to nothing, which the
/// plan reads as a declaration that matches nothing.
fn resolve(facade: &Facade, resource_type: &str, criteria: &Criteria) -> Result<Resolved, Refusal> {
    let namespace = facade.settings().subject_namespace.as_str();
    let mut resolved = Resolved::default();
    for subjects in &criteria.subjects {
        let mut ehrs: Vec<EhrId> = Vec::new();
        for subject in subjects {
            let Subject::Literal(reference) = subject else {
                continue;
            };
            // NOTE: no specification governs this: our own design, a reference no
            // person key can hold names no subject the map binds, so it matches none.
            let Ok(person) = PersonId::new(namespace, reference) else {
                continue;
            };
            let found = facade
                .store()
                .ehr_of(&person)
                .map_err(|error| write::store_refusal(&error))?;
            if let Some(ehr) = found
                && !ehrs.contains(&ehr)
            {
                ehrs.push(ehr);
            }
        }
        resolved.ehrs.push(ehrs);
    }
    for ids in &criteria.ids {
        let mut bindings = Vec::new();
        for id in ids {
            // NOTE: R4 resource.html: an id outside the `id` grammar names no
            // resource of this server, so it matches nothing rather than failing.
            let Ok(internal) = FhirResourceId::new(id) else {
                continue;
            };
            if let Some(binding) = read::binding(facade, resource_type, &internal)? {
                bindings.push(binding);
            }
        }
        resolved.ids.push(bindings);
    }
    Ok(resolved)
}

/// Reads one page, starting at `start`, of at most `size` compositions.
///
/// A query that binds a node answers a row per matching node, so the rows of
/// one composition sit together under the composition-level order, and the
/// page reads one entry per composition: a row of a composition the page
/// already holds is passed over. Each query asks one row more than the page
/// still has room for. The first row of a composition the page has no room
/// for is where the next page starts, and a declaration that runs out hands
/// over to the next one at its first row.
async fn page(
    facade: &Facade,
    client: &CdrClient,
    plans: &[Plan<'_>],
    start: Position,
    size: u32,
) -> Result<(Vec<Match>, Option<Position>), Refusal> {
    let mut matches = Vec::new();
    let mut room = usize::try_from(size).unwrap_or(usize::MAX);
    let mut offset = start.offset;
    for (index, plan) in plans.iter().enumerate().skip(start.declaration) {
        let Some(conditions) = plan.conditions.as_ref() else {
            offset = 0;
            continue;
        };
        let mut held: BTreeSet<String> = BTreeSet::new();
        loop {
            let fetch = i64::try_from(room.saturating_add(1)).unwrap_or(i64::MAX);
            let request = aql::rows_request(conditions, &plan.order, offset, fetch);
            let rows = rows_of(client, &request).await?;
            for (position, row) in rows.iter().enumerate() {
                let version = version_of(row, position)?;
                if held.contains(version) {
                    continue;
                }
                if room == 0 {
                    let skipped = i64::try_from(position).unwrap_or(i64::MAX);
                    return Ok((
                        matches,
                        Some(Position {
                            declaration: index,
                            offset: offset.saturating_add(skipped),
                        }),
                    ));
                }
                held.insert(version.to_owned());
                matches.push(render_row(facade, client, plan.declared, row, position)?);
                room = room.saturating_sub(1);
            }
            let fetched = i64::try_from(rows.len()).unwrap_or(i64::MAX);
            if fetched < fetch {
                break;
            }
            offset = offset.saturating_add(fetched);
        }
        offset = 0;
    }
    Ok((matches, None))
}

/// Returns the version uid one row names, or the `502` a row without one
/// answers.
fn version_of(row: &ResultSetRow, position: usize) -> Result<&str, Refusal> {
    row.get(aql::column::VERSION_UID)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| malformed_row(position, "version_uid"))
}

/// Returns the `Bundle.total` of one page, when it is known or asked for.
///
/// A first page that leaves no next page holds every match, so its size is
/// the exact total at no cost. Otherwise the count queries run only when the
/// client asked for a total (<https://hl7.org/fhir/R4/search.html>,
/// §3.1.1.5.2), and `_total=none` never gets one.
async fn total_of(
    client: &CdrClient,
    plans: &[Plan<'_>],
    criteria: &Criteria,
    start: Position,
    next: Option<Position>,
    answered: usize,
) -> Result<Option<u64>, Refusal> {
    match criteria.total {
        Some(Total::None) => Ok(None),
        _ if start == Position::START && next.is_none() => {
            Ok(Some(u64::try_from(answered).unwrap_or(u64::MAX)))
        }
        Some(Total::Accurate | Total::Estimate) => Ok(Some(count_all(client, plans).await?)),
        None => Ok(None),
    }
}

/// Returns the number of matches across every declaration.
async fn count_all(client: &CdrClient, plans: &[Plan<'_>]) -> Result<u64, Refusal> {
    let mut total: u64 = 0;
    for plan in plans {
        let Some(conditions) = plan.conditions.as_ref() else {
            continue;
        };
        let rows = rows_of(client, &aql::count_request(conditions)).await?;
        let counted = rows
            .first()
            .and_then(|row| row.first())
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                malformed_answer("the CDR answered the count query without a whole number")
            })?;
        total = total.saturating_add(counted);
    }
    Ok(total)
}

/// Runs one query and returns its rows, or the refusal the status table
/// gives its failure.
async fn rows_of(
    client: &CdrClient,
    request: &openehr_its::rest::generated::query::AdhocQueryExecute,
) -> Result<Vec<ResultSetRow>, Refusal> {
    let answered = client
        .query_aql(request)
        .await
        .map_err(|error| write::refuse(&status::of_client_error(&error)))?;
    match answered.outcome {
        QueryExecuteAdhocQueryBodyOutcome::Ok { body, .. } => Ok(body.rows),
        QueryExecuteAdhocQueryBodyOutcome::BadRequest { .. }
        | QueryExecuteAdhocQueryBodyOutcome::RequestTimeout { .. } => {
            Err(write::refuse(&status::of_query_refusal(&answered.upstream)))
        }
    }
}

/// Maps one row back to the resource it holds.
fn render_row(
    facade: &Facade,
    client: &CdrClient,
    declared: &Declared,
    row: &ResultSetRow,
    position: usize,
) -> Result<Match, Refusal> {
    let text = |column: usize| row.get(column).and_then(serde_json::Value::as_str);
    let ehr_id = text(aql::column::EHR_ID)
        .and_then(|value| EhrId::new(value).ok())
        .ok_or_else(|| malformed_row(position, "ehr_id"))?;
    let version = text(aql::column::VERSION_UID)
        .and_then(|value| ObjectVersionId::new(value).ok())
        .ok_or_else(|| malformed_row(position, "version_uid"))?;
    let composition = row
        .get(aql::column::COMPOSITION)
        .and_then(|value| serde_json::to_string(value).ok())
        .and_then(|value| openehr_its::json::from_canonical_json::<Composition>(&value).ok())
        .ok_or_else(|| malformed_row(position, "composition"))?;
    let canonical = read::canonical(facade, &composition, declared.template_id())?;
    let program = facade
        .programs()
        .by_context(declared.context())
        .ok_or_else(|| {
            reply::refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(format!(
                    "the search declaration {} names the context {}, which this deployment did not load",
                    declared.file().display(),
                    declared.context()
                )),
            )
        })?;
    let (id, _binding) = facade.ingest(client.clone()).bind(
        program,
        declared.resource_type(),
        &ehr_id,
        &version,
        &canonical,
    )?;
    let source = render::composition_url(client.base_url(), &ehr_id, &version)?;
    let subject = render::subject_of(facade, &ehr_id)?;
    let rendered = render::render(
        program.program(),
        program.index(),
        &canonical,
        render::Origin {
            id: &id,
            version: &version,
            source: &source,
            subject: subject.as_ref(),
        },
    )?;
    render::log_outcome(&rendered);
    Ok(Match {
        id,
        resource: rendered.body,
    })
}

/// Returns the `502` a row the bridge cannot read answers.
///
/// The row is clinical content, so the refusal names the column alone.
fn malformed_row(position: usize, column: &str) -> Refusal {
    malformed_answer(&format!(
        "the CDR answered row {position} of the search query with no readable {column}"
    ))
}

/// Returns the `502` an AQL answer the bridge cannot read answers.
fn malformed_answer(detail: &str) -> Refusal {
    write::refuse(&status::Answer::new(status::UPSTREAM, detail.to_owned()))
}

/// Returns the warning issues naming the parameters lenient handling ignored.
fn ignored_issues(criteria: &Criteria) -> Vec<Issue> {
    criteria
        .ignored
        .iter()
        .map(|name| {
            Issue::warning(IssueType::NotSupported)
                .diagnosing(format!(
                    "{name} is not supported here and was ignored, as Prefer: handling=lenient asks"
                ))
                .at(name.clone())
        })
        .collect()
}
