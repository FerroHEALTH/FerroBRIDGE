// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Read, vread and the instance history of a party resource.
//!
//! A read fetches the latest version of the party the id names; a vread
//! composes the `OBJECT_VERSION_ID` of the `[vid]` from the latest version's
//! object id and creating system (RM Common IM §`OBJECT_VERSION_ID`) and
//! fetches it; the history lists the `VERSIONED_PARTY` revision history,
//! newest first, and fetches each version
//! (<https://hl7.org/fhir/R4/http.html#history>). ITS-REST 1.1.0 Demographic
//! API, DEVELOPMENT.

use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_base::v1_3::base_types::identification::version_tree_id::VersionTreeId;
use openehr_its::rest::generated::demographic::client::VersionedPartyRevisionHistoryOutcome;

use crate::cdr::Answered;
use crate::facade::Facade;
use crate::facade::handlers::Body;
use crate::facade::handlers::Refusal;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::party;
use crate::facade::programs::Loaded;
use crate::facade::reply;
use crate::facade::reply::Header;

/// `GET [base]/{type}/{id}` of a party type.
pub(crate) async fn read(
    facade: &Facade,
    access: &Access,
    loaded: &Loaded,
    (resource_type, id): (&str, &str),
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<axum::response::Response, Refusal> {
    crate::facade::handlers::guard(headers, uri, Body::Absent)?;
    party::admit(access, Permission::Read, resource_type, Some(id))?;
    let uid = party::party_id(resource_type, id)?;
    let client = facade.client_for(headers);
    let demographic = party::demographic(&client)?;
    let class = class_of(loaded)?;
    let held = party::get(demographic, class, resource_type, uid.value()).await?;
    answer(facade, loaded, (resource_type, id), &held).await
}

/// `GET [base]/{type}/{id}/_history/{vid}` of a party type.
///
/// A `[vid]` that is no version tree id is `404`.
pub(crate) async fn vread(
    facade: &Facade,
    access: &Access,
    loaded: &Loaded,
    (resource_type, id, vid): (&str, &str, &str),
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<axum::response::Response, Refusal> {
    crate::facade::handlers::guard(headers, uri, Body::Absent)?;
    party::admit(access, Permission::Read, resource_type, Some(id))?;
    let uid = party::party_id(resource_type, id)?;
    let tree = VersionTreeId::new(vid).map_err(|error| {
        reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotFound).diagnosing(format!(
                "{resource_type}/{id} has no version {vid}: {}",
                crate::facade::outcome::chain(&error)
            )),
        )
    })?;
    let client = facade.client_for(headers);
    let demographic = party::demographic(&client)?;
    let class = class_of(loaded)?;
    let latest = party::get(demographic, class, resource_type, uid.value()).await?;
    let version = ObjectVersionId::compose(
        &latest.version.object_id(),
        &latest.version.creating_system_id(),
        &tree,
    );
    let held = party::get(demographic, class, resource_type, version.value()).await?;
    answer(facade, loaded, (resource_type, id), &held).await
}

/// `GET [base]/{type}/{id}/_history` of a party type: a `history` Bundle,
/// newest version first.
pub(crate) async fn history(
    facade: &Facade,
    access: &Access,
    loaded: &Loaded,
    (resource_type, id): (&str, &str),
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<axum::response::Response, Refusal> {
    crate::facade::handlers::guard(headers, uri, Body::Absent)?;
    party::admit(access, Permission::Read, resource_type, Some(id))?;
    let uid = party::party_id(resource_type, id)?;
    let client = facade.client_for(headers);
    let demographic = party::demographic(&client)?;
    let class = class_of(loaded)?;
    let Answered { outcome, upstream } = demographic
        .revision_history(uid.value())
        .await
        .map_err(|error| party::client_refusal(&error))?;
    let VersionedPartyRevisionHistoryOutcome::Ok { body, .. } = outcome else {
        return Err(party::refused(&upstream));
    };
    let mut entries = Vec::new();
    for item in body.items.iter().rev() {
        let held = party::get(demographic, class, resource_type, item.version_id.value()).await?;
        let accepted = party::accept(loaded, resource_type, id, &held)?;
        let resource = party::render(facade, loaded, &accepted, &held.version).await?;
        entries.push(entry(facade, resource_type, &held.version, resource));
    }
    let mut bundle = Object::new();
    bundle.insert(
        String::from("resourceType"),
        Value::String(String::from("Bundle")),
    );
    bundle.insert(String::from("type"), Value::String(String::from("history")));
    bundle.insert(
        String::from("total"),
        Value::Number(fhir_types::codec::Number::from(
            u32::try_from(entries.len()).unwrap_or(u32::MAX),
        )),
    );
    bundle.insert(String::from("entry"), Value::Array(entries));
    Ok(reply::resource(StatusCode::OK, &bundle, &[]))
}

/// Returns one `history` Bundle entry: the version, its request and its
/// response (<https://hl7.org/fhir/R4/bundle.html#history>).
fn entry(
    facade: &Facade,
    resource_type: &str,
    version: &ObjectVersionId,
    resource: Object,
) -> Value {
    let location = party::location(facade, resource_type, version);
    let id = crate::cdr::ids::versioned_object_uid(version);
    let method = if version.version_tree_id().is_first() {
        "POST"
    } else {
        "PUT"
    };
    let url = if method == "POST" {
        String::from(resource_type)
    } else {
        format!("{resource_type}/{}", id.value())
    };
    let mut request = Object::new();
    request.insert(String::from("method"), Value::String(String::from(method)));
    request.insert(String::from("url"), Value::String(url));
    let mut response = Object::new();
    response.insert(
        String::from("status"),
        Value::String(String::from(if method == "POST" { "201" } else { "200" })),
    );
    response.insert(String::from("location"), Value::String(location.clone()));
    response.insert(
        String::from("etag"),
        Value::String(format!("W/\"{}\"", version.version_tree_id().value())),
    );
    let mut entry = Object::new();
    entry.insert(String::from("fullUrl"), Value::String(location));
    entry.insert(String::from("resource"), Value::Object(resource));
    entry.insert(String::from("request"), Value::Object(request));
    entry.insert(String::from("response"), Value::Object(response));
    Value::Object(entry)
}

/// Answers one party version as its resource.
async fn answer(
    facade: &Facade,
    loaded: &Loaded,
    (resource_type, id): (&str, &str),
    held: &party::Held,
) -> Result<axum::response::Response, Refusal> {
    let accepted = party::accept(loaded, resource_type, id, held)?;
    let resource = party::render(facade, loaded, &accepted, &held.version).await?;
    Ok(reply::resource(
        StatusCode::OK,
        &resource,
        &[Header::entity_tag(held.version.version_tree_id().value())],
    ))
}

/// Returns the party class `loaded` serves.
pub(crate) fn class_of(
    loaded: &Loaded,
) -> Result<openehr_mapping_core::party::PartyClass, Refusal> {
    loaded.program().party().ok_or_else(|| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "the context {} is served as a party and compiled no party context",
                loaded.program().context()
            )),
        )
    })
}
