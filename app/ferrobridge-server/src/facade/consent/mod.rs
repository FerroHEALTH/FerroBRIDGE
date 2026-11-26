// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `Consent` on the facade, over the CDR's `EHR_ACCESS`, and the IHE BPPC and
//! APPC exchange beside it.
//!
//! One access directive is one `Consent` ([`crate::consent::fhir`]). The
//! facade serves create, read and search on the `patient` and `scope` parameters
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
use crate::facade::search::declaration::FixedToken;
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

/// The `Consent.scope` code of an advance directive (R4
/// `codesystem-consentscope`), which the access directives never carry.
const ADVANCE_DIRECTIVE: &str = "adr";

/// The search parameter that names a `Consent`'s scope (R4
/// `consent.html#search`).
pub const SCOPE: &str = "scope";

/// The canonical of the standard R4 `scope` parameter of `Consent`.
const SCOPE_DEFINITION: &str = "http://hl7.org/fhir/SearchParameter/Consent-scope";

/// The `Consent.scope` code every access directive is exported with (R4
/// `codesystem-consentscope`).
pub const PATIENT_PRIVACY: &str = "patient-privacy";

/// Whether a `Consent` create of `body` writes an access directive: the
/// facade serves them, and the body is no advance directive, which a
/// loaded mapping writes as a record.
///
/// An access directive is one kind of `Consent`, and its scope tells it from
/// an advance directive (R4 `Consent.scope`). No specification governs the
/// split: our own design.
#[must_use]
pub fn directs_access(facade: &Facade, body: &[u8]) -> bool {
    let advance = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|resource| resource.get("scope").cloned())
        .and_then(|scope| scope.get("coding").cloned())
        .and_then(|coding| coding.as_array().cloned())
        .is_some_and(|coding| {
            coding.iter().any(|code| {
                code.get("code").and_then(serde_json::Value::as_str) == Some(ADVANCE_DIRECTIVE)
            })
        });
    facade.consent().is_some() && !(advance && maps_consent(facade))
}

/// The lanes a `Consent` search reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lanes {
    /// The access directives of the patient's `EHR_ACCESS` alone.
    Access,
    /// The advance directives a loaded mapping serves alone.
    Mapped,
    /// Both, the access directives first.
    Both,
}

/// Returns the lanes a `Consent` search of `pairs` reaches.
///
/// A searchset holds every resource that matches (R4 `search.html`
/// §3.1.1), so a search without `scope` reaches both lanes. Each `scope`
/// occurrence is `AND`ed and its values `OR`ed (§3.1.1.4.1): the access
/// directives are reached when every occurrence names `patient-privacy`, the
/// advance directives when every one names `adr`, the scope codes the two
/// lanes write (R4 `valueset-consent-scope`). A search no lane matches goes
/// to the mapped lane, which answers it with an empty searchset or refuses
/// a malformed value. No specification governs the lanes: our own design.
#[must_use]
pub fn lanes(facade: &Facade, pairs: &[(String, String)]) -> Lanes {
    lanes_of(facade.consent().is_some(), maps_consent(facade), pairs)
}

/// Returns the lanes a `Consent` search of `pairs` reaches, when the facade
/// serves the access directives (`access`) and a mapping serves `Consent`
/// (`mapped`).
fn lanes_of(access: bool, mapped: bool, pairs: &[(String, String)]) -> Lanes {
    if !access {
        return Lanes::Mapped;
    }
    if !mapped {
        return Lanes::Access;
    }
    let scopes: Vec<&(String, String)> = pairs
        .iter()
        .filter(|(name, value)| !value.is_empty() && scope_name(name))
        .collect();
    if scopes.is_empty() || scopes.iter().any(|(name, _)| name != SCOPE) {
        return Lanes::Both;
    }
    let names = |code: &str| {
        scopes
            .iter()
            .all(|(_, value)| scope_values(value).is_some_and(|tokens| names_scope(&tokens, code)))
    };
    match (names(PATIENT_PRIVACY), names(ADVANCE_DIRECTIVE)) {
        (true, true) => Lanes::Both,
        (true, false) => Lanes::Access,
        (false, _) => Lanes::Mapped,
    }
}

/// Whether the search parameter `name` is `scope`, with or without a
/// modifier.
fn scope_name(name: &str) -> bool {
    name.split_once(':').map_or(name, |(base, _)| base) == SCOPE
}

/// Reads the comma-separated token values of one `scope` occurrence, `None`
/// for a value that is no token (R4 `search.html` §3.1.1.4.10).
#[must_use]
pub fn scope_values(value: &str) -> Option<Vec<fhir_search::token::Token>> {
    fhir_search::escape::split(value, ',')
        .iter()
        .map(|text| fhir_search::token::Token::parse(text).ok())
        .collect()
}

