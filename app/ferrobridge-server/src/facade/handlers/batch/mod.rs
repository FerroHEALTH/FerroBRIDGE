// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `POST [base]` with a `batch` Bundle: each entry processed and authorised
//! on its own.
//!
//! "In the case of a batch each entry is treated as if an individual
//! interaction or operation" (<https://hl7.org/fhir/R4/http.html#transaction>,
//! §3.1.0.11), so every entry runs through the handler of the interaction its
//! `request` names, under the same type-level scope check the guard runs on
//! that interaction's route. The read, vread, search, create and update of a
//! batch are the single interactions, called with the entry's resource as the
//! body and its `request` fields as the conditional headers. An entry that
//! fails answers its own status and `OperationOutcome` inside the
//! `batch-response`, and the Bundle answers `200` (§3.1.0.11.1). Only a Bundle
//! that breaks the batch rules themselves, an entry with no `request` or a
//! `POST` or `PUT` with no resource, is refused whole with `400`.

mod response_entry;

use std::collections::BTreeSet;
use std::time::Instant;

use axum::response::Response;
use ferrohealth_smart::guard::AnsweredAs;
use ferrohealth_smart::resource::Access;
use fhir_types::codec::Json as _;
use fhir_types::codec::Value;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::bundle::BundleEntry;
use http::HeaderMap;
use http::HeaderName;
use http::HeaderValue;
use http::Method;
use http::StatusCode;
use http::Uri;
use http::header;

use crate::facade::BASE_PATH;
use crate::facade::Facade;
use crate::facade::guard;
use crate::facade::guard::AuditedEntry;
use crate::facade::guard::Interaction;
use crate::facade::handlers::Refusal;
use crate::facade::handlers::search;
use crate::facade::handlers::write;
use crate::facade::media::FHIR_JSON;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;

/// The `Bundle.type` a processed batch answers with.
const RESPONSE: &str = "batch-response";

/// The headers of the batch request every entry carries as its own: the
/// answer the client accepts, its return preference, the request id and the
/// `templateId` pin.
const CARRIED: [&str; 4] = [
    "accept",
    "prefer",
    "x-request-id",
    "x-ferrobridge-template-id",
];

/// One entry of the batch, read.
#[derive(Debug)]
struct Entry<'a> {
    /// Its position in the Bundle, from zero.
    position: usize,
    /// The method its `request` names.
    method: Method,
    /// The URL its `request` names.
    url: &'a str,
    /// The resource it carries.
    resource: Option<&'a Value>,
    /// Its `fullUrl`.
    full_url: Option<&'a str>,
    /// The conditional headers its `request` carries, by header name.
    conditions: Vec<(HeaderName, &'a str)>,
}

/// The interaction one entry's `request` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call<'a> {
    /// `GET [type]/[id]`.
    Read(&'a str, &'a str),
    /// `GET [type]/[id]/_history/[vid]`.
    Vread(&'a str, &'a str, &'a str),
    /// `GET [type]?…`.
    Search(&'a str),
    /// `POST [type]`.
    Create(&'a str),
    /// `PUT [type]/[id]`.
    Update(&'a str, &'a str),
}

impl<'a> Call<'a> {
    /// Returns the interaction the guard checks and the audit line records.
    const fn interaction(self) -> Interaction {
        match self {
            Self::Read(..) => Interaction::Read,
            Self::Vread(..) => Interaction::Vread,
            Self::Search(_) => Interaction::Search,
            Self::Create(_) => Interaction::Create,
            Self::Update(..) => Interaction::Update,
        }
    }

    /// Returns the resource type the entry names.
    const fn resource_type(self) -> &'a str {
        match self {
            Self::Read(resource_type, _)
            | Self::Vread(resource_type, ..)
            | Self::Search(resource_type)
            | Self::Create(resource_type)
            | Self::Update(resource_type, _) => resource_type,
        }
    }

    /// Returns the logical id the entry names, when it names one.
    const fn id(self) -> Option<&'a str> {
        match self {
            Self::Read(_, id) | Self::Vread(_, id, _) | Self::Update(_, id) => Some(id),
            Self::Search(_) | Self::Create(_) => None,
        }
    }
}

