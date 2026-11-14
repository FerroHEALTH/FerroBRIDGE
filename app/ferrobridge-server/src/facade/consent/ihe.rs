// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The type-level operations `Consent/$bppc` and `Consent/$appc`.
//!
//! IHE defines the documents and their transport over XDS (ITI TF-1 19,
//! APPC TF-1 43), and no FHIR operation for them, so the operations are our
//! own design:
//!
//! - `POST [base]/Consent/$bppc?patient=Patient/<id>&evidence=<ehr URI>`
//!   takes one BPPC acknowledgement (ITI TF-3 5.1) as `text/xml` or
//!   `application/xml`; `evidence` is the `DV_EHR_URI` of the composition the
//!   caller stored the document in. `POST [base]/Consent/$appc?patient=…`
//!   takes one APPC Privacy Consent Document (APPC TF-3 5.6). Either commits
//!   the directives it reads and answers `201` with an `OperationOutcome`
//!   naming each `Consent` it created.
//! - `GET` on either answers a `collection` Bundle of `Binary` resources, one
//!   per document, `contentType` `text/xml`; an `OperationOutcome` entry names
//!   each directive the format cannot state, by its `Consent` id.

use axum::body::Bytes;
use base64::Engine as _;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use fhir_types::codec::Json as _;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::binary::Binary;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;
use openehr_rm::draft::access_control::access_control::access_directive::AccessDirective;

use crate::consent::Refused;
use crate::consent::exchange;
use crate::facade::Facade;
use crate::facade::consent::CONSENT;
use crate::facade::consent::Committed;
use crate::facade::consent::PATIENT;
use crate::facade::consent::State;
use crate::facade::consent::Subject;
use crate::facade::consent::admit;
use crate::facade::consent::commit;
use crate::facade::consent::logical_id;
use crate::facade::consent::state;
use crate::facade::consent::subject;
use crate::facade::handlers::Body;
use crate::facade::handlers::write::refuse;
use crate::facade::media;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;
use crate::facade::status::consent::of_document_refusal;
use crate::facade::status::consent::of_unstatable;

/// The name of the BPPC operation.
pub const BPPC: &str = "bppc";

/// The name of the APPC operation.
pub const APPC: &str = "appc";

/// The query parameter naming the composition a BPPC document is stored in.
const EVIDENCE: &str = "evidence";

/// The media types a document is posted in.
const XML: [&str; 2] = ["text/xml", "application/xml"];

/// Which IHE document an operation exchanges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// BPPC, the Patient Privacy Acknowledgement Document.
    Bppc,
    /// APPC, the Privacy Consent Document.
    Appc,
}

impl Profile {
    /// Returns the profile the operation `name` exchanges, `None` for any
    /// other operation.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        match name {
            "$bppc" => Some(Self::Bppc),
            "$appc" => Some(Self::Appc),
            _ => None,
        }
    }
}

/// Returns the query parameters of `uri`, refusing one the operation does
/// not take.
fn parameters(uri: &Uri, allowed: &[&str]) -> Result<Vec<(String, String)>, Refusal> {
    let pairs = media::query_pairs(uri.query().unwrap_or_default().as_bytes());
    if let Some((name, _)) = pairs
        .iter()
        .find(|(name, _)| !allowed.contains(&name.as_str()))
    {
        return Err(reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::NotSupported)
                .diagnosing("the operation takes no such parameter")
                .at(name.clone()),
        ));
    }
    Ok(pairs)
}

/// Returns the one value of `name` in `pairs`.
fn one<'a>(pairs: &'a [(String, String)], name: &str) -> Result<&'a str, Refusal> {
    let mut found = pairs.iter().filter(|(key, _)| key == name);
    match (found.next(), found.next()) {
        (Some((_, value)), None) => Ok(value),
        _ => Err(reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::Required)
                .diagnosing(format!("the operation names one {name}"))
                .at(name),
        )),
    }
}

/// Imports the document `body` carries into the EHR of the `patient`.
///
/// # Errors
/// Returns `415` for a body that is no XML, `400` for a missing parameter,
/// `422` naming the element a refused document fails at, and the refusals of
/// [`subject`] and of the exchange.
pub async fn import(
    facade: &Facade,
    access: &Access,
    profile: Profile,
    headers: &HeaderMap,
    uri: &Uri,
    body: &Bytes,
) -> Result<axum::response::Response, Refusal> {
    let settings = crate::facade::consent::settings(facade)?;
    let xml = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or_default().trim())
        .is_some_and(|essence| XML.iter().any(|known| essence.eq_ignore_ascii_case(known)));
    if !xml {
        return Err(reply::refusal(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Issue::error(IssueType::NotSupported)
                .diagnosing("the document is posted as text/xml or application/xml"),
        ));
    }
    let text = std::str::from_utf8(body).map_err(|_not_utf8| {
        reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::Structure).diagnosing("the document is no UTF-8 text"),
        )
    })?;
    let allowed: &[&str] = match profile {
        Profile::Bppc => &[PATIENT, EVIDENCE],
        Profile::Appc => &[PATIENT],
    };
    let pairs = parameters(uri, allowed)?;
    let patient = one(&pairs, PATIENT)?;
    let client = facade.client_for(headers, access).await?;
    let reached = subject(facade, &client, access, Permission::Create, Some(patient)).await?;
    let State { current, policy } = state(&client, &reached.ehr, settings).await?;
    let catalogue = exchange::catalogue(policy.as_ref());
    let directives = match profile {
        Profile::Bppc => crate::consent::bppc::import(
            text,
            &crate::consent::bppc::ImportContext {
                catalogue,
                evidence: one(&pairs, EVIDENCE)?,
            },
        ),
        Profile::Appc => {
            crate::consent::appc::import(text, &crate::consent::appc::ImportContext { catalogue })
        }
    }
    .map_err(|why| refuse(&of_document_refusal(&why)))?;
    let Committed { added: _, ids } = commit(
        facade, &client, access, &reached, &current, directives, settings,
    )
    .await?;
    let ids = ids
        .into_iter()
        .map(|id| format!("{CONSENT}/{id}"))
        .collect::<Vec<_>>();
    Ok(reply::issues(
        StatusCode::CREATED,
        &[
            Issue::information(IssueType::Informational).diagnosing(format!(
                "the document was committed as {} access directives: {}",
                ids.len(),
                ids.join(", ")
            )),
        ],
        &[],
    ))
}

