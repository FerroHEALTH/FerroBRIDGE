// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART App State on the facade (SMART App Launch 2.2.0, Persisting App
//! State, `app-state.html`, experimental).
//!
//! With `[facade.app_state]` on, `Basic` at the facade's base is the App
//! State endpoint: create, update and delete with `If-Match`, and the search
//! by `code` and `subject` the page names (§App State Interactions). The
//! resources live in a store of their own ([`crate::facade::owned`]), apart
//! from the identity map, because "EHRs SHALL evaluate storage requirements
//! and MAY store App State data separately" (§Managing app state) and the CDR
//! holds no `Basic`. While the endpoint is on, no FHIRconnect program may map
//! `Basic` (no specification governs this: our own design).
//!
//! Access follows §Security and Access controls: the token's `Basic`
//! permission for the interaction, a granular `?code=` scope admitted for the
//! state code it names, a `patient/` grant reaching only state whose subject
//! is the patient in the launch context, and a `user/` or `system/` grant
//! reaching every subject and global state.

pub mod interactions;
pub mod profile;

use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Compartment;
use ferrohealth_smart::scope::Constraint;
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

/// The resource type App State is kept as.
pub const BASIC: &str = "Basic";

/// The profile every App State resource conforms to.
pub const PROFILE: &str =
    "http://hl7.org/fhir/smart-app-launch/StructureDefinition/smart-app-state-basic";

/// The `CapabilityStatement` the endpoint instantiates.
pub const SERVER_CAPABILITY: &str =
    "http://hl7.org/fhir/smart-app-launch/CapabilityStatement/smart-app-state-server";

/// The discovery capability that advertises the endpoint (§Discovery).
pub const CAPABILITY: &str = "smart-app-state";

/// The largest body a create or an update may carry: "Total resource size as
/// serialized in the POST body SHALL NOT exceed 256KB" (§Create).
pub const MAX_BODY: usize = 256 * 1024;

/// The search parameter naming the state code.
pub const CODE: &str = "code";

/// The search parameter naming the subject.
pub const SUBJECT: &str = "subject";

/// The interactions the endpoint answers, as the `CapabilityStatement`
/// `smart-app-state-server` lists them.
const INTERACTIONS: [&str; 4] = ["search-type", "create", "update", "delete"];

/// The `rest.resource.documentation` of `Basic`.
const DOCUMENTATION: &str = "SMART App State (SMART App Launch 2.2.0, Persisting App State). Every Basic here is App State of the smart-app-state-basic profile: create, update and delete with If-Match, and search by code (system|code) and subject (an absolute reference, or subject:missing=true for global state). A body is at most 256 KiB; a subject is a Patient, Practitioner, PractitionerRole, RelatedPerson or Person of this server; extensions carry valueString only.";

/// The state code of one App State resource, `Basic.code.coding[0]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateCode {
    /// The coding's system.
    pub system: String,
    /// The coding's code.
    pub code: String,
}

impl StateCode {
    /// Returns the code as the `code` search parameter and a granular scope
    /// write it, `system|code` (§Querying app state).
    #[must_use]
    pub fn token(&self) -> String {
        format!("{}|{}", self.system, self.code)
    }
}

/// Returns the `rest.resource` entry of `Basic`.
#[must_use]
pub fn capability() -> CapabilityStatementRestResource {
    CapabilityStatementRestResource {
        r#type: BASIC.into(),
        supported_profile: vec![PROFILE.into()],
        documentation: Some(DOCUMENTATION.into()),
        interaction: crate::facade::capability::interactions(INTERACTIONS),
        versioning: Some("versioned-update".into()),
        update_create: Some(false.into()),
        search_param: [
            (
                CODE,
                "http://hl7.org/fhir/SearchParameter/Basic-code",
                "token",
            ),
            (
                SUBJECT,
                "http://hl7.org/fhir/SearchParameter/Basic-subject",
                "reference",
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

/// Returns the App State store, or the `404` of a facade that serves none.
///
/// # Errors
/// Returns the `404` refusal when `[facade.app_state]` is off.
pub fn store(facade: &Facade) -> Result<&OwnedStore, Refusal> {
    facade.app_state().ok_or_else(|| {
        reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotSupported).diagnosing("this facade serves no App State"),
        )
    })
}

/// Checks that `access` lets `permission` reach the App State of `code`
/// about `subject`, `None` for global state.
///
/// A granular scope counts when every parameter it names is `code` with
/// exactly this state's `system|code` (SMART App Launch 2.2.0, Scopes and
/// Launch Context, §Finer-grained resource constraints); a scope naming any
/// other parameter is not evaluated and grants nothing. A `patient/` grant
/// reaches only `[base]/Patient/[patient]` of the launch context: "a token
/// granting `patient/Basic.s?code=https://app|1` with a context of
/// `"patient": "a"` would be authorized to query
/// `Basic?subject=Patient/a&code=https://app|1`" (§Implementation
/// considerations for EHRs), and never global state, which "should be limited
/// to administrative apps" (§Security and Access controls).
///
/// # Errors
/// Returns the guard's `403` when the token does not reach the state.
pub fn admit(
    facade: &Facade,
    access: &Access,
    permission: Permission,
    code: &StateCode,
    subject: Option<&str>,
) -> Result<(), Refusal> {
    match reach(access, permission, code) {
        Some(Compartment::Any) => Ok(()),
        Some(Compartment::Patient(patient)) => {
            let launched = format!(
                "{}/Patient/{patient}",
                facade.settings().base_url.trim_end_matches('/')
            );
            if subject == Some(launched.as_str()) {
                Ok(())
            } else {
                Err(guard::forbidden())
            }
        }
        None => Err(guard::forbidden()),
    }
}

/// Returns how far `permission` on App State of `code` reaches, a granular
/// scope admitted when every parameter it names is `code` with exactly this
/// state's `system|code`.
#[must_use]
pub fn reach(access: &Access, permission: Permission, code: &StateCode) -> Option<Compartment> {
    let token = code.token();
    let evaluate = |constraint: &Constraint| {
        !constraint.params().is_empty()
            && constraint
                .params()
                .iter()
                .all(|(name, value)| name == CODE && *value == token)
    };
    access.reach_with(permission, BASIC, &evaluate)
}
