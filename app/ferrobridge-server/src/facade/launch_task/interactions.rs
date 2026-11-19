// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The launch `Task` interactions.
//!
//! `POST [base]/Task`, `GET [base]/Task/[id]`, and the search by `patient`,
//! `status` and `code` (FHIR R4 `http.html`, `task.html#search`).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::Path;
use axum::extract::State;
use axum::response::Response;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Compartment;
use ferrohealth_smart::scope::Permission;
use fhir_search::escape::split;
use fhir_search::escape::unescape;
use fhir_search::token::System;
use fhir_search::token::Token;
use fhir_types::r4::codeable_concept::CodeableConcept;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;

use crate::facade::Facade;
use crate::facade::handlers::Body;
use crate::facade::handlers::guard;
use crate::facade::launch_task::CODE;
use crate::facade::launch_task::PATIENT;
use crate::facade::launch_task::STATUS;
use crate::facade::launch_task::TASK;
use crate::facade::launch_task::admit;
use crate::facade::launch_task::local;
use crate::facade::launch_task::profile;
use crate::facade::launch_task::profile::LaunchTask;
use crate::facade::launch_task::store;
use crate::facade::media;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::owned::Held;
use crate::facade::owned::answer::Written;
use crate::facade::owned::answer::answer;
use crate::facade::owned::answer::encoded;
use crate::facade::owned::answer::searchset;
use crate::facade::owned::answer::store_refusal;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// The code system of `Task.status` (R4 `valueset-task-status.html`).
const TASK_STATUS: &str = "http://hl7.org/fhir/task-status";

