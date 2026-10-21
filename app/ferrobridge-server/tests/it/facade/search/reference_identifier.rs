// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `:identifier` modifier on the subject parameters, which matches the
//! `Reference.identifier` of the subject a resource carries
//! (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.12).

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::Store;
use http::StatusCode;
use std::error::Error as StdError;

use super::super::EHR_ID;
use super::super::VERSION_ONE;
use super::super::first_issue;
use super::bound;
use super::get;
use super::matches;
use super::mount_rows;
use super::queries;
use super::row;
use super::searching;

/// The system of the synthetic identifier the subject is keyed by.
const SYSTEM: &str = "http://example.org/mrn";

/// The value of that identifier.
const VALUE: &str = "MRN-0001";

/// Returns a harness whose EHR is also keyed by the identifier
/// `SYSTEM|VALUE`, the way a write names a subject by identifier.
async fn keyed_by_identifier() -> super::super::Harness {
    let harness = searching().await;
    harness
        .store
        .record_ehr(
            &PersonId::new(SYSTEM, VALUE).expect("a legal person key"),
            &EhrId::new(EHR_ID).expect("a legal ehr_id"),
        )
        .expect("the identifier records");
    harness
}

#[tokio::test]
async fn an_identifier_names_the_subject_keyed_by_it() -> Result<(), Box<dyn StdError>> {
    let harness = keyed_by_identifier().await;
    let composition = harness.composition();
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    for query in [
        "Condition?patient:identifier=http://example.org/mrn%7CMRN-0001",
        "Condition?subject:identifier=http://example.org/mrn%7CMRN-0001",
    ] {
        let (status, body) = get(&harness, query).await?;
        assert_eq!(StatusCode::OK, status, "{query}: {body}");
        assert_eq!(1, matches(&body).len(), "{query}: {body}");
    }
    let sent = queries(&harness).await;
    let rows = sent.first().ok_or("the clinical query reached the CDR")?;
    assert!(bound(rows).contains(&String::from(EHR_ID)), "{rows}");
    Ok(())
}

#[tokio::test]
async fn an_identifier_no_subject_is_keyed_by_matches_nothing() -> Result<(), Box<dyn StdError>> {
    let harness = keyed_by_identifier().await;
    for query in [
        "Condition?patient:identifier=http://example.org/mrn%7CMRN-9999",
        "Condition?patient:identifier=%7CMRN-0001",
    ] {
        let (status, body) = get(&harness, query).await?;
        assert_eq!(StatusCode::OK, status, "{query}: {body}");
        assert!(matches(&body).is_empty(), "{query}: {body}");
    }
    assert_eq!(queries(&harness).await, Vec::<serde_json::Value>::new());
    Ok(())
}

#[tokio::test]
async fn a_subject_rendered_as_a_literal_reference_carries_no_identifier()
-> Result<(), Box<dyn StdError>> {
    // The suite's subject `Patient/p-1` is keyed in the link's own namespace,
    // so its resources name it by `reference` alone and no identifier matches.
    let harness = searching().await;
    let (status, body) = get(
        &harness,
        "Condition?patient:identifier=http://example.org/fhir%7CPatient/p-1",
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert!(matches(&body).is_empty(), "{body}");
    assert_eq!(queries(&harness).await, Vec::<serde_json::Value>::new());
    Ok(())
}

#[tokio::test]
async fn an_identifier_without_a_system_or_a_value_is_refused() -> Result<(), Box<dyn StdError>> {
    let harness = keyed_by_identifier().await;
    for query in [
        "Condition?patient:identifier=MRN-0001",
        "Condition?patient:identifier=http://example.org/mrn%7C",
    ] {
        let (status, body) = get(&harness, query).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {body}");
        let issue = first_issue(&body);
        assert_eq!(Some("not-supported"), issue["code"].as_str(), "{query}");
        assert_eq!(Some("patient"), issue["location"][0].as_str(), "{query}");
    }
    assert_eq!(queries(&harness).await, Vec::<serde_json::Value>::new());
    Ok(())
}
