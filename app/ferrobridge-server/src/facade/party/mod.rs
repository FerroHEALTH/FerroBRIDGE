// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources a party context serves, over the ITS-REST 1.1.0 Demographic
//! API, DEVELOPMENT.
//!
//! A party program (`Program::party`) maps one FHIR resource type to a
//! demographic `PARTY`: `Patient` and `Practitioner` to a `PERSON`,
//! `PractitionerRole` to a `ROLE`, `Organization` to an `ORGANISATION`, as the
//! loaded mapping files say. The facade reads, vreads, lists the history of,
//! creates, updates and deletes it through the generated Demographic client
//! behind `[cdr] demographic`, and writes no identity-map entry: the FHIR id
//! is the party's `versioned_object_uid`, the object id of its
//! `OBJECT_VERSION_ID`, which only stays stable across versions (RM Common IM
//! §`OBJECT_VERSION_ID`), and `meta.versionId` is the version tree id. An R4
//! `Resource.id` "never changes" (<https://hl7.org/fhir/R4/resource.html>).
//! A `Patient` create also gives the new party its EHR when the facade
//! creates EHRs (`provision`), which records only the subject's `ehr_id`.
//! Every CDR refusal reaches the wire through
//! [`crate::facade::status::demographic`]. No specification governs a party
//! context or the correspondences: our own design.
//!
//! The Demographic API has no search or list operation, so a party type
//! offers no search unless `[facade.party_search]` configures one through
//! Cadasto's experimental AQL demographic extension
//! (`crate::facade::search::party`), and the `CapabilityStatement` says so.

pub(crate) mod provision;
pub(crate) mod read;
pub(crate) mod write;

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_mapping_core::party::CanonicalParty;
use openehr_mapping_core::party::PartyClass;

use crate::auth::Access;
use crate::auth::Compartment;
use crate::auth::scope::Permission;
use crate::cdr::Answered;
use crate::cdr::CdrClient;
use crate::cdr::demographic::Demographic;
use crate::cdr::error::Upstream;
use crate::cdr::ids::versioned_object_uid;
use crate::facade::Facade;
use crate::facade::handlers::Refusal;
use crate::facade::handlers::write::refuse;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::programs::Loaded;
use crate::facade::reply;
use crate::facade::status;
use crate::terminology::Direction;

/// The label every surface the Demographic API backs carries.
pub const LABEL: &str = "ITS-REST 1.1.0 Demographic API, DEVELOPMENT";

/// The `RESOURCE` of the `CapabilityStatement` documentation of a party type.
pub const DOCUMENTATION: &str = "Served from a demographic PARTY over the ITS-REST 1.1.0 Demographic API, DEVELOPMENT. The id is the party's versioned_object_uid and meta.versionId its version tree id. The Demographic API has no search or list operation, so this type offers no search.";

/// The `RESOURCE` of the `CapabilityStatement` documentation of a party type
/// `[facade.party_search]` searches.
pub const SEARCHED_DOCUMENTATION: &str = "Served from a demographic PARTY over the ITS-REST 1.1.0 Demographic API, DEVELOPMENT. The id is the party's versioned_object_uid and meta.versionId its version tree id. The Demographic API has no search or list operation, so this type is searched by identifier only, through the Cadasto experimental AQL demographic extension.";

/// One party as the CDR holds it: its canonical JSON and its version.
#[derive(Debug)]
pub(crate) struct Held {
    /// The canonical JSON of the party.
    pub(crate) value: serde_json::Value,
    /// The version the CDR answered.
    pub(crate) version: ObjectVersionId,
}

/// Returns the Demographic API of `client`.
///
/// A party program loads only with `[cdr] demographic` set
/// ([`crate::facade::programs::check_parties`]), so an absent API is a
/// deployment fault.
pub(crate) fn demographic(client: &CdrClient) -> Result<Demographic<'_>, Refusal> {
    client.demographic().ok_or_else(|| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "a party program is loaded and the {LABEL} is off: set [cdr] demographic"
            )),
        )
    })
}

