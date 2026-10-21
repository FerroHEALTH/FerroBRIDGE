// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chained search through the patient identifier,
//! `Condition?patient.identifier=…` and `subject.identifier`, over a CDR that
//! serves Cadasto's experimental AQL demographic extension: the identifier
//! resolved to every party that carries it, each party to its EHR, then the
//! clinical query on those EHRs (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.15).

use std::sync::Arc;

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::search::Limits;
use ferrobridge_server::facade::search::identifier::IdentifierSearch;
use http::StatusCode;
use std::error::Error as StdError;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::EHR_ID;
use super::super::Harness;
use super::super::VERSION_ONE;
use super::super::first_issue;
use super::AQL;
use super::bound;
use super::get;
use super::matches;
use super::mount_count;
use super::mount_rows;
use super::queries;
use super::row;

/// The object id of a second synthetic party that carries the same
/// identifier value under another system.
const OTHER_PARTY: &str = "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d";

/// The EHR of [`OTHER_PARTY`].
const OTHER_EHR: &str = "2c3d4e5f-0000-4000-8000-00000000000e";

/// The identifier query every case configures, in the dialect's shape.
const QUERY: &str = "SELECT p/uid/value FROM PERSON p WHERE p/details/items[at0001]/value/id = $identifier ORDER BY p/uid/value ASC";

/// The identifier query of a value naming a system.
const SYSTEM_QUERY: &str = "SELECT p/uid/value FROM PERSON p WHERE p/details/items[at0001]/value/id = $identifier AND p/details/items[at0001]/value/issuer = $system ORDER BY p/uid/value ASC";

/// The party the synthetic identifier names, as a versioned uid.
const PARTY: &str = "0f1e2d3c-4b5a-4968-8776-655443322110::cdr.example.org::1";

/// The patient that party is, as the identity map keys a literal reference.
const PATIENT: &str = "Patient/0f1e2d3c-4b5a-4968-8776-655443322110";

/// The namespace the suite's subject link reads a reference in.
const NAMESPACE: &str = "http://example.org/fhir";

/// Returns a harness answering search with the identifier resolve `search`,
/// the patient [`PATIENT`] bound to [`EHR_ID`] when `bound` holds.
async fn identifying(search: Option<IdentifierSearch>, bound_patient: bool) -> Harness {
    let Harness { cdr, store, facade } = super::super::search_harness(Limits::default()).await;
    let facade = Arc::try_unwrap(facade)
        .expect("the harness holds the only facade")
        .with_identifier_search(search);
    if bound_patient {
        store
            .record_ehr(
                &PersonId::new(NAMESPACE, PATIENT).expect("a legal person key"),
                &EhrId::new(EHR_ID).expect("a legal ehr_id"),
            )
            .expect("the patient records");
    }
    Harness {
        cdr,
        store,
        facade: Arc::new(facade),
    }
}

/// Returns the resolve with the value query alone.
fn by_value() -> IdentifierSearch {
    IdentifierSearch::new(QUERY, None).expect("a legal identifier query")
}

/// Whether a request body is a demographic query.
fn is_demographic(request: &wiremock::Request) -> bool {
    serde_json::from_slice::<serde_json::Value>(&request.body)
        .ok()
        .and_then(|body| body["q"].as_str().map(|text| text.contains("FROM PERSON")))
        .unwrap_or(false)
}

/// Answers every demographic query with one row per uid of `parties`.
async fn mount_parties(harness: &Harness, parties: &[&str]) {
    let rows: Vec<serde_json::Value> = parties.iter().map(|uid| serde_json::json!([uid])).collect();
    Mock::given(matchers::method("POST"))
        .and(matchers::path(AQL))
        .and(is_demographic)
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "q": "synthetic",
            "columns": [{ "name": "uid", "path": "/uid/value" }],
            "rows": rows
        })))
        .with_priority(1)
        .mount(&harness.cdr)
        .await;
}

