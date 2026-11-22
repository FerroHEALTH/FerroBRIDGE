// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The committal metadata every write this facade makes carries.
//!
//! ITS-REST 1.1.0 leaves the committal metadata headers optional ("None of
//! these headers are mandatory"), and the CONTRIBUTION body carries the same
//! facts as schema members instead (`ehr-codegen.openapi.yaml`,
//! `contribution_create`). This module builds both from one description, so a
//! single write and a transaction record the same change type and system.
//!
//! The CDR writes who committed, from the caller it authenticated: a header
//! commit states no committer, and a CONTRIBUTION, whose `UpdateAudit`
//! requires one, names that same caller and nothing more. `system_id` always
//! names the bridge. Which form names the caller is our own design.

use std::collections::BTreeMap;

use crate::cdr::Principal;
use crate::cdr::commit::CommitContext;
use crate::cdr::commit::HeaderAudit;
use crate::cdr::ids::IdError;
use crate::cdr::ids::template_id;
use ferrohealth_smart::kind::CallerKind;
use ferrohealth_smart::resource::Access;
use openehr_base::v1_3::base_types::identification::terminology_id::TerminologyId;
use openehr_its::rest::generated::common::UpdateAudit;
use openehr_its::rest::generated::common::UpdateAuditData;
use openehr_rm::v1_2::common::generic::party_identified::PartyIdentified;
use openehr_rm::v1_2::common::generic::party_identified::PartyIdentifiedData;
use openehr_rm::v1_2::common::generic::party_proxy::PartyProxy;
use openehr_rm::v1_2::data_types::basic::dv_identifier::DvIdentifier;
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
use openehr_rm::v1_2::support::terminology::openehr_terminology_group_identifiers::OpenehrTerminologyGroupIdentifiersData;
use openehr_term::codes::AuditChangeType;
use openehr_term::codes::VersionLifecycleState;

/// The terminology the openEHR audit code sets belong to.
///
/// `AUDIT_DETAILS.change_type` and `VERSION.lifecycle_state` are coded from
/// the openEHR terminology
/// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html>).
const OPENEHR_TERMINOLOGY: &str = OpenehrTerminologyGroupIdentifiersData::TERMINOLOGY_ID_OPENEHR;

/// Whether a write adds a version container or a version to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The first version of a composition.
    Creation,
    /// A later version of a composition.
    Modification,
    /// The deletion of the latest version.
    Deleted,
}

impl Change {
    /// Returns the openEHR `change_type` code of this change.
    #[must_use]
    pub const fn code(self) -> &'static str {
        self.change_type().code()
    }

    /// Returns the openEHR `change_type` rubric of this change.
    #[must_use]
    pub const fn rubric(self) -> &'static str {
        self.change_type().rubric()
    }

    /// Returns the `audit_change_type` concept of this change.
    const fn change_type(self) -> AuditChangeType {
        match self {
            Self::Creation => AuditChangeType::Creation,
            Self::Modification => AuditChangeType::Modification,
            Self::Deleted => AuditChangeType::Deleted,
        }
    }
}

/// Who a CONTRIBUTION names as `AUDIT_DETAILS.committer`: the party the
/// CDR authenticated the call as, in a form that states nothing more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Committer {
    /// The bridge, named by its system id: the CDR authenticates no
    /// credential of the bridge's, or a Basic user with no
    /// `[cdr] committer_issuer`.
    Bridge,
    /// The client of the bridge's own client credentials grant, named by its
    /// client id, the name a CDR that writes the committer gives a system
    /// client.
    Client {
        /// The client id the grant authenticates as.
        client_id: String,
    },
    /// The placeholder a CDR that writes the committer itself discards: a
    /// `PARTY_IDENTIFIED` whose only member is one `DV_IDENTIFIER` of the
    /// authenticated caller's issuer and subject (an onward token's `iss` and
    /// `sub`, or a Basic user under `[cdr] committer_issuer`).
    Placeholder {
        /// The issuer the CDR names the caller under.
        issuer: String,
        /// The caller's subject at that issuer.
        subject: String,
    },
}

