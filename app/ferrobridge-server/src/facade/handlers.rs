// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The interactions: metadata, create, search, read, vread, update,
//! transaction, batch, `$validate`.
//!
//! Each handler is the same shape. It checks the media types and the query
//! parameters, resolves the program the request runs, calls the engine and the
//! CDR, and answers either a FHIR resource or an `OperationOutcome`
//! (no specification governs this: our own design). Every CDR answer reaches the wire through
//! the one table of [`crate::facade::status`], so a status is never invented
//! at a call site. Create, update and transaction write through
//! [`crate::facade::ingest`], so the handlers keep only the HTTP half.
//!
//! Every handler but metadata reads the [`Access`] the layer of
//! [`crate::facade::guard`] left on the request and holds a `patient/` token to
//! its patient's compartment ([`crate::facade::compartment`]). A request that
//! reaches a handler without one never passed the layer, and the extractor
//! refuses it with `500`.

mod batch;
mod complete;
mod conditional;
pub(crate) mod emit;
pub(crate) mod read;
pub(crate) mod render;
pub(crate) mod search;
mod transaction;
mod validate;
pub(crate) mod write;

use std::sync::Arc;

use axum::Extension;
use axum::body::Bytes;
use axum::extract::Path;
use axum::extract::State;
use axum::response::Response;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use fhir_types::codec::Json;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;

use crate::facade::Facade;
use crate::facade::app_state;
use crate::facade::capability;
use crate::facade::compartment;
use crate::facade::consent;
use crate::facade::endpoint;
use crate::facade::guard::OperatorScope;
use crate::facade::launch_task;
use crate::facade::media;
use crate::facade::mhd;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// `GET [base]/metadata`: the `CapabilityStatement` of the loaded programs.
///
/// `shape` says whether the router serves the FHIRconnect operations beside
/// the facade, whether the HL7 v2 face runs, and which authorization server
/// guards the facade, so the statement declares each only then.
#[must_use]
pub fn metadata_route(
    facade: &Facade,
    headers: &HeaderMap,
    uri: &Uri,
    shape: &capability::Shape,
) -> Response {
    metadata(facade, headers, uri, shape).unwrap_or_else(Refusal::into_response)
}

/// `POST [base]/{type}`: create, conditional and idempotent by source
/// identity.
pub async fn create_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path(resource_type): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if resource_type == consent::CONSENT && consent::directs_access(&facade, &body) {
        return consent::fhir::create(&facade, &access, &headers, &body)
            .await
            .unwrap_or_else(Refusal::into_response);
    }
    if let Some(loaded) = facade.programs().party(&resource_type) {
        return crate::facade::party::write::create(
            &facade,
            &access,
            loaded,
            &resource_type,
            &headers,
            &uri,
            &body,
        )
        .await
        .unwrap_or_else(Refusal::into_response);
    }
    write::create(&facade, &access, &resource_type, &headers, &uri, &body)
        .await
        .unwrap_or_else(Refusal::into_response)
}

/// `GET [base]/{type}?…`: search the type over its declared AQL projections.
pub async fn search_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path(resource_type): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if resource_type == consent::CONSENT
        && let Some(answer) = consent_search(&facade, &access, &headers, &uri, None).await
    {
        return answer;
    }
    if resource_type == endpoint::ENDPOINT && facade.endpoints().is_some() {
        return endpoint::search(&facade, &access, &headers, &uri, &[])
            .unwrap_or_else(Refusal::into_response);
    }
    if mhd::serves(&facade, &resource_type) {
        return mhd::search(&facade, &access, &resource_type, &headers, &uri, &[])
            .await
            .unwrap_or_else(Refusal::into_response);
    }
    search::search_get(&facade, &access, &resource_type, &headers, &uri)
        .await
        .unwrap_or_else(Refusal::into_response)
}