/// Returns the demographic and the clinical queries the CDR received.
async fn split_queries(harness: &Harness) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    queries(harness).await.into_iter().partition(|query| {
        query["q"]
            .as_str()
            .is_some_and(|text| text.contains("FROM PERSON"))
    })
}

#[tokio::test]
async fn patient_identifier_resolves_the_party_and_searches_its_ehr()
-> Result<(), Box<dyn StdError>> {
    let harness = identifying(Some(by_value()), true).await;
    mount_parties(&harness, &[PARTY]).await;
    let composition = harness.composition();
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-0001").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, matches(&body).len(), "{body}");
    let (demographic, clinical) = split_queries(&harness).await;
    let [ref resolve] = *demographic else {
        return Err(format!("one identifier resolve, not {}", demographic.len()).into());
    };
    assert_eq!(Some(QUERY), resolve["q"].as_str());
    assert_eq!(
        Some("MRN-0001"),
        resolve["query_parameters"]["identifier"].as_str(),
        "{resolve}"
    );
    let rows = clinical
        .first()
        .ok_or("the clinical query reached the CDR")?;
    let text = rows["q"].as_str().unwrap_or_default();
    assert!(text.contains("e/ehr_id/value = $"), "{text}");
    assert!(bound(rows).contains(&String::from(EHR_ID)), "{rows}");
    let self_link = body["link"][0]["url"].as_str().unwrap_or_default();
    assert!(
        self_link.contains("patient.identifier=MRN-0001"),
        "the self link names the chain the search used: {self_link}"
    );
    Ok(())
}

#[tokio::test]
async fn subject_identifier_and_its_patient_type_modifier_resolve_alike()
-> Result<(), Box<dyn StdError>> {
    for query in [
        "Condition?subject.identifier=MRN-0001",
        "Condition?subject:Patient.identifier=MRN-0001",
    ] {
        let harness = identifying(Some(by_value()), true).await;
        mount_parties(&harness, &[PARTY]).await;
        let composition = harness.composition();
        mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
        let (status, body) = get(&harness, query).await?;
        assert_eq!(StatusCode::OK, status, "{query}: {body}");
        assert_eq!(1, matches(&body).len(), "{query}: {body}");
    }
    Ok(())
}

#[tokio::test]
async fn an_unknown_identifier_answers_an_empty_searchset_with_no_clinical_query()
-> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.3: an unknown subject is a logical condition that
    // answers an empty search set, never a guess.
    let harness = identifying(Some(by_value()), true).await;
    mount_parties(&harness, &[]).await;
    mount_rows(&harness.cdr, &[]).await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-9999").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some("searchset"), body["type"].as_str());
    assert!(matches(&body).is_empty(), "{body}");
    let (demographic, clinical) = split_queries(&harness).await;
    assert_eq!(1, demographic.len());
    assert!(clinical.is_empty(), "{clinical:?}");
    Ok(())
}

#[tokio::test]
async fn an_identifier_two_parties_carry_matches_the_resources_of_both()
-> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.4.10: a bare value matches "irrespective of the
    // value of the system property", and §3.1.1.4.15 answers every match.
    let harness = identifying(Some(by_value()), true).await;
    harness.store.record_ehr(
        &PersonId::new(NAMESPACE, &format!("Patient/{OTHER_PARTY}"))?,
        &EhrId::new(OTHER_EHR)?,
    )?;
    mount_parties(
        &harness,
        &[PARTY, &format!("{OTHER_PARTY}::cdr.example.org::1")],
    )
    .await;
    let composition = harness.composition();
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-0001").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let (_, clinical) = split_queries(&harness).await;
    let rows = clinical
        .first()
        .ok_or("the clinical query reached the CDR")?;
    let parameters = bound(rows);
    assert!(parameters.contains(&String::from(EHR_ID)), "{rows}");
    assert!(parameters.contains(&String::from(OTHER_EHR)), "{rows}");
    Ok(())
}

