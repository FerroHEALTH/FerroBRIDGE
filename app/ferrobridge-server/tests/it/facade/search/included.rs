// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `_include` of a resource the match's run creates, and a token the mapping
//! writes as a constant, over the include fixture's Condition and the
//! evidence Observation it creates.

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::search::Limits;
use http::StatusCode;
use std::error::Error;

use super::super::Harness;
use super::super::VERSION_ONE;
use super::super::harness_built;
use super::super::settings;
use super::SUBJECT;
use super::get;
use super::matches;
use super::mount_rows;
use super::queries;
use super::row;

/// The fixture directory of the include cases.
const INCLUDED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/included");

/// The context the include fixture maps.
const CONTEXT: &str = "ferrobridge_included.context";

/// The category every Condition of the context carries.
const CATEGORY: &str = "http://terminology.hl7.org/CodeSystem/condition-category";

/// Returns a harness over the include fixture, its subject bound.
async fn including() -> Harness {
    let harness = harness_built(INCLUDED, settings(), Some(Limits::default())).await;
    let person =
        PersonId::new("http://example.org/fhir", super::PSEUDONYM).expect("a legal person key");
    harness
        .store
        .record_ehr(
            &person,
            &EhrId::new(super::super::EHR_ID).expect("a legal ehr_id"),
        )
        .await
        .expect("the subject records");
    harness
}

/// Returns the composition of a Condition whose evidence is an Observation.
fn with_evidence(harness: &Harness) -> serde_json::Value {
    let mut resource: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)
            .expect("the synthetic Condition reads");
    resource["meta"] = serde_json::json!({
        "profile": ["http://example.org/fhir/StructureDefinition/ferrobridge-included"]
    });
    resource["contained"] = serde_json::json!([{
        "resourceType": "Observation",
        "id": "evidence",
        "status": "final",
        "code": {"text": "Synthetic evidence"}
    }]);
    resource["evidence"] = serde_json::json!([{"detail": [{"reference": "#evidence"}]}]);
    harness.composition_under(CONTEXT, &resource)
}

