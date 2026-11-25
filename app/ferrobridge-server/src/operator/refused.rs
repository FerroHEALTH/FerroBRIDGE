// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusal an operator handler answers: a problem of the registry in
//! `crate::problem`, or an `about:blank` problem of a status.
//!
//! No specification governs the operator surface: our own design.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::problem::{Kind, blank, refuse};

/// A refusal: the problem kind and its fixed detail, or a status whose
/// meaning says enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    /// A problem of the registry.
    Problem(Kind, &'static str),
    /// An `about:blank` problem of the status.
    Blank(StatusCode, &'static str),
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        match self {
            Self::Problem(kind, detail) => refuse(kind, detail),
            Self::Blank(status, detail) => blank(status, detail),
        }
    }
}
