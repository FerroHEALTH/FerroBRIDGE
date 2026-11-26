// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The IHE MHD 4.2.3 Document Responder over the documents the Composition
//! contexts assemble.
//!
//! The Document Responder answers Find Document Lists [ITI-66], Find
//! Document References [ITI-67] and Retrieve Document [ITI-68], all three
//! Required (ITI TF-1: Table 33.1-1). Each document is the Bundle
//! `Composition/[id]/$document` assembles from one composition of a
//! Composition context, so a `DocumentReference` is the draft `EEHRxF`
//! Annex III description of that document under the Composition's id, its
//! `content.attachment.url` the `$document` URL ITI-68 retrieves (ITI TF-2:
//! 3.67.4.2.2.1). A `List` is a submission set, one per CONTRIBUTION that
//! committed the latest version of a document composition; no specification
//! maps a CONTRIBUTION to a submission set: our own design.
//!
//! A search ignores a parameter it does not support, as MHD asks ("should be
//! ignored", ITI TF-2: 3.66.4.1.2, 3.67.4.1.2) and FHIR R4 does under
//! lenient handling, and refuses it under `Prefer: handling=strict`
//! (<https://hl7.org/fhir/R4/search.html#errors>). The `self` link names the
//! parameters the search applied (§3.1.1.6). Every parameter the Responder
//! SHALL process is matched against the element the description fills; an
//! element the description leaves empty matches no value.

mod list;
mod listing;
mod params;
mod reference;

use axum::response::Response;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::resource::Compartment;
use ferrohealth_smart::scope::Permission;
use fhir_types::codec::Object;
use fhir_types::r4::capability_statement::CapabilityStatementRestResource;
use http::StatusCode;

use crate::cdr::CdrClient;
use crate::cdr::ids::EhrId;
use crate::facade::Facade;
use crate::facade::context::Context;
use crate::facade::handlers::format_of;
use crate::facade::media;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;
use params::Values;

/// The resource type of a document's description.
pub const DOCUMENT_REFERENCE: &str = "DocumentReference";

/// The resource type of a submission set.
pub const LIST: &str = "List";

/// The `CapabilityStatement` of the MHD Document Responder.
pub const RESPONDER: &str =
    "https://profiles.ihe.net/ITI/MHD/CapabilityStatement/IHE.MHD.DocumentResponder";

/// The profile the Document Responder serves `DocumentReference` under.
const DOCUMENT_REFERENCE_PROFILE: &str =
    "https://profiles.ihe.net/ITI/MHD/StructureDefinition/IHE.MHD.Minimal.DocumentReference";

/// The profile the Document Responder serves `List` under.
const LIST_PROFILE: &str = "https://profiles.ihe.net/ITI/MHD/StructureDefinition/IHE.MHD.List";

/// The profile a submission set this server writes is.
const SUBMISSION_SET_PROFILE: &str =
    "https://profiles.ihe.net/ITI/MHD/StructureDefinition/IHE.MHD.Minimal.SubmissionSet";

/// The interactions both types answer.
const INTERACTIONS: [&str; 2] = ["read", "search-type"];

/// Whether the Document Responder serves `resource_type` on `facade`.
#[must_use]
pub fn serves(facade: &Facade, resource_type: &str) -> bool {
    facade.mhd() && matches!(resource_type, DOCUMENT_REFERENCE | LIST)
}

/// `GET [base]/{DocumentReference|List}/{id}`.
///
/// # Errors
///
/// Returns `406` for a format neither FHIR JSON nor XML, `403` for a caller
/// without read on the type, `404` for an id that names nothing served, and
/// the refusal a CDR answer renders as.
pub async fn read(cx: Context<'_>, (resource_type, id): (&str, &str)) -> Result<Response, Refusal> {
    let format = format_of(cx.headers, cx.uri)?;
    let client = cx.facade.client_for(cx.headers, cx.access).await?;
    let resource = match resource_type {
        DOCUMENT_REFERENCE => reference::read(cx.facade, cx.access, &client, id).await?,
        _ => list::read(cx.facade, cx.access, &client, id).await?,
    };
    Ok(reply::formatted(StatusCode::OK, &resource, format, &[]))
}

