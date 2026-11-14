// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `Consent` on the facade, over the CDR's `EHR_ACCESS`, and the IHE BPPC and
//! APPC exchange beside it.
//!
//! One access directive is one `Consent` ([`crate::consent::fhir`]). The
//! facade serves create, read and search on the `patient` parameter
//! ([`fhir`]) and the type-level operations `$bppc` and `$appc` ([`ihe`]).
//! A `Consent` is R4 unless the request names FHIR R5 with the `fhirVersion`
//! parameter of its media type, `application/fhir+json; fhirVersion=5.0`
//! (FHIR R4 `versioning.html`, "Version Negotiation"). Its logical id is the
//! 32 hex digits of the EHR's UUID followed by the 32 of the directive's key
//! ([`crate::consent::exchange::key`]), 64 characters, the most a FHIR id
//! holds (R4 `datatypes.html#id`), so a read needs no identity map entry. No
//! specification governs the id or the operations: our own design.

pub mod fhir;
pub mod ihe;

use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use fhir_types::r4::capability_statement::CapabilityStatementRestResource;
use fhir_types::r4::capability_statement::CapabilityStatementRestResourceInteraction;
use fhir_types::r4::capability_statement::CapabilityStatementRestResourceOperation;
use fhir_types::r4::capability_statement::CapabilityStatementRestResourceSearchParam;
use http::HeaderMap;
use http::StatusCode;
use openehr_rm::draft::access_control::access_control::access_directive::AccessDirective;
use openehr_rm::draft::access_control::access_control::access_policy::AccessPolicy;

use crate::cdr::CdrClient;
use crate::cdr::ids::EhrId;
use crate::consent::exchange::Current;
use crate::consent::exchange::Settings;
use crate::consent::fhir::Release;
use crate::consent::fhir::export::ExportContext;
use crate::consent::fhir::model::Ref;
use crate::facade::Facade;
use crate::facade::compartment;
use crate::facade::ehr;
use crate::facade::handlers::write::refuse;
use crate::facade::identity::PersonId;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;
use crate::facade::status::consent::of_exchange_error;

/// The resource type this module serves.
pub const CONSENT: &str = "Consent";

/// The `searchParam` a `Consent` search takes (R4
/// `consent.html#search`).
pub const PATIENT: &str = "patient";

/// The canonical of the standard R4 `patient` parameter of `Consent`.
const PATIENT_DEFINITION: &str = "http://hl7.org/fhir/SearchParameter/clinical-patient";

/// The type-level interactions `Consent` answers.
const INTERACTIONS: [&str; 3] = ["create", "read", "search-type"];

/// The canonical base of the operations this module defines.
const OPERATION_BASE: &str = "https://ferrobridge.eu/fhir/OperationDefinition/";

/// The `rest.resource.documentation` of `Consent`.
const DOCUMENTATION: &str = "One openEHR access directive of the EHR_ACCESS of the patient's EHR is one Consent. A Consent with a construct no access directive holds is refused naming the element. FHIR R5 is read and written under application/fhir+json; fhirVersion=5.0. $bppc and $appc import and export the directives as IHE BPPC acknowledgements and IHE APPC Privacy Consent Documents.";

/// Returns the `rest.resource` entry of `Consent`.
#[must_use]
pub fn capability() -> CapabilityStatementRestResource {
    CapabilityStatementRestResource {
        r#type: CONSENT.into(),
        documentation: Some(DOCUMENTATION.into()),
        interaction: INTERACTIONS
            .iter()
            .map(|code| CapabilityStatementRestResourceInteraction {
                code: (*code).into(),
                ..CapabilityStatementRestResourceInteraction::default()
            })
            .collect(),
        search_param: vec![CapabilityStatementRestResourceSearchParam {
            name: PATIENT.into(),
            definition: Some(PATIENT_DEFINITION.into()),
            r#type: "reference".into(),
            ..CapabilityStatementRestResourceSearchParam::default()
        }],
        operation: [ihe::BPPC, ihe::APPC]
            .into_iter()
            .map(|name| CapabilityStatementRestResourceOperation {
                name: name.into(),
                definition: format!("{OPERATION_BASE}Consent-{name}").into(),
                ..CapabilityStatementRestResourceOperation::default()
            })
            .collect(),
        update_create: Some(false.into()),
        ..CapabilityStatementRestResource::default()
    }
}

/// Returns the exchange settings, or the `404` of a facade that serves no
/// `Consent`.
///
/// # Errors
/// Returns the `404` refusal when `[facade.consent]` is off.
pub fn settings(facade: &Facade) -> Result<&Settings, Refusal> {
    facade.consent().ok_or_else(|| {
        reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotSupported)
                .diagnosing("this facade serves no Consent resource"),
        )
    })
}

