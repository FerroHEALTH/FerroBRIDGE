// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITS-REST 1.1.0 Demographic API, DEVELOPMENT.
//!
//! The release marks the module `x-status: DEVELOPMENT`
//! (`demographic-codegen.openapi.yaml`), so the bridge reaches it only when
//! `[cdr] demographic` is set, and every surface built on it carries that
//! label. The generated `openehr_its::rest::generated::demographic::client`
//! owns each operation, as it does for the rest of the CDR client; this module
//! adds the gate, the committal headers and the Release 1.1.0 `PARTY` reader
//! ([`party`]).
//!
//! openEHR is a registered trademark of the openEHR Foundation.

pub mod party;

use http::HeaderMap;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::client::Client;
use openehr_its::rest::client::ClientError;
use openehr_its::rest::generated::demographic::OrganisationCreateParams;
use openehr_its::rest::generated::demographic::OrganisationDeleteParams;
use openehr_its::rest::generated::demographic::OrganisationGetParams;
use openehr_its::rest::generated::demographic::OrganisationUpdateParams;
use openehr_its::rest::generated::demographic::PersonCreateParams;
use openehr_its::rest::generated::demographic::PersonDeleteParams;
use openehr_its::rest::generated::demographic::PersonGetParams;
use openehr_its::rest::generated::demographic::PersonUpdateParams;
use openehr_its::rest::generated::demographic::RoleCreateParams;
use openehr_its::rest::generated::demographic::RoleDeleteParams;
use openehr_its::rest::generated::demographic::RoleGetParams;
use openehr_its::rest::generated::demographic::RoleUpdateParams;
use openehr_its::rest::generated::demographic::VersionedPartyRevisionHistoryParams;
use openehr_its::rest::generated::demographic::client::DemographicClient;
use openehr_its::rest::generated::demographic::client::OrganisationCreateOutcome;
use openehr_its::rest::generated::demographic::client::OrganisationDeleteOutcome;
use openehr_its::rest::generated::demographic::client::OrganisationGetOutcome;
use openehr_its::rest::generated::demographic::client::OrganisationUpdateOutcome;
use openehr_its::rest::generated::demographic::client::PersonCreateOutcome;
use openehr_its::rest::generated::demographic::client::PersonDeleteOutcome;
use openehr_its::rest::generated::demographic::client::PersonGetOutcome;
use openehr_its::rest::generated::demographic::client::PersonUpdateOutcome;
use openehr_its::rest::generated::demographic::client::RoleCreateOutcome;
use openehr_its::rest::generated::demographic::client::RoleDeleteOutcome;
use openehr_its::rest::generated::demographic::client::RoleGetOutcome;
use openehr_its::rest::generated::demographic::client::RoleUpdateOutcome;
use openehr_its::rest::generated::demographic::client::VersionedPartyRevisionHistoryOutcome;
use openehr_rm::v1_2::demographic::organisation::Organisation;
use openehr_rm::v1_2::demographic::person::Person;
use openehr_rm::v1_2::demographic::role::Role;

use crate::cdr::Answered;
use crate::cdr::CdrClient;
use crate::cdr::Prefer;
use crate::cdr::VersionAtTime;
use crate::cdr::answered_by;
use crate::cdr::commit::CommitContext;
use crate::cdr::error::CdrError;
use crate::cdr::if_match_value;
use crate::cdr::representation;
use crate::cdr::transport::Scoped;

/// A documented answer of one Demographic API operation, with how many
/// `reverse_relationships` entries the `PARTY` it carried held before they
/// were dropped ([`party`]).
#[derive(Debug)]
pub struct PartyAnswered<O> {
    /// The generated outcome and the upstream answer it was read from.
    pub answered: Answered<O>,
    /// The `reverse_relationships` entries dropped from the body; zero when
    /// the body carried none or no `PARTY`.
    pub reverse_relationships_dropped: usize,
}