/// Answers one `batch` Bundle.
pub(crate) async fn batch(
    facade: &Facade,
    access: &Access,
    headers: &HeaderMap,
    bundle: &Value,
) -> Result<Response, Refusal> {
    let entries = read_entries(bundle)?;
    let full_urls: BTreeSet<&str> = entries.iter().filter_map(|entry| entry.full_url).collect();
    let request_id = ferrohealth_server::request_id::of_client(headers).unwrap_or_default();
    let mut answered: Vec<Option<BundleEntry>> = vec![None; entries.len()];
    for entry in in_processing_order(&entries) {
        let started = Instant::now();
        let (call, response) = process(facade, access, headers, entry, &full_urls).await;
        let status = response.status();
        // NOTE: no specification governs this: our own design, each entry is
        // one access to one record, so each writes its own audit line (EHDS
        // Annex II 3.2(b)) beside the batch's, and none carries its content.
        guard::audit_entry(
            access,
            &AuditedEntry {
                position: entry.position,
                method: &entry.method,
                interaction: call.map_or(Interaction::Unclassified, Call::interaction),
                resource_type: call.map(Call::resource_type),
                id: call.and_then(Call::id),
                status,
            },
            started,
            request_id,
        );
        let built = response_entry::entry_of(response, &facade.settings().base_url).await;
        if let Some(slot) = answered.get_mut(entry.position) {
            *slot = Some(built);
        }
    }
    let failed = answered
        .iter()
        .flatten()
        .filter(|entry| {
            entry
                .response
                .as_ref()
                .and_then(|response| response.status.value.as_deref())
                .is_none_or(|status| !status.starts_with('2'))
        })
        .count();
    tracing::info!(
        entries = entries.len(),
        failed,
        "a batch Bundle was answered"
    );
    let response = Bundle {
        r#type: RESPONSE.into(),
        entry: answered.into_iter().flatten().collect(),
        ..Bundle::default()
    };
    let body = response.to_json().map_err(|error| {
        reply::refusal(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "the batch-response Bundle could not be encoded: {error}"
            )),
        )
    })?;
    let mut answer = reply::resource(StatusCode::OK, &body, &[]);
    answer
        .extensions_mut()
        .insert(AnsweredAs(Interaction::Batch.code()));
    Ok(answer)
}

/// Reads every entry of `bundle`, or refuses the Bundle with `400` naming each
/// entry that breaks the batch rules.
///
/// "Each entry SHALL carry request details", and "if the HTTP command is a
/// PUT or POST, then the entry SHALL contain a resource for the body of the
/// action" (<https://hl7.org/fhir/R4/http.html#transaction>, §3.1.0.11). A
/// Bundle that breaks either is no batch the server can process, so it is
/// refused whole rather than entry by entry (no specification governs the
/// refusal's status: our own design).
fn read_entries(bundle: &Value) -> Result<Vec<Entry<'_>>, Refusal> {
    let mut entries = Vec::new();
    let mut issues = Vec::new();
    let listed = bundle
        .get("entry")
        .and_then(Value::as_array)
        .unwrap_or_default();
    for (position, entry) in listed.iter().enumerate() {
        let request = entry.get("request");
        let method = request
            .and_then(|request| request.get("method"))
            .and_then(Value::as_str)
            .and_then(|method| Method::from_bytes(method.as_bytes()).ok());
        let url = request
            .and_then(|request| request.get("url"))
            .and_then(Value::as_str);
        let (Some(method), Some(url)) = (method, url) else {
            issues.push(
                Issue::error(IssueType::Required)
                    .diagnosing("each entry of a batch carries a request with its method and url")
                    .at(format!("Bundle.entry[{position}].request")),
            );
            continue;
        };
        let resource = entry.get("resource");
        if resource.is_none() && (method == Method::POST || method == Method::PUT) {
            issues.push(
                Issue::error(IssueType::Required)
                    .diagnosing(format!(
                        "a {method} entry of a batch carries the resource it sends"
                    ))
                    .at(format!("Bundle.entry[{position}].resource")),
            );
            continue;
        }
        let conditions = [
            (header::IF_MATCH, "ifMatch"),
            (header::IF_NONE_MATCH, "ifNoneMatch"),
            (header::IF_MODIFIED_SINCE, "ifModifiedSince"),
            (HeaderName::from_static("if-none-exist"), "ifNoneExist"),
        ]
        .into_iter()
        .filter_map(|(name, member)| {
            request
                .and_then(|request| request.get(member))
                .and_then(Value::as_str)
                .map(|value| (name, value))
        })
        .collect();
        entries.push(Entry {
            position,
            method,
            url,
            resource,
            full_url: entry.get("fullUrl").and_then(Value::as_str),
            conditions,
        });
    }
    if issues.is_empty() {
        Ok(entries)
    } else {
        Err(Refusal::new(reply::issues(
            StatusCode::BAD_REQUEST,
            &issues,
            &[],
        )))
    }
}