#[tokio::test]
async fn a_party_with_no_ehr_among_several_leaves_the_others_matching()
-> Result<(), Box<dyn StdError>> {
    let harness = identifying(Some(by_value()), true).await;
    mount_parties(
        &harness,
        &[PARTY, &format!("{OTHER_PARTY}::cdr.example.org::1")],
    )
    .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&harness.cdr)
        .await;
    let composition = harness.composition();
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-0001").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, matches(&body).len(), "{body}");
    let (_, clinical) = split_queries(&harness).await;
    let rows = clinical
        .first()
        .ok_or("the clinical query reached the CDR")?;
    let parameters = bound(rows);
    assert!(parameters.contains(&String::from(EHR_ID)), "{rows}");
    assert!(!parameters.contains(&String::from(OTHER_EHR)), "{rows}");
    Ok(())
}

#[tokio::test]
async fn two_versions_of_one_party_are_one_patient() -> Result<(), Box<dyn StdError>> {
    let harness = identifying(Some(by_value()), true).await;
    mount_parties(
        &harness,
        &[
            PARTY,
            "0f1e2d3c-4b5a-4968-8776-655443322110::cdr.example.org::2",
        ],
    )
    .await;
    mount_rows(&harness.cdr, &[]).await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-0001").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn count_zero_answers_the_total_alone() -> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.5.3: `_count=0` is treated as `_summary=count`.
    let harness = identifying(Some(by_value()), true).await;
    mount_parties(&harness, &[PARTY]).await;
    let composition = harness.composition();
    mount_count(&harness.cdr, std::slice::from_ref(&composition)).await;
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-0001&_count=0").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(1), body["total"].as_u64(), "{body}");
    assert!(matches(&body).is_empty(), "{body}");
    let (_, clinical) = split_queries(&harness).await;
    assert!(
        clinical.iter().all(|query| query["q"]
            .as_str()
            .is_some_and(|text| text.contains("COUNT("))),
        "no row query is sent: {clinical:?}"
    );
    Ok(())
}

#[tokio::test]
async fn summary_count_answers_the_total_alone() -> Result<(), Box<dyn StdError>> {
    // R4 search.html, `_summary`: `count` returns the count of matches alone.
    let harness = identifying(Some(by_value()), true).await;
    mount_parties(&harness, &[PARTY]).await;
    let composition = harness.composition();
    mount_count(&harness.cdr, std::slice::from_ref(&composition)).await;
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(
        &harness,
        "Condition?patient.identifier=MRN-0001&_summary=count",
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(1), body["total"].as_u64(), "{body}");
    assert!(matches(&body).is_empty(), "{body}");
    let (demographic, clinical) = split_queries(&harness).await;
    assert_eq!(1, demographic.len());
    assert!(
        clinical.iter().all(|query| query["q"]
            .as_str()
            .is_some_and(|text| text.contains("COUNT("))),
        "no row query is sent: {clinical:?}"
    );
    Ok(())
}

#[tokio::test]
async fn sort_and_count_apply_to_the_clinical_query() -> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.5.1 (`_sort`) and §3.1.1.5.3 (`_count`): the chain
    // only narrows the EHR, so both reach the clinical query as on any search.
    let harness = identifying(Some(by_value()), true).await;
    mount_parties(&harness, &[PARTY]).await;
    let composition = harness.composition();
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(
        &harness,
        "Condition?patient.identifier=MRN-0001&_sort=-context-start&_count=5",
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let (_, clinical) = split_queries(&harness).await;
    let rows = clinical
        .first()
        .ok_or("the clinical query reached the CDR")?;
    let text = rows["q"].as_str().unwrap_or_default();
    assert!(
        text.contains("ORDER BY c/context/start_time/value DESC"),
        "{text}"
    );
    assert_eq!(
        Some(6),
        rows["fetch"].as_i64(),
        "a page of 5 reads one row more: {rows}"
    );
    let self_link = body["link"][0]["url"].as_str().unwrap_or_default();
    assert!(self_link.contains("_sort=-context-start"), "{self_link}");
    assert!(self_link.contains("_count=5"), "{self_link}");
    Ok(())
}

