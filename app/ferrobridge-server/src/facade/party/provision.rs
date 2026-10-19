// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The EHR a `Patient` created in a party context is given.
//!
//! A clinical resource names its patient as `Patient/<id>` and finds its EHR
//! through the configured subject link, so a `Patient` whose id is a new
//! party's `versioned_object_uid` is given the EHR that link keys: found with
//! `GET /ehr?subject_id=&subject_namespace=`, or created with `POST /ehr`
//! (ITS-REST 1.1.0 EHR, `ehr_get_by_subject` and `ehr_create`). This runs
//! under [`Policy::CreateOnFirstWrite`] only; under [`Policy::Existing`] the
//! CDR's owner provisions EHRs and the create sends no EHR call. No
//! specification governs provisioning an EHR from a FHIR `Patient` create:
//! our own design.

use http::StatusCode;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;

use crate::cdr::CdrClient;
use crate::cdr::ids::versioned_object_uid;
use crate::facade::Facade;
use crate::facade::ehr;
use crate::facade::ehr::Policy;
use crate::facade::handlers::Refusal;
use crate::facade::ingest::Refused;
use crate::facade::ingest::refusal::ehr_refusal;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::outcome::chain;

/// The party resource type that is the subject of an EHR.
const PATIENT: &str = "Patient";

/// Gives the `Patient` committed as `version` its EHR, when the facade's
/// policy creates EHRs.
///
/// The party is committed before this runs, so a refusal keeps the status
/// the EHR call decided and adds a warning that the `Patient` stands under
/// its id without an EHR.
pub(crate) async fn provision(
    facade: &Facade,
    client: &CdrClient,
    resource_type: &str,
    version: &ObjectVersionId,
) -> Result<(), Refusal> {
    if resource_type != PATIENT || facade.settings().ehr_policy != Policy::CreateOnFirstWrite {
        return Ok(());
    }
    let id = versioned_object_uid(version);
    let stored = Issue::warning(IssueType::Informational).diagnosing(format!(
        "the Patient was stored as {PATIENT}/{} and has no EHR",
        id.value()
    ));
    let link = &facade.settings().subject;
    let person = link.person_of_patient(id.value()).map_err(|error| {
        Refusal::from(
            Refused::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                Issue::error(IssueType::Processing).diagnosing(chain(&error)),
            )
            .with_issue(stored.clone()),
        )
    })?;
    ehr::resolve(
        client,
        facade.store(),
        &person,
        link,
        Policy::CreateOnFirstWrite,
    )
    .await
    .map_err(|error| Refusal::from(ehr_refusal(&error).with_issue(stored)))?;
    Ok(())
}