/// Checks that `access` lets `permission` on `resource_type` reach the party
/// `id` names.
///
/// A party is the resource, so a `patient/` token reaches the one `Patient`
/// whose id its launch context names and no other party ("the `patient`
/// launch context" names that `Patient`,
/// <https://hl7.org/fhir/smart-app-launch/scopes-and-launch-context.html>).
/// No specification governs the mapping onto a party: our own design.
pub(crate) fn admit(
    access: &Access,
    permission: Permission,
    resource_type: &str,
    id: Option<&str>,
) -> Result<(), Refusal> {
    match access.reach(permission, resource_type) {
        Some(Compartment::Any) => Ok(()),
        Some(Compartment::Patient(patient))
            if resource_type == "Patient" && id == Some(patient.as_str()) =>
        {
            Ok(())
        }
        Some(Compartment::Patient(_)) | None => Err(crate::auth::guard::forbidden()),
    }
}

/// Reads `id` as the `versioned_object_uid` of a party.
///
/// An id that is no `uid` names no party, so it is `404`.
pub(crate) fn party_id(resource_type: &str, id: &str) -> Result<HierObjectId, Refusal> {
    HierObjectId::new(id).map_err(|error| {
        reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotFound).diagnosing(format!(
                "{resource_type}/{id} names no party: {}",
                crate::facade::outcome::chain(&error)
            )),
        )
    })
}

/// Returns the refusal a CDR answer outside the documented successes renders
/// as: its row in the Demographic table, or the undocumented row.
pub(crate) fn refused(upstream: &Upstream) -> Refusal {
    match status::demographic::of_demographic_refusal(upstream) {
        Some(answer) => refuse(&answer),
        None => refuse(&status::Answer::new(
            status::UNDOCUMENTED,
            format!(
                "the {LABEL} answered {} outside the operation's documented answers",
                upstream.status()
            ),
        )),
    }
}

/// Returns the refusal a call that reached no documented answer renders as.
pub(crate) fn client_refusal(error: &crate::cdr::error::CdrError) -> Refusal {
    refuse(&status::of_client_error(error))
}

/// Returns the version the `ETag` of a successful answer names.
pub(crate) fn version_of(
    operation: &'static str,
    upstream: &Upstream,
    etag: Option<&str>,
) -> Result<ObjectVersionId, Refusal> {
    crate::cdr::version_from_etag(operation, upstream.status(), etag)
        .map_err(|error| client_refusal(&error))
}

/// Reads the party `uid` names, a `versioned_object_uid` for the latest
/// version or an `OBJECT_VERSION_ID` for one version.
///
/// A party the CDR reports deleted (`204`) is `410`.
pub(crate) async fn get(
    demographic: Demographic<'_>,
    class: PartyClass,
    resource_type: &str,
    uid: &str,
) -> Result<Held, Refusal> {
    macro_rules! get_as {
        ($call:ident, $outcome:ident) => {{
            use openehr_its::rest::generated::demographic::client::$outcome;
            let Answered { outcome, upstream } = demographic
                .$call(uid, None)
                .await
                .map_err(|error| client_refusal(&error))?
                .answered;
            match outcome {
                $outcome::Ok { body, headers } => Held {
                    value: openehr_its::json::to_canonical_value(&body),
                    version: version_of(stringify!($call), &upstream, headers.etag.as_deref())?,
                },
                $outcome::NoContent => return Err(gone(resource_type, uid)),
                _ => return Err(refused(&upstream)),
            }
        }};
    }
    Ok(match class {
        PartyClass::Person => get_as!(person_get, PersonGetOutcome),
        PartyClass::Organisation => get_as!(organisation_get, OrganisationGetOutcome),
        PartyClass::Role => get_as!(role_get, RoleGetOutcome),
    })
}

