// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART launch `Task`s on the facade (SMART App Launch 2.2.0, Task profile
//! for requesting a SMART launch, `task-launch.html`, experimental).
//!
//! With `[facade.launch_task]` on, `Task` at the facade's base serves the
//! two launch profiles, `task-ehr-launch` and `task-standalone-launch`:
//! create, read, and search by `patient`, `status` and `code`, so an EHR
//! viewer can list the launches asked of it and act on one through the
//! authorization server's launch API. A `Task` that conforms to neither
//! profile is refused. The CDR holds no `Task`, so the facade keeps them in a
//! store of its own ([`crate::facade::owned`]); while the endpoint is on no
//! FHIRconnect program may map `Task` (no specification governs the store or
//! the refusal: our own design).

pub mod interactions;
pub mod profile;

use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Compartment;
use ferrohealth_smart::scope::Permission;
use fhir_types::r4::capability_statement::CapabilityStatementRestResource;
use fhir_types::r4::capability_statement::CapabilityStatementRestResourceSearchParam;
use http::StatusCode;

use crate::facade::Facade;
use crate::facade::guard;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::owned::OwnedStore;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// The resource type a launch request is.
pub const TASK: &str = "Task";

/// The profile of a `Task` that requests an EHR launch.
pub const EHR_LAUNCH: &str =
    "http://hl7.org/fhir/smart-app-launch/StructureDefinition/task-ehr-launch";

/// The profile of a `Task` that requests a standalone launch.
pub const STANDALONE_LAUNCH: &str =
    "http://hl7.org/fhir/smart-app-launch/StructureDefinition/task-standalone-launch";

/// The search parameter naming the patient, `Task.for`.
pub const PATIENT: &str = "patient";

/// The search parameter naming the status.
pub const STATUS: &str = "status";

/// The search parameter naming the code.
pub const CODE: &str = "code";

/// The interactions the endpoint answers.
const INTERACTIONS: [&str; 3] = ["create", "read", "search-type"];

/// The `rest.resource.documentation` of `Task`.
const DOCUMENTATION: &str = "SMART launch requests (SMART App Launch 2.2.0, Task profile for requesting a SMART launch). Every Task here conforms to task-ehr-launch or task-standalone-launch: a code of the smart-launch-types value set, exactly one smartonfhir-application input with valueUrl and at most one smartonfhir-appcontext input with valueString. Create, read and search by patient, status and code.";

/// Returns the `rest.resource` entry of `Task`.
#[must_use]
pub fn capability() -> CapabilityStatementRestResource {
    CapabilityStatementRestResource {
        r#type: TASK.into(),
        supported_profile: vec![EHR_LAUNCH.into(), STANDALONE_LAUNCH.into()],
        documentation: Some(DOCUMENTATION.into()),
        interaction: crate::facade::capability::interactions(INTERACTIONS),
        versioning: Some("versioned".into()),
        update_create: Some(false.into()),
        search_param: [
            (
                PATIENT,
                "http://hl7.org/fhir/SearchParameter/Task-patient",
                "reference",
            ),
            (
                STATUS,
                "http://hl7.org/fhir/SearchParameter/Task-status",
                "token",
            ),
            (
                CODE,
                "http://hl7.org/fhir/SearchParameter/Task-code",
                "token",
            ),
        ]
        .into_iter()
        .map(
            |(name, definition, kind)| CapabilityStatementRestResourceSearchParam {
                name: name.into(),
                definition: Some(definition.into()),
                r#type: kind.into(),
                ..CapabilityStatementRestResourceSearchParam::default()
            },
        )
        .collect(),
        ..CapabilityStatementRestResource::default()
    }
}

/// Returns the launch `Task` store, or the `404` of a facade that serves
/// none.
///
/// # Errors
/// Returns the `404` refusal when `[facade.launch_task]` is off.
pub fn store(facade: &Facade) -> Result<&OwnedStore, Refusal> {
    facade.launch_tasks().ok_or_else(|| {
        reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotSupported).diagnosing("this facade serves no launch Task"),
        )
    })
}

/// Returns `reference` as this server names a patient, `Patient/[id]`, when
/// it is relative or under the facade's base, and unchanged otherwise.
#[must_use]
pub fn local(facade: &Facade, reference: &str) -> String {
    let prefix = format!("{}/", facade.settings().base_url.trim_end_matches('/'));
    reference
        .strip_prefix(&prefix)
        .unwrap_or(reference)
        .to_owned()
}

/// Checks that `access` lets `permission` reach a launch `Task` for the
/// patient `patient` names, `None` for a `Task` naming none.
///
/// A `patient/` grant reaches only a `Task` whose `for` is the patient of
/// its launch context, `Patient/[id]` relative or under the facade's base
/// (SMART App Launch 2.2.0, Scopes and Launch Context, §Launch context
/// arrives with your `access_token`).
///
/// # Errors
/// Returns the guard's `403` when the token does not reach the `Task`.
pub fn admit(
    facade: &Facade,
    access: &Access,
    permission: Permission,
    patient: Option<&str>,
) -> Result<(), Refusal> {
    match access.reach(permission, TASK) {
        Some(Compartment::Any) => Ok(()),
        Some(Compartment::Patient(launched)) => {
            let own = format!("Patient/{launched}");
            if patient.map(|reference| local(facade, reference)) == Some(own) {
                Ok(())
            } else {
                Err(guard::forbidden())
            }
        }
        None => Err(guard::forbidden()),
    }
}
