// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one refusal every operator handler answers: a problem of the
//! registry in `crate::problem`, or an `about:blank` problem of a status.
//!
//! No specification governs the operator surface: our own design.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::problem::{Kind, blank, refuse, refuse_with};

/// A refusal: the problem kind and its detail, or a status whose meaning
/// says enough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refused {
    /// A problem of the registry with a fixed detail.
    Problem(Kind, &'static str),
    /// A problem of the registry with a detail the refusal composed.
    Detailed(Kind, String),
    /// An `about:blank` problem of the status.
    Blank(StatusCode, &'static str),
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        match self {
            Self::Problem(kind, detail) => refuse(kind, detail),
            Self::Detailed(kind, detail) => refuse_with(kind, detail),
            Self::Blank(status, detail) => blank(status, detail),
        }
    }
}
