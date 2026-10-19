// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An identified party through the facade's demographics port: the recorder
//! reference reaches the CDR as the identity the port mints, and a CDR that
//! refuses identified parties in clinical content answers through the plain
//! `422` row with its own words.

use std::error::Error as StdError;

use http::StatusCode;
use wiremock::Mock;
use wiremock::matchers;

use super::BASE_URL;
use super::EHR_ID;
use super::call;
use super::condition;
use super::first_issue;
use super::harness_over;
use super::post_condition;
use super::settings;
use super::stub::mount_ehr;

/// The mapping set of this suite: the facade profile with the recorder
/// written as `ENTRY.provider`.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures_party");

/// The refusal a CDR that does not admit identified parties answers with.
const REFUSAL: &str = "identified parties are not admitted in EHR content";

/// The validation error the refusal carries.
const DETAIL: &str = "/content[0]/provider/identifiers is not allowed";

#[tokio::test]
async fn a_cdr_refusing_an_identified_party_answers_through_the_422_row_unchanged()
-> Result<(), Box<dyn StdError>> {
    let harness = harness_over(FIXTURES, settings()).await;
    mount_ehr(&harness.cdr).await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(format!("/ehr/{EHR_ID}/composition")))
        .respond_with(ferrobridge_testkit::stubs::its_rest::unprocessable(
            REFUSAL,
            &[DETAIL],
        ))
        .mount(&harness.cdr)
        .await;
    let mut resource = condition();
    resource["recorder"] =
        serde_json::json!({ "reference": "Practitioner/synthetic-practitioner-1" });
    let (status, body) = call(harness.app(), post_condition(&resource)).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    let issue = first_issue(&body);
    assert_eq!(Some("invalid"), issue["code"].as_str());
    let diagnostics = issue["diagnostics"].as_str().unwrap_or_default();
    assert!(diagnostics.contains(REFUSAL), "{diagnostics}");
    assert!(diagnostics.contains(DETAIL), "{diagnostics}");
    let sent = harness
        .cdr
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|request| request.method == http::Method::POST)
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .ok_or("the composition reached the CDR")?;
    assert!(
        sent.contains("Practitioner/synthetic-practitioner-1") && sent.contains(BASE_URL),
        "the provider carried the minted identity: {sent}"
    );
    Ok(())
}