/// The Demographic API of one CDR, reached through a [`CdrClient`] whose
/// `[cdr] demographic` is set.
#[derive(Debug, Clone, Copy)]
pub struct Demographic<'c> {
    /// The client every call goes through.
    cdr: &'c CdrClient,
}

impl CdrClient {
    /// Returns the Demographic API of this CDR, or `None` when
    /// `[cdr] demographic` is off.
    ///
    /// The module is `x-status: DEVELOPMENT` in ITS-REST 1.1.0, so it is
    /// reached only on the operator's word (no specification governs this:
    /// our own design).
    #[must_use]
    pub const fn demographic(&self) -> Option<Demographic<'_>> {
        if self.demographic {
            Some(Demographic { cdr: self })
        } else {
            None
        }
    }
}

/// Returns the generated answer of `client` with the upstream answer and the
/// dropped `reverse_relationships` count it kept.
fn party_answered<O>(
    client: &Client<Scoped<'_>>,
    answered: Result<O, ClientError>,
) -> Result<PartyAnswered<O>, CdrError> {
    let reverse_relationships_dropped = client.transport().dropped();
    answered_by(client, answered).map(|answered| PartyAnswered {
        answered,
        reverse_relationships_dropped,
    })
}

/// Writes the four operations of one party kind: read, create, update and
/// delete, each through its generated operation.
macro_rules! party_operations {
    (
        kind: $kind:literal,
        party: $party:ty,
        get: $get:ident, $get_params:ident, $get_outcome:ident,
        create: $create:ident, $create_params:ident, $create_outcome:ident,
        update: $update:ident, $update_params:ident, $update_outcome:ident,
        delete: $delete:ident, $delete_params:ident, $delete_outcome:ident $(,)?
    ) => {
        #[doc = concat!("Reads the ", $kind, " `uid_based_id` names, at `version_at_time` when")]
        #[doc = concat!("given (`demographic-codegen.openapi.yaml`, `", stringify!($get), "`).")]
        ///
        /// # Errors
        /// Returns [`CdrError`] when the call did not reach a documented
        /// answer, a body the Release 1.1.0 reader refuses included.
        pub async fn $get(
            &self,
            uid_based_id: &str,
            version_at_time: Option<&VersionAtTime>,
        ) -> Result<PartyAnswered<$get_outcome>, CdrError> {
            let client = self.cdr.party_call(representation())?;
            let params = $get_params {
                uid_based_id: String::from(uid_based_id),
                version_at_time: version_at_time.map(|at| String::from(at.as_str())),
                accept: None,
            };
            let answered = DemographicClient::new(&client).$get(&params).await;
            party_answered(&client, answered)
        }

        #[doc = concat!("Creates the first version of a ", $kind, " (`demographic-codegen.openapi.yaml`,")]
        #[doc = concat!("`", stringify!($create), "`). The call is a `POST`, so it is never retried.")]
        ///
        /// # Errors
        /// Returns [`CdrError`] when the commit metadata cannot travel in a
        /// header or the call did not reach a documented answer.
        pub async fn $create(
            &self,
            party: &$party,
            commit: &CommitContext,
            prefer: Prefer,
        ) -> Result<PartyAnswered<$create_outcome>, CdrError> {
            let headers = commit.headers()?;
            let client = self.cdr.party_call(HeaderMap::new())?;
            let params = $create_params {
                prefer: Some(prefer.param()),
                accept: None,
                content_type: None,
                openehr_item_tag: None,
                openehr_version_item_tag: None,
                openehr_version: headers.version,
                openehr_audit_details: headers.audit_details,
            };
            let answered = DemographicClient::new(&client).$create(&params, party).await;
            party_answered(&client, answered)
        }

        #[doc = concat!("Commits a new version of the ", $kind, " `uid_based_id` names over")]
        #[doc = concat!("`preceding`, which travels as `If-Match` (`demographic-codegen.openapi.yaml`, `", stringify!($update), "`).")]
        ///
        /// # Errors
        /// Returns [`CdrError`] when the commit metadata cannot travel in a
        /// header or the call did not reach a documented answer.
        pub async fn $update(
            &self,
            uid_based_id: &str,
            preceding: &ObjectVersionId,
            party: &$party,
            commit: &CommitContext,
            prefer: Prefer,
        ) -> Result<PartyAnswered<$update_outcome>, CdrError> {
            let headers = commit.headers()?;
            let client = self.cdr.party_call(HeaderMap::new())?;
            let params = $update_params {
                uid_based_id: String::from(uid_based_id),
                if_match: if_match_value(preceding),
                prefer: Some(prefer.param()),
                accept: None,
                content_type: None,
                openehr_version_item_tag: None,
                openehr_version: headers.version,
                openehr_audit_details: headers.audit_details,
            };
            let answered = DemographicClient::new(&client).$update(&params, party).await;
            party_answered(&client, answered)
        }

        #[doc = concat!("Deletes the ", $kind, " version `version_id` names, which must be the latest")]
        #[doc = concat!("(`demographic-codegen.openapi.yaml`, `", stringify!($delete), "`).")]
        ///
        /// # Errors
        /// Returns [`CdrError`] when the commit metadata cannot travel in a
        /// header or the call did not reach a documented answer.
        pub async fn $delete(
            &self,
            version_id: &ObjectVersionId,
            commit: &CommitContext,
        ) -> Result<PartyAnswered<$delete_outcome>, CdrError> {
            let headers = commit.headers()?;
            let client = self.cdr.party_call(HeaderMap::new())?;
            let params = $delete_params {
                uid_based_id: String::from(version_id.value()),
                openehr_version: headers.version,
                openehr_audit_details: headers.audit_details,
            };
            let answered = DemographicClient::new(&client).$delete(&params).await;
            party_answered(&client, answered)
        }
    };
}

