// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The subject reference a run offers, written into the EHR's `EHR_STATUS`.
//!
//! A `mappingCode` function such as `ehrStatusAddExternalREF` offers the
//! resource's identifiers as the subject of the EHR
//! (`fhirconnect::engine::subject`). `EHR_STATUS.subject.external_ref` "can be
//! used to contain a direct reference to the subject in a demographic or
//! identity service" (openEHR RM EHR IM Release 1.1.0, the `EHR_STATUS`
//! class), so which identifier it holds is the deployment's:
//! `[facade] ehr_status_identifier_system` names its `system`. The reference
//! takes the shape the facade gives the subject of an EHR it creates
//! (`facade::ehr::status_of`), namespace and scheme from `system`. The write
//! is `ehr_status_update` with `If-Match` on the version just read
//! (`ehr-codegen.openapi.yaml`), before the composition is committed, so a
//! refused write commits nothing. No specification governs the selection or
//! the order: our own design, recorded on the tracker.

use std::collections::BTreeSet;

use fhirconnect::engine::subject::SubjectIdentifier;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::generic_id::GenericId;
use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_base::v1_3::base_types::identification::party_ref::PartyRef;
use openehr_its::rest::generated::ehr::client::EhrStatusGetAtTimeOutcome;
use openehr_its::rest::generated::ehr::client::EhrStatusUpdateOutcome;

use crate::cdr::Prefer;
use crate::cdr::ids::EhrId;
use crate::facade::commit;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::status;

use super::Ingest;
use super::Refused;
use super::refusal::cdr_refusal;
use super::refusal::upstream_refusal;

/// The configuration key that names the identifier system.
const KEY: &str = "facade.ehr_status_identifier_system";

/// The operation a read of the `EHR_STATUS` is, for a diagnostic.
const GET: &str = "ehr_status_get_at_time";

/// The operation a write of the `EHR_STATUS` is, for a diagnostic.
const UPDATE: &str = "ehr_status_update";

/// What the subject the run offered asks of the `EHR_STATUS`.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Wanted {
    /// The run offered no identifier the deployment names, so nothing is
    /// written.
    Nothing,
    /// The reference `EHR_STATUS.subject.external_ref` is to hold.
    Reference(PartyRef),
}

/// Returns what the identifiers a run offered ask of the `EHR_STATUS`.
///
/// # Errors
///
/// Returns a `422` when the run offered identifiers and the deployment names
/// no identifier system, and when two distinct identifiers of that system are
/// offered, since `PARTY_SELF.external_ref` is `0..1` (RM Common IM, the
/// `PARTY_SELF` class).
pub(super) fn wanted(
    offered: &[SubjectIdentifier],
    system: Option<&str>,
    party_type: &str,
) -> Result<Wanted, Refused> {
    if offered.is_empty() {
        return Ok(Wanted::Nothing);
    }
    let Some(system) = system else {
        return Err(Refused::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            Issue::error(IssueType::BusinessRule).diagnosing(format!(
                "the mapping offers the subject of the EHR, and `{KEY}` names no identifier \
                 system to write as EHR_STATUS.subject.external_ref"
            )),
        ));
    };
    let values: BTreeSet<&str> = offered
        .iter()
        .filter(|identifier| identifier.system() == Some(system))
        .map(SubjectIdentifier::value)
        .collect();
    let mut values = values.into_iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        if offered
            .iter()
            .any(|identifier| identifier.system() == Some(system))
        {
            return Err(Refused::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                Issue::error(IssueType::BusinessRule).diagnosing(format!(
                    "the resource carries more than one identifier of `{system}`, and \
                     EHR_STATUS.subject.external_ref holds one"
                )),
            ));
        }
        return Ok(Wanted::Nothing);
    };
    Ok(Wanted::Reference(PartyRef {
        namespace: String::from(system),
        r#type: String::from(party_type),
        id: ObjectId::GenericId(GenericId {
            value: String::from(value),
            scheme: String::from(system),
        }),
    }))
}