/// Whether one of `tokens` matches the `consentscope` code `code`.
#[must_use]
pub fn names_scope(tokens: &[fhir_search::token::Token], code: &str) -> bool {
    let coding = FixedToken {
        system: String::from(crate::consent::fhir::CONSENT_SCOPE),
        code: String::from(code),
    };
    tokens.iter().any(|token| coding.matches(token))
}

/// Whether a `Consent` read of `id` reads an access directive: the facade
/// serves them and `id` is the logical id of one ([`parse_id`]), or no
/// loaded mapping serves `Consent`.
///
/// The two id schemes are disjoint by construction: an access directive's
/// id is 64 hex digits ([`logical_id`]), and every mapped `Consent` id is
/// the 52 base32 characters of a FHIRconnect deterministic id
/// ([`crate::facade::identity::derive`]), since the facade declares
/// `updateCreate: false` and assigns every id itself. No specification
/// governs the ids: our own design.
#[must_use]
pub fn reads_access(facade: &Facade, id: &str) -> bool {
    facade.consent().is_some() && (parse_id(id).is_some() || !maps_consent(facade))
}

/// Whether a loaded mapping serves `Consent`.
fn maps_consent(facade: &Facade) -> bool {
    !facade.mappings().search().of_type(CONSENT).is_empty()
}

