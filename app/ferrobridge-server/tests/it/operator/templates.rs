// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The example composition of a template the running set compiled against.
//! No specification governs the operator surface: our own design.

use std::error::Error;

use http::StatusCode;

use super::{Case, OPERATOR, bytes};

#[tokio::test]
async fn a_loaded_template_answers_its_example_composition() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let response = case
        .get(
            "/operator/templates/ferrobridge.diagnose.v1/example",
            Some(OPERATOR),
        )
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    let composition: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert_eq!("COMPOSITION", composition["_type"], "{composition}");
    assert_eq!(
        "ferrobridge.diagnose.v1",
        composition["archetype_details"]["template_id"]["value"],
        "{composition}"
    );
    assert!(
        composition["content"].as_array().is_some_and(|content| !content.is_empty()),
        "{composition}"
    );
    Ok(())
}

#[tokio::test]
async fn a_template_the_set_does_not_hold_is_not_found() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let response = case
        .get("/operator/templates/no.such.template/example", Some(OPERATOR))
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    Ok(())
}

#[tokio::test]
async fn the_example_route_needs_the_operator_scope() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let response = case
        .get("/operator/templates/ferrobridge.diagnose.v1/example", None)
        .await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    Ok(())
}
