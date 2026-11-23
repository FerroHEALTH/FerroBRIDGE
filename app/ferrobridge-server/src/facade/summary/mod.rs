// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET [base]/Patient/[id]/$summary`: the patient summary of one patient.
//!
//! The IPS operation "returns an IPS document", generated on demand, which
//! "SHOULD represent the latest available information", and its `profile`
//! input names the `Composition` profile the document conforms to, the IPS
//! `Composition-uv-ips` when none is named (HL7 IPS 2.0.1,
//! `OperationDefinition/summary`). This server answers the IPS 2.0.1 form by
//! default and the HL7 Europe Patient Summary form when `profile` names
//! `composition-eu-eps`.
//!
//! The patient is the party the `Patient` context renders. The record is
//! every patient summary composition of the patient's EHR, read as the
//! caller through [`Facade::client_for`], so the CDR applies the caller's own
//! access rules. Before it is served, the document is validated in process
//! against the form's `Bundle` profile; a document that breaks it answers
//! `422` with the validator's issues and is never served. A `patient/` token
//! for another patient answers `404`, so the answer says nothing of whether
//! that patient exists. No specification governs these rules: our own
//! design.

mod assemble;
pub mod terminology;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::Path;
use axum::extract::State;
use axum::response::Response;
use eehrxf::crosswalk::patient_summary::EPS_COMPOSITION;
use eehrxf::crosswalk::patient_summary::PATIENT_SUMMARY_TEMPLATE;
use eehrxf::document::IPS_COMPOSITION;
use eehrxf::document::form::Form;
use eehrxf::document::form::IPS_PATIENT_SUMMARY_FORM;
use eehrxf::document::form::PATIENT_SUMMARY_FORM;
use eehrxf::mapping::Mapping;
use eehrxf::mapping::MappingError;
use eehrxf::mappings::EPS;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::validation::ValidationError;
use ferrobridge_validator::set::PackageSet;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use fhir_types::codec::Json;
use fhir_types::codec::Value;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::resource::Resource;
use futures_util::TryStreamExt as _;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;
use openehr_its::rest::generated::query::AdhocQueryExecute;
use openehr_mapping_core::template::TemplateSource;

use crate::cdr::CdrClient;
use crate::cdr::ids::EhrId;
use crate::cdr::query::PageSize;
use crate::cdr::query::QueryPageError;
use crate::facade::Facade;
use crate::facade::compartment;
use crate::facade::ehr;
use crate::facade::handlers::Body;
use crate::facade::handlers::write::refuse;
use crate::facade::media;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::party;
use crate::facade::reply;
use crate::facade::reply::Refusal;
use crate::facade::status;

/// The resource type the operation is defined on.
pub const PATIENT: &str = "Patient";

/// The operation's name.
pub const OPERATION: &str = "summary";

/// The canonical `OperationDefinition` of the operation (HL7 IPS 2.0.1).
pub const DEFINITION: &str = "http://hl7.org/fhir/uv/ips/OperationDefinition/summary";

/// The version of the IPS guide whose `Composition-uv-ips` this server
/// writes.
const IPS_VERSION: &str = "2.0.1";

/// The version of the HL7 Europe Patient Summary guide whose
/// `composition-eu-eps` this server writes.
const EPS_VERSION: &str = "1.0.0-ballot";

/// The patient summary compositions of one EHR, every current version.
// NOTE: AQL 1.1.0 §6 `versionPredicate`: under `VERSION [LATEST_VERSION]` no superseded or deleted
// version is read.
const QUERY: &str = "SELECT c FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
WHERE e/ehr_id/value = $ehr_id AND c/archetype_details/template_id/value = $template_id \
ORDER BY v/uid/value";

/// The rows one page of [`QUERY`] asks for.
const PAGE: u32 = 50;

/// What the operation runs on: the EPS project compiled against the patient
/// summary template, the profile packages the document is validated over,
/// and the operator the document names.
#[derive(Debug)]
pub struct Summary {
    /// The EPS project of `eehrxf`, compiled.
    mapping: Mapping,
    /// The packages the document is validated over.
    packages: Arc<PackageSet>,
    /// The name of the organisation that operates this server.
    operator: String,
}

