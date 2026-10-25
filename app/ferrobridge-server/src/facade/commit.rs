// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The committal metadata every write this facade makes carries.
//!
//! ITS-REST 1.1.0 leaves the committal metadata headers optional ("None of
//! these headers are mandatory"), and the CONTRIBUTION body carries the same
//! facts as schema members instead (`ehr-codegen.openapi.yaml`,
//! `contribution_create`). This module builds both from one description, so a
//! single write and a transaction record the same committer and change type.
//!
//! The committer is the caller the request authenticated, provisioned "based
//! on authentication and authorization runtime data" as ITS-REST 1.1.0
//! §openehr-version and openehr-audit-details recommends, and the bridge
//! itself only for a write no caller made; `system_id` always names the
//! bridge, so the version records both who asked and what committed.

use std::collections::BTreeMap;

use crate::cdr::commit::CommitContext;
use crate::cdr::ids::IdError;
use crate::cdr::ids::template_id;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Via;
use openehr_base::v1_3::base_types::identification::generic_id::GenericId;
use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_base::v1_3::base_types::identification::party_ref::PartyRef;
use openehr_base::v1_3::base_types::identification::terminology_id::TerminologyId;
use openehr_its::rest::generated::common::UpdateAudit;
use openehr_its::rest::generated::common::UpdateAuditData;
use openehr_rm::v1_2::common::generic::party_identified::PartyIdentified;
use openehr_rm::v1_2::common::generic::party_identified::PartyIdentifiedData;
use openehr_rm::v1_2::common::generic::party_proxy::PartyProxy;
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
use openehr_rm::v1_2::support::terminology::openehr_terminology_group_identifiers::OpenehrTerminologyGroupIdentifiersData;

/// The terminology the openEHR audit code sets belong to.
///
/// `AUDIT_DETAILS.change_type` and `VERSION.lifecycle_state` are coded from
/// the openEHR terminology
/// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html>).
const OPENEHR_TERMINOLOGY: &str = OpenehrTerminologyGroupIdentifiersData::TERMINOLOGY_ID_OPENEHR;

// TODO(#618): take the three codes and their rubrics from named openehr-rm
// constants once the crate carries them (sibling request S3).
/// The `change_type` code of a first commit, "creation".
const CREATION_CODE: &str = "249";

/// The `change_type` code of a later commit, "modification".
const MODIFICATION_CODE: &str = "251";

/// The `change_type` code of a delete, "deleted".
const DELETED_CODE: &str = "523";

/// The `lifecycle_state` code of a committed version, "complete".
const COMPLETE_CODE: &str = "532";

/// The `lifecycle_state` code of a deleted version, "deleted".
const DELETED_LIFECYCLE_CODE: &str = "523";

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
        match self {
            Self::Creation => CREATION_CODE,
            Self::Modification => MODIFICATION_CODE,
            Self::Deleted => DELETED_CODE,
        }
    }

    /// Returns the openEHR `change_type` rubric of this change.
    #[must_use]
    pub const fn rubric(self) -> &'static str {
        match self {
            Self::Creation => "creation",
            Self::Modification => "modification",
            Self::Deleted => "deleted",
        }
    }
}

/// The `PARTY_REF.type` of a caller: the abstract `PARTY`, since a token
/// does not say whether its subject is a person or a client application
/// (openEHR BASE, `PARTY_REF`: "Abstract supertypes are allowed").
const CALLER_PARTY_TYPE: &str = "PARTY";

/// The `GENERIC_ID.scheme` of a caller's identifier: the `sub` claim of
/// RFC 7519 §4.1.2, or a Basic user-id (no specification governs the scheme
/// name: our own design).
const CALLER_ID_SCHEME: &str = "sub";

/// Who a commit is attributed to as `AUDIT_DETAILS.committer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Committer {
    /// The bridge, named by its system id: a write no authenticated caller
    /// made, such as an HL7 v2 message or a request on an open surface.
    Bridge,
    /// The caller the request authenticated.
    Caller {
        /// The token's `sub`, or the Basic user-id.
        subject: String,
        /// The namespace the subject belongs to: the issuer that vouched for
        /// it, or the bridge's system id for a Basic user the bridge checked.
        namespace: String,
    },
}