/// `POST [base]/{type}/_search`: the same search, its parameters in a form
/// body.
pub async fn search_form_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path(resource_type): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if resource_type == endpoint::ENDPOINT && facade.endpoints().is_some() {
        return endpoint::search(&facade, &access, &headers, &uri, &body)
            .unwrap_or_else(Refusal::into_response);
    }
    if mhd::serves(&facade, &resource_type) {
        return mhd::search(&facade, &access, &resource_type, &headers, &uri, &body)
            .await
            .unwrap_or_else(Refusal::into_response);
    }
    if resource_type == consent::CONSENT
        && let Some(answer) = consent_search(&facade, &access, &headers, &uri, Some(&body)).await
    {
        return answer;
    }
    search::search_post(&facade, &access, &resource_type, &headers, &uri, &body)
        .await
        .unwrap_or_else(Refusal::into_response)
}

/// Answers a `Consent` search, its parameters in the query and in `form`, a
/// `POST _search` body, when it reaches the access directives
/// ([`consent::lanes`]); `None` for one the mapped search answers alone.
async fn consent_search(
    facade: &Facade,
    access: &Access,
    headers: &HeaderMap,
    uri: &Uri,
    form: Option<&[u8]>,
) -> Option<Response> {
    let mut pairs = media::query_pairs(uri.query().unwrap_or_default().as_bytes());
    if let Some(body) = form {
        if let Err(refusal) = search::form::check_form(headers) {
            return Some(refusal.into_response());
        }
        pairs.extend(media::query_pairs(body));
    }
    let lanes = consent::lanes(facade, &pairs);
    if lanes == consent::Lanes::Mapped {
        return None;
    }
    Some(
        consent::fhir::search(facade, access, headers, uri, &pairs, lanes)
            .await
            .unwrap_or_else(Refusal::into_response),
    )
}

/// `GET [base]/{type}/{id}`: read one mapped resource back out of the CDR.
pub async fn read_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path((resource_type, id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if resource_type == consent::CONSENT && consent::reads_access(&facade, &id) {
        return consent::fhir::read(&facade, &access, &id, &headers)
            .await
            .unwrap_or_else(Refusal::into_response);
    }
    if resource_type == endpoint::ENDPOINT && facade.endpoints().is_some() {
        return endpoint::read(&facade, &access, &id, &headers, &uri)
            .unwrap_or_else(Refusal::into_response);
    }
    if mhd::serves(&facade, &resource_type) {
        return mhd::read(&facade, &access, (&resource_type, &id), &headers, &uri)
            .await
            .unwrap_or_else(Refusal::into_response);
    }
    read(&facade, &access, (&resource_type, &id), &headers, &uri)
        .await
        .unwrap_or_else(Refusal::into_response)
}

/// `GET [base]/{type}/{id}/$document`: the document Bundle of one mapped
/// Composition (<https://hl7.org/fhir/R4/composition-operation-document.html>).
pub async fn document_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path((resource_type, id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    document(&facade, &access, &resource_type, Some(&id), &headers, &uri)
        .await
        .unwrap_or_else(Refusal::into_response)
}

/// `GET [base]/{type}/$document?id=[id]`, identical in meaning to the
/// instance form (<https://hl7.org/fhir/R4/composition-operation-document.html>).
pub async fn document_type_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path(resource_type): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    document(&facade, &access, &resource_type, None, &headers, &uri)
        .await
        .unwrap_or_else(Refusal::into_response)
}

/// The `$document` operation behind its two routes.
///
/// The R4 `OperationDefinition` `Composition-document` takes `id`, `persist`
/// and `graph`. This server stores no document and reads no `GraphDefinition`,
/// so `persist=true` and any `graph` are refused rather than ignored, and a
/// Composition on another server is refused; `persist=false` is honoured
/// (no specification governs refusing them: our own design). The document
/// is the one MHD 4.2.3 Retrieve Document [ITI-68] retrieves, so it answers
/// in FHIR JSON or XML as the request asks, and `406` for any other media
/// type (ITI TF-2: 3.68.4.1.3).
async fn document(
    facade: &Facade,
    access: &Access,
    resource_type: &str,
    path_id: Option<&str>,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Response, Refusal> {
    if resource_type != read::COMPOSITION {
        return Err(reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotSupported).diagnosing(format!(
                "$document is an operation of Composition, not of {resource_type}"
            )),
        ));
    }
    let format = format_of(headers, uri)?;
    let invalid = |code: IssueType, text: String| {
        reply::refusal(StatusCode::BAD_REQUEST, Issue::error(code).diagnosing(text))
    };
    let mut named = None;
    for (name, value) in media::query_pairs(uri.query().unwrap_or_default().as_bytes()) {
        match name.as_str() {
            "id" => named = Some(value),
            "persist" => match value.as_str() {
                "false" => {}
                "true" => {
                    return Err(invalid(
                        IssueType::NotSupported,
                        String::from(
                            "persist=true asks this server to store the document at /Bundle, which it does not do",
                        ),
                    ));
                }
                other => {
                    return Err(invalid(
                        IssueType::Invalid,
                        format!("persist is a boolean, not {other:?}"),
                    ));
                }
            },
            "graph" => {
                return Err(invalid(
                    IssueType::NotSupported,
                    String::from("this server reads no GraphDefinition, so graph is not supported"),
                ));
            }
            "_format" | "_pretty" => {}
            other => {
                return Err(invalid(
                    IssueType::NotSupported,
                    format!("$document takes no parameter {other:?}"),
                ));
            }
        }
    }
    let id = match (path_id, named) {
        (Some(_), Some(_)) => {
            return Err(invalid(
                IssueType::Invalid,
                String::from(
                    "the id parameter SHALL NOT be used when the operation names a Composition",
                ),
            ));
        }
        (Some(id), None) => id.to_owned(),
        (None, Some(id)) => local_id(facade, &id).ok_or_else(|| {
            invalid(
                IssueType::NotSupported,
                format!("{id} names a Composition on another server"),
            )
        })?,
        (None, None) => {
            return Err(invalid(
                IssueType::Required,
                String::from("$document needs the Composition, in the path or as id"),
            ));
        }
    };
    admit_existing(facade, access, Permission::Read, read::COMPOSITION, &id).await?;
    let client = facade.client_for(headers, access).await?;
    let assembled = read::assembled(facade, access, &client, &id).await?;
    Ok(reply::formatted(
        StatusCode::OK,
        &assembled.bundle,
        format,
        &[],
    ))
}