impl Committer {
    /// Returns the committer of a commit sent through `client`: the
    /// placeholder of the onward token it carries, or the party its own
    /// credential authenticates as.
    #[must_use]
    pub fn of(client: &crate::cdr::CdrClient) -> Self {
        if let Some(onward) = client.on_behalf() {
            return Self::Placeholder {
                issuer: onward.issuer.clone(),
                subject: onward.subject.clone(),
            };
        }
        match client.principal() {
            Some(Principal::Client { client_id }) => Self::Client {
                client_id: client_id.clone(),
            },
            Some(Principal::Basic {
                user,
                issuer: Some(issuer),
            }) => Self::Placeholder {
                issuer: issuer.clone(),
                subject: user.clone(),
            },
            Some(Principal::Basic { issuer: None, .. }) | None => Self::Bridge,
        }
    }

    /// Returns the `PARTY_PROXY` this committer is written as.
    fn party(&self, system_id: &str) -> PartyProxy {
        let party = match self {
            Self::Placeholder { issuer, subject } => PartyIdentifiedData {
                external_ref: None,
                name: None,
                identifiers: Some(openehr_base::containers::NonEmptyVec::of(DvIdentifier {
                    issuer: Some(issuer.clone()),
                    assigner: None,
                    id: subject.clone(),
                    r#type: None,
                })),
            },
            Self::Bridge => named(system_id),
            Self::Client { client_id } => named(client_id),
        };
        PartyProxy::PartyIdentified(PartyIdentified::PartyIdentified(party))
    }
}

/// Returns a `PARTY_IDENTIFIED` stating `name` alone.
fn named(name: &str) -> PartyIdentifiedData {
    PartyIdentifiedData {
        external_ref: None,
        name: Some(String::from(name)),
        identifiers: None,
    }
}

/// Returns the system a request admitted with `access` originated in, apart
/// from the bridge: the client of a system caller, by its client claim or its
/// `sub`.
///
/// A machine's write is an act of that machine fed through the bridge, so it
/// becomes `originating_system_audit.system_id` and the bridge the feeder
/// system (RM `FEEDER_AUDIT.feeder_system_audit`, "if different from the
/// originating system"). A person's write has none; no specification governs
/// the choice: our own design.
#[must_use]
pub fn originating_system(access: &Access) -> Option<String> {
    let caller = access.caller()?;
    matches!(caller.kind(), CallerKind::System)
        .then(|| caller.client_id().unwrap_or(caller.subject()).to_owned())
}

/// Why the committal metadata could not be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CommitError {
    /// The template identifier cannot travel in the commit header.
    #[error("{template} cannot travel as a template identifier")]
    Template {
        /// The identifier the composition names.
        template: String,
        /// What the identifier's constructor reported.
        #[source]
        source: Box<IdError>,
    },
}

/// Returns the commit headers one `EHR_STATUS` write carries.
///
/// The write is a modification through `system_id` in the lifecycle a
/// composition write takes, with no template, since an `EHR_STATUS` is built
/// from none.
#[must_use]
pub fn status_context(system_id: &str) -> CommitContext {
    CommitContext {
        lifecycle_state: Some(lifecycle_state(VersionLifecycleState::Complete)),
        audit: Some(header_audit(Change::Modification, system_id)),
        template_id: None,
    }
}

/// Returns the commit headers one composition write carries.
///
/// The audit states the change type and the system [`audit`] puts into a
/// CONTRIBUTION entry, and no committer, which the CDR writes.
///
/// # Errors
///
/// Returns [`CommitError::Template`] when the template identifier cannot
/// travel.
pub fn context(
    change: Change,
    system_id: &str,
    template: &str,
) -> Result<CommitContext, CommitError> {
    Ok(CommitContext {
        lifecycle_state: Some(match change {
            // NOTE: RM Common IM, Logical Deletion: a delete commits a version in the `deleted`
            // lifecycle state (`version_lifecycle_state` 523 of the openEHR terminology).
            Change::Deleted => lifecycle_state(VersionLifecycleState::Deleted),
            Change::Creation | Change::Modification => {
                lifecycle_state(VersionLifecycleState::Complete)
            }
        }),
        audit: Some(header_audit(change, system_id)),
        template_id: Some(
            template_id(template).map_err(|source| CommitError::Template {
                template: String::from(template),
                source: Box::new(source),
            })?,
        ),
    })
}