/// Returns `entries` in the order a server executes a batch: every `DELETE`,
/// then every `POST`, every `PUT` and `PATCH`, then every `GET` and `HEAD`,
/// each kept in Bundle order.
///
/// "It is considered that servers execute the batch in the same order as that
/// specified below for transactions"
/// (<https://hl7.org/fhir/R4/http.html#transaction>, §3.1.0.11.1 and
/// §3.1.0.11.2); the response keeps the Bundle's order whatever the
/// execution order (§3.1.0.11.3).
fn in_processing_order<'e, 'a>(entries: &'e [Entry<'a>]) -> Vec<&'e Entry<'a>> {
    let mut ordered: Vec<&Entry<'a>> = entries.iter().collect();
    ordered.sort_by_key(|entry| match entry.method.as_str() {
        "DELETE" => 0_u8,
        "POST" => 1,
        "PUT" | "PATCH" => 2,
        "GET" | "HEAD" => 3,
        _ => 4,
    });
    ordered
}

/// Runs one entry and returns the interaction it named and its answer.
async fn process<'a>(
    facade: &Facade,
    access: &Access,
    headers: &HeaderMap,
    entry: &'a Entry<'a>,
    full_urls: &BTreeSet<&str>,
) -> (Option<Call<'a>>, Response) {
    let (path, query) = match relative(entry.url, &facade.settings().base_url) {
        Ok(relative) => relative
            .split_once('?')
            .map_or((relative, None), |(path, query)| (path, Some(query))),
        Err(refusal) => return (None, refusal.into_response()),
    };
    let call = match call_of(&entry.method, path) {
        Ok(call) => call,
        Err(refusal) => return (None, refusal.into_response()),
    };
    let answered = run(
        facade,
        access,
        headers,
        entry,
        call,
        (path, query),
        full_urls,
    )
    .await;
    (Some(call), answered.unwrap_or_else(Refusal::into_response))
}

/// Runs `call` for `entry` through the handler of its interaction.
async fn run(
    facade: &Facade,
    access: &Access,
    headers: &HeaderMap,
    entry: &Entry<'_>,
    call: Call<'_>,
    (path, query): (&str, Option<&str>),
    full_urls: &BTreeSet<&str>,
) -> Result<Response, Refusal> {
    if let Some(resource) = entry.resource {
        refuse_dependency(entry, resource, full_urls)?;
    }
    guard::admit(access, call.interaction(), call.resource_type())?;
    let entry_headers = headers_of(headers, entry)?;
    let uri = uri_of(path, query)?;
    let body = match entry.resource {
        Some(resource) => serde_json::to_vec(resource).map_err(|error| {
            reply::refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(format!(
                    "the entry's resource could not be serialized: {error}"
                )),
            )
        })?,
        None => Vec::new(),
    };
    match call {
        Call::Read(resource_type, id) => {
            super::read(facade, access, (resource_type, id), &entry_headers, &uri).await
        }
        Call::Vread(resource_type, id, vid) => {
            super::vread(
                facade,
                access,
                (resource_type, id, vid),
                &entry_headers,
                &uri,
            )
            .await
        }
        Call::Search(resource_type) => {
            search::search_get(facade, access, resource_type, &entry_headers, &uri).await
        }
        Call::Create(resource_type) => {
            write::create(facade, access, resource_type, &entry_headers, &uri, &body).await
        }
        Call::Update(resource_type, id) => {
            super::update(
                facade,
                access,
                (resource_type, id),
                &entry_headers,
                &uri,
                &body,
            )
            .await
        }
    }
}

/// Returns `url` relative to the service base, or the `400` of a URL that
/// names another server.
///
/// "The URL for this entry, relative to the root (the address to which the
/// request is posted)" (<https://hl7.org/fhir/R4/bundle-definitions.html#Bundle.entry.request.url>);
/// an absolute URL under this server's base reads as the same relative one.
fn relative<'a>(url: &'a str, base_url: &str) -> Result<&'a str, Refusal> {
    let base = base_url.trim_end_matches('/');
    if let Some(rest) = url.strip_prefix(base)
        && (rest.is_empty() || rest.starts_with('/') || rest.starts_with('?'))
    {
        return Ok(rest.trim_start_matches('/'));
    }
    if url.contains("://") {
        return Err(reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::NotSupported)
                .diagnosing(format!(
                    "the entry's request.url names a server other than {base}"
                ))
                .at("Bundle.entry.request.url"),
        ));
    }
    Ok(url.trim_start_matches('/'))
}