/// Returns the format a resource served in both FHIR formats answers in, or
/// the `406` of a request that asks for neither
/// (<https://hl7.org/fhir/R4/http.html#mime-type>).
pub(crate) fn format_of(headers: &HeaderMap, uri: &Uri) -> Result<media::Format, Refusal> {
    media::Format::negotiate(headers, uri.query()).map_err(|refusal| {
        reply::refusal(
            StatusCode::NOT_ACCEPTABLE,
            Issue::error(IssueType::NotSupported).diagnosing(refusal.to_string()),
        )
    })
}

/// Returns the logical id an `id` parameter names on this server: a bare id,
/// or the URL of a Composition under the facade's base.
fn local_id(facade: &Facade, id: &str) -> Option<String> {
    if !id.contains('/') {
        return Some(id.to_owned());
    }
    let prefix = format!(
        "{}/{}/",
        facade.settings().base_url.trim_end_matches('/'),
        read::COMPOSITION
    );
    id.strip_prefix(&prefix)
        .filter(|rest| !rest.is_empty() && !rest.contains('/'))
        .map(str::to_owned)
}

/// `GET [base]/{type}/{id}/_history/{vid}`: read one version of a mapped
/// resource back out of the CDR.
pub async fn vread_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path((resource_type, id, vid)): Path<(String, String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    vread(
        &facade,
        &access,
        (&resource_type, &id, &vid),
        &headers,
        &uri,
    )
    .await
    .unwrap_or_else(Refusal::into_response)
}