impl Summary {
    /// Compiles the EPS project against `template`, the patient summary
    /// template the CDR serves, over `packages`, naming `operator`.
    ///
    /// # Errors
    ///
    /// The [`MappingError`] of a project that does not compile against the
    /// template.
    pub fn new(
        template: &TemplateSource,
        packages: PackageSet,
        operator: String,
    ) -> Result<Self, MappingError> {
        Ok(Self {
            mapping: Mapping::compile_project(template, &EPS)?,
            packages: Arc::new(packages),
            operator,
        })
    }

    /// Returns the id of the template the project compiles against.
    #[must_use]
    pub const fn template_id() -> &'static str {
        PATIENT_SUMMARY_TEMPLATE
    }
}

/// `GET [base]/{type}/{id}/$summary`.
pub async fn route(
    State(facade): State<Arc<Facade>>,
    access: Access,
    Path((resource_type, id)): Path<(String, String)>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    summarize(&facade, &access, (&resource_type, &id), &headers, &uri)
        .await
        .unwrap_or_else(Refusal::into_response)
}

/// The operation behind [`route`].
async fn summarize(
    facade: &Facade,
    access: &Access,
    (resource_type, id): (&str, &str),
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Response, Refusal> {
    if resource_type != PATIENT {
        return Err(unsupported(format!(
            "$summary is an operation of Patient, not of {resource_type}"
        )));
    }
    let Some(summary) = facade.summary() else {
        return Err(unsupported(String::from(
            "this server serves no patient summary; GET /fhir/metadata names its operations",
        )));
    };
    crate::facade::handlers::guard(headers, uri, Body::Absent)?;
    let form = form_of(uri)?;
    party::admit(access, Permission::Read, PATIENT, Some(id)).map_err(|_refused| unknown(id))?;
    let loaded = facade.programs().party(PATIENT).cloned().ok_or_else(|| {
        internal(String::from(
            "the patient summary needs a Patient party context, and none is loaded",
        ))
    })?;
    let person = facade
        .settings()
        .subject
        .person_of_patient(id)
        .map_err(|_refused| unknown(id))?;
    let client = facade.client_for(headers, access).await?;
    let (patient, compositions) = record(facade, &client, &loaded, (id, &person)).await?;
    let timestamp = jiff::Timestamp::now().to_string();
    let base = facade.settings().base_url.trim_end_matches('/').to_owned();
    let identifier = uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        format!(
            "{base}/Patient/{id}/$summary?profile={}&at={timestamp}",
            form.composition
        )
        .as_bytes(),
    );
    let terminology = facade.translator().client().cloned().map_or_else(
        eehrxf::terminology::Terminology::none,
        |client| {
            eehrxf::terminology::Terminology::server(Arc::new(terminology::Server::new(client)))
        },
    );
    let assembled = assemble::assemble(
        assemble::Inputs {
            form,
            base: &base,
            identifier: format!("urn:uuid:{identifier}"),
            timestamp,
            patient,
            operator: &summary.operator,
            compositions: &compositions,
        },
        &summary.mapping,
        &terminology,
    )
    .await
    .map_err(|error| internal(ferrohealth_server::run::chain(&error)))?;
    let object = Json::to_json(&assembled.bundle)
        .map_err(|error| internal(format!("the patient summary could not be encoded: {error}")))?;
    for kind in entry_kinds(&object, &assembled.authored) {
        compartment::admit_person(facade, access, Permission::Read, &kind, Some(&person))?;
    }
    validate(summary, facade, form, &object).await?;
    Ok(reply::resource(StatusCode::OK, &object, &[]))
}