impl Ingest<'_> {
    /// Writes the subject the run offered into the `EHR_STATUS` of `ehr_id`.
    ///
    /// The current `EHR_STATUS` is read first: one that already holds the
    /// reference takes no write, one that holds none takes it, and one that
    /// holds another subject is refused, because moving a record to another
    /// subject is the correction of "errors being discovered in the
    /// allocation of records to patients" (RM EHR IM §4.2.3), never a side
    /// effect of a data write.
    ///
    /// # Errors
    ///
    /// Returns the refusals of [`wanted`], a `409` when the `EHR_STATUS` names
    /// another subject, and every CDR refusal of the read and the write
    /// through the status table (a `412` carrying the latest version as its
    /// `ETag`).
    pub(super) async fn write_subject(
        &self,
        ehr_id: &EhrId,
        offered: &[SubjectIdentifier],
    ) -> Result<(), Refused> {
        let Wanted::Reference(reference) = wanted(
            offered,
            self.settings.ehr_status_system.as_deref(),
            self.settings.subject.party_type(),
        )?
        else {
            return Ok(());
        };
        let read = self
            .client
            .ehr_status(ehr_id)
            .await
            .map_err(|error| cdr_refusal(&error))?;
        let (mut current, version) = match read.outcome {
            EhrStatusGetAtTimeOutcome::Ok { body, headers } => {
                let version =
                    crate::cdr::version_from_etag(GET, StatusCode::OK, headers.etag.as_deref())
                        .map_err(|error| cdr_refusal(&error))?;
                (body, version)
            }
            EhrStatusGetAtTimeOutcome::NotFound { body } => {
                return Err(upstream_refusal(
                    status::NOT_FOUND,
                    StatusCode::NOT_FOUND,
                    &body,
                ));
            }
            EhrStatusGetAtTimeOutcome::BadRequest { body } => {
                return Err(upstream_refusal(
                    status::BAD_REQUEST,
                    StatusCode::BAD_REQUEST,
                    &body,
                ));
            }
        };
        match current.subject.external_ref {
            Some(ref held) if *held == reference => return Ok(()),
            Some(_) => {
                return Err(Refused::new(
                    StatusCode::CONFLICT,
                    Issue::error(IssueType::Conflict).diagnosing(String::from(
                        "the EHR_STATUS of the EHR names another subject, and the facade does \
                         not move a record to another subject on a data write",
                    )),
                ));
            }
            None => {}
        }
        current.subject.external_ref = Some(reference);
        let context = commit::status_context(&self.settings.system_id);
        let answered = self
            .client
            .update_ehr_status(ehr_id, &version, &current, &context, Prefer::Minimal)
            .await
            .map_err(|error| cdr_refusal(&error))?;
        match answered.outcome {
            EhrStatusUpdateOutcome::Ok { .. } | EhrStatusUpdateOutcome::NoContent { .. } => Ok(()),
            EhrStatusUpdateOutcome::PreconditionFailed { body, headers } => {
                let latest =
                    crate::cdr::optional_version_from_etag(UPDATE, headers.etag.as_deref())
                        .map_err(|error| cdr_refusal(&error))?;
                let mut answer = status::Answer::new(
                    status::PRECONDITION_FAILED,
                    status::diagnostics(StatusCode::PRECONDITION_FAILED, &body),
                );
                if let Some(latest) = latest {
                    answer = answer.with_entity_tag(String::from(latest.version_tree_id().value()));
                }
                Err(Refused::of_answer(&answer))
            }
            EhrStatusUpdateOutcome::NotFound { body } => Err(upstream_refusal(
                status::NOT_FOUND,
                StatusCode::NOT_FOUND,
                &body,
            )),
            EhrStatusUpdateOutcome::BadRequest { body } => Err(upstream_refusal(
                status::BAD_REQUEST,
                StatusCode::BAD_REQUEST,
                &body,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use fhirconnect::engine::subject::SubjectIdentifier;
    use http::StatusCode;

    use super::Wanted;
    use super::wanted;

    /// The system the cases configure.
    const SYSTEM: &str = "urn:oid:2.999.1";

    /// Returns an identifier of `system`.
    fn of(system: Option<&str>, value: &str) -> SubjectIdentifier {
        SubjectIdentifier::new(system.map(String::from), value)
    }

    #[test]
    fn no_offered_identifier_asks_nothing_even_unconfigured() {
        assert!(matches!(wanted(&[], None, "PERSON"), Ok(Wanted::Nothing)));
    }

    #[test]
    fn the_same_value_offered_twice_is_one_reference() {
        let offered = [of(Some(SYSTEM), "a"), of(Some(SYSTEM), "a"), of(None, "b")];
        match wanted(&offered, Some(SYSTEM), "PERSON") {
            Ok(Wanted::Reference(reference)) => {
                assert_eq!(reference.namespace, SYSTEM);
                assert_eq!(reference.r#type, "PERSON");
            }
            other => panic!("expected one reference, got {other:?}"),
        }
    }

    #[test]
    fn two_values_of_the_system_are_refused() {
        let offered = [of(Some(SYSTEM), "a"), of(Some(SYSTEM), "b")];
        let refused = wanted(&offered, Some(SYSTEM), "PERSON");
        assert!(
            matches!(refused, Err(ref refusal) if refusal.status() == StatusCode::UNPROCESSABLE_ENTITY),
            "{refused:?}"
        );
    }
}
