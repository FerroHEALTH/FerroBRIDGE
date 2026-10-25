// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Create, update and delete of a party resource.
//!
//! A create maps the resource to a party and commits its first version; an
//! update commits a new version over the one `If-Match` names, or over the
//! latest when the request names none, so a concurrent writer still meets the
//! `412` the CDR answers; a delete removes the latest version
//! (<https://hl7.org/fhir/R4/http.html>). Each answers from the party it sent,
//! under the version the CDR's `ETag` names, and writes no identity-map entry.
//! ITS-REST 1.1.0 Demographic API, DEVELOPMENT. No specification governs the
//! missing-`If-Match` rule: our own design.

use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;
use http::header;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_base::v1_3::base_types::identification::version_tree_id::VersionTreeId;
use openehr_mapping_core::party::CanonicalParty;
use openehr_mapping_core::party::PartyClass;
use openehr_rm::v1_2::demographic::organisation::Organisation;
use openehr_rm::v1_2::demographic::person::Person;
use openehr_rm::v1_2::demographic::role::Role;

use crate::cdr::Answered;
use crate::cdr::Prefer;
use crate::cdr::commit::CommitContext;
use crate::cdr::demographic::Demographic;
use crate::facade::Facade;
use crate::facade::commit::Change;
use crate::facade::commit::Committer;
use crate::facade::handlers::Body;
use crate::facade::handlers::Refusal;
use crate::facade::handlers::read::parse;
use crate::facade::handlers::write::same_id;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::party;
use crate::facade::party::read::class_of;
use crate::facade::programs::Loaded;
use crate::facade::reply;
use crate::facade::reply::Header;
use crate::terminology::Direction;

/// `POST [base]/{type}` of a party type.
pub(crate) async fn create(
    facade: &Facade,
    access: &Access,
    loaded: &Loaded,
    resource_type: &str,
    headers: &HeaderMap,
    uri: &Uri,
    body: &[u8],
) -> Result<axum::response::Response, Refusal> {
    crate::facade::handlers::guard(headers, uri, Body::Present)?;
    party::admit(access, Permission::Create, resource_type, None)?;
    let inbound = parse(body, resource_type)?;
    let built = build(facade, loaded, inbound.document()).await?;
    let client = facade.client_for(headers);
    let demographic = party::demographic(&client)?;
    let commit = commit(facade, access, loaded, Change::Creation)?;
    let version = commit_party(demographic, class_of(loaded)?, &built, &commit, None).await?;
    party::provision::provision(facade, &client, resource_type, &version).await?;
    respond(
        facade,
        loaded,
        resource_type,
        &built,
        &version,
        StatusCode::CREATED,
    )
    .await
}

/// `PUT [base]/{type}/{id}` of a party type.
///
/// R4 update requires the body's id to be the URL's
/// (<https://hl7.org/fhir/R4/http.html#update>).
pub(crate) async fn update(
    facade: &Facade,
    access: &Access,
    loaded: &Loaded,
    (resource_type, id): (&str, &str),
    headers: &HeaderMap,
    uri: &Uri,
    body: &[u8],
) -> Result<axum::response::Response, Refusal> {
    crate::facade::handlers::guard(headers, uri, Body::Present)?;
    party::admit(access, Permission::Update, resource_type, Some(id))?;
    let uid = party::party_id(resource_type, id)?;
    let inbound = parse(body, resource_type)?;
    same_id(&inbound, id)?;
    let built = build(facade, loaded, inbound.document()).await?;
    let client = facade.client_for(headers);
    let demographic = party::demographic(&client)?;
    let class = class_of(loaded)?;
    let latest = party::get(demographic, class, resource_type, uid.value()).await?;
    party::accept(loaded, resource_type, id, &latest)?;
    let preceding = preceding(headers, &latest.version)?;
    let commit = commit(facade, access, loaded, Change::Modification)?;
    let version = commit_party(
        demographic,
        class,
        &built,
        &commit,
        Some((uid.value(), &preceding)),
    )
    .await?;
    respond(
        facade,
        loaded,
        resource_type,
        &built,
        &version,
        StatusCode::OK,
    )
    .await
}

/// `DELETE [base]/{type}/{id}` of a party type: the latest version is
/// deleted, and the answer is `204`.
pub(crate) async fn delete(
    facade: &Facade,
    access: &Access,
    loaded: &Loaded,
    (resource_type, id): (&str, &str),
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<axum::response::Response, Refusal> {
    crate::facade::handlers::guard(headers, uri, Body::Absent)?;
    party::admit(access, Permission::Delete, resource_type, Some(id))?;
    let uid = party::party_id(resource_type, id)?;
    let client = facade.client_for(headers);
    let demographic = party::demographic(&client)?;
    let class = class_of(loaded)?;
    let latest = party::get(demographic, class, resource_type, uid.value()).await?;
    party::accept(loaded, resource_type, id, &latest)?;
    let commit = commit(facade, access, loaded, Change::Deleted)?;
    macro_rules! delete_as {
        ($call:ident, $outcome:ident) => {{
            use openehr_its::rest::generated::demographic::client::$outcome;
            let Answered { outcome, upstream } = demographic
                .$call(&latest.version, &commit)
                .await
                .map_err(|error| party::client_refusal(&error))?
                .answered;
            match outcome {
                $outcome::NoContent { .. } => {}
                _ => return Err(party::refused(&upstream)),
            }
        }};
    }
    match class {
        PartyClass::Person => delete_as!(person_delete, PersonDeleteOutcome),
        PartyClass::Organisation => delete_as!(organisation_delete, OrganisationDeleteOutcome),
        PartyClass::Role => delete_as!(role_delete, RoleDeleteOutcome),
    }
    Ok(reply::minimal(StatusCode::NO_CONTENT, &[]))
}

/// Maps one inbound resource to its party.
async fn build(
    facade: &Facade,
    loaded: &Loaded,
    document: &fhir_types::codec::Value,
) -> Result<CanonicalParty, Refusal> {
    facade
        .translator()
        .run(Direction::ToOpenehr, |terminology| {
            crate::facade::engine::party_inbound(
                loaded.program(),
                loaded.index(),
                document,
                terminology,
                facade.demographics(),
            )
        })
        .await
        .map(fhirconnect::engine::outcome::Outcome::into_value)
        .map_err(|error| Refusal::from(crate::facade::ingest::refusal::engine_refusal(&error)))
}

/// Returns the commit headers one party write carries.
fn commit(
    facade: &Facade,
    access: &Access,
    loaded: &Loaded,
    change: Change,
) -> Result<CommitContext, Refusal> {
    let system_id = &facade.settings().system_id;
    crate::facade::commit::context(
        change,
        system_id,
        &Committer::of(access, system_id),
        loaded.program().template().id().as_str(),
    )
    .map_err(|error| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(crate::facade::outcome::chain(&error)),
        )
    })
}