/// Reads the latest version of the party `uid` names, `None` when the CDR
/// holds none (`404`) or reports it deleted (`204`).
///
/// A search by `_id` matches the party or nothing, so neither answer is a
/// refusal there (no specification governs this: our own design).
pub(crate) async fn find(
    demographic: Demographic<'_>,
    class: PartyClass,
    uid: &str,
) -> Result<Option<Held>, Refusal> {
    macro_rules! find_as {
        ($call:ident, $outcome:ident) => {{
            use openehr_its::rest::generated::demographic::client::$outcome;
            let Answered { outcome, upstream } = demographic
                .$call(uid, None)
                .await
                .map_err(|error| client_refusal(&error))?
                .answered;
            match outcome {
                $outcome::Ok { body, headers } => Some(Held {
                    value: openehr_its::json::to_canonical_value(&body),
                    version: version_of(stringify!($call), &upstream, headers.etag.as_deref())?,
                }),
                $outcome::NoContent | $outcome::NotFound { .. } => None,
            }
        }};
    }
    Ok(match class {
        PartyClass::Person => find_as!(person_get, PersonGetOutcome),
        PartyClass::Organisation => find_as!(organisation_get, OrganisationGetOutcome),
        PartyClass::Role => find_as!(role_get, RoleGetOutcome),
    })
}

/// Answers whether `held` is a party of `loaded`'s template.
pub(crate) fn of_template(loaded: &Loaded, held: &Held) -> bool {
    held.value
        .pointer("/archetype_details/template_id/value")
        .and_then(serde_json::Value::as_str)
        == Some(loaded.program().template().id().as_str())
}

/// Returns the `410` of a party the CDR reports deleted.
fn gone(resource_type: &str, uid: &str) -> Refusal {
    reply::refusal(
        StatusCode::GONE,
        Issue::error(IssueType::Deleted)
            .diagnosing(format!("{resource_type}/{uid} is deleted in the {LABEL}")),
    )
}

/// Returns the party `held` carries as one of `loaded`'s template.
///
/// A party built against another template is no resource this program
/// serves, so it is `404` (no specification governs this: our own design).
pub(crate) fn accept(
    loaded: &Loaded,
    resource_type: &str,
    id: &str,
    held: &Held,
) -> Result<CanonicalParty, Refusal> {
    let template = held
        .value
        .pointer("/archetype_details/template_id/value")
        .and_then(serde_json::Value::as_str);
    let wanted = loaded.program().template().id().as_str();
    if template != Some(wanted) {
        return Err(reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotFound).diagnosing(format!(
                "{resource_type}/{id} is a party of the template {}, and this server serves \
                 {resource_type} from {wanted}",
                template.unwrap_or("(none)")
            )),
        ));
    }
    loaded.index().accept_party(&held.value).map_err(|error| {
        reply::refusal(
            StatusCode::BAD_GATEWAY,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "the {LABEL} answered {resource_type}/{id} with a party the template does not \
                 admit: {}",
                crate::facade::outcome::chain(&error)
            )),
        )
    })
}

/// Maps `party` back to a resource carrying its id and version.
pub(crate) async fn render(
    facade: &Facade,
    loaded: &Loaded,
    party: &CanonicalParty,
    version: &ObjectVersionId,
) -> Result<Object, Refusal> {
    let outcome = facade
        .translator()
        .run(Direction::ToFhir, |terminology| {
            crate::facade::engine::party_outbound(
                loaded.program(),
                loaded.index(),
                party,
                terminology,
                facade.demographics(),
            )
        })
        .await
        .map_err(|error| refuse(&status::of_outbound_refusal(&error)))?;
    let Value::Object(mut resource) = outcome.into_value() else {
        return Err(reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception)
                .diagnosing("the party program produced no resource object"),
        ));
    };
    stamp(&mut resource, version);
    Ok(resource)
}

/// Writes the id and `meta.versionId` a party version gives its resource.
pub(crate) fn stamp(resource: &mut Object, version: &ObjectVersionId) {
    resource.insert(
        String::from("id"),
        Value::String(String::from(versioned_object_uid(version).value())),
    );
    let mut meta = match resource.remove("meta") {
        Some(Value::Object(meta)) => meta,
        _ => Object::new(),
    };
    meta.insert(
        String::from("versionId"),
        Value::String(String::from(version.version_tree_id().value())),
    );
    resource.insert(String::from("meta"), Value::Object(meta));
}

/// Returns the `[base]/[type]/[id]/_history/[vid]` of a party version.
pub(crate) fn location(facade: &Facade, resource_type: &str, version: &ObjectVersionId) -> String {
    format!(
        "{}/{resource_type}/{}/_history/{}",
        facade.settings().base_url.trim_end_matches('/'),
        versioned_object_uid(version).value(),
        version.version_tree_id().value()
    )
}