/// Returns the FHIR release the media type of `header` names, R4 when it
/// names none.
///
/// # Errors
/// Returns `status` naming the `fhirVersion` when it is neither `4.0` nor
/// `5.0`.
pub fn release_of(
    headers: &HeaderMap,
    header: &http::header::HeaderName,
    status: StatusCode,
) -> Result<Release, Refusal> {
    for value in headers.get_all(header) {
        let Ok(text) = value.to_str() else {
            continue;
        };
        for parameter in text.split([';', ',']).skip(1) {
            let Some((name, version)) = parameter.split_once('=') else {
                continue;
            };
            if !name.trim().eq_ignore_ascii_case("fhirVersion") {
                continue;
            }
            return match version.trim().trim_matches('"') {
                "4.0" | "4.0.1" => Ok(Release::R4),
                "5.0" | "5.0.0" => Ok(Release::R5),
                other => Err(reply::refusal(
                    status,
                    Issue::error(IssueType::NotSupported).diagnosing(format!(
                        "fhirVersion {other} is not a release this facade writes Consent in; it writes 4.0 and 5.0"
                    )),
                )),
            };
        }
    }
    Ok(Release::R4)
}

/// Returns the response `body` of `release` answers with: the R4 media type,
/// or the R5 one with its `fhirVersion`.
#[must_use]
pub fn answer(
    status: StatusCode,
    release: Release,
    body: &fhir_types::codec::Object,
    headers: &[reply::Header],
) -> axum::response::Response {
    let mut response = reply::resource(status, body, headers);
    if release == Release::R5 {
        response.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/fhir+json; fhirVersion=5.0"),
        );
    }
    response
}

/// Returns the exchange settings of a request the media guard of the facade
/// admits.
///
/// # Errors
/// Returns the `404` of a facade that serves no `Consent`, and the refusals
/// of the media types and the page size.
pub(crate) fn admit<'f>(
    facade: &'f Facade,
    headers: &HeaderMap,
    uri: &http::Uri,
    body: crate::facade::handlers::Body,
) -> Result<&'f Settings, Refusal> {
    let settings = settings(facade)?;
    crate::facade::handlers::guard(headers, uri, body)?;
    Ok(settings)
}

/// What the CDR holds for one EHR: its `EHR_ACCESS` and the controller's
/// `ACCESS_POLICY`.
#[derive(Debug, Clone)]
pub struct State {
    /// The current `EHR_ACCESS`.
    pub current: Current,
    /// The policy in force, `None` when none is read.
    pub policy: Option<AccessPolicy>,
}

/// Reads the [`State`] of `ehr`.
///
/// # Errors
/// Returns every exchange failure through `facade::status::consent`.
pub async fn state(client: &CdrClient, ehr: &EhrId, settings: &Settings) -> Result<State, Refusal> {
    let current = crate::consent::exchange::current(client, ehr)
        .await
        .map_err(|error| refuse(&of_exchange_error(&error)))?;
    let policy = crate::consent::exchange::policy(client, settings)
        .await
        .map_err(|error| refuse(&of_exchange_error(&error)))?;
    Ok(State { current, policy })
}

/// The directives one import committed, with their `Consent` ids.
#[derive(Debug, Clone)]
pub struct Committed {
    /// The directives as committed.
    pub added: Vec<AccessDirective>,
    /// Their logical ids, in the same order.
    pub ids: Vec<String>,
}

/// Commits `directives` into the EHR `reached` names, appended to the ones
/// of `current`, under the caller of `access`.
///
/// # Errors
/// Returns every exchange failure through `facade::status::consent`.
pub async fn commit(
    facade: &Facade,
    client: &CdrClient,
    access: &Access,
    reached: &Subject,
    current: &Current,
    directives: Vec<AccessDirective>,
    settings: &Settings,
) -> Result<Committed, Refusal> {
    let system_id = &facade.settings().system_id;
    let committer = crate::facade::commit::Committer::of(access, client, system_id);
    let (_contribution, added) = crate::consent::exchange::append(
        client,
        &reached.ehr,
        current,
        directives,
        settings,
        &committer,
        system_id,
    )
    .await
    .map_err(|error| refuse(&of_exchange_error(&error)))?;
    let ids = added
        .iter()
        .filter_map(|directive| logical_id(&reached.ehr, crate::consent::exchange::key(directive)))
        .collect();
    Ok(Committed { added, ids })
}

/// The EHR a request reaches and the person it was resolved from.
#[derive(Debug, Clone)]
pub struct Subject {
    /// The person.
    pub person: PersonId,
    /// The person's EHR.
    pub ehr: EhrId,
}

