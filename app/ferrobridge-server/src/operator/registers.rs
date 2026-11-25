// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The runtime settings registers on the operator surface: `GET
//! /operator/registers/{register}`, `PUT /operator/registers/{register}`
//! and `GET /operator/registers/{register}/history`.
//!
//! A read answers the version in force with the version as its `ETag`. A
//! write replaces the whole document and names, in `If-Match`, the version it
//! replaces; a writer that read an older version answers `412` (RFC 9110
//! §13.1.1). The audit line names the caller, the register and the two
//! versions, never the document. No specification governs the operator
//! surface: our own design.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{FromRequestParts, Path};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use ferrohealth_smart::resource::Access;
use http::request::Parts;
use http::{HeaderMap, StatusCode, header};

use crate::operator::guard::audited;
use crate::operator::lane::Lane;
use crate::operator::refused::Refused;
use crate::operator::save::{Precondition, subject_of};
use crate::problem::Kind;
use crate::settings::{Register, RegisterError, Registers, Version, Written};

/// One version of a register as the operator API answers it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Answered {
    /// The register's name.
    pub register: &'static str,
    /// The version; `0` is the default.
    pub version: i64,
    /// The document.
    pub document: serde_json::Value,
    /// Who committed it; absent for the default.
    pub committer: Option<String>,
    /// When it was committed, RFC 3339 UTC; absent for the default.
    pub committed_at: Option<String>,
}

impl Answered {
    /// Returns `version` of `register` as the API answers it.
    fn of(register: Register, version: &Version) -> Self {
        Self {
            register: register.name(),
            version: version.number,
            document: version.document.clone(),
            committer: version.committer.clone(),
            committed_at: version.committed_at.clone(),
        }
    }
}

/// Returns the register routes over `registers`, each guarded by the gate of
/// `smart` and its operator scope.
pub fn routes(registers: Option<Arc<Registers>>, smart: &crate::smart::Smart) -> Router {
    Router::new()
        .route("/operator/registers/{register}", get(read).put(write))
        .route("/operator/registers/{register}/history", get(history))
        .with_state(registers)
        .route_layer(crate::operator::guard::layer(smart))
}

/// The registers and the register a request's path names.
#[derive(Debug)]
struct Named(Arc<Registers>, Register);

impl FromRequestParts<Option<Arc<Registers>>> for Named {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        registers: &Option<Arc<Registers>>,
    ) -> Result<Self, Self::Rejection> {
        let Path(name) = Path::<String>::from_request_parts(parts, registers)
            .await
            .map_err(IntoResponse::into_response)?;
        let Lane(registers) = Lane::from_request_parts(parts, registers)
            .await
            .map_err(IntoResponse::into_response)?;
        let register = Register::of_name(&name).ok_or_else(|| {
            Refused::Problem(Kind::NotFound, "no settings register has this name").into_response()
        })?;
        Ok(Self(registers, register))
    }
}

/// `GET /operator/registers/{register}`: the version in force.
async fn read(Named(registers, register): Named) -> Response {
    match registers.current(register).await {
        Ok(version) => answer(StatusCode::OK, register, &version),
        Err(error) => unavailable(&error),
    }
}

/// `GET /operator/registers/{register}/history`: every committed version,
/// newest first.
async fn history(Named(registers, register): Named) -> Response {
    match registers.history(register).await {
        Ok(versions) => axum::Json(
            versions
                .iter()
                .map(|version| Answered::of(register, version))
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => unavailable(&error),
    }
}

/// `PUT /operator/registers/{register}`: appends the body as the version
/// after the one `If-Match` names.
async fn write(
    Named(registers, register): Named,
    access: Access,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = subject_of(&access).to_owned();
    let (replaces, response) = match (Precondition::of(&headers), replaced(&headers)) {
        (Precondition::Missing | Precondition::Absent, _) => (
            None,
            Refused::Problem(
                Kind::PreconditionRequired,
                "send If-Match with the version the write replaces, as the read's ETag gave it",
            )
            .into_response(),
        ),
        (Precondition::Match(_), None) => (
            None,
            Refused::Problem(
                Kind::PreconditionFailed,
                "If-Match names no version of the register",
            )
            .into_response(),
        ),
        (Precondition::Match(_), Some(replaces)) => (
            Some(replaces),
            committed(&registers, register, replaces, &body, &subject).await,
        ),
    };
    audited!(
        subject.as_str(),
        "register",
        response.status(),
        register = register.name(),
        replaces = replaces.unwrap_or(-1),
    );
    response
}

/// Writes `body` as the version after `replaces` and answers the outcome.
async fn committed(
    registers: &Registers,
    register: Register,
    replaces: i64,
    body: &[u8],
    subject: &str,
) -> Response {
    let document: serde_json::Value = match serde_json::from_slice(body) {
        Ok(document) => document,
        Err(_unreadable) => {
            return Refused::Problem(Kind::BadRegister, "the body is no JSON document")
                .into_response();
        }
    };
    match registers
        .write(register, replaces, &document, subject)
        .await
    {
        Ok(Written::Committed(version)) => answer(StatusCode::OK, register, &version),
        Ok(Written::Stale { .. }) => Refused::Problem(
            Kind::PreconditionFailed,
            "the register changed since the version If-Match names",
        )
        .into_response(),
        Err(RegisterError::Shape(_) | RegisterError::Categories(_)) => Refused::Problem(
            Kind::BadRegister,
            "the document is not the register's shape, or its rules do not hold",
        )
        .into_response(),
        Err(error) => unavailable(&error),
    }
}

/// Returns the version `If-Match` names, `None` for anything but one quoted
/// decimal number.
fn replaced(headers: &HeaderMap) -> Option<i64> {
    let value = headers.get(header::IF_MATCH)?.to_str().ok()?.trim();
    let digits = value.strip_prefix('"')?.strip_suffix('"')?;
    if digits.is_empty() || !digits.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Answers `version` of `register` with `status` and the version as the
/// `ETag`.
fn answer(status: StatusCode, register: Register, version: &Version) -> Response {
    (
        status,
        [(header::ETAG, format!("\"{}\"", version.number))],
        axum::Json(Answered::of(register, version)),
    )
        .into_response()
}

/// Returns the `503` of a register the store could not answer, its cause
/// logged.
fn unavailable(error: &RegisterError) -> Response {
    tracing::error!(
        error = ferrohealth_server::run::chain(error),
        "a settings register could not be read or written"
    );
    Refused::Problem(
        Kind::RegistersUnavailable,
        "the settings register could not be read or written",
    )
    .into_response()
}

#[cfg(test)]
mod tests {
    use http::{HeaderMap, HeaderValue, header};

    use super::replaced;

    #[test]
    fn if_match_names_a_version_only_as_one_quoted_number() {
        let named = |value: &'static str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::IF_MATCH, HeaderValue::from_static(value));
            replaced(&headers)
        };
        assert_eq!(Some(3), named("\"3\""));
        assert_eq!(Some(0), named(" \"0\" "));
        for value in ["3", "\"\"", "\"-1\"", "*", "\"3\", \"4\"", "W/\"3\""] {
            assert_eq!(None, named(value), "{value}");
        }
    }
}