/// Exports the directives of the `patient` as documents of `profile`.
///
/// # Errors
/// Returns `400` for a missing parameter, an APPC export a directive cannot
/// be written in (`500`, naming the attribute), and the refusals of
/// [`subject`] and of the exchange.
pub async fn export(
    facade: &Facade,
    access: &Access,
    profile: Profile,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<axum::response::Response, Refusal> {
    let settings = admit(facade, headers, uri, Body::Absent)?;
    let pairs = parameters(uri, &[PATIENT])?;
    let patient = one(&pairs, PATIENT)?;
    let client = facade.client_for(headers, access).await?;
    let reached = subject(facade, &client, access, Permission::Search, Some(patient)).await?;
    let State { current, policy } = state(&client, &reached.ehr, settings).await?;
    let directives = current.directives();
    let (documents, refused) = match profile {
        Profile::Bppc => bppc(&reached, directives, policy.as_ref()),
        Profile::Appc => (vec![appc(&reached, directives)?], Vec::new()),
    };
    let issues = refused
        .iter()
        .map(|(at, why)| {
            let named = directives
                .get(*at)
                .and_then(|directive| logical_id(&reached.ehr, exchange::key(directive)))
                .map_or_else(|| format!("directive {at}"), |id| format!("{CONSENT}/{id}"));
            Issue::warning(IssueType::NotSupported).diagnosing(format!("{named}: {why}"))
        })
        .collect::<Vec<_>>();
    let mut entry = Vec::with_capacity(documents.len().saturating_add(1));
    for document in documents {
        entry.push(resource_entry(binary(&document)?));
    }
    if !issues.is_empty() {
        let outcome = crate::facade::outcome::outcome(&issues)
            .to_json()
            .map_err(|error| internal(&error))?;
        entry.push(resource_entry(outcome));
    }
    let bundle = Object::from([
        (
            String::from("resourceType"),
            Value::String(String::from("Bundle")),
        ),
        (
            String::from("type"),
            Value::String(String::from("collection")),
        ),
        (String::from("entry"), Value::Array(entry)),
    ]);
    Ok(reply::resource(StatusCode::OK, &bundle, &[]))
}

/// Returns the BPPC documents of `directives`, with the position of each one
/// an acknowledgement cannot state.
fn bppc(
    reached: &Subject,
    directives: &[AccessDirective],
    policy: Option<&openehr_rm::draft::access_control::access_control::access_policy::AccessPolicy>,
) -> (Vec<String>, Vec<(usize, Refused)>) {
    let patient = Some(reached.person.id()).filter(|id| uuid::Uuid::try_parse(id).is_ok());
    let controller = policy.and_then(|policy| match &policy.controller.id {
        openehr_base::v1_3::base_types::identification::object_id::ObjectId::HierObjectId(id) => {
            uuid::Uuid::try_parse(id.value())
                .ok()
                .map(|uuid| uuid.to_string())
        }
        _ => None,
    });
    let now = jiff::Timestamp::now().to_string();
    let exported = crate::consent::bppc::export(
        directives,
        &crate::consent::bppc::ExportContext {
            catalogue: exchange::catalogue(policy),
            patient,
            controller: controller.as_deref(),
            now: &now,
        },
    );
    (exported.documents, exported.refused)
}

/// Returns the APPC Privacy Consent Document of `directives`.
fn appc(reached: &Subject, directives: &[AccessDirective]) -> Result<String, Refusal> {
    let description = format!(
        "The access directives of EHR {} in force, exported by FerroBRIDGE.",
        reached.ehr
    );
    crate::consent::appc::export(
        directives,
        &crate::consent::appc::ExportContext {
            patient_root: reached.person.namespace(),
            patient_extension: Some(reached.person.id()),
            description: &description,
        },
    )
    .map_err(|why| refuse(&of_unstatable(&why)))
}

/// Returns `document` as an R4 `Binary` of `text/xml`.
fn binary(document: &str) -> Result<Object, Refusal> {
    Binary {
        content_type: "text/xml".into(),
        data: Some(
            base64::engine::general_purpose::STANDARD
                .encode(document.as_bytes())
                .into(),
        ),
        ..Binary::default()
    }
    .to_json()
    .map_err(|error| internal(&error))
}

/// Returns a Bundle entry holding `resource`.
fn resource_entry(resource: Object) -> Value {
    Value::Object(Object::from([(
        String::from("resource"),
        Value::Object(resource),
    )]))
}

/// Returns the `500` of a resource that does not encode.
fn internal(error: &fhir_types::codec::EncodeError) -> Refusal {
    reply::refusal(
        StatusCode::INTERNAL_SERVER_ERROR,
        Issue::error(IssueType::Exception)
            .diagnosing(format!("the answer could not be encoded: {error}")),
    )
}