/// `PUT [base]/{type}/{id}`: update a resource the identity map knows.
pub async fn update_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path((resource_type, id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    update(
        &facade,
        &access,
        (&resource_type, &id),
        &headers,
        &uri,
        &body,
    )
    .await
    .unwrap_or_else(Refusal::into_response)
}

/// `DELETE [base]/{type}/{id}`: delete a party resource, ITS-REST 1.1.0
/// Demographic API, DEVELOPMENT.
///
/// A type no party program serves is `not-supported`, and a party program
/// loads only with `[cdr] demographic` on (no specification governs this: our
/// own design).
pub async fn delete_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path((resource_type, id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let programs = facade.programs();
    let Some(loaded) = programs.party(&resource_type) else {
        return not_supported(&resource_type).into_response();
    };
    crate::facade::party::write::delete(
        &facade,
        &access,
        loaded,
        (&resource_type, &id),
        &headers,
        &uri,
    )
    .await
    .unwrap_or_else(Refusal::into_response)
}

/// `GET [base]/{type}/{id}/_history`: the history of a party resource,
/// ITS-REST 1.1.0 Demographic API, DEVELOPMENT.
///
/// A type no party program serves is `not-supported`, as for [`delete_route`].
pub async fn history_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path((resource_type, id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let programs = facade.programs();
    let Some(loaded) = programs.party(&resource_type) else {
        return not_supported(&resource_type).into_response();
    };
    crate::facade::party::read::history(
        &facade,
        &access,
        loaded,
        (&resource_type, &id),
        &headers,
        &uri,
    )
    .await
    .unwrap_or_else(Refusal::into_response)
}

/// `POST [base]/{type}/$validate`: the dry run that commits nothing.
pub async fn validate_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Extension(operator): Extension<OperatorScope>,
    Path(resource_type): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let admission = validate::Admission {
        access: &access,
        operator: operator.0.as_deref(),
    };
    validate::validate(&facade, admission, &resource_type, &headers, &uri, &body)
        .await
        .unwrap_or_else(Refusal::into_response)
}

/// `POST [base]`: a `transaction` Bundle, all or nothing, or a `batch`
/// Bundle, each entry on its own.
pub async fn transaction_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    Box::pin(transaction::transaction(
        &facade, &access, &headers, &uri, &body,
    ))
    .await
    .unwrap_or_else(Refusal::into_response)
}

/// The read interaction behind its route and a batch entry: the compartment
/// check, then the read.
pub(crate) async fn read(
    facade: &Facade,
    access: &Access,
    (resource_type, id): (&str, &str),
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Response, Refusal> {
    if let Some(loaded) = facade.programs().party(resource_type) {
        return crate::facade::party::read::read(
            facade,
            access,
            loaded,
            (resource_type, id),
            headers,
            uri,
        )
        .await;
    }
    admit_existing(facade, access, Permission::Read, resource_type, id).await?;
    read::read(
        facade,
        &facade.client_for(headers, access).await?,
        resource_type,
        id,
        headers,
        uri,
    )
    .await
}

/// The vread interaction behind its route and a batch entry.
pub(crate) async fn vread(
    facade: &Facade,
    access: &Access,
    (resource_type, id, vid): (&str, &str, &str),
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Response, Refusal> {
    if let Some(loaded) = facade.programs().party(resource_type) {
        return crate::facade::party::read::vread(
            facade,
            access,
            loaded,
            (resource_type, id, vid),
            headers,
            uri,
        )
        .await;
    }
    admit_existing(facade, access, Permission::Read, resource_type, id).await?;
    read::vread(
        facade,
        &facade.client_for(headers, access).await?,
        (resource_type, id, vid),
        headers,
        uri,
    )
    .await
}

/// The update interaction behind its route and a batch entry.
pub(crate) async fn update(
    facade: &Facade,
    access: &Access,
    (resource_type, id): (&str, &str),
    headers: &HeaderMap,
    uri: &Uri,
    body: &[u8],
) -> Result<Response, Refusal> {
    if let Some(loaded) = facade.programs().party(resource_type) {
        return crate::facade::party::write::update(
            facade,
            access,
            loaded,
            (resource_type, id),
            headers,
            uri,
            body,
        )
        .await;
    }
    admit_existing(facade, access, Permission::Update, resource_type, id).await?;
    write::update(facade, access, (resource_type, id), headers, uri, body).await
}

/// Checks that `access` reaches the composition the identity map binds to
/// `resource_type`/`id`.
///
/// An id the map does not know passes, so the handler answers its `404` as
/// it does for any caller.
async fn admit_existing(
    facade: &Facade,
    access: &Access,
    permission: Permission,
    resource_type: &str,
    id: &str,
) -> Result<(), Refusal> {
    let Ok(internal) = crate::facade::identity::FhirResourceId::new(id) else {
        return Ok(());
    };
    let Some(binding) = read::binding(facade, resource_type, &internal).await? else {
        return Ok(());
    };
    let ehr = binding
        .ehr(&internal)
        .map_err(|error| read::stored_identifier(&error))?;
    compartment::admit_ehr(facade, access, permission, resource_type, &ehr).await
}

/// Returns the IHE operation `uri` ends in on `resource_type`, or the
/// `404` of any other type.
fn exchange_profile(resource_type: &str, uri: &Uri) -> Result<consent::ihe::Profile, Refusal> {
    let operation = uri.path().rsplit('/').next().unwrap_or_default();
    match consent::ihe::Profile::of(operation) {
        Some(profile) if resource_type == consent::CONSENT => Ok(profile),
        _ => Err(reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotSupported)
                .diagnosing(format!("{resource_type} defines no operation {operation}")),
        )),
    }
}

/// `POST [base]/Consent/$bppc` and `POST [base]/Consent/$appc`: imports one
/// IHE document ([`consent::ihe`]).
pub async fn exchange_import_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path(resource_type): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let imported = match exchange_profile(&resource_type, &uri) {
        Ok(profile) => consent::ihe::import(&facade, &access, profile, &headers, &uri, &body).await,
        Err(refusal) => Err(refusal),
    };
    imported.unwrap_or_else(Refusal::into_response)
}

