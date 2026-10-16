// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One `batch-response` entry, read off the answer of the interaction its
//! request entry ran.

use axum::response::Response;
use fhir_types::codec::Json as _;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::bundle::BundleEntry;
use fhir_types::r4::bundle::BundleEntryResponse;
use fhir_types::r4::resource::Resource;
use http::HeaderName;
use http::StatusCode;
use http::header;

use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;

/// Returns the `batch-response` entry of one interaction's answer.
///
/// The entry carries the status, the `Location` and the `ETag` the
/// interaction answered, and its body: a resource on success, the
/// `OperationOutcome` in `response.outcome` otherwise or when the client
/// asked for one (<https://hl7.org/fhir/R4/http.html#transaction-response>).
pub(super) async fn entry_of(response: Response, base_url: &str) -> BundleEntry {
    let (parts, body) = response.into_parts();
    let header_of = |name: HeaderName| {
        parts
            .headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let mut answered = BundleEntryResponse {
        status: status_line(parts.status).into(),
        location: header_of(header::LOCATION).map(Into::into),
        etag: header_of(header::ETAG).map(Into::into),
        ..BundleEntryResponse::default()
    };
    let resource = match resource_of(body).await {
        Ok(resource) => resource,
        Err(issue) => return failed(StatusCode::INTERNAL_SERVER_ERROR, issue),
    };
    let mut entry = BundleEntry::default();
    match resource {
        Some(Resource::OperationOutcome(outcome)) => {
            answered.outcome = Some(Resource::OperationOutcome(outcome));
        }
        Some(resource) if parts.status.is_success() => {
            entry.full_url = full_url_of(&resource, base_url).map(Into::into);
            entry.resource = Some(resource);
        }
        Some(_) | None => {}
    }
    entry.response = Some(answered);
    entry
}

/// Reads one interaction's body as the resource it carries, `None` for an
/// empty body.
async fn resource_of(body: axum::body::Body) -> Result<Option<Resource>, Issue> {
    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|error| {
            Issue::error(IssueType::Exception).diagnosing(format!(
                "the entry's answer could not be read: {}",
                crate::facade::outcome::chain(&error)
            ))
        })?;
    if bytes.is_empty() {
        return Ok(None);
    }
    let unreadable = |detail: String| {
        Issue::error(IssueType::Exception)
            .diagnosing(format!("the entry's answer is no R4 resource: {detail}"))
    };
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|error| unreadable(error.to_string()))?;
    let Value::Object(object) = value else {
        return Err(unreadable(String::from("it is not a JSON object")));
    };
    let resource_type = object
        .get("resourceType")
        .and_then(Value::as_str)
        .unwrap_or("Resource")
        .to_owned();
    Resource::from_json(&object, &mut Path::root(&resource_type))
        .map(Some)
        .map_err(|error| unreadable(crate::facade::outcome::chain(&error)))
}

/// Returns the `fullUrl` of a resource an entry answered, when it names its
/// own type and id.
///
/// A searchset carries its matches' URLs itself, so it gets none.
fn full_url_of(resource: &Resource, base_url: &str) -> Option<String> {
    let object = resource.to_json().ok()?;
    let resource_type = object.get("resourceType").and_then(Value::as_str)?;
    if resource_type == "Bundle" {
        return None;
    }
    let id = object.get("id").and_then(Value::as_str)?;
    Some(format!(
        "{}/{resource_type}/{id}",
        base_url.trim_end_matches('/')
    ))
}

/// Returns the entry of an answer the batch could not carry.
fn failed(status: StatusCode, issue: Issue) -> BundleEntry {
    BundleEntry {
        response: Some(BundleEntryResponse {
            status: status_line(status).into(),
            outcome: Some(Resource::OperationOutcome(Box::new(
                crate::facade::outcome::outcome(&[issue]),
            ))),
            ..BundleEntryResponse::default()
        }),
        ..BundleEntry::default()
    }
}

/// Returns the `response.status` of `status`: "the status code returned by
/// processing this entry … SHALL start with a 3 digit HTTP code"
/// (<https://hl7.org/fhir/R4/bundle-definitions.html#Bundle.entry.response.status>).
fn status_line(status: StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("{} {reason}", status.as_str()),
        None => status.as_str().to_owned(),
    }
}
#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::status_line;

    #[test]
    fn a_status_line_starts_with_its_three_digit_code() {
        assert_eq!("201 Created", status_line(StatusCode::CREATED));
        assert_eq!("403 Forbidden", status_line(StatusCode::FORBIDDEN));
    }
}
