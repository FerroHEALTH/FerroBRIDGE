// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The routes of the links lane.
//!
//! The manifest request, the manifest, the `401` of a wrong passcode with
//! `remainingAttempts`, the `404` of a link that is no longer active, the
//! single-use `location` and the direct file request under `U` follow SMART
//! Health Cards and Links 1.0.0, `links-specification`. The person's routes
//! (create, list, revoke) are FerroBRIDGE's own design, and every refusal
//! but the `401` the specification shapes is an `OperationOutcome`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::Request;
use axum::extract::State;
use axum::middleware::Next;
use axum::response::IntoResponse as _;
use axum::response::Response;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrohealth_smart::guard::AuthLayer;
use ferrohealth_smart::guard::Requirement;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use http::HeaderMap;
use http::HeaderValue;
use http::StatusCode;
use http::header;
use secrecy::ExposeSecret as _;
use secrecy::SecretString;
use serde::Deserialize;
use serde::Serialize;

use super::Links;
use super::Place;
use super::record;
use super::record::Actor;
use super::record::Event;
use super::seal;
use super::store;
use super::store::Answer;
use super::store::Fetched;
use super::store::Resolution;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;
use crate::facade::summary;

/// The `contentType` of a FHIR file and its JWE's `cty`: "Servers SHOULD
/// populate the `fhirVersion` parameter" (`links-specification`).
pub(super) const FHIR_CONTENT: &str = "application/fhir+json;fhirVersion=4.0.1";

/// The content type of a file served by location or by a direct request.
const JOSE: &str = "application/jose";

/// The longest `label`: "no longer than 80 characters".
const LABEL_MAX: usize = 80;

/// The longest passcode a link takes (no specification governs this: our
/// own design).
const PASSCODE_MAX: usize = 128;

/// The detail of the `404` of a link that is no longer active.
const INACTIVE: &str = "no active link answers here";

/// The detail of the `403` of a caller with no patient context.
const NO_PERSON: &str =
    "a link is shared by a person, with a token that names their patient record";

/// Returns the gate of the person's routes, refusing with an
/// `OperationOutcome`.
// NOTE: no specification governs this: our own design; the person's routes need a read
// permission on `Patient`, the patient summary's subject.
pub(super) fn guard(smart: &crate::smart::Smart) -> AuthLayer<crate::smart::Routed> {
    let gate = smart.gate().clone();
    let render = crate::facade::guard::Outcome::new(gate.realm().map(str::to_owned));
    let classifier = crate::smart::Routed::new(
        "shl",
        Requirement::Fhir {
            permission: Permission::Read,
            resource_type: String::from("Patient"),
        },
    );
    AuthLayer::new(gate, classifier).with_render(render)
}

/// A create request.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Create {
    /// The expiry, in epoch seconds.
    exp: Option<i64>,
    /// The passcode.
    passcode: Option<SecretString>,
    /// The label.
    label: Option<String>,
    /// The most answers the link gives.
    answers: Option<u32>,
    /// The `Composition` profile the summary is written in.
    profile: Option<String>,
}

/// A manifest request (`links-specification`, SMART Health Link Manifest
/// Request); a property it does not name is ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestRequest {
    /// "A string describing the recipient".
    recipient: Option<String>,
    /// The passcode, under `P`.
    passcode: Option<String>,
    /// The longest `embedded` the receiver takes.
    embedded_length_max: Option<usize>,
}

/// The query of a direct file request.
#[derive(Debug, Deserialize)]
pub(super) struct DirectQuery {
    /// "Required. A string describing the recipient".
    recipient: Option<String>,
}

/// The SMART Health Link payload (`links-specification`, Construct a SMART
/// Health Link Payload).
#[derive(Debug, Serialize)]
struct Payload<'a> {
    /// The manifest URL.
    url: String,
    /// The key, base64url.
    key: &'a str,
    /// The expiry, in epoch seconds.
    exp: i64,
    /// `P` or `U`.
    flag: &'static str,
    /// The label.
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
    /// The payload version.
    v: u8,
}

/// The answer to a create request.
#[derive(Debug, Serialize)]
struct Created<'a> {
    /// The handle the person manages the link by.
    handle: &'a str,
    /// The `shlink:/` URI.
    shlink: String,
    /// The payload the URI carries.
    payload: Payload<'a>,
}

