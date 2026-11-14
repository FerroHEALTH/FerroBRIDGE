// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `EHR_ACCESS` of an EHR: its read, its commit as a CONTRIBUTION, and
//! the controller's `ACCESS_POLICY` its directives draw on.
//!
//! ITS-REST 1.1.0 publishes no `EHR_ACCESS` endpoint and its generated
//! `Versionable` names no `EHR_ACCESS`, so these calls go out as raw requests
//! through `CdrClient::extension`. The read is the CDR's extension shaped on
//! `ehr_status_get_at_time` (`GET /ehr/{ehr_id}/ehr_access`, the version uid
//! as the `ETag`), the commit the ITS-REST `contribution_create` operation
//! carrying the `EHR_ACCESS` version (`ehr-codegen.openapi.yaml`,
//! `NewContribution`), and the policy read the CDR's administrative
//! extension (`GET /admin/access_policy`). No specification governs the two
//! extension reads: our own design, after the routes of the reference CDR.
//!
//! openEHR is a registered trademark of the openEHR Foundation.

use std::collections::BTreeMap;

use http::HeaderValue;
use http::Method;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::client::Request;
use openehr_its::rest::client::path_segment;
use openehr_its::rest::generated::common::UpdateAudit;
use openehr_its::rest::generated::common::UpdateVersion;
use openehr_its::rest::generated::ehr::NewContribution;
use openehr_rm::draft::access_control::access_control::access_policy::AccessPolicy;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
use openehr_rm::v1_2::ehr::ehr_access::EhrAccess;

use crate::cdr::CdrClient;
use crate::cdr::Prefer;
use crate::cdr::error::CdrError;
use crate::cdr::error::Upstream;
use crate::cdr::ids::ContributionUid;
use crate::cdr::ids::EhrId;

/// The `ETag` response header.
const ETAG: &str = "etag";

/// Why an `EHR_ACCESS` or `ACCESS_POLICY` call reached no answer the bridge
/// can use.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AccessError {
    /// The call never reached a documented answer.
    #[error("the CDR call did not complete")]
    Cdr(#[from] CdrError),
    /// The CDR holds no such EHR, or serves no `EHR_ACCESS` for it.
    #[error("the CDR answered {upstream} for the EHR")]
    NotFound {
        /// What the CDR answered.
        upstream: Box<Upstream>,
    },
    /// The commit named a version that is no longer the latest.
    #[error("the CDR refused the commit as stale: {upstream}")]
    Conflict {
        /// What the CDR answered.
        upstream: Box<Upstream>,
    },
    /// The CDR refused the content: an invariant, an unknown party.
    #[error("the CDR refused the access directives: {upstream}")]
    Refused {
        /// What the CDR answered.
        upstream: Box<Upstream>,
    },
    /// The CDR refused the bridge's credential or its grant.
    #[error("the CDR refused the call: {upstream}")]
    Denied {
        /// What the CDR answered.
        upstream: Box<Upstream>,
    },
    /// The CDR answered a status the call does not document.
    #[error("the CDR answered {upstream}, which the call does not document")]
    Undocumented {
        /// What the CDR answered.
        upstream: Box<Upstream>,
    },
    /// The body is not the canonical JSON of the class the call answers.
    #[error("the {operation} answer is no canonical JSON {class}")]
    Body {
        /// The call.
        operation: &'static str,
        /// The RM class the body should be.
        class: &'static str,
        /// What the decoder reported.
        #[source]
        source: openehr_its::json::JsonParseError,
    },
    /// The contribution could not be written as JSON.
    #[error("the contribution could not be written")]
    Request(#[source] openehr_its::rest::client::ClientError),
}

impl AccessError {
    /// Returns the upstream answer the error carries, when it carries one.
    #[must_use]
    pub fn upstream(&self) -> Option<&Upstream> {
        match self {
            Self::NotFound { upstream }
            | Self::Conflict { upstream }
            | Self::Refused { upstream }
            | Self::Denied { upstream }
            | Self::Undocumented { upstream } => Some(upstream),
            Self::Cdr(_) | Self::Body { .. } | Self::Request(_) => None,
        }
    }
}

/// The current `EHR_ACCESS` of one EHR.
#[derive(Debug, Clone)]
pub struct CurrentAccess {
    /// The version the read answered, which a commit names as preceding.
    pub version: ObjectVersionId,
    /// The `EHR_ACCESS` itself.
    pub access: EhrAccess,
}

/// The refusal a status every call shares means, or the answer back for the
/// call to read.
fn shared(upstream: Upstream) -> Result<Upstream, AccessError> {
    let status = upstream.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(AccessError::Denied {
            upstream: Box::new(upstream),
        });
    }
    if status == StatusCode::NOT_FOUND {
        return Err(AccessError::NotFound {
            upstream: Box::new(upstream),
        });
    }
    Ok(upstream)
}