/// Returns the interaction `method` on `path` names, or the refusal its
/// route would answer outside a batch.
///
/// A method the facade serves on no route is `405` and a path it routes for
/// no method is `404`, as the router answers both
/// (<https://hl7.org/fhir/R4/http.html#general>).
fn call_of<'a>(method: &Method, path: &'a str) -> Result<Call<'a>, Refusal> {
    let segments: Vec<&str> = path.split('/').collect();
    let call = match (method.as_str(), segments.as_slice()) {
        ("GET", [resource_type, id, "_history", vid]) => Some(Call::Vread(resource_type, id, vid)),
        ("GET", [resource_type, id]) => Some(Call::Read(resource_type, id)),
        ("GET", [resource_type]) => Some(Call::Search(resource_type)),
        ("POST", [resource_type]) => Some(Call::Create(resource_type)),
        ("PUT", [resource_type, id]) => Some(Call::Update(resource_type, id)),
        ("GET" | "POST" | "PUT", _) => None,
        _ => {
            return Err(reply::refusal(
                StatusCode::METHOD_NOT_ALLOWED,
                Issue::error(IssueType::NotSupported).diagnosing(format!(
                    "this server answers a batch entry's GET, POST and PUT; {method} is not among the interactions its CapabilityStatement declares"
                )),
            ));
        }
    };
    call.filter(|call| !call.resource_type().is_empty() && call.id() != Some(""))
        .ok_or_else(|| {
            reply::refusal(
                StatusCode::NOT_FOUND,
                Issue::error(IssueType::NotSupported).diagnosing(format!(
                    "a batch entry's {method} of `{path}` names no interaction this server answers"
                )),
            )
        })
}

/// Refuses an entry whose resource references another entry of the batch.
///
/// "References within a Bundle.entry.resource to another Bundle.entry.resource
/// that is being created within the batch are considered to be
/// non-conformant" (<https://hl7.org/fhir/R4/http.html#transaction>,
/// §3.1.0.11.1). The refusal falls on the referencing entry alone, so the
/// others still run (no specification governs which entry answers: our own
/// design).
fn refuse_dependency(
    entry: &Entry<'_>,
    resource: &Value,
    full_urls: &BTreeSet<&str>,
) -> Result<(), Refusal> {
    let Some(target) = reference_into(resource, full_urls, entry.full_url) else {
        return Ok(());
    };
    Err(reply::refusal(
        StatusCode::BAD_REQUEST,
        Issue::error(IssueType::Invalid)
            .diagnosing(format!(
                "the entry references {target}, the fullUrl of another entry of this batch; the entries of a batch are independent of each other"
            ))
            .at(format!("Bundle.entry[{}].resource", entry.position)),
    ))
}