/// One of the person's links, as `GET /shl` lists it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListedLink {
    /// The handle.
    handle: String,
    /// The label.
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    /// When it was created, in epoch seconds.
    created: i64,
    /// When it expires, in epoch seconds.
    exp: i64,
    /// Whether it needs a passcode.
    passcode: bool,
    /// The answers it still gives.
    answers_left: u32,
}

/// The person's links.
#[derive(Debug, Serialize)]
struct Listing {
    /// The active links, oldest first.
    links: Vec<ListedLink>,
}

/// One file of a manifest (`links-specification`, SMART Health Link
/// Manifest File).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFile {
    /// The file's content type.
    content_type: String,
    /// When the file was written.
    last_updated: String,
    /// The JWE, when it fits the receiver's `embeddedLengthMax`.
    #[serde(skip_serializing_if = "Option::is_none")]
    embedded: Option<String>,
    /// The single-use location, otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<String>,
}

/// A manifest.
#[derive(Debug, Serialize)]
struct Manifest {
    /// Always `finalized`: a link's file is sealed once.
    status: &'static str,
    /// The files.
    files: Vec<ManifestFile>,
}

/// The `401` body of a missing or wrong passcode.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WrongPasscode {
    /// The wrong passcodes the link still takes.
    remaining_attempts: u32,
}

/// Returns the person's patient id and the actor their records name.
fn person(access: &Access) -> Result<(String, Actor), Refusal> {
    let Access::Caller(caller) = access else {
        return Err(refused(StatusCode::FORBIDDEN, NO_PERSON));
    };
    let patient = caller
        .patient()
        .ok_or_else(|| refused(StatusCode::FORBIDDEN, NO_PERSON))?;
    Ok((
        patient.to_owned(),
        Actor::Person {
            issuer: caller.issuer().map(str::to_owned),
            subject: caller.subject().to_owned(),
            client_id: caller.client_id().map(str::to_owned),
        },
    ))
}

/// `POST /shl`: creates a link to the person's patient summary.
pub(super) async fn create(
    State(links): State<Arc<Links>>,
    access: Access,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    answer(created(&links, &access, &headers, &body).await)
}