/// Returns the `UpdateAudit` a contribution entry carries.
///
/// The CONTRIBUTION body states the committer and the change type as schema
/// members, and ITS-REST requires `committer` there, so it names the party
/// the CDR authenticated the call as.
#[must_use]
pub fn audit(change: Change, system_id: &str, committer: &Committer) -> UpdateAudit {
    UpdateAudit::UpdateAudit(audit_data(change, system_id, committer))
}

/// Returns the `openehr-audit-details` a header commit states.
fn header_audit(change: Change, system_id: &str) -> HeaderAudit {
    HeaderAudit {
        system_id: Some(String::from(system_id)),
        ..HeaderAudit::new(coded(change.code(), change.rubric()))
    }
}

/// Returns the `UPDATE_AUDIT` members one write states.
fn audit_data(change: Change, system_id: &str, committer: &Committer) -> UpdateAuditData {
    UpdateAuditData {
        _type: Some(String::from("UPDATE_AUDIT")),
        system_id: Some(String::from(system_id)),
        change_type: coded(change.code(), change.rubric()),
        description: None,
        committer: committer.party(system_id),
        additional_properties: BTreeMap::new(),
    }
}

/// Returns the `lifecycle_state` naming `state`; a contribution entry carries
/// [`VersionLifecycleState::Complete`].
#[must_use]
pub fn lifecycle_state(state: VersionLifecycleState) -> DvCodedText {
    coded(state.code(), state.rubric())
}

