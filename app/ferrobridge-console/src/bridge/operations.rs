// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bridge's operations the console's tools call.
//!
//! They are `$tofhir` and `$toopenehr` (the FHIRconnect REST API chapter, enveloped in the
//! FHIR R4 operations framework), and `$validate` on a resource type (FHIR
//! R4 `resource-operation-validate.html`).
//!
//! The operator's payload travels in the request body alone, never in a
//! query, and no line here logs it.

use fhir_types::codec::{Json as _, Path, Value};
use fhir_types::r4::operation_outcome::OperationOutcome;
use fhir_types::r4::parameters::Parameters;
use http::StatusCode;

use crate::bridge::{AccessToken, Bridge, BridgeError, FHIR_JSON};

impl Bridge {
    /// Runs `$tofhir`, `POST {base}fhir/$tofhir`, with `parameters` as the
    /// body, and reads the `Bundle` it answers as JSON.
    ///
    /// The answer is read as JSON, not decoded: the console's model carries
    /// the terminology root set, and a mapped `Bundle` holds clinical
    /// resources outside it.
    ///
    /// # Errors
    /// Returns [`BridgeError::Encode`] when `parameters` has no JSON form,
    /// [`BridgeError::Refused`] for any status but `200`, and the transport
    /// and body errors of [`Self::capability_statement`].
    pub async fn to_fhir(
        &self,
        parameters: &Parameters,
        token: &AccessToken,
    ) -> Result<serde_json::Value, BridgeError> {
        let body = encode(parameters)?;
        let url = self.url("fhir/$tofhir")?;
        let (_status, _headers, answer) = self
            .post(url, (FHIR_JSON, body), FHIR_JSON, token, &[StatusCode::OK])
            .await?;
        object(&answer)
    }

    /// Runs `$toopenehr`, `POST {base}fhir/$toopenehr`, with `envelope` as
    /// the body, and decodes the `Parameters` it answers.
    ///
    /// # Errors
    /// Returns [`BridgeError::Refused`] for any status but `200`,
    /// [`BridgeError::Decode`] for an answer that is no R4 `Parameters`, and
    /// the transport and body errors of [`Self::capability_statement`].
    pub async fn to_openehr(
        &self,
        envelope: &serde_json::Value,
        token: &AccessToken,
    ) -> Result<(Parameters, serde_json::Value), BridgeError> {
        let url = self.url("fhir/$toopenehr")?;
        let (_status, _headers, answer) = self
            .post(
                url,
                (FHIR_JSON, envelope.to_string()),
                FHIR_JSON,
                token,
                &[StatusCode::OK],
            )
            .await?;
        let raw = object(&answer)?;
        let Value::Object(object) = Value::from_serde_json(raw.clone()) else {
            return Err(BridgeError::NotAnObject);
        };
        let parameters =
            Parameters::from_json(&object, &mut Path::root("Parameters")).map_err(|source| {
                BridgeError::Decode {
                    expected: "Parameters",
                    source: Box::new(source),
                }
            })?;
        Ok((parameters, raw))
    }

    /// Runs `$validate` on `resource_type`, `POST
    /// {base}fhir/{resource_type}/$validate`, with `resource` as the body
    /// verbatim, and decodes the `OperationOutcome` it answers.
    ///
    /// # Errors
    /// Returns [`BridgeError::ResourceType`] for a type name that is not
    /// ASCII letters, before anything is sent; [`BridgeError::Refused`] for
    /// any status but `200`; [`BridgeError::Decode`] for an answer that is no
    /// R4 `OperationOutcome`; and the transport and body errors of
    /// [`Self::capability_statement`].
    pub async fn validate(
        &self,
        resource_type: &str,
        resource: String,
        token: &AccessToken,
    ) -> Result<(OperationOutcome, serde_json::Value), BridgeError> {
        // NOTE: FHIR R4 `resource.html`: a resource type name is letters
        // alone, so anything else is refused before a URL is built from it.
        if resource_type.is_empty() || !resource_type.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(BridgeError::ResourceType);
        }
        let mut url = self.url("fhir/")?;
        url.path_segments_mut()
            .map_err(|()| BridgeError::ResourceType)?
            .pop_if_empty()
            .extend([resource_type, "$validate"]);
        let (_status, _headers, answer) = self
            .post(
                url,
                (FHIR_JSON, resource),
                FHIR_JSON,
                token,
                &[StatusCode::OK],
            )
            .await?;
        let raw = object(&answer)?;
        let Value::Object(object) = Value::from_serde_json(raw.clone()) else {
            return Err(BridgeError::NotAnObject);
        };
        let outcome = OperationOutcome::from_json(&object, &mut Path::root("OperationOutcome"))
            .map_err(|source| BridgeError::Decode {
                expected: "OperationOutcome",
                source: Box::new(source),
            })?;
        Ok((outcome, raw))
    }
}

/// The JSON text of `parameters`.
fn encode(parameters: &Parameters) -> Result<String, BridgeError> {
    let object = parameters.to_json().map_err(|source| BridgeError::Encode {
        source: Box::new(source),
    })?;
    let value = Value::Object(object)
        .into_serde_json(&mut Path::root("Parameters"))
        .map_err(|source| BridgeError::Number {
            source: Box::new(source),
        })?;
    Ok(value.to_string())
}

/// The JSON object `body` holds.
fn object(body: &[u8]) -> Result<serde_json::Value, BridgeError> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|source| BridgeError::Payload { source })?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(BridgeError::NotAnObject)
    }
}
