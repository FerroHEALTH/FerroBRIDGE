// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-release reader and writer of the FHIR bodies.
//!
//! Each release has its own generated model, so the same code is generated
//! once per release by `per_release` and the functions below dispatch on
//! [`WireVersion`]. The request and response contracts come from
//! `fhir_types::stu3::operations`, `fhir_types::r4::operations` and
//! `fhir_types::r4b::operations`; this module writes no FHIR by hand.
//!
//! R4 and R4B declare identical `in` and `out` parameter sets for the five
//! operations (the vendored `OperationDefinition-CodeSystem-lookup`,
//! `-ConceptMap-translate`, `-ValueSet-validate-code`, `-ValueSet-expand` and
//! `-CodeSystem-subsumes` of both packages), so they share `r4_contracts`.
//! STU3 differs in four places, which `stu3_contracts` carries (the same
//! definitions in `hl7.fhir.r3.core` 3.0.2): `$validate-code` names the code
//! system version `version` and has no `valueSetVersion`, `$expand` has no
//! `valueSetVersion`, `$translate` has no `url`, and `$lookup` types
//! `property.value` as `code`. `$subsumes` is the same in all three. A parameter the request
//! carries and STU3 does not declare is refused before anything is sent.

use crate::config::WireVersion;
use crate::error::{BodyError, Error, Outcome};
use crate::outcome::{
    ExpandOutcome, LookupOutcome, Request, SubsumesOutcome, TranslateOutcome, ValidateOutcome,
};
use http::StatusCode;

/// The `HTTPVerb` code a batch entry carries for the `POST` form of an
/// operation (<https://hl7.org/fhir/R4/valueset-http-verb.html>).
const POST: &str = "POST";

/// The `BundleType` code of a request bundle whose entries are independent
/// (<https://hl7.org/fhir/R4/valueset-bundle-type.html>).
const BATCH: &str = "batch";

/// The `BundleType` code the server answers a `batch` with.
const BATCH_RESPONSE: &str = "batch-response";

/// The `BundleType` code the server answers a search with.
const SEARCHSET: &str = "searchset";

/// One entry of a `batch-response` `Bundle`, in request order.
#[derive(Debug)]
pub(crate) struct BatchEntry {
    /// The status `entry.response.status` names.
    pub(crate) status: StatusCode,
    /// `entry.resource`, written back as FHIR JSON.
    pub(crate) body: String,
}

/// Why a request could not be written in the configured release.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RequestError {
    /// The `Parameters` or the `Bundle` could not be written as JSON.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// The request carries a parameter the release's operation does not
    /// declare and has no equivalent for.
    #[error("the {operation} request carries `{parameter}`, which the release does not declare")]
    Unsupported {
        /// The operation, as `Resource/$code`.
        operation: &'static str,
        /// The parameter, as the R4 `OperationDefinition` names it.
        parameter: &'static str,
    },
    /// A numeric input is larger than the FHIR `integer` it is sent as.
    #[error("the {operation} request carries a `{parameter}` larger than a FHIR integer holds")]
    Range {
        /// The operation, as `Resource/$code`.
        operation: &'static str,
        /// The parameter, as the R4 `OperationDefinition` names it.
        parameter: &'static str,
        /// What the conversion reported.
        #[source]
        source: std::num::TryFromIntError,
    },
}

impl RequestError {
    /// Returns the client error for this failure of the `operation` request
    /// in `version`.
    pub(crate) fn into_error(self, operation: &'static str, version: WireVersion) -> Error {
        match self {
            Self::Json(source) => Error::RequestBody { operation, source },
            Self::Unsupported {
                operation,
                parameter,
            } => Error::UnsupportedParameter {
                operation,
                parameter,
                wire_version: version,
            },
            Self::Range {
                operation,
                parameter,
                source,
            } => Error::ParameterRange {
                operation,
                parameter,
                source,
            },
        }
    }
}

#[macro_use]
mod contracts;
#[macro_use]
mod release;

