// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pseudonym and the EHR a `Patient` written in a party context is given.
//!
//! Every EHR is keyed by the patient's ITI-83 pseudonym (`PIXm` 3.1.0
//! §2:3.83.4.1.2.2), so a `Patient` party is asked for its pseudonym by its
//! identifier in the first accepted domain before it is committed, and the
//! map records the party's `versioned_object_uid` against it, which a
//! `Patient/<uid>` reference is read through. Under
//! [`Policy::CreateOnFirstWrite`] the party is then given its EHR: found with
//! `GET /ehr?subject_id=&subject_namespace=`, or created with `POST /ehr`
//! (ITS-REST 1.1.0 EHR, `ehr_get_by_subject` and `ehr_create`). No
//! specification governs provisioning an EHR from a FHIR `Patient` create:
//! our own design.

use http::StatusCode;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;

use crate::cdr::CdrClient;
use crate::facade::Facade;
use crate::facade::ingest::Refused;
use crate::facade::ingest::refusal::ehr_refusal;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;
use crate::identity::PersonId;
use crate::identity::ehr;
use crate::identity::ehr::Policy;
use ferrohealth_server::run::chain;

/// The party resource type that is the subject of an EHR.
const PATIENT: &str = "Patient";

/// Returns the pseudonym of the `Patient` party `document`, asked of the
/// identity source before the party is committed; `None` for another party
/// type.
///
/// # Errors
///
/// Returns a `422` when the `Patient` carries no identifier in an accepted
/// domain, and the refusal of a failed pseudonym query.
pub(crate) async fn pseudonym(
    facade: &Facade,
    (access, client): (&ferrohealth_smart::resource::Access, &CdrClient),
    resource_type: &str,
    document: &fhir_types::codec::Value,
) -> Result<Option<PersonId>, Refusal> {
    if resource_type != PATIENT {
        return Ok(None);
    }
    let link = &facade.settings().subject;
    let subject = link.patient(document).map_err(|error| {
        reply::refusal(
            StatusCode::UNPROCESSABLE_ENTITY,
            Issue::error(IssueType::Required)
                .diagnosing(chain(&error))
                .at(format!("{PATIENT}.identifier")),
        )
    })?;
    Ok(Some(facade.patients(client, access).person(subject).await?))
}

/// Records `person` as the pseudonym of the `Patient` party committed as
/// `version`, and gives it its EHR when the facade's policy creates EHRs.
///
/// The party is committed before this runs, so a refusal keeps the status
/// the call decided and adds a warning that the `Patient` stands under its
/// id without an EHR.
pub(crate) async fn provision(
    facade: &Facade,
    client: &CdrClient,
    version: &ObjectVersionId,
    person: &PersonId,
) -> Result<(), Refusal> {
    let id = version.versioned_object_id();
    let stored = Issue::warning(IssueType::Informational).diagnosing(format!(
        "the Patient was stored as {PATIENT}/{} and has no EHR",
        id.value()
    ));
    facade
        .store()
        .record_party(id.value(), person)
        .await
        .map_err(|error| {
            Refusal::from(
                Refused::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Issue::error(IssueType::Exception).diagnosing(chain(&error)),
                )
                .with_issue(stored.clone()),
            )
        })?;
    if facade.settings().ehr_policy != Policy::CreateOnFirstWrite {
        return Ok(());
    }
    ehr::resolve(client, facade.store(), person, Policy::CreateOnFirstWrite)
        .await
        .map_err(|error| Refusal::from(ehr_refusal(&error).with_issue(stored)))?;
    Ok(())
}