/// Resolves the patient `reference` names to its EHR, admitting `permission`
/// on `Consent` for `access`.
///
/// # Errors
/// Returns `422` for a reference that is no patient of the subject link, the
/// `403` of a patient-scoped token for another patient, and the EHR
/// refusals of the ingest service (an EHR the CDR does not hold included,
/// which the exchange never creates).
pub async fn subject(
    facade: &Facade,
    client: &CdrClient,
    access: &Access,
    permission: Permission,
    reference: Option<&str>,
) -> Result<Subject, Refusal> {
    let link = &facade.settings().subject;
    let person = reference
        .ok_or_else(|| {
            reply::refusal(
                StatusCode::UNPROCESSABLE_ENTITY,
                Issue::error(IssueType::Required)
                    .diagnosing("the Consent names no patient whose EHR it belongs to"),
            )
        })
        .and_then(|literal| {
            link.person_of_reference(literal).map_err(|error| {
                reply::refusal(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Issue::error(IssueType::Value)
                        .diagnosing(ferrohealth_server::run::chain(&error)),
                )
            })
        })?;
    compartment::admit_person(facade, access, permission, CONSENT, Some(&person))?;
    let ehr = ehr::resolve(client, facade.store(), &person, link, ehr::Policy::Existing)
        .await
        .map_err(|error| Refusal::from(crate::facade::ingest::refusal::ehr_refusal(&error)))?;
    Ok(Subject { person, ehr })
}

/// Returns the logical id of the directive keyed `key` of `ehr`, `None` for
/// an EHR whose id is no UUID.
#[must_use]
pub fn logical_id(ehr: &EhrId, key: uuid::Uuid) -> Option<String> {
    let ehr = uuid::Uuid::try_parse(ehr.as_str()).ok()?;
    Some(format!("{}{}", ehr.simple(), key.simple()))
}

/// Returns the EHR and the directive key a logical id names, `None` for an
/// id this facade never issued.
#[must_use]
pub fn parse_id(id: &str) -> Option<(EhrId, uuid::Uuid)> {
    if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let (ehr, key) = id.split_at_checked(32)?;
    let ehr = uuid::Uuid::try_parse(ehr).ok()?;
    let key = uuid::Uuid::try_parse(key).ok()?;
    EhrId::new(&ehr.hyphenated().to_string())
        .ok()
        .map(|ehr| (ehr, key))
}

/// Returns what an export of the directives of `ehr` writes beside them:
/// the patient the identity map records for the EHR, the policy's
/// controller, and `stated`.
///
/// # Errors
/// Returns a `500` when the identity map cannot be read.
pub fn export_context(
    facade: &Facade,
    ehr: &EhrId,
    policy: Option<&AccessPolicy>,
    stated: String,
) -> Result<ExportContext, Refusal> {
    let person = facade
        .store()
        .person_of(ehr)
        .map_err(|error| crate::facade::handlers::write::store_refusal(&error))?;
    let reference = person
        .as_ref()
        .and_then(|person| facade.settings().subject.reference_of(person));
    let subject = match reference {
        Some(reference) => Ref {
            reference: Some(reference),
            ..Ref::default()
        },
        None => Ref {
            display: Some(format!("the subject of EHR {ehr}")),
            ..Ref::default()
        },
    };
    Ok(ExportContext {
        subject,
        controller: policy.map(|policy| policy.controller.clone()),
        stated,
    })
}

#[cfg(test)]
mod tests {
    use super::{logical_id, parse_id, release_of};
    use crate::cdr::ids::EhrId;
    use crate::consent::fhir::Release;
    use http::{HeaderMap, HeaderValue, StatusCode, header};

    #[test]
    fn a_logical_id_is_64_hex_digits_and_reads_back() {
        let ehr = EhrId::new("7d44b88c-4199-4bad-97dc-d78268e01398").expect("an id");
        let key = uuid::Uuid::new_v4();
        let id = logical_id(&ehr, key).expect("a UUID EHR id");
        assert_eq!(id.len(), 64);
        assert_eq!(parse_id(&id), Some((ehr, key)));
        assert_eq!(parse_id("Consent-1"), None);
        let other = EhrId::new("not-a-uuid").expect("an id");
        assert_eq!(logical_id(&other, key), None);
    }

    #[test]
    fn the_fhir_version_parameter_picks_the_release_and_refuses_another() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            release_of(&headers, &header::ACCEPT, StatusCode::NOT_ACCEPTABLE).ok(),
            Some(Release::R4)
        );
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/fhir+json; fhirVersion=5.0"),
        );
        assert_eq!(
            release_of(&headers, &header::ACCEPT, StatusCode::NOT_ACCEPTABLE).ok(),
            Some(Release::R5)
        );
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/fhir+json; fhirVersion=3.0"),
        );
        assert!(release_of(&headers, &header::ACCEPT, StatusCode::NOT_ACCEPTABLE).is_err());
    }
}
