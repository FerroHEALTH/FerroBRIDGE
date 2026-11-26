// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `Consent` create, read and search (FHIR R4 `http.html`, `consent.html`).
//!
//! A create reads the `Consent` into directives, appends them to the
//! `EHR_ACCESS` of the patient's EHR and answers `201` with the `Location`
//! of the first. One `Consent` with root provisions that nest further reads
//! as several directives, each its own `Consent` from then on, so the answer
//! names every id it created. A read and a search export the directives in
//! force, one `Consent` each.

use axum::body::Bytes;
use ferrohealth_smart::resource::Access;
use ferrohealth_smart::scope::Permission;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use http::HeaderMap;
use http::StatusCode;
use http::Uri;
use http::header;
use openehr_rm::draft::access_control::access_control::access_directive::AccessDirective;

use crate::cdr::CdrClient;
use crate::cdr::ids::EhrId;
use crate::consent::exchange;
use crate::consent::fhir::Release;
use crate::consent::fhir::import::ImportContext;
use crate::consent::fhir::model::Code;
use crate::facade::Facade;
use crate::facade::consent::CONSENT;
use crate::facade::consent::Committed;
use crate::facade::consent::Lanes;
use crate::facade::consent::PATIENT;
use crate::facade::consent::PATIENT_PRIVACY;
use crate::facade::consent::SCOPE;
use crate::facade::consent::State;
use crate::facade::consent::admit;
use crate::facade::consent::answer;
use crate::facade::consent::commit;
use crate::facade::consent::export_context;
use crate::facade::consent::logical_id;
use crate::facade::consent::names_scope;
use crate::facade::consent::parse_id;
use crate::facade::consent::scope_values;
use crate::facade::consent::state;
use crate::facade::consent::subject;
use crate::facade::handlers::Body;
use crate::facade::handlers::write::refuse;
use crate::facade::media;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;
use crate::facade::search::bundle::Match;
use crate::facade::status::consent::of_document_refusal;
use crate::facade::status::consent::of_exchange_error;
use crate::facade::status::consent::of_unstatable;
use crate::identity::FhirResourceId;

/// The instant the bridge answers, the `dateTime` of an exported `Consent`.
fn now() -> String {
    jiff::Timestamp::now().to_string()
}