#[tokio::test]
async fn count_zero_for_an_unknown_identifier_makes_no_clinical_call()
-> Result<(), Box<dyn StdError>> {
    let harness = identifying(Some(by_value()), true).await;
    mount_parties(&harness, &[]).await;
    mount_count(&harness.cdr, &[]).await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-9999&_count=0").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(Some(0), body["total"].as_u64(), "{body}");
    let (_, clinical) = split_queries(&harness).await;
    assert!(clinical.is_empty(), "{clinical:?}");
    Ok(())
}

#[tokio::test]
async fn a_party_the_map_does_not_know_is_looked_up_on_the_cdr_by_subject()
-> Result<(), Box<dyn StdError>> {
    // ITS-REST 1.1.0 EHR, `ehr_get_by_subject`: the patient-to-EHR link.
    let harness = identifying(Some(by_value()), false).await;
    mount_parties(&harness, &[PARTY]).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .and(matchers::query_param("subject_id", PATIENT))
        .and(matchers::query_param("subject_namespace", NAMESPACE))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .set_body_string(super::super::ehr_body()),
        )
        .mount(&harness.cdr)
        .await;
    let composition = harness.composition();
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-0001").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, matches(&body).len(), "{body}");
    Ok(())
}

#[tokio::test]
async fn a_party_with_no_ehr_matches_nothing() -> Result<(), Box<dyn StdError>> {
    let harness = identifying(Some(by_value()), false).await;
    mount_parties(&harness, &[PARTY]).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&harness.cdr)
        .await;
    mount_rows(&harness.cdr, &[]).await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-0001").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert!(matches(&body).is_empty(), "{body}");
    let (_, clinical) = split_queries(&harness).await;
    assert!(clinical.is_empty(), "{clinical:?}");
    Ok(())
}

/// A CDR without the extension refuses the identifier query, and that is the
/// one answer the party search gives too: `501 not-supported` (RFC 9110
/// §15.6.2), never a `500`.
#[tokio::test]
async fn a_refused_identifier_query_is_a_501_naming_the_extension() -> Result<(), Box<dyn StdError>>
{
    let harness = identifying(Some(by_value()), true).await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(AQL))
        .and(is_demographic)
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": "UnsupportedSourceClass",
            "message": "PERSON is no source class this service queries"
        })))
        .mount(&harness.cdr)
        .await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-0001").await?;
    assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "{body}");
    assert_eq!(first_issue(&body)["code"], "not-supported", "{body}");
    let diagnostics = first_issue(&body)["diagnostics"]
        .as_str()
        .unwrap_or_default();
    assert!(
        diagnostics.contains("Cadasto experimental AQL demographic extension"),
        "{diagnostics}"
    );
    Ok(())
}