/// `GET [base]/Consent/$bppc` and `GET [base]/Consent/$appc`: exports the
/// patient's directives as IHE documents ([`consent::ihe`]).
pub async fn exchange_export_route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path(resource_type): Path<String>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let exported = match exchange_profile(&resource_type, &uri) {
        Ok(profile) => consent::ihe::export(&facade, &access, profile, &headers, &uri).await,
        Err(refusal) => Err(refusal),
    };
    exported.unwrap_or_else(Refusal::into_response)
}

/// Answers the `CapabilityStatement`.
fn metadata(
    facade: &Facade,
    headers: &HeaderMap,
    uri: &Uri,
    shape: &capability::Shape,
) -> Result<Response, Refusal> {
    guard(headers, uri, Body::Absent)?;
    let mappings = facade.mappings();
    let statement = capability::statement_of(
        mappings.programs(),
        mappings.search(),
        &facade.settings().base_url,
        shape,
        if facade.identifier_search().is_some() {
            capability::Chains::Identifier
        } else {
            capability::Chains::None
        },
        facade.party_search(),
    );
    let mut statement = statement;
    if facade.consent().is_some()
        && let Some(rest) = statement.rest.first_mut()
    {
        consent::declare(&mut rest.resource);
    }
    if facade.app_state().is_some() {
        statement
            .instantiates
            .push(app_state::SERVER_CAPABILITY.into());
        if let Some(rest) = statement.rest.first_mut() {
            rest.resource
                .retain(|entry| entry.r#type.value.as_deref() != Some(app_state::BASIC));
            let at = rest
                .resource
                .partition_point(|entry| entry.r#type.value.as_deref() < Some(app_state::BASIC));
            rest.resource.insert(at, app_state::capability());
        }
    }
    if facade.summary().is_some()
        && let Some(rest) = statement.rest.first_mut()
        && let Some(patient) = rest
            .resource
            .iter_mut()
            .find(|entry| entry.r#type.value.as_deref() == Some(crate::facade::summary::PATIENT))
    {
        patient.operation.push(
            fhir_types::r4::capability_statement::CapabilityStatementRestResourceOperation {
                name: crate::facade::summary::OPERATION.into(),
                definition: crate::facade::summary::DEFINITION.into(),
                ..Default::default()
            },
        );
    }
    if facade.endpoints().is_some()
        && let Some(rest) = statement.rest.first_mut()
    {
        rest.resource
            .retain(|entry| entry.r#type.value.as_deref() != Some(endpoint::ENDPOINT));
        let at = rest
            .resource
            .partition_point(|entry| entry.r#type.value.as_deref() < Some(endpoint::ENDPOINT));
        rest.resource.insert(at, endpoint::capability());
    }
    if facade.mhd() {
        statement.instantiates.push(mhd::RESPONDER.into());
        if let Some(rest) = statement.rest.first_mut() {
            rest.resource.retain(|entry| {
                !matches!(
                    entry.r#type.value.as_deref(),
                    Some(mhd::DOCUMENT_REFERENCE | mhd::LIST)
                )
            });
            for resource in mhd::capability() {
                let at = rest.resource.partition_point(|entry| {
                    entry.r#type.value.as_deref() < resource.r#type.value.as_deref()
                });
                rest.resource.insert(at, resource);
            }
        }
    }
    if facade.launch_tasks().is_some()
        && let Some(rest) = statement.rest.first_mut()
    {
        rest.resource
            .retain(|entry| entry.r#type.value.as_deref() != Some(launch_task::TASK));
        let at = rest
            .resource
            .partition_point(|entry| entry.r#type.value.as_deref() < Some(launch_task::TASK));
        rest.resource.insert(at, launch_task::capability());
    }
    let encoded = statement.to_json().map_err(|error| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "the CapabilityStatement could not be encoded: {error}"
            )),
        )
    })?;
    Ok(reply::resource(StatusCode::OK, &encoded, &[]))
}

/// Whether the request carries a body whose media type is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Body {
    /// The interaction takes a body.
    Present,
    /// The interaction takes none.
    Absent,
}

/// Checks the media types and the query parameters of one request.
///
/// `Accept` that names nothing this server produces is `406`, a body media
/// type it does not read is `415`
/// (<https://hl7.org/fhir/R4/http.html#mime-type>), and a `_count` that is not
/// a page size is `400 invalid` rather than ignored
/// (<https://hl7.org/fhir/R4/search.html#count>).
pub(crate) fn guard(headers: &HeaderMap, uri: &Uri, body: Body) -> Result<(), Refusal> {
    if let Err(refusal) = media::check_accept(headers) {
        return Err(reply::refusal(
            StatusCode::NOT_ACCEPTABLE,
            Issue::error(IssueType::NotSupported).diagnosing(refusal.to_string()),
        ));
    }
    if body == Body::Present
        && let Err(refusal) = media::check_content_type(headers)
    {
        return Err(reply::refusal(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Issue::error(IssueType::NotSupported).diagnosing(refusal.to_string()),
        ));
    }
    if let Err(refusal) = media::count(uri.query()) {
        return Err(reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::Invalid)
                .diagnosing(refusal.to_string())
                .at("_count"),
        ));
    }
    Ok(())
}

