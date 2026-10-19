// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITS-REST 1.1.0 Demographic API, DEVELOPMENT: the gate, the generated
//! client against the vendored document, the Release 1.1.0 `PARTY` reader and
//! one case per refusal `demographic-codegen.openapi.yaml` documents.

use std::collections::BTreeSet;
use std::error::Error;
use std::time::Duration;

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::Prefer;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::cdr::demographic::Demographic;
use ferrobridge_server::cdr::error::CdrError;
use ferrobridge_server::cdr::ids::version_from_etag;
use ferrobridge_server::facade::outcome::IssueType;
use ferrobridge_server::facade::status::demographic::of_demographic_refusal;
use http::StatusCode;
use openehr_its::rest::client::ClientError;
use openehr_its::rest::client::RetryPolicy;
use openehr_its::rest::generated::demographic::ROUTES;
use openehr_its::rest::generated::demographic::client::PersonCreateOutcome;
use openehr_its::rest::generated::demographic::client::PersonDeleteOutcome;
use openehr_its::rest::generated::demographic::client::PersonGetOutcome;
use openehr_its::rest::generated::demographic::client::PersonUpdateOutcome;
use openehr_rm::v1_2::demographic::person::Person;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::support;

/// The vendored Demographic document of ITS-REST 1.1.0.
const DOCUMENT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/its-rest/computable/OAS/demographic-codegen.openapi.yaml"
));

/// The synthetic person every case reads or writes.
const PERSON_UID: &str = "a1b2c3d4-0000-4000-8000-000000000001::synthetic.example::1";

/// A synthetic `PERSON` in canonical JSON, with `{extra}` spliced in before its
/// identities.
fn person_json(extra: &str) -> String {
    format!(
        r#"{{
  "_type": "PERSON",
  "archetype_node_id": "openEHR-DEMOGRAPHIC-PERSON.person.v2",
  "name": {{"_type": "DV_TEXT", "value": "Synthetic person"}},
  {extra}
  "identities": [{{
    "_type": "PARTY_IDENTITY",
    "archetype_node_id": "at0001",
    "name": {{"_type": "DV_TEXT", "value": "identity"}},
    "details": {{
      "_type": "ITEM_TREE",
      "archetype_node_id": "at0002",
      "name": {{"_type": "DV_TEXT", "value": "details"}}
    }}
  }}]
}}"#
    )
}

/// One `LOCATABLE_REF` to a synthetic `PARTY_RELATIONSHIP`.
const RELATIONSHIP_REF: &str = r#"{
  "_type": "LOCATABLE_REF",
  "namespace": "demographic",
  "type": "PARTY_RELATIONSHIP",
  "id": {"_type": "OBJECT_VERSION_ID", "value": "b5a4c3d2-0000-4000-8000-000000000002::synthetic.example::1"}
}"#;

/// Returns a client for `server` with `[cdr] demographic` set as `on`.
fn client(server: &MockServer, on: bool) -> Result<CdrClient, Box<dyn Error>> {
    let config = CdrConfig::new(format!("{}/v1", server.uri()).parse()?)
        .with_timeout(Duration::from_secs(5))
        .with_retry(RetryPolicy {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
        })
        .with_demographic(on);
    Ok(CdrClient::new(&config)?)
}

/// Returns the Demographic API of `cdr`, which the case turned on.
fn demographic(cdr: &CdrClient) -> Result<Demographic<'_>, Box<dyn Error>> {
    Ok(cdr
        .demographic()
        .ok_or("the case turned the Demographic API on")?)
}

/// Answers `GET /v1/demographic/person/{PERSON_UID}` on `server` with `body`.
async fn person_answers(server: &MockServer, body: String) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/demographic/person/{PERSON_UID}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{PERSON_UID}\""))
                .set_body_string(body),
        )
        .mount(server)
        .await;
}

/// Returns the synthetic person as an RM value.
fn person() -> Result<Person, Box<dyn Error>> {
    Ok(openehr_its::json::from_canonical_json::<Person>(
        &person_json(""),
    )?)
}

/// Returns the `(method, path, operationId)` of every operation `document`
/// declares under `paths`.
fn declared_operations(document: &str) -> BTreeSet<(String, String, String)> {
    let mut found = BTreeSet::new();
    let mut in_paths = false;
    let mut current_path = String::new();
    let mut current_method = String::new();
    for line in document.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            in_paths = line == "paths:";
            continue;
        }
        if !in_paths {
            continue;
        }
        if let Some(path) = line
            .strip_prefix("  /")
            .and_then(|rest| rest.strip_suffix(':'))
        {
            current_path = format!("/{path}");
        } else if let Some(verb) = line
            .strip_prefix("    ")
            .and_then(|rest| rest.strip_suffix(':'))
            .filter(|verb| ["get", "put", "post", "delete"].contains(verb))
        {
            current_method = verb.to_uppercase();
        } else if let Some(id) = line.trim_start().strip_prefix("operationId: ") {
            found.insert((current_method.clone(), current_path.clone(), id.to_owned()));
        }
    }
    found
}