/// The work of [`create`].
async fn created(
    links: &Links,
    access: &Access,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<Response, Refusal> {
    let (owner, actor) = person(access)?;
    let asked: Create = if body.is_empty() {
        Create::default()
    } else {
        serde_json::from_slice(body)
            .map_err(|_error| refused(StatusCode::BAD_REQUEST, "the body is no create request"))?
    };
    let now = jiff::Timestamp::now().as_second();
    let rules = rules(links, &asked, now)?;
    let requested = match asked.profile.as_deref() {
        Some(profile) => summary::profile_named(profile)?,
        None => summary::default_profile(),
    };
    let place = links.reserve().await.map_err(|error| unrecorded(&error))?;
    let document = summary::document(&links.facade, access, &owner, headers, requested).await?;
    let plaintext = serde_json::to_vec(&document).map_err(|_error| unsealed())?;
    let key = seal::LinkKey::generate().map_err(|_error| unsealed())?;
    let jwe = key
        .seal(FHIR_CONTENT, &plaintext)
        .map_err(|_error| unsealed())?;
    drop(plaintext);
    let id = seal::random_token().map_err(|_error| unsealed())?;
    let handle = uuid::Uuid::new_v4().to_string();
    let hashed = match asked.passcode {
        Some(passcode) => Some(
            tokio::task::spawn_blocking(move || super::passcode::hash(&passcode))
                .await
                .map_err(|_error| unsealed())?
                .map_err(|_error| unsealed())?,
        ),
        None => None,
    };
    let resolution = if hashed.is_some() {
        Resolution::Passcode
    } else {
        Resolution::Direct
    };
    let link = store::Link {
        handle: handle.clone(),
        owner: owner.clone(),
        created: now,
        expires: rules.expires,
        label: asked.label.clone(),
        resolution,
        passcode: hashed,
        attempts_left: links.settings.passcode_attempts,
        answers_left: rules.answers,
        files: vec![store::FileEntry {
            content_type: FHIR_CONTENT.to_owned(),
            last_updated: jiff::Timestamp::now().to_string(),
        }],
        outstanding: 0,
    };
    links
        .store
        .create(&id, &link, &[jwe])
        .await
        .map_err(|error| unstored(&error))?;
    place
        .store(record::exchange(
            Event::Created,
            (&handle, &owner),
            &actor,
            &links.endpoint,
        ))
        .await
        .map_err(|error| unrecorded(&error))?;
    let key = key.encoded();
    let payload = Payload {
        url: format!("{}{}{id}", links.settings.base_url, super::MANIFEST),
        key: key.expose_secret(),
        exp: rules.expires,
        flag: match resolution {
            Resolution::Passcode => "P",
            Resolution::Direct => "U",
        },
        label: asked.label.as_deref(),
        v: 1,
    };
    let minified = serde_json::to_string(&payload).map_err(|_error| unsealed())?;
    let created = Created {
        handle: &handle,
        shlink: format!("shlink:/{}", URL_SAFE_NO_PAD.encode(minified)),
        payload,
    };
    Ok((StatusCode::CREATED, axum::Json(created)).into_response())
}

/// The expiry and the answers a create request is held to.
#[derive(Debug, Clone, Copy)]
struct Rules {
    /// The expiry, in epoch seconds.
    expires: i64,
    /// The answers the link gives.
    answers: u32,
}

/// Returns the rules `asked` sets at `now`, or the `400` of one outside
/// what the deployment allows.
fn rules(links: &Links, asked: &Create, now: i64) -> Result<Rules, Refusal> {
    let settings = &links.settings;
    let longest = i64::try_from(settings.max_expiry.as_secs()).unwrap_or(i64::MAX);
    let default = i64::try_from(settings.default_expiry.as_secs()).unwrap_or(i64::MAX);
    let expires = asked.exp.unwrap_or_else(|| now.saturating_add(default));
    if expires <= now || expires.saturating_sub(now) > longest {
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "exp is in the past or beyond the longest life this server gives a link",
        ));
    }
    let answers = asked.answers.unwrap_or(1);
    if answers == 0 || answers > settings.max_answers {
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "answers is zero or more than this server allows a link",
        ));
    }
    if asked
        .label
        .as_ref()
        .is_some_and(|label| label.chars().count() > LABEL_MAX)
    {
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "label is longer than 80 characters",
        ));
    }
    if asked.passcode.as_ref().is_some_and(|passcode| {
        let length = passcode.expose_secret().chars().count();
        length == 0 || length > PASSCODE_MAX
    }) {
        return Err(refused(
            StatusCode::BAD_REQUEST,
            "passcode is empty or longer than 128 characters",
        ));
    }
    Ok(Rules { expires, answers })
}

/// `GET /shl`: the person's active links.
pub(super) async fn list(State(links): State<Arc<Links>>, access: Access) -> Response {
    answer(listed(&links, &access).await)
}

/// The work of [`list`].
async fn listed(links: &Links, access: &Access) -> Result<Response, Refusal> {
    let (owner, _actor) = person(access)?;
    let now = jiff::Timestamp::now().as_second();
    let listed = links
        .store
        .list(&owner, now)
        .await
        .map_err(|error| unstored(&error))?;
    let listing = Listing {
        links: listed
            .into_iter()
            .map(|link| ListedLink {
                handle: link.handle,
                label: link.label,
                created: link.created,
                exp: link.expires,
                passcode: link.passcode,
                answers_left: link.answers_left,
            })
            .collect(),
    };
    Ok(axum::Json(listing).into_response())
}

/// `DELETE /shl/{handle}`: revokes one of the person's links.
pub(super) async fn revoke(
    State(links): State<Arc<Links>>,
    access: Access,
    Path(handle): Path<String>,
) -> Response {
    answer(revoked(&links, &access, handle).await)
}