/// `GET [base]/{DocumentReference|List}?…` and `POST …/_search`, its
/// parameters the query and the form `body` beside it.
///
/// # Errors
///
/// Returns `406` for a format neither FHIR JSON nor XML, `400` for a
/// malformed value or, under `Prefer: handling=strict`, a parameter this
/// server does not support, `403` for a caller without search on the type,
/// and the refusal a CDR answer renders as.
pub async fn search(
    cx: Context<'_>,
    resource_type: &str,
    body: &[u8],
) -> Result<Response, Refusal> {
    let format = format_of(cx.headers, cx.uri)?;
    if !body.is_empty() {
        crate::facade::handlers::search::form::check_form(cx.headers)?;
    }
    let mut pairs = media::query_pairs(cx.uri.query().unwrap_or_default().as_bytes());
    pairs.extend(media::query_pairs(body));
    let strict = params::strict(cx.headers);
    let client = cx.facade.client_for(cx.headers, cx.access).await?;
    let found = match resource_type {
        DOCUMENT_REFERENCE => {
            reference::search(cx.facade, cx.access, &client, &pairs, strict).await?
        }
        _ => list::search(cx.facade, cx.access, &client, &pairs, strict).await?,
    };
    let bundle = found.searchset(cx.facade, resource_type)?;
    Ok(reply::formatted(StatusCode::OK, &bundle, format, &[]))
}

/// What one search found: the matches of the page, the parameters it
/// applied and ignored, and where the page sits.
#[derive(Debug)]
struct Found {
    /// The matches of the page, in order.
    page: Vec<Object>,
    /// Every match, the page among them.
    total: usize,
    /// What the request asked of the search.
    read: params::Read,
}

impl Found {
    /// Returns the matches of the page `read` asks for out of `matches`.
    fn paged(matches: Vec<Object>, read: params::Read) -> Self {
        let total = matches.len();
        let page = matches
            .into_iter()
            .skip(read.offset)
            .take(read.count)
            .collect();
        Self { page, total, read }
    }

    /// Returns the `searchset` Bundle of this page of `resource_type`
    /// (<https://hl7.org/fhir/R4/bundle.html>), with an `OperationOutcome`
    /// entry naming the ignored parameters (ITI TF-2: 3.67.4.2.2).
    fn searchset(&self, facade: &Facade, resource_type: &str) -> Result<Object, Refusal> {
        let base = facade.settings().base_url.trim_end_matches('/');
        let link = |offset: usize| {
            format!(
                "{base}/{resource_type}?{}",
                self.read.query(offset, self.read.count)
            )
        };
        let mut links =
            vec![serde_json::json!({"relation": "self", "url": link(self.read.offset)})];
        let next = self.read.offset.saturating_add(self.page.len());
        if next < self.total {
            links.push(serde_json::json!({"relation": "next", "url": link(next)}));
        }
        let mut entries: Vec<serde_json::Value> = self
            .page
            .iter()
            .map(|resource| {
                let id = resource
                    .get("id")
                    .and_then(fhir_types::codec::Value::as_str)
                    .unwrap_or_default();
                serde_json::json!({
                    "fullUrl": format!("{base}/{resource_type}/{id}"),
                    "resource": resource,
                    "search": {"mode": "match"},
                })
            })
            .collect();
        if !self.read.ignored.is_empty() {
            let issues: Vec<serde_json::Value> = self
                .read
                .ignored
                .iter()
                .map(|name| {
                    serde_json::json!({
                        "severity": "warning",
                        "code": "not-supported",
                        "diagnostics": format!("{name} is not supported here and was ignored"),
                        "expression": [name],
                    })
                })
                .collect();
            entries.push(serde_json::json!({
                "resource": {"resourceType": "OperationOutcome", "issue": issues},
                "search": {"mode": "outcome"},
            }));
        }
        serde_json::from_value(serde_json::json!({
            "resourceType": "Bundle",
            "type": "searchset",
            "total": self.total,
            "link": links,
            "entry": entries,
        }))
        .map_err(|error| {
            internal(format!(
                "the {resource_type} searchset could not be written: {error}"
            ))
        })
    }
}

/// Reads a search of `resource_type` over `declared` from `pairs`, refusing
/// an unsupported parameter when `strict`, and returns it with the EHRs it
/// reaches.
async fn scoped(
    facade: &Facade,
    access: &Access,
    client: &CdrClient,
    (resource_type, declared): (&str, &[params::Declared]),
    (pairs, strict): (&[(String, String)], bool),
) -> Result<(params::Read, Vec<EhrId>), Refusal> {
    let request = params::read(pairs, declared, strict)?;
    let resolver = facade.patients(client, access);
    let named = patients(facade, (client, &resolver), &request).await?;
    let ehrs = reach(facade, (access, &resolver), resource_type, named).await?;
    Ok((request, ehrs))
}