/// Returns the patient `id` names, as the party context `loaded` renders
/// it, and the patient summary compositions of the EHR of `person`, read
/// through `client`.
async fn record(
    facade: &Facade,
    client: &CdrClient,
    loaded: &crate::facade::programs::Loaded,
    (id, person): (&str, &crate::facade::identity::PersonId),
) -> Result<(Patient, Vec<String>), Refusal> {
    let uid = party::party_id(PATIENT, id)?;
    let held = party::get(
        party::demographic(client)?,
        party::read::class_of(loaded)?,
        PATIENT,
        uid.value(),
    )
    .await?;
    let rendered = party::render(
        facade,
        loaded,
        &party::accept(loaded, PATIENT, id, &held)?,
        &held.version,
    )
    .await?;
    let patient = patient_of(&rendered)?;
    let compositions = match ehr::resolve(
        client,
        facade.store(),
        person,
        &facade.settings().subject,
        ehr::Policy::Existing,
    )
    .await
    {
        Ok(ehr_id) => {
            crate::cdr::reach::touched(
                crate::cdr::reach::Touch::Read,
                Some(&ehr_id),
                crate::cdr::reach::canonical(&held.value),
            );
            compositions(client, &ehr_id).await?
        }
        // NOTE: no specification governs this: our own design; a patient with no EHR has an
        // empty record, so every section the project feeds is `nilknown`.
        Err(ehr::EhrError::Absent { .. }) => Vec::new(),
        Err(error) => {
            return Err(Refusal::from(crate::facade::ingest::refusal::ehr_refusal(
                &error,
            )));
        }
    };
    Ok((patient, compositions))
}

/// Returns the form the `profile` input names, the IPS form when it names
/// none.
fn form_of(uri: &Uri) -> Result<&'static Form, Refusal> {
    let mut form = &IPS_PATIENT_SUMMARY_FORM;
    for (name, value) in media::query_pairs(uri.query().unwrap_or_default().as_bytes()) {
        match name.as_str() {
            "profile" => {
                let (url, version) = match value.split_once('|') {
                    Some((url, version)) => (url, Some(version)),
                    None => (value.as_str(), None),
                };
                form = match (url, version) {
                    (IPS_COMPOSITION, None | Some(IPS_VERSION)) => &IPS_PATIENT_SUMMARY_FORM,
                    (EPS_COMPOSITION, None | Some(EPS_VERSION)) => &PATIENT_SUMMARY_FORM,
                    _ => {
                        return Err(bad_request(
                            IssueType::NotSupported,
                            format!(
                                "this server writes {IPS_COMPOSITION}|{IPS_VERSION} and \
                                 {EPS_COMPOSITION}|{EPS_VERSION}, not {value}"
                            ),
                        ));
                    }
                };
            }
            "identifier" => {
                return Err(bad_request(
                    IssueType::NotSupported,
                    String::from(
                        "this server finds the patient by its logical id in the path, not by identifier",
                    ),
                ));
            }
            "_format" | "_pretty" => {}
            other => {
                return Err(bad_request(
                    IssueType::NotSupported,
                    format!("$summary takes no parameter {other:?}"),
                ));
            }
        }
    }
    Ok(form)
}

/// Returns the canonical JSON of every patient summary composition the EHR
/// `ehr_id` holds, each recorded as read.
async fn compositions(client: &CdrClient, ehr_id: &EhrId) -> Result<Vec<String>, Refusal> {
    let request = AdhocQueryExecute {
        q: QUERY.to_owned(),
        offset: Some(0),
        fetch: None,
        query_parameters: Some(BTreeMap::from([
            (
                String::from("ehr_id"),
                serde_json::Value::String(ehr_id.as_str().to_owned()),
            ),
            (
                String::from("template_id"),
                serde_json::Value::String(PATIENT_SUMMARY_TEMPLATE.to_owned()),
            ),
        ])),
        additional_properties: BTreeMap::new(),
    };
    let page = PageSize::new(PAGE).map_err(|error| internal(error.to_string()))?;
    let rows: Vec<_> = client
        .query_aql_rows(request, page)
        .try_collect()
        .await
        .map_err(|error| match error {
            QueryPageError::Refused(upstream) => refuse(&status::of_query_refusal(&upstream)),
            QueryPageError::Call(error) => refuse(&status::of_client_error(&error)),
        })?;
    let mut held = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(composition) = row.first() else {
            return Err(bad_gateway("the CDR answered a row with no composition"));
        };
        crate::cdr::reach::touched(
            crate::cdr::reach::Touch::Read,
            Some(ehr_id),
            crate::cdr::reach::canonical(composition),
        );
        held.push(
            serde_json::to_string(composition).map_err(|error| {
                internal(format!("a composition could not be encoded: {error}"))
            })?,
        );
    }
    Ok(held)
}