/// The work of [`revoke`].
async fn revoked(links: &Links, access: &Access, handle: String) -> Result<Response, Refusal> {
    let (owner, actor) = person(access)?;
    let place = links.reserve().await.map_err(|error| unrecorded(&error))?;
    let gone = links
        .store
        .revoke(&owner, &handle)
        .await
        .map_err(|error| unstored(&error))?
        .ok_or_else(|| {
            refused(
                StatusCode::NOT_FOUND,
                "no active link of this person has the handle",
            )
        })?;
    place
        .store(record::exchange(
            Event::Revoked,
            (&gone.handle, &gone.owner),
            &actor,
            &links.endpoint,
        ))
        .await
        .map_err(|error| unrecorded(&error))?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `POST /shl/m/{id}`: the manifest request.
pub(super) async fn manifest(
    State(links): State<Arc<Links>>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    answer(manifested(&links, id, &body).await)
}

/// The work of [`manifest`].
async fn manifested(links: &Links, id: String, body: &[u8]) -> Result<Response, Refusal> {
    let asked = serde_json::from_slice::<ManifestRequest>(body)
        .map_err(|_error| refused(StatusCode::BAD_REQUEST, "the body is no manifest request"))?;
    let recipient = asked
        .recipient
        .filter(|recipient| !recipient.is_empty())
        .ok_or_else(|| {
            refused(
                StatusCode::BAD_REQUEST,
                "a manifest request names its recipient",
            )
        })?;
    if !well_formed(&id) {
        return Err(refused(StatusCode::NOT_FOUND, INACTIVE));
    }
    let place = links.reserve().await.map_err(|error| unrecorded(&error))?;
    let now = jiff::Timestamp::now().as_second();
    let lifetime = i64::try_from(links.settings.location_lifetime.as_secs()).unwrap_or(i64::MAX);
    let answered = links
        .store
        .answer(
            &id,
            asked.passcode.as_deref(),
            asked.embedded_length_max,
            (now, lifetime),
        )
        .await
        .map_err(|error| unstored(&error))?;
    match answered {
        Answer::Inactive { gone } => {
            if let Some(gone) = gone {
                links.ended(&gone).await;
            }
            Err(refused(StatusCode::NOT_FOUND, INACTIVE))
        }
        Answer::Refused {
            handle,
            owner,
            remaining,
            locked,
        } => {
            let event = Event::Refused {
                recipient: &recipient,
            };
            let ending = locked.then_some(store::Ending::Locked);
            recorded(links, place, event, (handle, owner), ending).await?;
            Ok((
                StatusCode::UNAUTHORIZED,
                axum::Json(WrongPasscode {
                    remaining_attempts: remaining,
                }),
            )
                .into_response())
        }
        Answer::Manifest {
            handle,
            owner,
            files,
            used,
        } => {
            let event = Event::Answered {
                recipient: &recipient,
            };
            let ending = used.then_some(store::Ending::Used);
            recorded(links, place, event, (handle, owner), ending).await?;
            let files = files
                .into_iter()
                .map(|served| ManifestFile {
                    content_type: served.entry.content_type,
                    last_updated: served.entry.last_updated,
                    embedded: served.embedded,
                    location: served
                        .location
                        .map(|token| format!("{}{}{token}", links.settings.base_url, super::FILE)),
                })
                .collect();
            Ok(axum::Json(Manifest {
                status: "finalized",
                files,
            })
            .into_response())
        }
    }
}

/// Records `event` on the link `handle` of `owner` in `place`, and its end
/// when the request ended it.
///
/// # Errors
///
/// The `503` of a record that could not be stored.
async fn recorded(
    links: &Links,
    place: Place<'_>,
    event: Event<'_>,
    (handle, owner): (String, String),
    ending: Option<store::Ending>,
) -> Result<(), Refusal> {
    let stored = place
        .store(record::exchange(
            event,
            (&handle, &owner),
            &Actor::System,
            &links.endpoint,
        ))
        .await;
    if let Some(ending) = ending {
        links
            .ended(&store::Gone {
                handle,
                owner,
                ending,
            })
            .await;
    }
    stored.map_err(|error| unrecorded(&error))
}

/// `GET /shl/f/{token}`: a file, once, by its location.
pub(super) async fn file(State(links): State<Arc<Links>>, Path(token): Path<String>) -> Response {
    answer(fetched(&links, token, None).await)
}

/// `GET /shl/m/{id}?recipient=`: the direct file request under `U`.
pub(super) async fn direct(
    State(links): State<Arc<Links>>,
    Path(id): Path<String>,
    Query(query): Query<DirectQuery>,
) -> Response {
    let Some(recipient) = query.recipient.filter(|recipient| !recipient.is_empty()) else {
        return answer(Err(refused(
            StatusCode::BAD_REQUEST,
            "a direct file request names its recipient",
        )));
    };
    answer(fetched(&links, id, Some(&recipient)).await)
}

/// Fetches the file the location `token` names, or, for a direct request
/// naming `recipient`, the file of the link `token` is the manifest id of.
async fn fetched(
    links: &Links,
    token: String,
    recipient: Option<&str>,
) -> Result<Response, Refusal> {
    if !well_formed(&token) {
        return Err(refused(StatusCode::NOT_FOUND, INACTIVE));
    }
    let place = match recipient {
        Some(_) => Some(links.reserve().await.map_err(|error| unrecorded(&error))?),
        None => None,
    };
    let now = jiff::Timestamp::now().as_second();
    let fetched = if recipient.is_some() {
        links.store.direct(&token, now).await
    } else {
        links.store.fetch(&token, now).await
    }
    .map_err(|error| unstored(&error))?;
    match fetched {
        Fetched::Inactive { gone } => {
            if let Some(gone) = gone {
                links.ended(&gone).await;
            }
            Err(refused(StatusCode::NOT_FOUND, INACTIVE))
        }
        Fetched::File {
            handle,
            owner,
            jwe,
            used,
        } => {
            let ending = used.then_some(store::Ending::Used);
            match (recipient, place) {
                (Some(recipient), Some(place)) => {
                    let event = Event::Answered { recipient };
                    recorded(links, place, event, (handle, owner), ending).await?;
                }
                _ => {
                    if let Some(ending) = ending {
                        links
                            .ended(&store::Gone {
                                handle,
                                owner,
                                ending,
                            })
                            .await;
                    }
                }
            }
            Ok(([(header::CONTENT_TYPE, JOSE)], jwe).into_response())
        }
    }
}

/// Answers a CORS preflight of the receiver's routes.
pub(super) async fn preflight() -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
    response
}