impl CdrClient {
    /// Reads the current `EHR_ACCESS` of the EHR `ehr_id` and the version
    /// its `ETag` names (`GET /ehr/{ehr_id}/ehr_access`, the CDR's extension
    /// shaped on ITS-REST `ehr_status_get_at_time`).
    ///
    /// # Errors
    /// Returns [`AccessError::NotFound`] for a `404`, [`AccessError::Denied`]
    /// for a `401` or `403`, [`AccessError::Cdr`] for a `200` with no usable
    /// `ETag`, [`AccessError::Body`] for a body the bridge cannot read, [`AccessError::Undocumented`] for
    /// any other status, and [`AccessError::Cdr`] when the call never reached
    /// the CDR.
    pub async fn ehr_access(&self, ehr_id: &EhrId) -> Result<CurrentAccess, AccessError> {
        const OPERATION: &str = "ehr_access_get";
        let request = Request::new(
            Method::GET,
            format!("/ehr/{}/ehr_access", path_segment(&ehr_id.as_str())),
        );
        let upstream = shared(self.extension(self.base_url(), request).await?)?;
        if upstream.status() != StatusCode::OK {
            return Err(AccessError::Undocumented {
                upstream: Box::new(upstream),
            });
        }
        let version = crate::cdr::version_from_etag(
            OPERATION,
            StatusCode::OK,
            upstream.header(ETAG).as_deref(),
        )?;
        let access = openehr_its::json::from_canonical_json(upstream.body()).map_err(|source| {
            AccessError::Body {
                operation: OPERATION,
                class: "EHR_ACCESS",
                source,
            }
        })?;
        Ok(CurrentAccess { version, access })
    }

    /// Commits `access` as the version of the `EHR_ACCESS` of `ehr_id` that
    /// follows `preceding`, in a CONTRIBUTION of its own
    /// (`ehr-codegen.openapi.yaml`, `contribution_create`).
    ///
    /// The `EHR_ACCESS` is one versioned object per EHR (openEHR RM EHR IM,
    /// the `EHR` class), so the version is a modification whose
    /// `preceding_version_uid` is `preceding`, and the CDR refuses it once
    /// another version came first. The call is a `POST`, never retried.
    ///
    /// # Errors
    /// Returns [`AccessError::Conflict`] for a `409` or `412`,
    /// [`AccessError::Refused`] for a `400` or `422`, the shared refusals of
    /// [`CdrClient::ehr_access`], and [`AccessError::Cdr`] for a success
    /// that names no contribution.
    pub async fn commit_ehr_access(
        &self,
        ehr_id: &EhrId,
        preceding: &ObjectVersionId,
        access: &EhrAccess,
        lifecycle_state: DvCodedText,
        audit: UpdateAudit,
    ) -> Result<ContributionUid, AccessError> {
        let contribution = NewContribution {
            uid: None,
            versions: vec![UpdateVersion {
                preceding_version_uid: Some(preceding.clone()),
                signature: None,
                lifecycle_state,
                attestations: None,
                data: Some(access.clone()),
                commit_audit: audit.clone(),
                additional_properties: BTreeMap::new(),
            }],
            audit,
            additional_properties: BTreeMap::new(),
        };
        let mut request = Request::new(
            Method::POST,
            format!("/ehr/{}/contribution", path_segment(&ehr_id.as_str())),
        );
        request
            .json_body(&contribution, None)
            .map_err(AccessError::Request)?;
        request
            .headers_mut()
            .insert("prefer", HeaderValue::from_static(Prefer::Minimal.as_str()));
        let upstream = shared(self.extension(self.base_url(), request).await?)?;
        let status = upstream.status();
        if status == StatusCode::CONFLICT || status == StatusCode::PRECONDITION_FAILED {
            return Err(AccessError::Conflict {
                upstream: Box::new(upstream),
            });
        }
        if status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY {
            return Err(AccessError::Refused {
                upstream: Box::new(upstream),
            });
        }
        if status != StatusCode::CREATED && status != StatusCode::NO_CONTENT {
            return Err(AccessError::Undocumented {
                upstream: Box::new(upstream),
            });
        }
        Ok(crate::cdr::contribution_uid_from_etag(
            "contribution_create",
            status,
            upstream.header(ETAG).as_deref(),
        )?)
    }

    /// Reads the controller's `ACCESS_POLICY` in force under `admin_root`
    /// (`GET /admin/access_policy`, the CDR's administrative extension), or
    /// `None` when the CDR holds none.
    ///
    /// # Errors
    /// Returns [`AccessError::Denied`] for a `401` or `403`,
    /// [`AccessError::Body`] for a `200` that is no `ACCESS_POLICY`,
    /// [`AccessError::Undocumented`] for any other status, and
    /// [`AccessError::Cdr`] when the call never reached the CDR.
    pub async fn access_policy(
        &self,
        admin_root: &url::Url,
    ) -> Result<Option<AccessPolicy>, AccessError> {
        let request = Request::new(Method::GET, String::from("/admin/access_policy"));
        let upstream = match shared(self.extension(admin_root, request).await?) {
            Err(AccessError::NotFound { .. }) => return Ok(None),
            answered => answered?,
        };
        if upstream.status() != StatusCode::OK {
            return Err(AccessError::Undocumented {
                upstream: Box::new(upstream),
            });
        }
        openehr_its::json::from_canonical_json(upstream.body())
            .map(Some)
            .map_err(|source| AccessError::Body {
                operation: "admin_access_policy_read",
                class: "ACCESS_POLICY",
                source,
            })
    }
}