impl Committer {
    /// Returns the committer of a request admitted with `access`, the bridge
    /// for an open surface.
    #[must_use]
    pub fn of(access: &Access, system_id: &str) -> Self {
        let Some(caller) = access.caller() else {
            return Self::Bridge;
        };
        let namespace = match (caller.via(), caller.issuer()) {
            (Via::Bearer, Some(issuer)) => issuer,
            _ => system_id,
        };
        Self::Caller {
            subject: caller.subject().to_owned(),
            namespace: namespace.to_owned(),
        }
    }

    /// Returns the `PARTY_PROXY` this committer is written as.
    ///
    /// A caller is a `PARTY_IDENTIFIED` named by its subject, with a
    /// `PARTY_REF` into the namespace of its issuer when that namespace is a
    /// legal one (openEHR BASE, `OBJECT_REF.namespace`: the regex
    /// `[a-zA-Z][a-zA-Z0-9_.:/&?=+-]*`); the name alone carries it otherwise.
    fn party(&self, system_id: &str) -> PartyProxy {
        let (name, external_ref) = match self {
            Self::Bridge => (system_id, None),
            Self::Caller { subject, namespace } => (
                subject.as_str(),
                legal_namespace(namespace).then(|| PartyRef {
                    namespace: namespace.clone(),
                    r#type: String::from(CALLER_PARTY_TYPE),
                    id: ObjectId::GenericId(GenericId {
                        value: subject.clone(),
                        scheme: String::from(CALLER_ID_SCHEME),
                    }),
                }),
            ),
        };
        PartyProxy::PartyIdentified(PartyIdentified::PartyIdentified(PartyIdentifiedData {
            external_ref,
            name: Some(String::from(name)),
            identifiers: None,
        }))
    }
}