/// Returns a `DV_CODED_TEXT` in the openEHR terminology.
fn coded(code: &str, rubric: &str) -> DvCodedText {
    DvCodedText {
        value: String::from(rubric),
        hyperlink: None,
        formatting: None,
        mappings: None,
        language: None,
        encoding: None,
        defining_code: CodePhrase {
            terminology_id: TerminologyId {
                value: String::from(OPENEHR_TERMINOLOGY),
            },
            code_string: String::from(code),
            preferred_term: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{Change, Committer, audit, context, lifecycle_state, status_context};
    use openehr_its::rest::generated::common::UpdateAudit;
    use openehr_rm::v1_2::common::generic::party_identified::PartyIdentified;
    use openehr_rm::v1_2::common::generic::party_proxy::PartyProxy;
    use openehr_term::codes::VersionLifecycleState;

    #[test]
    fn the_change_codes_are_the_openehr_terminology_ones() {
        assert_eq!("249", Change::Creation.code());
        assert_eq!("251", Change::Modification.code());
        assert_eq!("creation", Change::Creation.rubric());
    }

    #[test]
    fn a_commit_context_carries_the_change_type_the_system_and_the_template() {
        let built =
            context(Change::Creation, "FerroBRIDGE", "ferrobridge.diagnose.v1").expect("builds");
        let audit = built.audit.as_ref().expect("the context carries an audit");
        assert_eq!("249", audit.change_type.defining_code.code_string);
        assert_eq!(Some("FerroBRIDGE"), audit.system_id.as_deref());
        assert_eq!(
            Some("ferrobridge.diagnose.v1"),
            built.template_id.as_ref().map(|id| id.value.as_str())
        );
        let status = status_context("FerroBRIDGE");
        let audit = status
            .audit
            .as_ref()
            .expect("a status write carries an audit");
        assert_eq!("251", audit.change_type.defining_code.code_string);
        assert!(status.template_id.is_none());
    }

    #[test]
    fn the_contribution_audit_states_the_same_change_type_as_the_header() {
        let UpdateAudit::UpdateAudit(data) =
            audit(Change::Modification, "FerroBRIDGE", &Committer::Bridge)
        else {
            panic!("the facade writes an UPDATE_AUDIT, never an attestation");
        };
        assert_eq!("251", data.change_type.defining_code.code_string);
        assert_eq!(
            "openehr",
            data.change_type.defining_code.terminology_id.value
        );
        assert_eq!(
            "532",
            lifecycle_state(VersionLifecycleState::Complete)
                .defining_code
                .code_string
        );
    }

    #[test]
    fn a_template_identifier_a_header_cannot_hold_is_refused() {
        assert!(context(Change::Creation, "FerroBRIDGE", "").is_err());
    }

    #[test]
    fn a_contribution_through_an_open_cdr_names_the_bridge() {
        let UpdateAudit::UpdateAudit(data) =
            audit(Change::Creation, "FerroBRIDGE", &Committer::Bridge)
        else {
            panic!("the facade writes an UPDATE_AUDIT");
        };
        let PartyProxy::PartyIdentified(PartyIdentified::PartyIdentified(party)) = data.committer
        else {
            panic!("the bridge is a PARTY_IDENTIFIED");
        };
        assert_eq!(Some("FerroBRIDGE"), party.name.as_deref());
        assert_eq!(None, party.external_ref);
        assert_eq!(None, party.identifiers);
    }

    #[test]
    fn a_contribution_under_the_bridges_own_grant_names_its_client_alone() {
        let client = Committer::Client {
            client_id: String::from("ferrobridge"),
        };
        let UpdateAudit::UpdateAudit(data) = audit(Change::Creation, "FerroBRIDGE", &client) else {
            panic!("the facade writes an UPDATE_AUDIT");
        };
        assert_eq!(Some("FerroBRIDGE"), data.system_id.as_deref());
        let PartyProxy::PartyIdentified(PartyIdentified::PartyIdentified(party)) = data.committer
        else {
            panic!("the client is a PARTY_IDENTIFIED");
        };
        assert_eq!(Some("ferrobridge"), party.name.as_deref());
        assert_eq!(None, party.external_ref, "the client states nothing more");
        assert_eq!(None, party.identifiers);
    }

    fn client(config: crate::cdr::config::CdrConfig) -> crate::cdr::CdrClient {
        crate::cdr::CdrClient::new(&config).expect("a client")
    }

    fn base() -> crate::cdr::config::CdrConfig {
        crate::cdr::config::CdrConfig::new("http://cdr.example.org/v1".parse().expect("a URL"))
    }

    #[test]
    fn the_committer_is_the_party_the_cdr_authenticates() {
        assert_eq!(Committer::Bridge, Committer::of(&client(base())));
        let granted = base().with_oauth(ferrohealth_smart::client::ClientCredentials::new(
            ferrohealth_smart::client::Endpoint::Token(
                "http://as.example.org/token".parse().expect("a token URL"),
            ),
            String::from("ferrobridge"),
            secrecy::SecretString::from("synthetic-secret"),
        ));
        assert_eq!(
            Committer::Client {
                client_id: String::from("ferrobridge"),
            },
            Committer::of(&client(granted))
        );
        let basic = base().with_basic("synthetic-user", "synthetic-password");
        assert_eq!(
            Committer::Bridge,
            Committer::of(&client(basic.clone())),
            "a Basic user with no issuer has no placeholder"
        );
        assert_eq!(
            Committer::Placeholder {
                issuer: String::from("urn:example:cdr"),
                subject: String::from("synthetic-user"),
            },
            Committer::of(&client(
                basic.with_committer_issuer(Some(String::from("urn:example:cdr")))
            ))
        );
    }

    #[test]
    fn the_placeholder_committer_is_one_identifier_of_the_issuer_and_subject() {
        let placeholder = Committer::Placeholder {
            issuer: String::from("https://as.example.org"),
            subject: String::from("synthetic-sub"),
        };
        let UpdateAudit::UpdateAudit(data) = audit(Change::Creation, "FerroBRIDGE", &placeholder)
        else {
            panic!("the facade writes an UPDATE_AUDIT, never an attestation");
        };
        let json = openehr_its::json::to_canonical_value(&data.committer);
        assert_eq!(
            serde_json::json!({
                "_type": "PARTY_IDENTIFIED",
                "identifiers": [{
                    "_type": "DV_IDENTIFIER",
                    "issuer": "https://as.example.org",
                    "id": "synthetic-sub"
                }]
            }),
            json
        );
    }
}
