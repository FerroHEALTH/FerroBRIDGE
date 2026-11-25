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
//!
//! This module owns what every party interaction shares: the [`Call`] it
//! serves, its admission, the party it opens and the read of one version.

pub(crate) mod ehds;
pub(crate) mod provision;
pub(crate) mod read;
pub(crate) mod write;

use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Compartment;
use ferrohealth_smart::scope::Permission;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_mapping_core::party::CanonicalParty;
use openehr_mapping_core::party::PartyClass;

use crate::cdr::Answered;
use crate::cdr::CdrClient;
use crate::cdr::demographic::Demographic;
use crate::cdr::error::Upstream;
use crate::facade::Facade;
use crate::facade::handlers::Body;
use crate::facade::handlers::write::refuse;
use crate::facade::ingest::Refused;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::programs::Loaded;
use crate::facade::reply;
use crate::facade::reply::Refusal;
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
        Some(Compartment::Patient(_)) | None => Err(crate::facade::guard::forbidden()),
    }
}

/// The request one party interaction serves: the facade, the caller, the
/// party program of the type and the request's headers and URL.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Call<'a> {
    /// The facade serving the request.
    pub(crate) facade: &'a Facade,
    /// What the layer admitted the caller with.
    pub(crate) access: &'a Access,
    /// The party program of the type.
    pub(crate) loaded: &'a Loaded,
    /// The request's headers.
    pub(crate) headers: &'a HeaderMap,
    /// The request's URL.
    pub(crate) uri: &'a Uri,
}

impl<'a> Call<'a> {
    /// Returns the call of `facade` for the caller `access` on the party
    /// program `loaded`, with the request's headers and URL.
    #[must_use]
    pub(crate) const fn new(
        facade: &'a Facade,
        access: &'a Access,
        loaded: &'a Loaded,
        (headers, uri): (&'a HeaderMap, &'a Uri),
    ) -> Self {
        Self {
            facade,
            access,
            loaded,
            headers,
            uri,
        }
    }

    /// Admits the call on the party `id` names and opens the party, the
    /// first steps of a read, a history and a delete.
    ///
    /// # Errors
    ///
    /// Returns the [`Refusal`] of [`Self::admit`] or [`Self::connect`].
    pub(crate) async fn open(
        &self,
        permission: Permission,
        party: (&str, &str),
        body: Body,
    ) -> Result<Opened, Refusal> {
        let uid = self.admit(permission, party, body)?;
        self.connect(uid).await
    }

    /// Opens the party `uid` names: the CDR client the call runs through and
    /// the class of the party the program maps.
    ///
    /// # Errors
    ///
    /// Returns the [`Refusal`] of the client, of a CDR with no Demographic
    /// API or of a program of no party class.
    pub(crate) async fn connect(&self, uid: HierObjectId) -> Result<Opened, Refusal> {
        let client = self.facade.client_for(self.headers, self.access).await?;
        let (_, class) = reach(&client, self.loaded)?;
        Ok(Opened { uid, client, class })
    }

    /// Admits the call on the party `id` names and returns its id: the
    /// request shape the route takes, then the caller's `permission` on the
    /// type.
    ///
    /// # Errors
    ///
    /// Returns the [`Refusal`] of the first check that fails.
    pub(crate) fn admit(
        &self,
        permission: Permission,
        (resource_type, id): (&str, &str),
        body: Body,
    ) -> Result<HierObjectId, Refusal> {
        crate::facade::handlers::guard(self.headers, self.uri, body)?;
        admit(self.access, permission, resource_type, Some(id))?;
        party_id(resource_type, id)
    }
}

/// One party a call has opened: its id, the CDR client and its class.
#[derive(Debug)]
pub(crate) struct Opened {
    /// The `versioned_object_uid` of the party.
    pub(crate) uid: HierObjectId,
    /// The client the call reaches the CDR through.
    client: CdrClient,
    /// The class of the party the program maps.
    class: PartyClass,
}

impl Opened {
    /// Returns the Demographic API the party is reached through, and its
    /// class.
    ///
    /// # Errors
    ///
    /// Returns the `500` of a CDR with no Demographic API.
    pub(crate) fn party(&self) -> Result<(Demographic<'_>, PartyClass), Refusal> {
        Ok((demographic(&self.client)?, self.class))
    }

    /// Reads the latest version of the party.
    ///
    /// # Errors
    ///
    /// Returns the [`Refusal`] of [`get`].
    pub(crate) async fn latest(&self, resource_type: &str) -> Result<Held, Refusal> {
        let (demographic, class) = self.party()?;
        get(demographic, class, resource_type, self.uid.value()).await
    }
}

/// Returns the Demographic API of `client` and the class of the party
/// `loaded` maps, the pair every party read and write goes through.
///
/// # Errors
///
/// Returns the `500` of a CDR with no Demographic API or a program of no
/// party class.
pub(crate) fn reach<'client>(
    client: &'client CdrClient,
    loaded: &Loaded,
) -> Result<(Demographic<'client>, PartyClass), Refusal> {
    Ok((demographic(client)?, read::class_of(loaded)?))
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
                ferrohealth_server::run::chain(&error)
            )),
        )
    })
}

/// Returns the refusal a CDR answer outside the documented successes renders
/// as: its row in the Demographic table, or the undocumented row.
pub(crate) fn refused(upstream: &Upstream) -> Refused {
    let answer = status::demographic::of_demographic_refusal(upstream).unwrap_or_else(|| {
        status::Answer::new(
            status::UNDOCUMENTED,
            format!(
                "the {LABEL} answered {} outside the operation's documented answers",
                upstream.status()
            ),
        )
    });
    Refused::of_answer(&answer)
}