/// Returns whether `namespace` matches the `OBJECT_REF.namespace` regex
/// `[a-zA-Z][a-zA-Z0-9_.:/&?=+-]*` of openEHR BASE.
fn legal_namespace(namespace: &str) -> bool {
    let mut characters = namespace.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && characters
            .all(|character| character.is_ascii_alphanumeric() || "_.:/&?=+-".contains(character))
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
/// The write is a modification by `committer` through `system_id` in the
/// lifecycle a composition write takes, with no template, since an
/// `EHR_STATUS` is built from none.
#[must_use]
pub fn status_context(system_id: &str, committer: &Committer) -> CommitContext {
    CommitContext {
        lifecycle_state: Some(lifecycle()),
        audit: Some(audit_data(Change::Modification, system_id, committer)),
        template_id: None,
    }
}

/// Returns the commit headers one composition write carries.
///
/// The audit is the one [`audit`] puts into a CONTRIBUTION entry, so a single
/// write and a transaction state the same committer and change type.
///
/// # Errors
///
/// Returns [`CommitError::Template`] when the template identifier cannot
/// travel.
pub fn context(
    change: Change,
    system_id: &str,
    committer: &Committer,
    template: &str,
) -> Result<CommitContext, CommitError> {
    Ok(CommitContext {
        lifecycle_state: Some(match change {
            // NOTE: RM Common IM, Logical Deletion: a delete commits a version in the `deleted`
            // lifecycle state (`version_lifecycle_state` 523 of the openEHR terminology).
            Change::Deleted => coded(DELETED_LIFECYCLE_CODE, "deleted"),
            Change::Creation | Change::Modification => lifecycle(),
        }),
        audit: Some(audit_data(change, system_id, committer)),
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
/// members rather than as headers, so a transaction records exactly what a
/// single write does.
#[must_use]
pub fn audit(change: Change, system_id: &str, committer: &Committer) -> UpdateAudit {
    UpdateAudit::UpdateAudit(audit_data(change, system_id, committer))
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

/// Returns the `lifecycle_state` a contribution entry carries.
#[must_use]
pub fn lifecycle() -> DvCodedText {
    coded(COMPLETE_CODE, "complete")
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
    use super::{Change, Committer, audit, context, legal_namespace, lifecycle};
    use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
    use openehr_its::rest::generated::common::UpdateAudit;
    use openehr_rm::v1_2::common::generic::party_identified::PartyIdentified;
    use openehr_rm::v1_2::common::generic::party_proxy::PartyProxy;

    #[test]
    fn the_change_codes_are_the_openehr_terminology_ones() {
        assert_eq!("249", Change::Creation.code());
        assert_eq!("251", Change::Modification.code());
        assert_eq!("creation", Change::Creation.rubric());
    }

    #[test]
    fn a_commit_context_carries_the_change_type_the_system_and_the_template() {
        let built = context(
            Change::Creation,
            "FerroBRIDGE",
            &Committer::Bridge,
            "ferrobridge.diagnose.v1",
        )
        .expect("the context builds");
        let audit = built.audit.as_ref().expect("the context carries an audit");
        assert_eq!("249", audit.change_type.defining_code.code_string);
        assert_eq!(Some("FerroBRIDGE"), audit.system_id.as_deref());
        assert_eq!(
            Some("ferrobridge.diagnose.v1"),
            built.template_id.as_ref().map(|id| id.value.as_str())
        );
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
        assert_eq!("532", lifecycle().defining_code.code_string);
    }

    #[test]
    fn a_template_identifier_a_header_cannot_hold_is_refused() {
        assert!(context(Change::Creation, "FerroBRIDGE", &Committer::Bridge, "").is_err());
    }

    #[test]
    fn a_caller_commits_under_its_subject_in_its_issuers_namespace() {
        let caller = Committer::Caller {
            subject: String::from("synthetic-sub"),
            namespace: String::from("https://as.example.org/realms/ferro"),
        };
        let UpdateAudit::UpdateAudit(data) = audit(Change::Creation, "FerroBRIDGE", &caller) else {
            panic!("the facade writes an UPDATE_AUDIT");
        };
        assert_eq!(
            Some("FerroBRIDGE"),
            data.system_id.as_deref(),
            "the bridge commits"
        );
        let PartyProxy::PartyIdentified(PartyIdentified::PartyIdentified(party)) = data.committer
        else {
            panic!("a caller is a PARTY_IDENTIFIED");
        };
        assert_eq!(Some("synthetic-sub"), party.name.as_deref());
        let reference = party
            .external_ref
            .expect("a legal namespace carries a PARTY_REF");
        assert_eq!("https://as.example.org/realms/ferro", reference.namespace);
        assert_eq!("PARTY", reference.r#type);
        assert!(matches!(reference.id, ObjectId::GenericId(ref id) if id.value == "synthetic-sub"));
    }

    #[test]
    fn a_namespace_outside_the_openehr_regex_leaves_the_name_alone() {
        assert!(legal_namespace("https://as.example.org/realms/ferro"));
        assert!(legal_namespace("FerroBRIDGE"));
        for illegal in ["", "1as", "https://as.example.org/~user", "a b", "x%20"] {
            assert!(!legal_namespace(illegal), "{illegal}");
        }
        let caller = Committer::Caller {
            subject: String::from("synthetic-sub"),
            namespace: String::from("https://as.example.org/~tenant"),
        };
        let UpdateAudit::UpdateAudit(data) = audit(Change::Creation, "FerroBRIDGE", &caller) else {
            panic!("the facade writes an UPDATE_AUDIT");
        };
        let PartyProxy::PartyIdentified(PartyIdentified::PartyIdentified(party)) = data.committer
        else {
            panic!("a caller is a PARTY_IDENTIFIED");
        };
        assert_eq!(Some("synthetic-sub"), party.name.as_deref());
        assert_eq!(None, party.external_ref);
    }

    #[test]
    fn a_write_no_caller_made_is_the_bridges() {
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
    }
}
