// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A party bound to a login is never written through the facade: a
//! `Practitioner` whose `PERSON` carries a binding key, a `DV_IDENTIFIER` of
//! an issuer `[facade] binding_identifier_issuers` names, is refused with
//! `403` before any write reaches the CDR. The binding key and its
//! provisioning through SCIM are the CDR's party binding; no specification
//! governs it: our own design.

use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use http::Request;
use http::StatusCode;
use wiremock::MockServer;

use super::app_over;
use super::call;
use super::demographic::UID;
use super::demographic::facade_over;
use super::demographic::mount_person;
use super::demographic::person;
use super::demographic::with_body;
use super::settings;

/// The `DV_IDENTIFIER.issuer` of the synthetic binding keys.
const BINDING_ISSUER: &str = "https://login.example/persons";

/// Returns the synthetic `PERSON` at version `tree` carrying the identifier
/// `value` of `issuer` in its details.
fn identified(tree: u32, issuer: &str, value: &str) -> serde_json::Value {
    let mut party = person(tree);
    party["details"] = serde_json::json!({
        "_type": "ITEM_TREE",
        "name": {"_type": "DV_TEXT", "value": "Details"},
        "archetype_node_id": "at0001",
        "items": [{
            "_type": "ELEMENT",
            "name": {"_type": "DV_TEXT", "value": "Identifier"},
            "archetype_node_id": "at0002",
            "value": {
                "_type": "DV_IDENTIFIER",
                "issuer": issuer,
                "id": value,
                "type": "login"
            }
        }]
    });
    party
}

/// Returns a `Practitioner` with id `id`, a name and no identifier.
fn practitioner(id: &str) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Practitioner",
        "id": id,
        "name": [{"text": "Synthetic Practitioner"}]
    })
}

/// Returns the stub CDR and the router of a facade binding `BINDING_ISSUER`.
async fn bound() -> Result<(MockServer, axum::Router), Box<dyn Error>> {
    let (cdr, facade) = facade_over(settings()).await?;
    let facade = Arc::new(facade.with_binding_issuers(vec![String::from(BINDING_ISSUER)]));
    Ok((cdr, app_over(&facade)))
}

/// Returns the methods of every request the CDR received outside the
/// template fetch.
async fn writes(cdr: &MockServer) -> Vec<String> {
    cdr.received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.method.as_str() != "GET")
        .map(|request| format!("{} {}", request.method, request.url.path()))
        .collect()
}

#[tokio::test]
async fn an_update_of_a_bound_practitioner_is_forbidden_and_writes_nothing()
-> Result<(), Box<dyn Error>> {
    let (cdr, app) = bound().await?;
    mount_person(
        &cdr,
        UID,
        &identified(1, BINDING_ISSUER, "synthetic-oid-1"),
        1,
    )
    .await;

    let request = with_body(
        http::Method::PUT,
        &format!("/fhir/Practitioner/{UID}"),
        &practitioner(UID),
    )?;
    let (status, body) = call(app, request).await?;

    assert_eq!(StatusCode::FORBIDDEN, status, "{body}");
    assert_eq!(body["issue"][0]["code"], "forbidden", "{body}");
    let diagnostics = body["issue"][0]["diagnostics"].as_str().unwrap_or_default();
    assert!(diagnostics.contains("SCIM"), "{diagnostics}");
    assert!(writes(&cdr).await.is_empty(), "a write reached the CDR");
    Ok(())
}

#[tokio::test]
async fn a_delete_of_a_bound_practitioner_is_forbidden_and_writes_nothing()
-> Result<(), Box<dyn Error>> {
    let (cdr, app) = bound().await?;
    mount_person(
        &cdr,
        UID,
        &identified(1, BINDING_ISSUER, "synthetic-oid-1"),
        1,
    )
    .await;

    let (status, body) = call(
        app,
        Request::delete(format!("/fhir/Practitioner/{UID}")).body(Body::empty())?,
    )
    .await?;

    assert_eq!(StatusCode::FORBIDDEN, status, "{body}");
    assert!(writes(&cdr).await.is_empty(), "a write reached the CDR");
    Ok(())
}

#[tokio::test]
async fn a_create_carrying_a_binding_key_is_forbidden_and_reaches_no_cdr()
-> Result<(), Box<dyn Error>> {
    let (cdr, app) = bound().await?;
    let mut resource = practitioner(UID);
    resource
        .as_object_mut()
        .ok_or("a resource object")?
        .remove("id");
    resource["identifier"] = serde_json::json!([{
        "system": BINDING_ISSUER,
        "value": "synthetic-oid-1"
    }]);

    let request = with_body(http::Method::POST, "/fhir/Practitioner", &resource)?;
    let (status, body) = call(app, request).await?;

    assert_eq!(StatusCode::FORBIDDEN, status, "{body}");
    assert!(writes(&cdr).await.is_empty(), "a write reached the CDR");
    Ok(())
}

#[tokio::test]
async fn an_update_of_a_party_with_another_issuer_s_identifier_goes_through()
-> Result<(), Box<dyn Error>> {
    let (cdr, app) = bound().await?;
    mount_person(
        &cdr,
        UID,
        &identified(1, "https://registry.example/staff", "synthetic-7"),
        1,
    )
    .await;
    wiremock::Mock::given(wiremock::matchers::method("PUT"))
        .and(wiremock::matchers::path(format!(
            "/demographic/person/{UID}"
        )))
        .respond_with(
            wiremock::ResponseTemplate::new(204)
                .insert_header("ETag", format!("\"{}\"", super::demographic::version(2))),
        )
        .mount(&cdr)
        .await;

    let request = with_body(
        http::Method::PUT,
        &format!("/fhir/Practitioner/{UID}"),
        &practitioner(UID),
    )?;
    let (status, body) = call(app, request).await?;

    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, writes(&cdr).await.len(), "the update reached the CDR");
    Ok(())
}