/// `POST [base]/Task`: stores a launch `Task` under a new id.
///
/// "If an id is provided, the server SHALL ignore it"
/// (<https://hl7.org/fhir/R4/http.html#create>), so the store writes its own.
///
/// # Errors
/// Returns the media refusals, the refusals of [`profile::read`], the `403`
/// of a token that does not reach the `Task`'s patient, and a `500` when the
/// store fails.
pub async fn create(
    State(facade): State<Arc<Facade>>,
    access: Access,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Response, Refusal> {
    let store = store(&facade)?;
    guard(&headers, &uri, Body::Present)?;
    let task = profile::read(&body)?;
    admit(
        &facade,
        &access,
        Permission::Create,
        task.patient.as_deref(),
    )?;
    let created = store
        .create(TASK, task.resource)
        .map_err(|error| store_refusal(&error))?;
    Ok(answer(
        &facade,
        StatusCode::CREATED,
        &headers,
        Written {
            resource_type: TASK,
            resource: &created,
            version: 1,
            created: true,
        },
    ))
}

/// `GET [base]/Task/[id]`: reads one launch `Task`.
///
/// # Errors
/// Returns `404` for an id never issued, the `403` of a token that does not
/// reach the `Task`'s patient, and a `500` when the store fails.
pub async fn read(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path(id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, Refusal> {
    let store = store(&facade)?;
    guard(&headers, &uri, Body::Absent)?;
    let held = store
        .get(TASK, &id)
        .map_err(|error| store_refusal(&error))?;
    let Some(Held::Live { version, resource }) = held else {
        return Err(reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotFound).diagnosing(format!("no {TASK}/{id} is held")),
        ));
    };
    let task = profile::read(&encoded(&resource)?)?;
    admit(&facade, &access, Permission::Read, task.patient.as_deref())?;
    Ok(answer(
        &facade,
        StatusCode::OK,
        &headers,
        Written {
            resource_type: TASK,
            resource: &resource,
            version,
            created: false,
        },
    ))
}

/// `GET [base]/Task?…`: the launch `Task`s the parameters match.
///
/// # Errors
/// Returns the refusals of the search the parameters ask for.
pub async fn search(
    State(facade): State<Arc<Facade>>,
    access: Access,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Response, Refusal> {
    let parameters = media::query_pairs(uri.query().unwrap_or_default().as_bytes());
    searched(&facade, &access, &headers, &uri, &parameters)
}

/// `POST [base]/Task/_search`: the same search, its parameters in a form
/// body beside the query (<https://hl7.org/fhir/R4/http.html#search>).
///
/// # Errors
/// Returns the refusals of the search the parameters ask for.
pub async fn search_form(
    State(facade): State<Arc<Facade>>,
    access: Access,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<Response, Refusal> {
    let mut parameters = media::query_pairs(uri.query().unwrap_or_default().as_bytes());
    parameters.extend(media::query_pairs(&body));
    searched(&facade, &access, &headers, &uri, &parameters)
}

/// What one search asks for: each parameter a set of values any of which
/// matches, and every parameter given matching (R4 `search.html` §3.1.1.4.3
/// and §3.1.1.4.4).
#[derive(Debug, Default)]
struct Criteria {
    /// The patients `patient` names, as [`local`] writes them.
    patients: Vec<Vec<String>>,
    /// The statuses `status` names.
    statuses: Vec<Vec<Token>>,
    /// The codes `code` names.
    codes: Vec<Vec<Token>>,
}

/// Answers the search `parameters` ask for, the `Task`s the token does not
/// reach left out.
///
/// # Errors
/// Returns `400` for a parameter the endpoint does not take or a value it
/// cannot read, the `403` of a token with no `Task` search, and a `500` when
/// the store fails.
fn searched(
    facade: &Facade,
    access: &Access,
    headers: &HeaderMap,
    uri: &Uri,
    parameters: &[(String, String)],
) -> Result<Response, Refusal> {
    let store = store(facade)?;
    guard(headers, uri, Body::Absent)?;
    let criteria = criteria(facade, parameters)?;
    let reach = access
        .reach(Permission::Search, TASK)
        .ok_or_else(crate::facade::guard::forbidden)?;
    let mut matched = Vec::new();
    for resource in store.scan(TASK).map_err(|error| store_refusal(&error))? {
        let task = profile::read(&encoded(&resource)?)?;
        let reached = match &reach {
            Compartment::Any => true,
            Compartment::Patient(_) => {
                admit(facade, access, Permission::Search, task.patient.as_deref()).is_ok()
            }
        };
        if reached && criteria.match_task(facade, &task) {
            matched.push(resource);
        }
    }
    searchset(facade, TASK, uri, matched)
}

/// Returns what `parameters` ask for.
fn criteria(facade: &Facade, parameters: &[(String, String)]) -> Result<Criteria, Refusal> {
    let refused = |name: &str, text: String| {
        reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::Invalid)
                .diagnosing(text)
                .at(name.to_owned()),
        )
    };
    let tokens = |name: &str, value: &str| -> Result<Vec<Token>, Refusal> {
        split(value, ',')
            .iter()
            .map(|part| {
                Token::parse(part).map_err(|error| refused(name, format!("{name}: {error}")))
            })
            .collect()
    };
    let mut criteria = Criteria::default();
    for (name, value) in parameters {
        match name.as_str() {
            PATIENT => {
                let patients = split(value, ',')
                    .iter()
                    .map(|part| {
                        let text =
                            unescape(part).map_err(|error| refused(PATIENT, error.to_string()))?;
                        let reference = if text.contains('/') {
                            local(facade, &text)
                        } else {
                            format!("Patient/{text}")
                        };
                        Ok(reference)
                    })
                    .collect::<Result<Vec<String>, Refusal>>()?;
                criteria.patients.push(patients);
            }
            STATUS => criteria.statuses.push(tokens(STATUS, value)?),
            CODE => criteria.codes.push(tokens(CODE, value)?),
            "_format" | "_pretty" => {}
            _ => {
                return Err(reply::refusal(
                    StatusCode::BAD_REQUEST,
                    Issue::error(IssueType::NotSupported)
                        .diagnosing("a launch Task search takes patient, status and code")
                        .at(name.clone()),
                ));
            }
        }
    }
    Ok(criteria)
}

impl Criteria {
    /// Returns whether `task` matches every parameter given.
    fn match_task(&self, facade: &Facade, task: &LaunchTask) -> bool {
        let patient = task
            .patient
            .as_deref()
            .map(|reference| local(facade, reference));
        let status = task.task.status.value.as_deref();
        self.patients
            .iter()
            .all(|any| any.iter().any(|named| patient.as_ref() == Some(named)))
            && self.statuses.iter().all(|any| {
                any.iter().any(|token| {
                    token.code() == status
                        && matches!(token.system(), System::Any | System::Is(TASK_STATUS))
                })
            })
            && self.codes.iter().all(|any| {
                any.iter()
                    .any(|token| coded(task.task.code.as_ref(), token))
            })
    }
}

/// Returns whether a coding of `concept` matches `token` (R4 `search.html`
/// §3.1.1.4.10).
fn coded(concept: Option<&CodeableConcept>, token: &Token) -> bool {
    concept.is_some_and(|concept| {
        concept.coding.iter().any(|coding| {
            let system = coding.system.as_ref().and_then(|uri| uri.value.as_deref());
            let code = coding.code.as_ref().and_then(|code| code.value.as_deref());
            let system_matches = match token.system() {
                System::Any => true,
                System::Absent => system.is_none(),
                System::Is(named) => system == Some(named),
            };
            system_matches && token.code().is_none_or(|named| code == Some(named))
        })
    })
}