/// Returns the version an update follows: the one `If-Match` names, under the
/// latest version's object id and creating system, or the latest.
fn preceding(headers: &HeaderMap, latest: &ObjectVersionId) -> Result<ObjectVersionId, Refusal> {
    let Some(value) = headers.get(header::IF_MATCH) else {
        return Ok(latest.clone());
    };
    let unreadable = |detail: String| {
        reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::Invalid).diagnosing(detail),
        )
    };
    let text = value
        .to_str()
        .map_err(|_unreadable| unreadable(String::from("the If-Match header is not text")))?;
    let tag = text.trim();
    let tag = tag.strip_prefix("W/").unwrap_or(tag);
    let tag = tag
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(tag);
    let tree = VersionTreeId::new(tag).map_err(|error| {
        unreadable(format!(
            "the If-Match header names no version: {}",
            crate::facade::outcome::chain(&error)
        ))
    })?;
    Ok(ObjectVersionId::compose(
        &latest.object_id(),
        &latest.creating_system_id(),
        &tree,
    ))
}

/// Commits `party`: its first version, or a version over `over`.
async fn commit_party(
    demographic: Demographic<'_>,
    class: PartyClass,
    party: &CanonicalParty,
    commit: &CommitContext,
    over: Option<(&str, &ObjectVersionId)>,
) -> Result<ObjectVersionId, Refusal> {
    macro_rules! write_as {
        ($typed:ty, $create:ident, $created:ident, $update:ident, $updated:ident) => {{
            use openehr_its::rest::generated::demographic::client::$created;
            use openehr_its::rest::generated::demographic::client::$updated;
            let typed: $typed =
                openehr_its::json::from_canonical_value(party.value()).map_err(|error| {
                    reply::refusal(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Issue::error(IssueType::Exception)
                            .diagnosing(format!("the built party does not decode: {error}")),
                    )
                })?;
            match over {
                None => {
                    let Answered { outcome, upstream } = demographic
                        .$create(&typed, commit, Prefer::Minimal)
                        .await
                        .map_err(|error| party::client_refusal(&error))?
                        .answered;
                    match outcome {
                        $created::Created { headers, .. } => party::version_of(
                            stringify!($create),
                            &upstream,
                            headers.etag.as_deref(),
                        )?,
                        $created::NoContent { headers } => party::version_of(
                            stringify!($create),
                            &upstream,
                            headers.etag.as_deref(),
                        )?,
                        _ => return Err(party::refused(&upstream)),
                    }
                }
                Some((uid, preceding)) => {
                    let Answered { outcome, upstream } = demographic
                        .$update(uid, preceding, &typed, commit, Prefer::Minimal)
                        .await
                        .map_err(|error| party::client_refusal(&error))?
                        .answered;
                    match outcome {
                        $updated::Ok { headers, .. } => party::version_of(
                            stringify!($update),
                            &upstream,
                            headers.etag.as_deref(),
                        )?,
                        $updated::NoContent { headers } => party::version_of(
                            stringify!($update),
                            &upstream,
                            headers.etag.as_deref(),
                        )?,
                        _ => return Err(party::refused(&upstream)),
                    }
                }
            }
        }};
    }
    Ok(match class {
        PartyClass::Person => write_as!(
            Person,
            person_create,
            PersonCreateOutcome,
            person_update,
            PersonUpdateOutcome
        ),
        PartyClass::Organisation => write_as!(
            Organisation,
            organisation_create,
            OrganisationCreateOutcome,
            organisation_update,
            OrganisationUpdateOutcome
        ),
        PartyClass::Role => write_as!(
            Role,
            role_create,
            RoleCreateOutcome,
            role_update,
            RoleUpdateOutcome
        ),
    })
}

/// Answers a write from the party it sent, under `version`, as `Prefer`
/// asks (<https://hl7.org/fhir/R4/http.html#return>).
async fn respond(
    facade: &Facade,
    loaded: &Loaded,
    resource_type: &str,
    party: &CanonicalParty,
    version: &ObjectVersionId,
    status: StatusCode,
) -> Result<axum::response::Response, Refusal> {
    let headers = [
        Header::location(&party::location(facade, resource_type, version)),
        Header::entity_tag(version.version_tree_id().value()),
    ];
    let resource = party::render(facade, loaded, party, version).await?;
    Ok(reply::resource(status, &resource, &headers))
}