#[test]
fn the_generated_client_carries_every_operation_of_the_vendored_document() {
    let declared = declared_operations(DOCUMENT);
    let generated: BTreeSet<(String, String, String)> = ROUTES
        .iter()
        .map(|(verb, path, id)| ((*verb).to_owned(), (*path).to_owned(), (*id).to_owned()))
        .collect();
    assert!(
        !declared.is_empty(),
        "the vendored document declares no operation"
    );
    assert_eq!(declared, generated);
}

#[test]
fn the_vendored_document_is_marked_development() {
    assert!(
        DOCUMENT
            .lines()
            .any(|line| line.trim() == "x-status: DEVELOPMENT"),
        "the Demographic document is no longer DEVELOPMENT at the pinned tag"
    );
}

#[tokio::test]
async fn the_demographic_api_is_unreachable_unless_turned_on() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    assert!(client(&server, false)?.demographic().is_none());
    assert!(client(&server, true)?.demographic().is_some());
    let default = CdrConfig::new(format!("{}/v1", server.uri()).parse()?);
    assert!(!default.demographic);
    Ok(())
}

#[tokio::test]
async fn a_release_1_1_0_person_reads_with_its_reverse_relationships_dropped()
-> Result<(), Box<dyn Error>> {
    // RM Demographic IM Release 1.1.0 §2.2.1: `reverse_relationships` is a
    // `List<LOCATABLE_REF>` each of whose entries names a relationship the repository holds.
    let server = MockServer::start().await;
    let extra = format!("\"reverse_relationships\": [{RELATIONSHIP_REF}, {RELATIONSHIP_REF}],");
    person_answers(&server, person_json(&extra)).await;
    let cdr = client(&server, true)?;
    let read = demographic(&cdr)?.person_get(PERSON_UID, None).await?;
    assert_eq!(2, read.reverse_relationships_dropped);
    let PersonGetOutcome::Ok { body, .. } = read.answered.outcome else {
        return Err("the person read as no person".into());
    };
    assert_eq!(
        "openEHR-DEMOGRAPHIC-PERSON.person.v2",
        body.archetype_node_id
    );
    assert!(
        read.answered
            .upstream
            .body()
            .contains("reverse_relationships")
    );
    Ok(())
}

#[tokio::test]
async fn a_person_without_reverse_relationships_reads_with_nothing_dropped()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    person_answers(&server, person_json("")).await;
    let cdr = client(&server, true)?;
    let read = demographic(&cdr)?.person_get(PERSON_UID, None).await?;
    assert_eq!(0, read.reverse_relationships_dropped);
    assert!(matches!(read.answered.outcome, PersonGetOutcome::Ok { .. }));
    Ok(())
}

