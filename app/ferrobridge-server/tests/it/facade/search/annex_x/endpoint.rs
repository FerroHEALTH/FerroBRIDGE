// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `EHDSEndpoint`: the bridge's endpoints, read-only from its configuration.

use std::error::Error;

use ferrobridge_server::facade::endpoint::Endpoints;
use http::StatusCode;

use super::super::get;
use super::super::matches;
use super::super::queries;
use super::annex_x;
use super::annex_x_with;
use super::ensure;

/// connection type and read by its id.
pub(super) async fn serve_endpoint() -> Result<(), Box<dyn Error>> {
    let harness = annex_x().await?;
    let (status, body) = get(&harness, "Endpoint?connection-type=hl7-fhir-rest").await?;
    ensure(status == StatusCode::OK, || {
        format!("the search answered {status}: {body}")
    })?;
    let found = matches(&body);
    ensure(found.len() == 1, || {
        format!("the search answered {} matches", found.len())
    })?;
    let id = found[0]["resource"]["id"]
        .as_str()
        .ok_or("an id")?
        .to_owned();
    let (status, read) = get(&harness, &format!("Endpoint/{id}")).await?;
    ensure(status == StatusCode::OK, || {
        format!("the read answered {status}: {read}")
    })?;
    ensure(read["address"] == found[0]["resource"]["address"], || {
        format!("the read answered {read}")
    })
}

#[tokio::test]
async fn the_endpoints_are_the_fhir_base_and_the_configured_wado_rs() -> Result<(), Box<dyn Error>>
{
    // R4 endpoint.html; the connection types of
    // http://terminology.hl7.org/CodeSystem/endpoint-connection-type.
    let harness = annex_x_with(Endpoints {
        wado_rs: Some(String::from("https://pacs.example.org/dicom-web")),
    })
    .await?;
    let (status, body) = get(&harness, "Endpoint").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    assert_eq!(2, found.len(), "{body}");
    assert_eq!(
        Some("hl7-fhir-rest"),
        found[0]["resource"]["connectionType"]["code"].as_str()
    );
    let (status, wado) = get(&harness, "Endpoint/dicom-wado-rs").await?;
    assert_eq!(StatusCode::OK, status, "{wado}");
    assert_eq!(
        Some("dicom-wado-rs"),
        wado["connectionType"]["code"].as_str()
    );
    assert_eq!(
        Some("https://pacs.example.org/dicom-web"),
        wado["address"].as_str()
    );
    let (status, none) = get(&harness, "Endpoint?connection-type=dicom-wado-rs&_id=fhir").await?;
    assert_eq!(StatusCode::OK, status, "{none}");
    assert!(matches(&none).is_empty(), "{none}");
    let (status, refused) = get(&harness, "Endpoint?address=x").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{refused}");
    let (status, missing) = get(&harness, "Endpoint/other").await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{missing}");
    let (status, statement) = get(&harness, "metadata").await?;
    assert_eq!(StatusCode::OK, status, "{statement}");
    assert!(
        statement["rest"][0]["resource"]
            .as_array()
            .is_some_and(|resources| resources
                .iter()
                .any(|resource| resource["type"] == "Endpoint")),
        "{statement}"
    );
    assert_eq!(0, queries(&harness).await.len());
    Ok(())
}