#[tokio::test]
async fn a_system_binds_the_system_query() -> Result<(), Box<dyn StdError>> {
    let search = IdentifierSearch::new(QUERY, Some(SYSTEM_QUERY)).expect("a legal identifier pair");
    let harness = identifying(Some(search), true).await;
    mount_parties(&harness, &[PARTY]).await;
    mount_rows(&harness.cdr, &[]).await;
    let (status, body) = get(
        &harness,
        "Condition?patient.identifier=http://example.org/mrn%7CMRN-0001",
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let (demographic, _) = split_queries(&harness).await;
    let resolve = demographic.first().ok_or("one identifier resolve")?;
    assert_eq!(Some(SYSTEM_QUERY), resolve["q"].as_str());
    assert_eq!(
        Some("http://example.org/mrn"),
        resolve["query_parameters"]["system"].as_str()
    );
    Ok(())
}

#[tokio::test]
async fn a_value_the_configured_queries_cannot_express_is_refused() -> Result<(), Box<dyn StdError>>
{
    for query in [
        "Condition?patient.identifier=http://example.org/mrn%7CMRN-0001",
        "Condition?patient.identifier=%7CMRN-0001",
        "Condition?patient.identifier=http://example.org/mrn%7C",
    ] {
        let harness = identifying(Some(by_value()), true).await;
        let (status, body) = get(&harness, query).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {body}");
        assert_eq!(
            Some("not-supported"),
            first_issue(&body)["code"].as_str(),
            "{query}: {body}"
        );
        assert!(queries(&harness).await.is_empty(), "{query}");
    }
    Ok(())
}

#[tokio::test]
async fn the_chain_is_unknown_when_the_resolve_is_off() -> Result<(), Box<dyn StdError>> {
    let harness = identifying(None, true).await;
    let (status, body) = get(&harness, "Condition?patient.identifier=MRN-0001").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    let issue = first_issue(&body);
    assert_eq!(Some("not-supported"), issue["code"].as_str(), "{body}");
    assert_eq!(
        Some("patient.identifier"),
        issue["location"][0].as_str(),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn another_chain_is_unknown_and_lenient_handling_ignores_it() -> Result<(), Box<dyn StdError>>
{
    let harness = identifying(Some(by_value()), true).await;
    let (status, body) = get(&harness, "Condition?patient.name=Muster").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{body}");
    assert_eq!(Some("not-supported"), first_issue(&body)["code"].as_str());
    mount_rows(&harness.cdr, &[]).await;
    let (status, body) = super::super::call(
        harness.app(),
        http::Request::get("/fhir/Condition?patient=p-1&patient.name=Muster")
            .header("prefer", "handling=lenient")
            .body(axum::body::Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    Ok(())
}

#[tokio::test]
async fn a_modifier_on_the_chain_is_refused() -> Result<(), Box<dyn StdError>> {
    // R4 search.html §3.1.1.4.4: a server SHALL reject an unsupported modifier.
    let harness = identifying(Some(by_value()), true).await;
    for query in [
        "Condition?patient.identifier:of-type=MRN-0001",
        "Condition?subject:Patient.identifier:exact=MRN-0001",
    ] {
        let (status, body) = get(&harness, query).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {body}");
        assert_eq!(
            Some("not-supported"),
            first_issue(&body)["code"].as_str(),
            "{query}"
        );
    }
    assert_eq!(queries(&harness).await, Vec::<serde_json::Value>::new());
    Ok(())
}

#[tokio::test]
async fn a_chain_through_another_target_type_matches_nothing() -> Result<(), Box<dyn StdError>> {
    let harness = identifying(Some(by_value()), true).await;
    let (status, body) = get(&harness, "Condition?subject:Group.identifier=G-1").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert!(matches(&body).is_empty(), "{body}");
    assert_eq!(queries(&harness).await, Vec::<serde_json::Value>::new());
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_declares_the_chain_exactly_when_it_is_on()
-> Result<(), Box<dyn StdError>> {
    for (search, declared) in [(Some(by_value()), true), (None, false)] {
        let harness = identifying(search, true).await;
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
        for name in ["patient", "subject"] {
            let parameter = condition["searchParam"]
                .as_array()
                .and_then(|parameters| {
                    parameters
                        .iter()
                        .find(|parameter| parameter["name"].as_str() == Some(name))
                })
                .ok_or("the statement names the subject parameter")?;
            let documentation = parameter["documentation"].as_str();
            assert_eq!(
                declared,
                documentation.is_some_and(|text| text.contains(&format!("{name}.identifier"))),
                "{name}: {parameter}"
            );
        }
        let other = condition["searchParam"]
            .as_array()
            .map(|parameters| {
                parameters
                    .iter()
                    .filter(|parameter| !parameter["documentation"].is_null())
                    .count()
            })
            .unwrap_or_default();
        assert_eq!(if declared { 2 } else { 0 }, other, "{condition}");
    }
    Ok(())
}