per_release!(stu3, stu3, stu3_contracts);
per_release!(r4, r4, r4_contracts);
per_release!(r4b, r4b, r4_contracts);

/// Returns the relative URL of `request`, the form a batch entry carries
/// (<https://hl7.org/fhir/R4/http.html#transaction>).
fn path_of(request: &Request) -> &'static str {
    match request {
        Request::Lookup { .. } => crate::LOOKUP,
        Request::Translate(_) => crate::TRANSLATE,
        Request::ValidateCode(_) => crate::VALIDATE_CODE,
        Request::Expand(_) => crate::EXPAND,
        Request::Subsumes(_) => crate::SUBSUMES,
    }
}

/// Returns `value` as the FHIR `integer` the `$expand` input `parameter` is
/// sent as, a signed 32-bit number
/// (<https://hl7.org/fhir/R4/datatypes.html#integer>).
fn integer(value: Option<u32>, parameter: &'static str) -> Result<Option<i32>, RequestError> {
    value
        .map(i32::try_from)
        .transpose()
        .map_err(|source| RequestError::Range {
            operation: crate::EXPAND,
            parameter,
            source,
        })
}

/// Returns the status an HTTP status line names, `None` when it names none.
///
/// `Bundle.entry.response.status` is "the status code returned by processing
/// this entry", written as the status line's code and optional reason phrase
/// (<https://hl7.org/fhir/R4/bundle-definitions.html#Bundle.entry.response.status>).
fn status_of(line: &str) -> Option<StatusCode> {
    let code = line.split_whitespace().next()?;
    StatusCode::from_bytes(code.as_bytes()).ok()
}

/// Returns the value of a primitive the operation declares as mandatory.
///
/// A FHIR primitive can be present with an extension and no value at all
/// (<https://hl7.org/fhir/R4/json.html#primitive>), so an answer whose
/// `display` states nothing is a defective answer, never an empty string.
fn required(value: Option<String>, parameter: &'static str) -> Result<String, BodyError> {
    value.ok_or(BodyError::MissingValue { parameter })
}

/// Returns the value of a mandatory `boolean` out parameter.
fn required_flag(value: Option<bool>, parameter: &'static str) -> Result<bool, BodyError> {
    value.ok_or(BodyError::MissingValue { parameter })
}

/// Returns the `OperationOutcome` that `body` carries, read in `version`.
pub(crate) fn outcome(version: WireVersion, body: &str) -> Result<Outcome, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::outcome(body),
        WireVersion::R4 => r4::outcome(body),
        WireVersion::R4B => r4b::outcome(body),
    }
}

/// Returns the parameter of `request` that `version` cannot carry.
pub(crate) const fn unsupported(version: WireVersion, request: &Request) -> Option<&'static str> {
    match version {
        WireVersion::Stu3 => stu3::unsupported_parameter(request),
        WireVersion::R4 => r4::unsupported_parameter(request),
        WireVersion::R4B => r4b::unsupported_parameter(request),
    }
}

/// Returns the FHIR JSON body that carries `request`, written in `version`.
pub(crate) fn request_body(
    version: WireVersion,
    request: &Request,
) -> Result<String, RequestError> {
    match version {
        WireVersion::Stu3 => stu3::request_body(request),
        WireVersion::R4 => r4::request_body(request),
        WireVersion::R4B => r4b::request_body(request),
    }
}

/// Returns the FHIR JSON `batch` Bundle that carries `requests`.
pub(crate) fn batch_body(
    version: WireVersion,
    requests: &[Request],
) -> Result<String, RequestError> {
    match version {
        WireVersion::Stu3 => stu3::batch_body(requests),
        WireVersion::R4 => r4::batch_body(requests),
        WireVersion::R4B => r4b::batch_body(requests),
    }
}