/// Returns the `Patient` the party context rendered.
fn patient_of(rendered: &fhir_types::codec::Object) -> Result<Patient, Refusal> {
    match Resource::from_json(rendered, &mut fhir_types::codec::Path::root(PATIENT)) {
        Ok(Resource::Patient(patient)) => Ok(*patient),
        Ok(_) => Err(internal(String::from(
            "the Patient party context rendered no Patient",
        ))),
        Err(error) => Err(internal(format!(
            "the Patient the party context rendered does not decode: {error}"
        ))),
    }
}

/// The resource types of the document's entries but its `Composition`, its
/// `Patient` and the authors this server wrote, each once.
fn entry_kinds(bundle: &fhir_types::codec::Object, authored: &[String]) -> Vec<String> {
    let mut kinds: Vec<String> = bundle
        .get("entry")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter(|entry| {
            entry
                .get("fullUrl")
                .and_then(Value::as_str)
                .is_none_or(|url| !authored.iter().any(|held| held == url))
        })
        .filter_map(|entry| entry.get("resource").and_then(Value::as_object))
        .filter_map(|resource| resource.get("resourceType").and_then(Value::as_str))
        .filter(|kind| !matches!(*kind, "Composition" | PATIENT))
        .map(str::to_owned)
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

/// Validates `object`, the document, against the `Bundle` profile of
/// `form` over the summary's packages, asking the `[terminology]` server the
/// bindings the packages cannot decide.
async fn validate(
    summary: &Summary,
    facade: &Facade,
    form: &Form,
    object: &fhir_types::codec::Object,
) -> Result<(), Refusal> {
    let text = serde_json::to_string(object)
        .map_err(|error| internal(format!("the patient summary could not be encoded: {error}")))?;
    let document = ReceivedDocument::read(&text).map_err(|error| {
        tracing::warn!(
            profile = form.bundle,
            "a patient summary broke the document rules and was not served"
        );
        reply::refusal(
            StatusCode::UNPROCESSABLE_ENTITY,
            Issue::error(IssueType::Processing).diagnosing(format!(
                "the patient summary breaks the document rules: {}",
                ferrohealth_server::run::chain(&error)
            )),
        )
    })?;
    match document
        .validate(&summary.packages, form.bundle, facade.translator().client())
        .await
    {
        Ok(_) => Ok(()),
        Err(ValidationError::NonConformant { findings }) => {
            tracing::warn!(
                findings = findings.len(),
                profile = form.bundle,
                "a patient summary broke its profile and was not served"
            );
            let issues: Vec<Issue> = findings
                .iter()
                .map(|finding| {
                    Issue::error(IssueType::Processing)
                        .diagnosing(format!("{}: {finding}", finding.profile))
                        .at(finding.location.clone())
                })
                .collect();
            Err(reply::refusals(
                StatusCode::UNPROCESSABLE_ENTITY,
                &issues,
                &[],
            ))
        }
        Err(error @ ValidationError::Terminology { .. }) => Err(reply::refusal(
            StatusCode::BAD_GATEWAY,
            Issue::error(IssueType::Exception).diagnosing(error.to_string()),
        )),
        Err(error) => Err(internal(ferrohealth_server::run::chain(&error))),
    }
}

/// The `404` of an operation this server does not serve.
fn unsupported(text: String) -> Refusal {
    reply::refusal(
        StatusCode::NOT_FOUND,
        Issue::error(IssueType::NotSupported).diagnosing(text),
    )
}

/// The `404` of a patient the caller cannot reach or that does not exist.
fn unknown(id: &str) -> Refusal {
    reply::refusal(
        StatusCode::NOT_FOUND,
        Issue::error(IssueType::NotFound).diagnosing(format!("Patient/{id} is not known")),
    )
}

/// The `400` of an input the operation does not take.
fn bad_request(code: IssueType, text: String) -> Refusal {
    reply::refusal(StatusCode::BAD_REQUEST, Issue::error(code).diagnosing(text))
}

/// The `502` of a CDR answer the operation cannot read.
fn bad_gateway(text: &str) -> Refusal {
    reply::refusal(
        StatusCode::BAD_GATEWAY,
        Issue::error(IssueType::Exception).diagnosing(text),
    )
}

/// The `500` of a failure of this server.
fn internal(text: String) -> Refusal {
    reply::refusal(
        StatusCode::INTERNAL_SERVER_ERROR,
        Issue::error(IssueType::Exception).diagnosing(text),
    )
}
