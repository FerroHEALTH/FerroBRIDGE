// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The App State interactions.
//!
//! SMART App Launch 2.2.0, Persisting App State, §App State Interactions:
//! `POST /Basic`, `PUT /Basic/[id]` and `DELETE /Basic/[id]` with
//! `If-Match`, and `GET /Basic?code={}&subject={}`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::Path;
use axum::extract::State;
use axum::response::Response;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use fhir_search::token::Token;
use fhir_types::codec::Object;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;
use http::header;

use crate::facade::Facade;
use crate::facade::app_state::BASIC;
use crate::facade::app_state::CODE;
use crate::facade::app_state::SUBJECT;
use crate::facade::app_state::StateCode;
use crate::facade::app_state::admit;
use crate::facade::app_state::profile;
use crate::facade::app_state::profile::Intent;
use crate::facade::app_state::reach;
use crate::facade::app_state::store;
use crate::facade::handlers::Body;
use crate::facade::handlers::guard;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::owned::Change;
use crate::facade::owned::Held;
use crate::facade::owned::OwnedStore;
use crate::facade::owned::answer::Search;
use crate::facade::owned::answer::Written;
use crate::facade::owned::answer::answer;
use crate::facade::owned::answer::encoded;
use crate::facade::owned::answer::id_of;
use crate::facade::owned::answer::scanned;
use crate::facade::owned::answer::searchset;
use crate::facade::owned::answer::store_refusal;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// `POST [base]/Basic`: persists new App State (§Create).
///
/// # Errors
/// Returns the media refusals, the refusals of [`profile::read`], the `403`
/// of a token that does not reach the state, and a `500` when the store
/// fails.
pub async fn create(
    State(facade): State<Arc<Facade>>,
    access: Access,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Response, Refusal> {
    let facade = facade.as_ref();
    let (store, state) = written(facade, &access, (&headers, &uri, &body), Intent::Create)?;
    let created = store
        .create(BASIC, state.resource)
        .map_err(|error| store_refusal(&error))?;
    Ok(answer(
        facade,
        StatusCode::CREATED,
        &headers,
        Written {
            resource_type: BASIC,
            resource: &created,
            version: 1,
            created: true,
        },
    ))
}

/// `PUT [base]/Basic/[id]`: replaces App State under `If-Match` (§Updates).
///
/// # Errors
/// Returns `412` for a missing or stale `If-Match`, a deleted id, or a
/// changed `subject` or `code`; `404` for an id never issued; the refusals
/// of [`profile::read`]; the `403` of a token that does not reach the state;
/// and a `500` when the store fails.
pub async fn update(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Response, Refusal> {
    let facade = facade.as_ref();
    let id = id.as_str();
    let expected = if_match(&headers)?;
    let (store, state) = written(facade, &access, (&headers, &uri, &body), Intent::Update(id))?;
    let base = facade.settings().base_url.clone();
    let held = store
        .change(BASIC, id, |held| {
            let prior = current(held, id, &expected)?;
            let persisted = profile::read(&encoded(prior)?, Intent::Update(id), &base)?;
            if persisted.code != state.code || persisted.subject != state.subject {
                return Err(precondition(
                    "Basic.subject and Basic.code SHALL NOT change from the persisted values",
                ));
            }
            Ok(Change::Put(state.resource.clone()))
        })
        .map_err(|error| store_refusal(&error))??;
    match held {
        Held::Live { version, resource } => Ok(answer(
            facade,
            StatusCode::OK,
            &headers,
            Written {
                resource_type: BASIC,
                resource: &resource,
                version,
                created: false,
            },
        )),
        Held::Deleted { .. } => Err(precondition("the App State was deleted")),
    }
}

/// `DELETE [base]/Basic/[id]`: deletes App State under `If-Match`
/// (§Deletes).
///
/// # Errors
/// Returns `412` for a missing or stale `If-Match` or an id already deleted,
/// `404` for an id never issued, the `403` of a token that does not reach
/// the state, and a `500` when the store fails.
pub async fn delete(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, Refusal> {
    let facade = facade.as_ref();
    let id = id.as_str();
    let store = store(facade)?;
    guard(&headers, &uri, Body::Absent)?;
    let expected = if_match(&headers)?;
    let base = facade.settings().base_url.clone();
    store
        .change(BASIC, id, |held| {
            let prior = current(held, id, &expected)?;
            let persisted = profile::read(&encoded(prior)?, Intent::Update(id), &base)?;
            admit(
                facade,
                &access,
                Permission::Delete,
                &persisted.code,
                persisted.subject.as_deref(),
            )?;
            Ok::<_, Refusal>(Change::Delete)
        })
        .map_err(|error| store_refusal(&error))??;
    Ok(reply::minimal(StatusCode::NO_CONTENT, &[]))
}

/// What a search asks of the subject.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Subject {
    /// No subject parameter: every subject the token reaches.
    Unnamed,
    /// `subject=[reference]`: exactly this absolute reference.
    Is(String),
    /// `subject:missing=true`: global state.
    Missing,
    /// `subject:missing=false`: state about any subject.
    Present,
}

/// Answers the search `parameters` ask for.
///
/// # Errors
/// Returns `400` for a search without `code`, with a `code` that is not
/// `system|code`, or with a parameter the endpoint does not take; the `403`
/// of a token that does not reach the subject named; and a `500` when the
/// store fails.
pub fn searched(search: &Search<'_>) -> Result<Response, Refusal> {
    let Search {
        facade,
        access,
        uri,
        parameters,
    } = *search;
    let store = store(facade)?;
    let (code, subject) = criteria(parameters)?;
    match &subject {
        Subject::Is(reference) => {
            admit(facade, access, Permission::Search, &code, Some(reference))?;
        }
        Subject::Missing => admit(facade, access, Permission::Search, &code, None)?,
        Subject::Unnamed | Subject::Present => {
            if reach(access, Permission::Search, &code).is_none() {
                return Err(crate::facade::guard::forbidden());
            }
        }
    }
    let base = facade.settings().base_url.clone();
    let mut matched = Vec::new();
    let held = scanned(store, BASIC, |resource, bytes| {
        profile::read(bytes, Intent::Update(id_of(resource)), &base)
    })?;
    for (resource, persisted) in held {
        let fits = persisted.code == code
            && match &subject {
                Subject::Is(reference) => persisted.subject.as_ref() == Some(reference),
                Subject::Missing => persisted.subject.is_none(),
                Subject::Present => persisted.subject.is_some(),
                Subject::Unnamed => true,
            };
        let reached = fits
            && admit(
                facade,
                access,
                Permission::Search,
                &code,
                persisted.subject.as_deref(),
            )
            .is_ok();
        if reached {
            matched.push(resource);
        }
    }
    searchset(facade, BASIC, uri, matched)
}

/// Returns the store and the App State a create or an update carries, checked
/// against the profile and admitted for the interaction `intent` names.
fn written<'a>(
    facade: &'a Facade,
    access: &Access,
    (headers, uri, body): (&HeaderMap, &Uri, &[u8]),
    intent: Intent<'_>,
) -> Result<(&'a OwnedStore, profile::State), Refusal> {
    let store = store(facade)?;
    guard(headers, uri, Body::Present)?;
    let state = profile::read(body, intent, &facade.settings().base_url)?;
    let permission = match intent {
        Intent::Create => Permission::Create,
        Intent::Update(_) => Permission::Update,
    };
    admit(
        facade,
        access,
        permission,
        &state.code,
        state.subject.as_deref(),
    )?;
    Ok((store, state))
}

/// Returns the state code and the subject `parameters` ask for.
fn criteria(parameters: &[(String, String)]) -> Result<(StateCode, Subject), Refusal> {
    let invalid = |name: &str, text: &str| {
        reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::Invalid)
                .diagnosing(text.to_owned())
                .at(name.to_owned()),
        )
    };
    let mut code = None;
    let mut subject = Subject::Unnamed;
    for (name, value) in parameters {
        match name.as_str() {
            CODE if code.is_none() => match Token::parse(value) {
                Ok(Token::SystemCode {
                    system,
                    code: named,
                }) => {
                    code = Some(StateCode {
                        system,
                        code: named,
                    });
                }
                _ => {
                    return Err(invalid(
                        CODE,
                        "code names a state code as system|code, both present",
                    ));
                }
            },
            SUBJECT if subject == Subject::Unnamed => subject = Subject::Is(value.clone()),
            "subject:missing" if subject == Subject::Unnamed => match value.as_str() {
                "true" => subject = Subject::Missing,
                "false" => subject = Subject::Present,
                _ => return Err(invalid(name, "subject:missing is true or false")),
            },
            "_format" | "_pretty" => {}
            _ => {
                return Err(reply::refusal(
                    StatusCode::BAD_REQUEST,
                    Issue::error(IssueType::NotSupported)
                        .diagnosing(
                            "an App State search takes code once and subject or subject:missing once",
                        )
                        .at(name.clone()),
                ));
            }
        }
    }
    let code = code.ok_or_else(|| invalid(CODE, "an App State search names its state code"))?;
    Ok((code, subject))
}

