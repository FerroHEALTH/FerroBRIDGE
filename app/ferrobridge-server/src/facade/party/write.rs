// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Create, update and delete of a party resource.
//!
//! A create maps the resource to a party and commits its first version, and
//! for a party holding relationships a second version that sources them at
//! the uid the CDR assigned; an update commits a new version over the one
//! `If-Match` names, or over the latest when the request names none, so a
//! concurrent writer still meets the `412` the CDR answers; a delete removes
//! the latest version
//! (<https://hl7.org/fhir/R4/http.html>). Each answers from the party it sent,
//! under the version the CDR's `ETag` names, and writes no identity-map entry.
//! ITS-REST 1.1.0 Demographic API, DEVELOPMENT. No specification governs the
//! missing-`If-Match` rule: our own design.

use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use fhirconnect::engine::lens::party_ref::NAMESPACE;
use fhirconnect::engine::traverse::error::EngineError;
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
use crate::facade::handlers::Body;
use crate::facade::handlers::read::parse;
use crate::facade::handlers::write::same_id;
use crate::facade::ingest::Refused;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::party;
use crate::facade::party::read::class_of;
use crate::facade::programs::Loaded;
use crate::facade::reply;
use crate::facade::reply::Header;
use crate::facade::reply::Refusal;
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
    party::unbound(facade, resource_type, built.value())?;
    let client = facade.client_for(headers, access).await?;
    let demographic = party::demographic(&client)?;
    let class = class_of(loaded)?;
    let creation = commit(facade, loaded, Change::Creation)?;
    let modification = commit(facade, loaded, Change::Modification)?;
    let first = commit_party(demographic, class, &built, &creation, None).await?;
    let (built, version) = relate(loaded, (demographic, class), &built, &modification, first)
        .await
        .map_err(|refused| Refusal::from(refused.with_issue(unrelated(resource_type))))?;
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
    party::unbound(facade, resource_type, built.value())?;
    let client = facade.client_for(headers, access).await?;
    let demographic = party::demographic(&client)?;
    let class = class_of(loaded)?;
    let latest = party::get(demographic, class, resource_type, uid.value()).await?;
    party::accept(loaded, resource_type, id, &latest)?;
    party::unbound(facade, resource_type, &latest.value)?;
    let built = sourced(loaded, &built, uid.value())?;
    let preceding = preceding(headers, &latest.version)?;
    let commit = commit(facade, loaded, Change::Modification)?;
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
    let client = facade.client_for(headers, access).await?;
    let demographic = party::demographic(&client)?;
    let class = class_of(loaded)?;
    let latest = party::get(demographic, class, resource_type, uid.value()).await?;
    party::accept(loaded, resource_type, id, &latest)?;
    party::unbound(facade, resource_type, &latest.value)?;
    let commit = commit(facade, loaded, Change::Deleted)?;
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
                _ => return Err(party::refused(&upstream).into()),
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
    party::ehds::typed_identifiers(loaded, document)?;
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

/// Commits the relationships `built` awaits the source of as a second
/// version over `first`, sourced at the party the CDR created, and returns
/// the party and version the create answers with.
///
/// Every relationship a party holds has the party as its source (RM
/// Demographic IM, the `PARTY` class, invariant `Relationships_validity`),
/// and the CDR assigns the uid when it commits the first version.
async fn relate(
    loaded: &Loaded,
    (demographic, class): (Demographic<'_>, PartyClass),
    built: &CanonicalParty,
    commit: &CommitContext,
    first: ObjectVersionId,
) -> Result<(CanonicalParty, ObjectVersionId), Refused> {
    if !built.awaits_source() {
        return Ok((built.clone(), first));
    }
    let uid = first.versioned_object_id();
    let related = loaded
        .index()
        .source_relationships(built, uid.value(), NAMESPACE)
        .map_err(unbuilt)?;
    let version = commit_party(
        demographic,
        class,
        &related,
        commit,
        Some((uid.value(), &first)),
    )
    .await?;
    Ok((related, version))
}

/// Returns the warning a create whose relationships were not committed adds:
/// the party stands under its id without them.
fn unrelated(resource_type: &str) -> Issue {
    Issue::warning(IssueType::Informational).diagnosing(format!(
        "the {resource_type} was created in the {} and its relationships were not: it stands \
         without them until an update writes them",
        party::LABEL
    ))
}

/// Returns `built` with the relationships it awaits sourced at the party
/// `uid` names.
fn sourced(loaded: &Loaded, built: &CanonicalParty, uid: &str) -> Result<CanonicalParty, Refusal> {
    loaded
        .index()
        .source_relationships(built, uid, NAMESPACE)
        .map_err(|error| Refusal::from(unbuilt(error)))
}

/// Returns the refusal of a party whose relationships do not build.
fn unbuilt(error: openehr_mapping_core::party::PartyError) -> Refused {
    crate::facade::ingest::refusal::engine_refusal(&EngineError::PartyBuild {
        source: Box::new(error),
    })
}

/// Returns the commit headers one party write carries.
fn commit(facade: &Facade, loaded: &Loaded, change: Change) -> Result<CommitContext, Refusal> {
    let system_id = &facade.settings().system_id;
    crate::facade::commit::context(change, system_id, loaded.program().template().id().as_str())
        .map_err(|error| {
            reply::refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception)
                    .diagnosing(ferrohealth_server::run::chain(&error)),
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
            ferrohealth_server::run::chain(&error)
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
) -> Result<ObjectVersionId, Refused> {
    let touch = if over.is_some() {
        crate::cdr::reach::Touch::Updated
    } else {
        crate::cdr::reach::Touch::Created
    };
    crate::cdr::reach::touched(touch, None, crate::cdr::reach::canonical(party.value()));
    macro_rules! write_as {
        ($typed:ty, $create:ident, $created:ident, $update:ident, $updated:ident) => {{
            use openehr_its::rest::generated::demographic::client::$created;
            use openehr_its::rest::generated::demographic::client::$updated;
            let typed: $typed =
                openehr_its::json::from_canonical_value(party.value()).map_err(|error| {
                    Refused::new(
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