/// Checks that some loaded program maps `resource_type`.
///
/// A type with no loaded context is `not-supported`
/// (no specification governs this: our own design), and it is absent from the
/// `CapabilityStatement`, so the wire answer is a `404` whose issue code says
/// why.
pub(crate) fn supported(facade: &Facade, resource_type: &str) -> Result<(), Refusal> {
    if facade.programs().supports(resource_type) {
        return Ok(());
    }
    Err(not_supported(resource_type))
}

/// Checks that some loaded program maps or creates `resource_type`, as read
/// and vread require.
///
/// A type only a `reference` mapping creates answers read and vread and no
/// other interaction, and the `CapabilityStatement` declares it with those
/// two (no specification governs this: our own design).
pub(crate) fn readable(facade: &Facade, resource_type: &str) -> Result<(), Refusal> {
    if facade.programs().readable(resource_type) {
        return Ok(());
    }
    Err(not_supported(resource_type))
}

/// Returns the `404` with `not-supported` a type no loaded program maps
/// answers.
fn not_supported(resource_type: &str) -> Refusal {
    reply::refusal(
        StatusCode::NOT_FOUND,
        Issue::error(IssueType::NotSupported).diagnosing(format!(
            "no loaded mapping answers for {resource_type}; GET {}/metadata names the types this server supports",
            crate::mount::BASE_PATH
        )),
    )
}