/// Returns the version `If-Match` names: "The request SHALL include an
/// `If-Match` header with an `ETag`" (§Updates, §Deletes).
fn if_match(headers: &HeaderMap) -> Result<String, Refusal> {
    let value = headers.get(header::IF_MATCH).ok_or_else(|| {
        precondition("the request SHALL include If-Match with the ETag it follows")
    })?;
    let text = value
        .to_str()
        .map_err(|_unreadable| precondition("the If-Match header is not text"))?
        .trim();
    let tag = text.strip_prefix("W/").unwrap_or(text);
    tag.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .filter(|version| !version.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| precondition("If-Match carries one entity tag, W/\"[versionId]\""))
}

/// Returns the live resource `held` holds when its version is `expected`.
///
/// "Servers SHALL process any subsequent requests about this `Basic.id` as a
/// failed precondition" once it is deleted (§Deletes).
fn current<'a>(held: Option<&'a Held>, id: &str, expected: &str) -> Result<&'a Object, Refusal> {
    match held {
        None => Err(reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotFound).diagnosing(format!("no {BASIC}/{id} was created")),
        )),
        Some(Held::Deleted { .. }) => Err(precondition("the App State was deleted")),
        Some(Held::Live { version, resource }) => {
            if version.to_string() == expected {
                Ok(resource)
            } else {
                Err(precondition(
                    "If-Match does not name the most recent version",
                ))
            }
        }
    }
}

/// Returns the `412` with `text`.
fn precondition(text: &str) -> Refusal {
    reply::refusal(
        StatusCode::PRECONDITION_FAILED,
        Issue::error(IssueType::Conflict).diagnosing(text.to_owned()),
    )
}