/// Creates the `Consent` `body` carries.
///
/// # Errors
/// Returns the refusal of the media types, a `Consent` refused at a named
/// element (`422`), a patient the caller may not reach, and every exchange
/// failure through `facade::status::consent`.
pub async fn create(
    facade: &Facade,
    access: &Access,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<axum::response::Response, Refusal> {
    let settings = admit(facade, headers, &Uri::from_static("/"), Body::Present)?;
    let release = crate::facade::consent::release_of(
        headers,
        &header::CONTENT_TYPE,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
    )?;
    let document = crate::consent::fhir::read(release, body)
        .map_err(|why| refuse(&of_document_refusal(&why)))?;
    let client = facade.client_for(headers, access).await?;
    let reached = subject(
        facade,
        &client,
        access,
        Permission::Create,
        document
            .subject
            .as_ref()
            .and_then(|subject| subject.reference.as_deref()),
    )
    .await?;
    let translated = exchange::operations(facade.translator().client(), &document)
        .await
        .map_err(|error| refuse(&of_exchange_error(&error)))?;
    let State { current, policy } = state(facade, &client, &reached.ehr, settings).await?;
    let operations = |coding: &Code| {
        translated
            .iter()
            .find(|(known, _)| known == coding)
            .map(|(_, class)| *class)
    };
    let directives = crate::consent::fhir::import::import(
        release,
        &document,
        &ImportContext {
            catalogue: exchange::catalogue(policy.as_ref()),
            default_access: current.default_access(settings.default_access),
            operations: &operations,
        },
    )
    .map_err(|why| refuse(&of_document_refusal(&why)))?;
    let Committed { added, ids } =
        commit(facade, &client, &reached, &current, directives, settings).await?;
    let base = facade.settings().base_url.trim_end_matches('/');
    let location = ids
        .first()
        .map(|id| reply::Header::location(&format!("{base}/{CONSENT}/{id}")));
    let headers_out: Vec<reply::Header> = location.into_iter().collect();
    match (media::Prefer::of(headers), added.as_slice()) {
        (media::Prefer::Minimal, _) => Ok(reply::minimal(StatusCode::CREATED, &headers_out)),
        (media::Prefer::Representation, [one]) => {
            let context = export_context(facade, &reached.ehr, policy.as_ref(), now()).await?;
            let body = render(release, one, &context, ids.first().map(String::as_str))?;
            Ok(answer(StatusCode::CREATED, release, &body, &headers_out))
        }
        _ => Ok(reply::issues(
            StatusCode::CREATED,
            &[
                Issue::information(IssueType::Informational).diagnosing(format!(
                    "the Consent was committed as {} access directives: {}",
                    ids.len(),
                    ids.iter()
                        .map(|id| format!("{CONSENT}/{id}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            ],
            &headers_out,
        )),
    }
}

/// Reads the `Consent` `id` names.
///
/// # Errors
/// Returns `404` for an id this facade never issued or a directive no longer
/// in force, the `403` of a patient-scoped token for another patient, and
/// the exchange failures.
pub async fn read(
    facade: &Facade,
    access: &Access,
    id: &str,
    headers: &HeaderMap,
) -> Result<axum::response::Response, Refusal> {
    let settings = admit(facade, headers, &Uri::from_static("/"), Body::Absent)?;
    let release =
        crate::facade::consent::release_of(headers, &header::ACCEPT, StatusCode::NOT_ACCEPTABLE)?;
    let not_found = || {
        reply::refusal(
            StatusCode::NOT_FOUND,
            Issue::error(IssueType::NotFound).diagnosing(format!("no {CONSENT}/{id} is in force")),
        )
    };
    let (ehr, key) = parse_id(id).ok_or_else(not_found)?;
    crate::facade::compartment::admit_ehr(facade, access, Permission::Read, CONSENT, &ehr).await?;
    let client = facade.client_for(headers, access).await?;
    let State { current, policy } = state(facade, &client, &ehr, settings).await?;
    let directive = current
        .directives()
        .iter()
        .find(|directive| exchange::key(directive) == key)
        .ok_or_else(not_found)?;
    let context = export_context(facade, &ehr, policy.as_ref(), now()).await?;
    let body = render(release, directive, &context, Some(id))?;
    Ok(answer(StatusCode::OK, release, &body, &[]))
}

/// Searches the `Consent`s of the patient the `patient` parameter names in
/// the `lanes` it reaches ([`crate::facade::consent::lanes`]).
///
/// The access directives take `patient` and, in R4, `scope` (R4
/// `consent.html#search`); any other parameter is refused, so a search never
/// answers more than it was asked for. Every directive is of scope
/// `patient-privacy`, so a `scope` that names no such value matches none.
///
/// A search of both lanes answers the access directives first, then the
/// mapped search's matches, paged as one searchset (R4 `search.html`
/// §3.1.1.5.3); it also takes the paging parameters, and another parameter
/// is refused naming `scope=adr`, which searches the advance directives by
/// it. The mapped lane writes R4 alone, so an R5 search of both is refused.
/// No specification governs the order of the lanes: our own design.
///
/// # Errors
/// Returns `406` for an R5 search of both lanes, `400` for a search without
/// one `patient`, with another parameter or with a `scope` in R5, the
/// refusals of [`subject`], the exchange failures and every refusal of the
/// mapped search.
pub async fn search(
    facade: &Facade,
    access: &Access,
    headers: &HeaderMap,
    uri: &Uri,
    pairs: &[(String, String)],
    lanes: Lanes,
) -> Result<axum::response::Response, Refusal> {
    let both = lanes == Lanes::Both;
    let settings = admit(facade, headers, uri, Body::Absent)?;
    let release =
        crate::facade::consent::release_of(headers, &header::ACCEPT, StatusCode::NOT_ACCEPTABLE)?;
    if both && release == Release::R5 {
        return Err(reply::refusal(
            StatusCode::NOT_ACCEPTABLE,
            Issue::error(IssueType::NotSupported).diagnosing(format!(
                "a {CONSENT} search that reaches the advance directives answers FHIR R4; search {SCOPE}=patient-privacy for the access directives in R5"
            )),
        ));
    }
    let asked = asked(pairs, release, if both { &PAGING } else { &[] })?;
    let client = facade.client_for(headers, access).await?;
    let reached = subject(
        facade,
        &client,
        access,
        Permission::Search,
        Some(&asked.patient),
    )
    .await?;
    let entries = if asked.matches {
        directives(facade, &client, &reached.ehr, settings, release).await?
    } else {
        Vec::new()
    };
    if both {
        let mut lead = Vec::with_capacity(entries.len());
        for (id, resource) in entries {
            let id = id
                .as_deref()
                .and_then(|id| FhirResourceId::new(id).ok())
                .ok_or_else(|| {
                    reply::refusal(
                        StatusCode::BAD_GATEWAY,
                        Issue::error(IssueType::Exception).diagnosing(format!(
                            "the EHR {} has no UUID id, so its access directives have no {CONSENT} id",
                            reached.ehr.as_str()
                        )),
                    )
                })?;
            lead.push(Match {
                id,
                resource,
                included: Vec::new(),
            });
        }
        return crate::facade::handlers::search::search(
            facade,
            access,
            CONSENT,
            headers,
            pairs,
            Some(lead),
        )
        .await;
    }
    let base = facade.settings().base_url.trim_end_matches('/');
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair(PATIENT, &asked.patient);
    for scope in &asked.scopes {
        query.append_pair(SCOPE, scope);
    }
    let self_link = format!("{base}/{CONSENT}?{}", query.finish());
    Ok(answer(
        StatusCode::OK,
        release,
        &searchset(base, &self_link, entries)?,
        &[],
    ))
}

/// The paging parameters of a mapped search, which a search of both lanes
/// takes beside `patient` and `scope`.
const PAGING: [&str; 4] = ["_count", "_cursor", "_total", "_summary"];

/// What a search of the access directives asks.
struct Asked {
    /// The one `patient` value.
    patient: String,
    /// The `scope` values, as received.
    scopes: Vec<String>,
    /// Whether every `scope` names `patient-privacy`.
    matches: bool,
}

/// Reads `pairs` as a search of the access directives in `release`, which
/// also passes over the parameters `also` names.
fn asked(pairs: &[(String, String)], release: Release, also: &[&str]) -> Result<Asked, Refusal> {
    let mut patients = Vec::new();
    let mut scopes = Vec::new();
    let mut matches = true;
    for (name, value) in pairs {
        // NOTE: R4 search.html §3.1.1.3: an empty parameter is ignored.
        if value.is_empty() || also.contains(&name.as_str()) {
            continue;
        }
        if name == PATIENT {
            patients.push(value.clone());
        } else if name == SCOPE && release == Release::R4 {
            let tokens = scope_values(value).ok_or_else(|| {
                reply::refusal(
                    StatusCode::BAD_REQUEST,
                    Issue::error(IssueType::Invalid)
                        .diagnosing(format!("`{value}` is no token value"))
                        .at(SCOPE),
                )
            })?;
            matches &= names_scope(&tokens, PATIENT_PRIVACY);
            scopes.push(value.clone());
        } else {
            let diagnosis = if also.is_empty() {
                format!("a {CONSENT} search takes only {PATIENT} and, in R4, {SCOPE}")
            } else {
                format!(
                    "the access directives are searched by {PATIENT} and {SCOPE} alone; search {SCOPE}=adr for the advance directives by {name}"
                )
            };
            return Err(reply::refusal(
                StatusCode::BAD_REQUEST,
                Issue::error(IssueType::NotSupported)
                    .diagnosing(diagnosis)
                    .at(name.clone()),
            ));
        }
    }
    let patient = match patients.as_slice() {
        [patient] => patient.clone(),
        _ => {
            return Err(reply::refusal(
                StatusCode::BAD_REQUEST,
                Issue::error(IssueType::Required)
                    .diagnosing(format!("a {CONSENT} search names one {PATIENT}"))
                    .at(PATIENT),
            ));
        }
    };
    Ok(Asked {
        patient,
        scopes,
        matches,
    })
}

/// Returns the directives in force of `ehr`, each rendered in `release`
/// with its logical id.
async fn directives(
    facade: &Facade,
    client: &CdrClient,
    ehr: &EhrId,
    settings: &exchange::Settings,
    release: Release,
) -> Result<Vec<(Option<String>, Object)>, Refusal> {
    let State { current, policy } = state(facade, client, ehr, settings).await?;
    let context = export_context(facade, ehr, policy.as_ref(), now()).await?;
    let mut entries = Vec::with_capacity(current.directives().len());
    for directive in current.directives() {
        let id = logical_id(ehr, exchange::key(directive));
        entries.push((
            id.clone(),
            render(release, directive, &context, id.as_deref())?,
        ));
    }
    Ok(entries)
}

/// Writes `directive` as one `Consent` of `release` with `id`.
fn render(
    release: Release,
    directive: &AccessDirective,
    context: &crate::consent::fhir::export::ExportContext,
    id: Option<&str>,
) -> Result<Object, Refusal> {
    let documents =
        crate::consent::fhir::export::export(release, std::slice::from_ref(directive), context)
            .map_err(|why| refuse(&of_unstatable(&why)))?;
    let mut resource = documents
        .first()
        .map(|document| crate::consent::fhir::resource(release, document))
        .transpose()
        .map_err(|why| refuse(&of_unstatable(&why)))?
        .unwrap_or_default();
    if let Some(id) = id {
        resource.insert(String::from("id"), Value::String(id.to_owned()));
    }
    Ok(resource)
}

/// Returns the `searchset` Bundle of `entries`, in either release (R4 and
/// R5 `bundle.html`; `search.html` §3.1.1.5.2).
fn searchset(
    base: &str,
    self_link: &str,
    entries: Vec<(Option<String>, Object)>,
) -> Result<Object, Refusal> {
    let total: fhir_types::codec::Number = entries.len().to_string().parse().map_err(|_nan| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing("the match count is no JSON number"),
        )
    })?;
    let entry = entries
        .into_iter()
        .map(|(id, resource)| {
            let mut entry = Object::new();
            if let Some(id) = id {
                entry.insert(
                    String::from("fullUrl"),
                    Value::String(format!("{base}/{CONSENT}/{id}")),
                );
            }
            entry.insert(String::from("resource"), Value::Object(resource));
            entry.insert(
                String::from("search"),
                Value::Object(Object::from([(
                    String::from("mode"),
                    Value::String(String::from("match")),
                )])),
            );
            Value::Object(entry)
        })
        .collect();
    Ok(Object::from([
        (
            String::from("resourceType"),
            Value::String(String::from("Bundle")),
        ),
        (
            String::from("id"),
            Value::String(uuid::Uuid::new_v4().to_string()),
        ),
        (
            String::from("meta"),
            Value::Object(Object::from([(
                String::from("lastUpdated"),
                Value::String(now()),
            )])),
        ),
        (
            String::from("type"),
            Value::String(String::from("searchset")),
        ),
        (String::from("total"), Value::Number(total)),
        (
            String::from("link"),
            Value::Array(vec![Value::Object(Object::from([
                (
                    String::from("relation"),
                    Value::String(String::from("self")),
                ),
                (String::from("url"), Value::String(self_link.to_owned())),
            ]))]),
        ),
        (String::from("entry"), Value::Array(entry)),
    ]))
}