/// Returns the first `reference` inside `value` that is one of `full_urls`
/// other than `own`.
fn reference_into<'v>(
    value: &'v Value,
    full_urls: &BTreeSet<&str>,
    own: Option<&str>,
) -> Option<&'v str> {
    match value {
        Value::Object(members) => members.iter().find_map(|(name, member)| {
            if name == "reference"
                && let Some(text) = member.as_str()
                && Some(text) != own
                && full_urls.contains(text)
            {
                return Some(text);
            }
            reference_into(member, full_urls, own)
        }),
        Value::Array(items) => items
            .iter()
            .find_map(|item| reference_into(item, full_urls, own)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

/// Returns the headers one entry's interaction runs with: the [`CARRIED`]
/// headers of the batch, the media type of its resource, and the conditional
/// headers its `request` names.
///
/// The return preference of the batch reaches every entry, so each answers
/// the body its single interaction would: "a resource may be included in the
/// entry, as specified by the Prefer header"
/// (<https://hl7.org/fhir/R4/http.html#transaction-response>).
fn headers_of(batch: &HeaderMap, entry: &Entry<'_>) -> Result<HeaderMap, Refusal> {
    let mut headers = HeaderMap::new();
    for name in CARRIED {
        for value in batch.get_all(name) {
            headers.append(HeaderName::from_static(name), value.clone());
        }
    }
    if entry.resource.is_some() {
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(FHIR_JSON));
    }
    for (name, value) in &entry.conditions {
        let value = HeaderValue::from_str(value).map_err(|_unrepresentable| {
            reply::refusal(
                StatusCode::BAD_REQUEST,
                Issue::error(IssueType::Invalid)
                    .diagnosing(format!(
                        "the entry's {name} cannot travel as an HTTP header value"
                    ))
                    .at(format!("Bundle.entry[{}].request", entry.position)),
            )
        })?;
        headers.insert(name.clone(), value);
    }
    Ok(headers)
}

/// Returns the URI of one entry's interaction under [`BASE_PATH`].
fn uri_of(path: &str, query: Option<&str>) -> Result<Uri, Refusal> {
    let text = match query {
        Some(query) => format!("{BASE_PATH}/{path}?{query}"),
        None => format!("{BASE_PATH}/{path}"),
    };
    text.parse().map_err(|error: http::uri::InvalidUri| {
        reply::refusal(
            StatusCode::BAD_REQUEST,
            Issue::error(IssueType::Invalid)
                .diagnosing(format!(
                    "the entry's request.url is not a legal URL: {error}"
                ))
                .at("Bundle.entry.request.url"),
        )
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use fhir_types::codec::Value;
    use http::Method;
    use http::StatusCode;

    use super::Call;
    use super::call_of;
    use super::reference_into;
    use super::relative;

    #[test]
    fn each_request_shape_names_its_interaction() {
        assert_eq!(
            Call::Read("Condition", "c-1"),
            call_of(&Method::GET, "Condition/c-1").expect("a read")
        );
        assert_eq!(
            Call::Vread("Condition", "c-1", "2"),
            call_of(&Method::GET, "Condition/c-1/_history/2").expect("a vread")
        );
        assert_eq!(
            Call::Search("Condition"),
            call_of(&Method::GET, "Condition").expect("a search")
        );
        assert_eq!(
            Call::Create("Condition"),
            call_of(&Method::POST, "Condition").expect("a create")
        );
        assert_eq!(
            Call::Update("Condition", "c-1"),
            call_of(&Method::PUT, "Condition/c-1").expect("an update")
        );
    }

    #[test]
    fn an_unserved_method_is_four_hundred_and_five_and_an_unrouted_path_four_hundred_and_four() {
        let delete = call_of(&Method::DELETE, "Condition/c-1").expect_err("not served");
        assert_eq!(
            StatusCode::METHOD_NOT_ALLOWED,
            delete.into_response().status()
        );
        for (method, path) in [
            (Method::GET, "Condition/c-1/_history"),
            (Method::POST, "Condition/c-1"),
            (Method::PUT, "Condition"),
            (Method::GET, ""),
        ] {
            let refused = call_of(&method, path).expect_err("not routed");
            assert_eq!(
                StatusCode::NOT_FOUND,
                refused.into_response().status(),
                "{method} {path}"
            );
        }
    }

    #[test]
    fn a_url_under_this_base_reads_as_relative_and_another_server_is_refused() {
        let base = "http://ferrobridge.invalid/fhir/";
        assert_eq!(
            "Condition/c-1",
            relative("http://ferrobridge.invalid/fhir/Condition/c-1", base).expect("ours")
        );
        assert_eq!(
            "Condition?code=x",
            relative("Condition?code=x", base).expect("relative")
        );
        assert!(relative("http://elsewhere.invalid/fhir/Condition/c-1", base).is_err());
        assert!(relative("http://ferrobridge.invalid/fhirx/Condition", base).is_err());
    }

    #[test]
    fn a_reference_to_another_entry_is_found_at_any_depth() {
        let resource: Value = serde_json::from_str(
            r#"{"resourceType":"Condition","evidence":[{"detail":[{"reference":"urn:uuid:other"}]}]}"#,
        )
        .expect("JSON");
        let urls = BTreeSet::from(["urn:uuid:other", "urn:uuid:own"]);
        assert_eq!(
            Some("urn:uuid:other"),
            reference_into(&resource, &urls, Some("urn:uuid:own"))
        );
        assert_eq!(
            None,
            reference_into(&resource, &urls, Some("urn:uuid:other"))
        );
    }
}