/// Returns the refusal a call that reached no documented answer renders as.
pub(crate) fn client_refusal(error: &crate::cdr::error::CdrError) -> Refused {
    Refused::of_answer(&status::of_client_error(error))
}

/// Returns the version the `ETag` of a successful answer names.
pub(crate) fn version_of(
    operation: &'static str,
    upstream: &Upstream,
    etag: Option<&str>,
) -> Result<ObjectVersionId, Refused> {
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
    match fetch(demographic, class, uid).await? {
        Fetched::Held(held) => Ok(held),
        Fetched::Deleted => Err(gone(resource_type, uid)),
        Fetched::Absent(upstream) => Err(refused(&upstream).into()),
    }
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
    Ok(match fetch(demographic, class, uid).await? {
        Fetched::Held(held) => Some(held),
        Fetched::Deleted | Fetched::Absent(_) => None,
    })
}

/// What the CDR answered for a party `uid`.
enum Fetched {
    /// The version it holds.
    Held(Held),
    /// It reports the party deleted (`204`).
    Deleted,
    /// It holds none (`404`), with the answer it gave.
    Absent(Upstream),
}

/// Reads the party `uid` names, the latest version for a `HIER_OBJECT_ID`
/// or one version for an `OBJECT_VERSION_ID`.
async fn fetch(
    demographic: Demographic<'_>,
    class: PartyClass,
    uid: &str,
) -> Result<Fetched, Refusal> {
    macro_rules! fetch_as {
        ($call:ident, $outcome:ident) => {{
            use openehr_its::rest::generated::demographic::client::$outcome;
            let Answered { outcome, upstream } = demographic
                .$call(uid, None)
                .await
                .map_err(|error| client_refusal(&error))?
                .answered;
            match outcome {
                $outcome::Ok { body, headers } => Fetched::Held(Held {
                    value: openehr_its::json::to_canonical_value(&body),
                    version: version_of(stringify!($call), &upstream, headers.etag.as_deref())?,
                }),
                $outcome::NoContent => Fetched::Deleted,
                $outcome::NotFound { .. } => Fetched::Absent(upstream),
            }
        }};
    }
    Ok(match class {
        PartyClass::Person => fetch_as!(person_get, PersonGetOutcome),
        PartyClass::Organisation => fetch_as!(organisation_get, OrganisationGetOutcome),
        PartyClass::Role => fetch_as!(role_get, RoleGetOutcome),
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
    crate::cdr::reach::touched(
        crate::cdr::reach::Touch::Read,
        None,
        crate::cdr::reach::canonical(&held.value),
    );
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
                ferrohealth_server::run::chain(&error)
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
    claim(&mut resource, loaded.program().profile().url().as_str());
    Ok(resource)
}

/// Writes the profile the party context names into `meta.profile`, as a
/// rendered entry claims its context's profile.
fn claim(resource: &mut Object, profile: &str) {
    if let Some(Value::Object(meta)) = resource.get_mut("meta") {
        meta.insert(
            String::from("profile"),
            Value::Array(vec![Value::String(profile.to_owned())]),
        );
    }
}

/// Writes the id and `meta.versionId` a party version gives its resource.
pub(crate) fn stamp(resource: &mut Object, version: &ObjectVersionId) {
    crate::facade::identity::stamp(
        resource,
        version.versioned_object_id().value(),
        version.version_tree_id().value(),
        None,
    );
}

/// Returns the `[base]/[type]/[id]/_history/[vid]` of a party version.
pub(crate) fn location(facade: &Facade, resource_type: &str, version: &ObjectVersionId) -> String {
    format!(
        "{}/{resource_type}/{}/_history/{}",
        facade.settings().base_url.trim_end_matches('/'),
        version.versioned_object_id().value(),
        version.version_tree_id().value()
    )
}

/// Refuses a write of `party`, the canonical JSON of a party the request
/// would commit or replace, when it carries a binding key: a `DV_IDENTIFIER`
/// whose `issuer` is one the facade is configured with.
///
/// A party bound to a login is provisioned through the CDR's SCIM service and
/// written only by an administrator or that SCIM client, so the facade
/// answers `403` before it commits anything (no specification governs the
/// binding: our own design).
pub(crate) fn unbound(
    facade: &Facade,
    resource_type: &str,
    party: &serde_json::Value,
) -> Result<(), Refusal> {
    let issuers = facade.binding_issuers();
    if issuers.is_empty() {
        return Ok(());
    }
    match binding_issuer(party, issuers) {
        None => Ok(()),
        Some(issuer) => Err(reply::refusal(
            StatusCode::FORBIDDEN,
            Issue::error(IssueType::Forbidden).diagnosing(format!(
                "this {resource_type} carries a binding identifier of {issuer}: a party bound \
                 to a login is provisioned through SCIM and never written through the facade"
            )),
        )),
    }
}

/// Returns the first of `issuers` a `DV_IDENTIFIER` anywhere in `value`
/// names as its `issuer`.
fn binding_issuer<'a>(value: &serde_json::Value, issuers: &'a [String]) -> Option<&'a str> {
    match value {
        serde_json::Value::Object(members) => {
            let named = (members.get("_type").and_then(serde_json::Value::as_str)
                == Some("DV_IDENTIFIER"))
            .then(|| members.get("issuer").and_then(serde_json::Value::as_str))
            .flatten()
            .and_then(|issuer| issuers.iter().find(|listed| *listed == issuer));
            named.map(String::as_str).or_else(|| {
                members
                    .values()
                    .find_map(|item| binding_issuer(item, issuers))
            })
        }
        serde_json::Value::Array(items) => {
            items.iter().find_map(|item| binding_issuer(item, issuers))
        }
        _ => None,
    }
}