/// Adds the `Consent` the access directives serve to `resources`, in type
/// order, joined with the entry of the advance directives a loaded mapping
/// serves.
pub fn declare(resources: &mut Vec<CapabilityStatementRestResource>) {
    let served = capability();
    if let Some(mapped) = resources
        .iter_mut()
        .find(|entry| entry.r#type.value.as_deref() == Some(CONSENT))
    {
        for interaction in served.interaction {
            if !mapped
                .interaction
                .iter()
                .any(|held| held.code == interaction.code)
            {
                mapped.interaction.push(interaction);
            }
        }
        mapped.operation.extend(served.operation);
        mapped.documentation = Some(
            format!(
                "{DOCUMENTATION} A Consent of scope adr is an advance directive, served from the mapped records and searched with scope."
            )
            .into(),
        );
        return;
    }
    let at = resources.partition_point(|entry| entry.r#type.value.as_deref() < Some(CONSENT));
    resources.insert(at, served);
}

/// Returns the `rest.resource` entry of `Consent`.
#[must_use]
pub fn capability() -> CapabilityStatementRestResource {
    CapabilityStatementRestResource {
        r#type: CONSENT.into(),
        documentation: Some(DOCUMENTATION.into()),
        interaction: crate::facade::capability::interactions(INTERACTIONS),
        search_param: vec![
            CapabilityStatementRestResourceSearchParam {
                name: PATIENT.into(),
                definition: Some(PATIENT_DEFINITION.into()),
                r#type: "reference".into(),
                ..CapabilityStatementRestResourceSearchParam::default()
            },
            CapabilityStatementRestResourceSearchParam {
                name: SCOPE.into(),
                definition: Some(SCOPE_DEFINITION.into()),
                r#type: "token".into(),
                ..CapabilityStatementRestResourceSearchParam::default()
            },
        ],
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

/// Reads the [`State`] of `ehr`: the `EHR_ACCESS` through `client`, the one
/// a request calls the CDR through under its caller's exchanged token, and
/// the policy through the facade's own client.
///
/// The catalogue is the controller's policy, which every caller of the
/// exchange reads alike, and the CDR serves it to its administrators only;
/// a patient's directives are never read under the bridge's credential. No
/// specification governs this: our own design.
///
/// # Errors
/// Returns every exchange failure through `facade::status::consent`: a CDR
/// `404` or `403` for the caller is that answer, never an empty result.
pub async fn state(
    facade: &Facade,
    client: &CdrClient,
    ehr: &EhrId,
    settings: &Settings,
) -> Result<State, Refusal> {
    let current = crate::consent::exchange::current(client, settings, ehr)
        .await
        .map_err(|error| refuse(&of_exchange_error(&error)))?;
    let policy = crate::consent::exchange::policy(facade.client(), settings)
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
/// of `current`, through `client`.
///
/// # Errors
/// Returns every exchange failure through `facade::status::consent`.
pub async fn commit(
    facade: &Facade,
    client: &CdrClient,
    reached: &Subject,
    current: &Current,
    directives: Vec<AccessDirective>,
    settings: &Settings,
) -> Result<Committed, Refusal> {
    let system_id = &facade.settings().system_id;
    let committer = crate::facade::commit::Committer::of(client);
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
    let subject = reference
        .ok_or_else(|| {
            reply::refusal(
                StatusCode::UNPROCESSABLE_ENTITY,
                Issue::error(IssueType::Required)
                    .diagnosing("the Consent names no patient whose EHR it belongs to"),
            )
        })
        .and_then(|literal| {
            crate::facade::subject::patient_of_reference(literal).map_err(|error| {
                reply::refusal(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Issue::error(IssueType::Value)
                        .diagnosing(ferrohealth_server::run::chain(&error)),
                )
            })
        })?;
    let person = facade.patients(client, access).person(subject).await?;
    compartment::admit_person(facade, access, permission, CONSENT, Some(&person)).await?;
    let ehr = ehr::resolve(client, facade.store(), &person, ehr::Policy::Existing)
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
pub async fn export_context(
    facade: &Facade,
    ehr: &EhrId,
    policy: Option<&AccessPolicy>,
    stated: String,
) -> Result<ExportContext, Refusal> {
    let person = facade
        .store()
        .person_of(ehr)
        .await
        .map_err(|error| crate::facade::handlers::write::store_refusal(&error))?;
    let reference = match person.as_ref() {
        Some(person) => {
            crate::facade::subject::reference_of(
                &facade.settings().subject,
                facade.store(),
                &facade.programs(),
                person,
            )
            .await?
        }
        None => None,
    };
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
    use super::{Lanes, lanes_of, logical_id, parse_id, release_of};
    use crate::cdr::ids::EhrId;
    use crate::consent::fhir::Release;
    use crate::facade::identity::derive::{EntryKey, entry_id};
    use fhirconnect::engine::identity::{DIGEST_CHARACTERS, created_id, deterministic_id};
    use fhirconnect::engine::seam::identity::IdentityRequest;
    use http::{HeaderMap, HeaderValue, StatusCode, header};
    use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;

    /// Returns the pairs of `query`.
    fn pairs(query: &str) -> Vec<(String, String)> {
        crate::facade::media::query_pairs(query.as_bytes())
    }

    #[test]
    fn a_search_reaches_every_lane_that_can_match() {
        let lanes = |query: &str| lanes_of(true, true, &pairs(query));
        assert_eq!(lanes("patient=Patient/p-1"), Lanes::Both);
        assert_eq!(lanes("patient=Patient/p-1&scope="), Lanes::Both);
        assert_eq!(lanes("scope=patient-privacy"), Lanes::Access);
        assert_eq!(
            lanes("scope=http://terminology.hl7.org/CodeSystem/consentscope|patient-privacy"),
            Lanes::Access
        );
        assert_eq!(lanes("scope=adr"), Lanes::Mapped);
        assert_eq!(lanes("scope=adr,patient-privacy"), Lanes::Both);
        assert_eq!(lanes("scope=adr&scope=patient-privacy"), Lanes::Mapped);
        assert_eq!(lanes("scope=research"), Lanes::Mapped);
        assert_eq!(
            lanes("scope=http://example.org|patient-privacy"),
            Lanes::Mapped
        );
        assert_eq!(lanes("scope:not=adr"), Lanes::Both);
        assert_eq!(lanes_of(true, false, &pairs("scope=adr")), Lanes::Access);
        assert_eq!(lanes_of(false, true, &pairs("")), Lanes::Mapped);
    }

    #[test]
    fn no_access_directive_id_is_a_mapped_id_and_no_mapped_id_an_access_directive_id() {
        let ehr = EhrId::new("7d44b88c-4199-4bad-97dc-d78268e01398").expect("an id");
        let container = "8849182c-82ad-4088-a07f-48ead4180515";
        for index in 0..256 {
            let key = uuid::Uuid::new_v4();
            let access = logical_id(&ehr, key).expect("a UUID EHR id");
            assert_ne!(access.len(), DIGEST_CHARACTERS, "{access}");
            assert!(
                access.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "{access}"
            );
            let path = format!(
                "/content[openEHR-EHR-EVALUATION.advance_care_directive.v2]/items[{index}]"
            );
            let entry = entry_id(&EntryKey::new(
                HierObjectId::new(container).expect("a uid"),
                path.as_str(),
            ));
            let created = created_id(
                container,
                &path,
                &IdentityRequest::new("Consent", "Consent", "created"),
            );
            for mapped in [
                entry.as_str(),
                created.as_str(),
                &deterministic_id(&key.to_string(), &path),
            ] {
                assert_eq!(mapped.len(), DIGEST_CHARACTERS, "{mapped}");
                assert_eq!(parse_id(mapped), None, "{mapped}");
            }
        }
    }

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
