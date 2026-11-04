// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET /operator/templates/{id}/example`: a synthetic example composition
//! of a template the running set compiled against, in canonical JSON, for
//! the playground to offer as input.
//!
//! The composition is generated from the template's Web Template
//! (`WebTemplateIndex::example_composition`) and validated against it, so
//! `$tofhir` reads it as it reads a composition from the CDR. Every value
//! is synthetic. No specification governs the operator surface: our own
//! design.

use std::sync::Arc;

use axum::extract::Path;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::Response;
use openehr_mapping_core::template::PathError;

use crate::operator::Operator;
use crate::problem::{Kind, refuse};

/// `GET /operator/templates/{id}/example`: the example composition of the
/// template `id`; `404` for a template the running set does not hold, and
/// `422` for one that has no example.
pub async fn example(State(operator): State<Arc<Operator>>, Path(id): Path<String>) -> Response {
    let snapshot = operator.snapshot();
    let Some(index) = snapshot.report().indexes().get(&id).cloned() else {
        return refuse(
            Kind::NotFound,
            "the running set holds no template of this id",
        );
    };
    let now = jiff::Timestamp::now().to_string();
    match tokio::task::spawn_blocking(move || index.example_composition(&now)).await {
        Ok(Ok(composition)) => axum::Json(composition.into_value()).into_response(),
        Ok(Err(error)) => {
            if !matches!(error, PathError::NotComposition { .. }) {
                tracing::warn!(
                    error = ferrohealth_server::run::chain(&error),
                    "a template's example composition did not build"
                );
            }
            refuse(Kind::NoExample, "the template has no example composition")
        }
        Err(error) => {
            tracing::error!(%error, "an example composition did not finish");
            refuse(Kind::NoExample, "the template has no example composition")
        }
    }
}