impl Demographic<'_> {
    /// Reads the revision history of the `VERSIONED_PARTY` `versioned_object_uid`
    /// names (`demographic-codegen.openapi.yaml`,
    /// `versioned_party_revision_history`).
    ///
    /// # Errors
    /// Returns [`CdrError`] when the call did not reach a documented answer.
    pub async fn revision_history(
        &self,
        versioned_object_uid: &str,
    ) -> Result<Answered<VersionedPartyRevisionHistoryOutcome>, CdrError> {
        let client = self.cdr.party_call(HeaderMap::new())?;
        let params = VersionedPartyRevisionHistoryParams {
            versioned_object_uid: String::from(versioned_object_uid),
            accept: None,
            content_type: None,
        };
        let answered = DemographicClient::new(&client)
            .versioned_party_revision_history(&params)
            .await;
        answered_by(&client, answered)
    }

    party_operations! {
        kind: "PERSON",
        party: Person,
        get: person_get, PersonGetParams, PersonGetOutcome,
        create: person_create, PersonCreateParams, PersonCreateOutcome,
        update: person_update, PersonUpdateParams, PersonUpdateOutcome,
        delete: person_delete, PersonDeleteParams, PersonDeleteOutcome,
    }

    party_operations! {
        kind: "ORGANISATION",
        party: Organisation,
        get: organisation_get, OrganisationGetParams, OrganisationGetOutcome,
        create: organisation_create, OrganisationCreateParams, OrganisationCreateOutcome,
        update: organisation_update, OrganisationUpdateParams, OrganisationUpdateOutcome,
        delete: organisation_delete, OrganisationDeleteParams, OrganisationDeleteOutcome,
    }

    party_operations! {
        kind: "ROLE",
        party: Role,
        get: role_get, RoleGetParams, RoleGetOutcome,
        create: role_create, RoleCreateParams, RoleCreateOutcome,
        update: role_update, RoleUpdateParams, RoleUpdateOutcome,
        delete: role_delete, RoleDeleteParams, RoleDeleteOutcome,
    }
}