/// Returns the entries of the `batch-response` `body` carries.
pub(crate) fn batch_entries(
    version: WireVersion,
    body: &str,
) -> Result<Vec<BatchEntry>, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::batch_entries(body),
        WireVersion::R4 => r4::batch_entries(body),
        WireVersion::R4B => r4b::batch_entries(body),
    }
}

/// Returns the `CodeSystem/$lookup` answer `body` carries.
pub(crate) fn lookup_found(version: WireVersion, body: &str) -> Result<LookupOutcome, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::lookup_found(body),
        WireVersion::R4 => r4::lookup_found(body),
        WireVersion::R4B => r4b::lookup_found(body),
    }
}

/// Returns the `ConceptMap/$translate` answer `body` carries.
pub(crate) fn translate(version: WireVersion, body: &str) -> Result<TranslateOutcome, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::translate(body),
        WireVersion::R4 => r4::translate(body),
        WireVersion::R4B => r4b::translate(body),
    }
}

/// Returns the `ValueSet/$validate-code` answer `body` carries.
pub(crate) fn validate_code(
    version: WireVersion,
    body: &str,
) -> Result<ValidateOutcome, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::validate_code(body),
        WireVersion::R4 => r4::validate_code(body),
        WireVersion::R4B => r4b::validate_code(body),
    }
}

/// Returns the `ValueSet/$expand` answer `body` carries.
pub(crate) fn expanded(version: WireVersion, body: &str) -> Result<ExpandOutcome, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::expanded(body),
        WireVersion::R4 => r4::expanded(body),
        WireVersion::R4B => r4b::expanded(body),
    }
}

/// Returns the `CodeSystem/$subsumes` answer `body` carries.
pub(crate) fn subsumes(version: WireVersion, body: &str) -> Result<SubsumesOutcome, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::subsumes(body),
        WireVersion::R4 => r4::subsumes(body),
        WireVersion::R4B => r4b::subsumes(body),
    }
}

/// Returns the `Bundle.total` of the `searchset` `body` carries.
pub(crate) fn search_total(version: WireVersion, body: &str) -> Result<u32, BodyError> {
    match version {
        WireVersion::Stu3 => stu3::search_total(body),
        WireVersion::R4 => r4::search_total(body),
        WireVersion::R4B => r4b::search_total(body),
    }
}

#[cfg(test)]
mod tests {
    use super::{batch_body, request_body, status_of};
    use crate::config::WireVersion;
    use crate::outcome::Request;
    use http::StatusCode;

    fn lookup() -> Request {
        Request::Lookup {
            system: "http://example.org/cs".to_owned(),
            code: "a".to_owned(),
            version: Some("1.0.0".to_owned()),
            date: None,
        }
    }

    #[test]
    fn a_status_line_reads_its_code() {
        assert_eq!(Some(StatusCode::OK), status_of("200 OK"));
        assert_eq!(Some(StatusCode::BAD_REQUEST), status_of("400"));
        assert_eq!(None, status_of(""));
        assert_eq!(None, status_of("not a status"));
    }

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn both_releases_write_the_same_lookup_parameters() -> Result<(), super::RequestError> {
        let r4 = request_body(WireVersion::R4, &lookup())?;
        let r4b = request_body(WireVersion::R4B, &lookup())?;
        assert_eq!(r4, r4b);
        assert!(r4.contains(r#""name":"system""#), "{r4}");
        assert!(r4.contains(r#""name":"code""#), "{r4}");
        assert!(r4.contains(r#""name":"version""#), "{r4}");
        Ok(())
    }

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn a_batch_bundle_carries_one_post_entry_per_request() -> Result<(), super::RequestError> {
        let body = batch_body(WireVersion::R4, &[lookup(), lookup()])?;
        assert!(body.contains(r#""type":"batch""#), "{body}");
        assert_eq!(2, body.matches(r#""method":"POST""#).count(), "{body}");
        assert_eq!(
            2,
            body.matches(r#""url":"CodeSystem/$lookup""#).count(),
            "{body}"
        );
        Ok(())
    }
}