/// Returns the EHRs the `patient` and `patient.identifier` occurrences of
/// `request` name together, `None` when it names neither.
///
/// A `patient` value is a `Patient` reference, relative, absolute under
/// this server's base or a bare id (R4 `search.html` §3.1.1.4.11); a
/// reference to another server names no patient of this one.
async fn patients(
    facade: &Facade,
    resolver: (&CdrClient, &crate::facade::patients::Patients<'_>),
    request: &params::Read,
) -> Result<Option<Vec<EhrId>>, Refusal> {
    let mut named: Option<Vec<EhrId>> = None;
    for occurrence in &request.applied {
        let mut found = Vec::new();
        match (occurrence.name, &occurrence.values) {
            ("patient", Values::References(references)) => {
                for reference in references {
                    if let Some(id) = patient_id(facade, reference)
                        && let Some(ehr) =
                            crate::facade::handlers::search::identifier::ehr_of_party(
                                facade, resolver, &id,
                            )
                            .await?
                    {
                        found.push(ehr);
                    }
                }
            }
            ("patient.identifier", Values::Tokens(tokens)) => {
                for token in tokens {
                    found.extend(
                        crate::facade::handlers::search::identifier::ehrs_of(
                            facade,
                            resolver,
                            "patient.identifier",
                            token,
                        )
                        .await?,
                    );
                }
            }
            _ => continue,
        }
        named = Some(match named {
            None => found,
            Some(earlier) => earlier
                .into_iter()
                .filter(|ehr| found.contains(ehr))
                .collect(),
        });
    }
    Ok(named)
}

/// Returns the logical id a `patient` value names on this server.
fn patient_id(facade: &Facade, reference: &str) -> Option<String> {
    let base = facade.settings().base_url.trim_end_matches('/');
    let relative = reference
        .strip_prefix(base)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(reference);
    match relative.split_once('/') {
        Some(("Patient", id)) if !id.contains('/') => Some(id.to_owned()),
        Some(_) => None,
        None => Some(relative.to_owned()),
    }
}

/// Returns the EHRs a search may reach: the ones its patient parameters
/// name, held to the patient compartment of a `patient/` token.
///
/// A search names `patient` or `patient.identifier` (ITI TF-2:
/// 3.66.4.1.2, 3.67.4.1.2). One that names neither reaches the compartment
/// of a `patient/` token and is refused for any other caller, since this
/// server lists documents one EHR at a time (no specification governs this:
/// our own design).
async fn reach(
    facade: &Facade,
    (access, patients): (&Access, &crate::facade::patients::Patients<'_>),
    resource_type: &str,
    named: Option<Vec<EhrId>>,
) -> Result<Vec<EhrId>, Refusal> {
    match access.reach(Permission::Search, resource_type) {
        Some(Compartment::Any) => named.ok_or_else(|| {
            reply::refusal(
                StatusCode::BAD_REQUEST,
                Issue::error(IssueType::Required).diagnosing(format!(
                    "a {resource_type} search names patient or patient.identifier"
                )),
            )
        }),
        Some(Compartment::Patient(patient)) => {
            let own = crate::facade::handlers::search::compartment_ehrs(facade, patients, &patient)
                .await?;
            Ok(match named {
                Some(named) => own.into_iter().filter(|ehr| named.contains(ehr)).collect(),
                None => own,
            })
        }
        None => Err(crate::facade::guard::forbidden()),
    }
}

/// Returns the `404` of an id that names nothing served as `resource_type`.
fn unknown(resource_type: &str, id: &str) -> Refusal {
    reply::refusal(
        StatusCode::NOT_FOUND,
        Issue::error(IssueType::NotFound).diagnosing(format!("no {resource_type}/{id} is served")),
    )
}

/// Returns the `500` of a failure inside this server.
fn internal(text: String) -> Refusal {
    reply::refusal(
        StatusCode::INTERNAL_SERVER_ERROR,
        Issue::error(IssueType::Exception).diagnosing(text),
    )
}

/// Returns the `rest.resource` entries of the two types.
#[must_use]
pub fn capability() -> [CapabilityStatementRestResource; 2] {
    let declared = crate::facade::capability::declared_params;
    [
        CapabilityStatementRestResource {
            r#type: DOCUMENT_REFERENCE.into(),
            profile: Some(DOCUMENT_REFERENCE_PROFILE.into()),
            documentation: Some(reference::DOCUMENTATION.into()),
            interaction: crate::facade::capability::interactions(INTERACTIONS),
            search_param: declared(reference::DECLARED),
            ..CapabilityStatementRestResource::default()
        },
        CapabilityStatementRestResource {
            r#type: LIST.into(),
            profile: Some(LIST_PROFILE.into()),
            supported_profile: vec![SUBMISSION_SET_PROFILE.into()],
            documentation: Some(list::DOCUMENTATION.into()),
            interaction: crate::facade::capability::interactions(INTERACTIONS),
            search_param: declared(list::DECLARED),
            ..CapabilityStatementRestResource::default()
        },
    ]
}

/// The formats a served resource is written in, as an `Endpoint` names
/// them.
pub const FORMATS: [&str; 2] = [media::FHIR_JSON, media::FHIR_XML];
