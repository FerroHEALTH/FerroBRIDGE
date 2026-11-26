// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR Mapping Language maps saved as console files of a mapping revision:
//! a save parses them, swaps them in and lists them, a map outside the
//! grammar is refused, and a reload takes the maps another replica recorded.

use std::error::Error;

use axum::body::Body;
use ferrobridge_server::mappings::revisions::Recording;
use http::{Request, StatusCode};

use super::Case;
use crate::operator::bytes;

/// A map the R4 tutorial's rules write (mapping-tutorial.html).
const MAP: &str = "map \"http://example.org/StructureMap/tutorial\" = tutorial\n\
group tutorial(source src : TLeft, target tgt : TRight) {\n  src.a as a -> tgt.a = a;\n}\n";

/// Returns the URLs of the maps a reload swapped in.
fn running(case: &Case) -> Vec<String> {
    case.structuremaps
        .current()
        .maps()
        .iter()
        .map(|map| map.url.clone())
        .collect()
}

#[tokio::test]
async fn a_saved_map_swaps_in_and_lists_as_a_map() -> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    let response = case.create_at("structuremap/tutorial.map", MAP).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some(MAP.as_bytes().to_vec()),
        case.console_at("structuremap/tutorial.map").await?
    );
    assert_eq!(
        vec!["http://example.org/StructureMap/tutorial"],
        running(&case)
    );
    let response = case
        .send(Request::get("/operator/mappings").body(Body::empty())?)
        .await?;
    let listing: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    let saved = listing
        .as_array()
        .ok_or("a listing")?
        .iter()
        .find(|entry| entry["path"] == "structuremap/tutorial.map")
        .ok_or("the saved map is listed")?;
    assert_eq!("structuremap", saved["language"]);
    assert_eq!("console", saved["owner"]);
    assert_eq!("map", saved["kind"]);
    Ok(())
}

#[tokio::test]
async fn a_map_outside_the_grammar_is_refused_and_writes_nothing() -> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    let response = case
        .create_at("structuremap/broken.map", "map = broken\n")
        .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, response.status());
    let report: serde_json::Value = serde_json::from_slice(&bytes(response).await?)?;
    assert!(
        report["diagnostics"]
            .as_array()
            .ok_or("a diagnostics array")?
            .iter()
            .any(|diagnostic| diagnostic["file"] == "structuremap/broken.map"
                && diagnostic["code"] == "fb-structuremap-syntax"),
        "{report}"
    );
    assert_eq!(None, case.console_at("structuremap/broken.map").await?);
    assert_eq!(Vec::<String>::new(), running(&case));
    Ok(())
}

#[tokio::test]
async fn a_reload_takes_the_maps_another_replica_recorded() -> Result<(), Box<dyn Error>> {
    let Some(case) = Case::new().await? else {
        return Ok(());
    };
    case.record(&[("structuremap/tutorial.map", MAP)], Recording::Run)
        .await?;
    assert_eq!(Vec::<String>::new(), running(&case));
    let response = case
        .send(Request::post("/operator/reload").body(Body::empty())?)
        .await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        vec!["http://example.org/StructureMap/tutorial"],
        running(&case)
    );
    Ok(())
}