/// Lets a receiver on any origin read the receiver's routes.
// NOTE: no specification governs this: our own design; `links-specification` names a browser page
// as a receiver, and these routes carry no credential, so any origin may read them.
pub(super) async fn any_origin(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response
}

/// Returns whether `token` has the shape of a manifest id or a location:
/// 43 base64url characters.
fn well_formed(token: &str) -> bool {
    token.len() == 43
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Returns the response of `result`, kept out of every cache.
fn answer(result: Result<Response, Refusal>) -> Response {
    let mut response = result.unwrap_or_else(Refusal::into_response);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The `OperationOutcome` of a refusal with `status`.
fn refused(status: StatusCode, detail: &'static str) -> Refusal {
    let code = match status {
        StatusCode::BAD_REQUEST => IssueType::Invalid,
        StatusCode::FORBIDDEN => IssueType::Forbidden,
        StatusCode::NOT_FOUND => IssueType::NotFound,
        StatusCode::SERVICE_UNAVAILABLE => IssueType::Transient,
        _ => IssueType::Exception,
    };
    reply::refusal(status, Issue::error(code).diagnosing(detail))
}

/// The `503` of an event whose record could not be placed or stored.
fn unrecorded(error: &ihe_atna::balp::AuditError) -> Refusal {
    tracing::warn!(error = %ferrohealth_server::run::chain(error), "a link request was refused: its record could not be kept");
    refused(
        StatusCode::SERVICE_UNAVAILABLE,
        "the request could not be recorded, so it was not answered",
    )
}

/// The `500` of a store that failed.
fn unstored(error: &store::StoreError) -> Refusal {
    tracing::warn!(error = %ferrohealth_server::run::chain(error), "the link store failed");
    refused(StatusCode::INTERNAL_SERVER_ERROR, "the link store failed")
}

/// The `500` of a secret that could not be drawn, a seal, a hash or an
/// encoding that failed, or a blocking task that did not finish.
fn unsealed() -> Refusal {
    tracing::warn!("a link could not be sealed");
    refused(
        StatusCode::INTERNAL_SERVER_ERROR,
        "the link could not be sealed",
    )
}