#[tokio::test]
async fn an_empty_reverse_relationships_list_is_refused() -> Result<(), Box<dyn Error>> {
    // RM Demographic IM Release 1.1.0 §2.2.1, `Reverse_relationships_validity`:
    // "reverse_relationships /= Void implies not reverse_relationships.empty".
    let server = MockServer::start().await;
    person_answers(&server, person_json("\"reverse_relationships\": [],")).await;
    let cdr = client(&server, true)?;
    let refused = demographic(&cdr)?.person_get(PERSON_UID, None).await;
    assert!(
        matches!(
            refused,
            Err(CdrError::Client {
                ref source,
                ..
            }) if matches!(**source, ClientError::Body { .. })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_release_1_1_0_does_not_define_is_refused() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    person_answers(&server, person_json("\"nickname\": \"synthetic\",")).await;
    let cdr = client(&server, true)?;
    let refused = demographic(&cdr)?.person_get(PERSON_UID, None).await;
    assert!(
        matches!(
            refused,
            Err(CdrError::Client {
                ref source,
                ..
            }) if matches!(**source, ClientError::Body { .. })
        ),
        "{refused:?}"
    );
    Ok(())
}

/// Mounts `status` with an openEHR error body (and `etag`, when given) on
/// `verb` of the person `uid_based_id` path, or of the collection when `None`.
async fn refusal(
    server: &MockServer,
    verb: &str,
    uid_based_id: Option<&str>,
    status: u16,
    etag: Option<&str>,
) {
    let route = uid_based_id.map_or_else(
        || String::from("/v1/demographic/person"),
        |uid| format!("/v1/demographic/person/{uid}"),
    );
    let mut answer = ResponseTemplate::new(status)
        .insert_header("Content-Type", "application/json")
        .set_body_string(
            r#"{"message":"synthetic refusal","validationErrors":["synthetic detail"]}"#,
        );
    if let Some(etag) = etag {
        answer = answer.insert_header("ETag", etag);
    }
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(answer)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_400_on_create_is_the_bad_request_row() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    refusal(&server, "POST", None, 400, None).await;
    let cdr = client(&server, true)?;
    let answered = demographic(&cdr)?
        .person_create(&person()?, &support::commit_context()?, Prefer::Minimal)
        .await?
        .answered;
    assert!(matches!(
        answered.outcome,
        PersonCreateOutcome::BadRequest { .. }
    ));
    let answer = of_demographic_refusal(&answered.upstream).ok_or("a documented refusal")?;
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, answer.status());
    assert_eq!(IssueType::Exception, answer.row().issue());
    Ok(())
}

#[tokio::test]
async fn a_404_on_read_is_the_not_found_row() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    refusal(&server, "GET", Some(PERSON_UID), 404, None).await;
    let cdr = client(&server, true)?;
    let answered = demographic(&cdr)?
        .person_get(PERSON_UID, None)
        .await?
        .answered;
    assert!(matches!(
        answered.outcome,
        PersonGetOutcome::NotFound { .. }
    ));
    let answer = of_demographic_refusal(&answered.upstream).ok_or("a documented refusal")?;
    assert_eq!(StatusCode::NOT_FOUND, answer.status());
    assert_eq!(IssueType::NotFound, answer.row().issue());
    Ok(())
}

#[tokio::test]
async fn a_409_on_delete_is_the_conflict_row_with_the_latest_version() -> Result<(), Box<dyn Error>>
{
    let server = MockServer::start().await;
    let latest = "\"a1b2c3d4-0000-4000-8000-000000000001::synthetic.example::2\"";
    refusal(&server, "DELETE", Some(PERSON_UID), 409, Some(latest)).await;
    let cdr = client(&server, true)?;
    let version = version_from_etag(&format!("\"{PERSON_UID}\""))?;
    let answered = demographic(&cdr)?
        .person_delete(&version, &support::commit_context()?)
        .await?
        .answered;
    assert!(matches!(
        answered.outcome,
        PersonDeleteOutcome::Conflict { .. }
    ));
    let answer = of_demographic_refusal(&answered.upstream).ok_or("a documented refusal")?;
    assert_eq!(StatusCode::CONFLICT, answer.status());
    assert_eq!(IssueType::Conflict, answer.row().issue());
    assert_eq!(Some(latest), answer.entity_tag());
    Ok(())
}

#[tokio::test]
async fn a_412_on_update_is_the_precondition_row_with_the_latest_version()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    let latest = "\"a1b2c3d4-0000-4000-8000-000000000001::synthetic.example::2\"";
    refusal(&server, "PUT", Some(PERSON_UID), 412, Some(latest)).await;
    let cdr = client(&server, true)?;
    let version = version_from_etag(&format!("\"{PERSON_UID}\""))?;
    let answered = demographic(&cdr)?
        .person_update(
            PERSON_UID,
            &version,
            &person()?,
            &support::commit_context()?,
            Prefer::Minimal,
        )
        .await?
        .answered;
    assert!(matches!(
        answered.outcome,
        PersonUpdateOutcome::PreconditionFailed { .. }
    ));
    let answer = of_demographic_refusal(&answered.upstream).ok_or("a documented refusal")?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, answer.status());
    assert_eq!(Some(latest), answer.entity_tag());
    Ok(())
}

#[tokio::test]
async fn a_422_on_create_is_the_unprocessable_row_with_the_validation_errors()
-> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    refusal(&server, "POST", None, 422, None).await;
    let cdr = client(&server, true)?;
    let answered = demographic(&cdr)?
        .person_create(&person()?, &support::commit_context()?, Prefer::Minimal)
        .await?
        .answered;
    assert!(matches!(
        answered.outcome,
        PersonCreateOutcome::UnprocessableEntity { .. }
    ));
    let answer = of_demographic_refusal(&answered.upstream).ok_or("a documented refusal")?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, answer.status());
    assert_eq!(IssueType::Invalid, answer.row().issue());
    let diagnostics = answer.issue().diagnostics().unwrap_or_default();
    assert!(diagnostics.contains("synthetic detail"), "{diagnostics}");
    Ok(())
}