/// Returns the entries of `body` whose `search.mode` is `include`.
fn included(body: &serde_json::Value) -> Vec<&serde_json::Value> {
    body["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["search"]["mode"].as_str() == Some("include"))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn an_include_adds_the_created_resource_the_match_references_once()
-> Result<(), Box<dyn Error>> {
    // R4 search.html §3.1.1.5.4: an included resource has `search.mode`
    // `include`, and _count applies to the matches alone.
    let harness = including().await;
    let composition = with_evidence(&harness);
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(
        &harness,
        &format!("Condition?patient={SUBJECT}&_include=Condition:evidence-detail"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    assert_eq!(1, found.len(), "{body}");
    let reference = found[0]["resource"]["evidence"][0]["detail"][0]["reference"]
        .as_str()
        .ok_or("the match references its evidence")?
        .to_owned();
    let added = included(&body);
    assert_eq!(1, added.len(), "{body}");
    assert_eq!(
        Some("Observation"),
        added[0]["resource"]["resourceType"].as_str()
    );
    let id = added[0]["resource"]["id"].as_str().ok_or("an id")?;
    assert_eq!(format!("Observation/{id}"), reference);
    assert!(
        added[0]["fullUrl"]
            .as_str()
            .is_some_and(|url| url.ends_with(&reference)),
        "{body}"
    );
    assert_eq!(Some(1), body["total"].as_u64(), "the total counts matches");
    assert!(
        body["link"][0]["url"]
            .as_str()
            .is_some_and(|url| url.contains("_include=Condition%3Aevidence-detail")),
        "{body}"
    );

    let (status, target) = get(
        &harness,
        &format!("Condition?patient={SUBJECT}&_include=Condition:evidence-detail:Observation"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{target}");
    assert_eq!(1, included(&target).len(), "{target}");
    let (status, other) = get(
        &harness,
        &format!("Condition?patient={SUBJECT}&_include=Condition:evidence-detail:Patient"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{other}");
    assert!(included(&other).is_empty(), "{other}");
    Ok(())
}

#[tokio::test]
async fn without_an_include_the_created_resource_of_another_type_is_left_out()
-> Result<(), Box<dyn Error>> {
    let harness = including().await;
    let composition = with_evidence(&harness);
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(&harness, &format!("Condition?patient={SUBJECT}")).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, matches(&body).len(), "{body}");
    assert!(included(&body).is_empty(), "{body}");
    Ok(())
}

#[tokio::test]
async fn an_include_the_declaration_does_not_list_is_refused_unless_lenient()
-> Result<(), Box<dyn Error>> {
    let harness = including().await;
    for query in [
        "_include=Condition:subject",
        "_include=Observation:evidence-detail",
        "_include=Condition:evidence-detail:Observation:extra",
        "_include:iterate=Condition:evidence-detail",
    ] {
        let (status, body) = get(&harness, &format!("Condition?patient={SUBJECT}&{query}")).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {body}");
        assert_eq!(Some("OperationOutcome"), body["resourceType"].as_str());
    }
    mount_rows(&harness.cdr, &[]).await;
    let request = http::Request::get(format!(
        "/fhir/Condition?patient={SUBJECT}&_include=Condition:subject"
    ))
    .header("Prefer", "handling=lenient")
    .body(axum::body::Body::empty())?;
    let (status, body) = super::super::call(harness.app(), request).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn a_fixed_token_matches_every_resource_or_none_without_a_condition()
-> Result<(), Box<dyn Error>> {
    // The category is the mapping's constant, so a matching value adds no
    // AQL condition and any other value answers an empty set with no query.
    let harness = including().await;
    let composition = with_evidence(&harness);
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    for value in [
        "problem-list-item".to_owned(),
        format!("{CATEGORY}|problem-list-item"),
        format!("{CATEGORY}|"),
        "encounter-diagnosis,problem-list-item".to_owned(),
    ] {
        let (status, body) = get(
            &harness,
            &format!("Condition?patient={SUBJECT}&category={value}"),
        )
        .await?;
        assert_eq!(StatusCode::OK, status, "{value}: {body}");
        assert_eq!(1, matches(&body).len(), "{value}: {body}");
        assert_eq!(
            Some("problem-list-item"),
            matches(&body)[0]["resource"]["category"][0]["coding"][0]["code"].as_str()
        );
    }
    let sent = queries(&harness).await.len();
    for value in [
        "encounter-diagnosis",
        "|problem-list-item",
        "http://example.org|problem-list-item",
    ] {
        let (status, body) = get(
            &harness,
            &format!("Condition?patient={SUBJECT}&category={value}"),
        )
        .await?;
        assert_eq!(StatusCode::OK, status, "{value}: {body}");
        assert!(matches(&body).is_empty(), "{value}: {body}");
        assert_eq!(Some(0), body["total"].as_u64(), "{value}: {body}");
    }
    assert_eq!(
        sent,
        queries(&harness).await.len(),
        "no value queried the CDR"
    );
    for query in queries(&harness).await {
        assert!(
            query["q"]
                .as_str()
                .is_some_and(|text| !text.contains("category")),
            "{query}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_statement_lists_the_include_and_the_fixed_token() -> Result<(), Box<dyn Error>> {
    let harness = including().await;
    let (status, body) = get(&harness, "metadata").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let condition = body["rest"][0]["resource"]
        .as_array()
        .and_then(|resources| {
            resources
                .iter()
                .find(|resource| resource["type"].as_str() == Some("Condition"))
        })
        .ok_or("the statement names Condition")?;
    assert_eq!(
        serde_json::json!(["Condition:evidence-detail"]),
        condition["searchInclude"]
    );
    assert!(
        condition["searchParam"]
            .as_array()
            .is_some_and(|params| params
                .iter()
                .any(|param| param["name"] == "category" && param["type"] == "token")),
        "{condition}"
    );
    Ok(())
}
