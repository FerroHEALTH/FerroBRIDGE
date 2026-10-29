// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every operator refusal is an RFC 9457 problem details document
//! (<https://www.rfc-editor.org/rfc/rfc9457>) whose `type` resolves to the
//! book page that documents it (§4) and which quotes nothing of the request
//! (§5).

use std::error::Error;

use axum::body::Body;
use ferrobridge_operator_api::problem::{BASE, MEDIA_TYPE, Problem};
use http::{Request, StatusCode, header};
use tower::ServiceExt as _;

use super::{Case, OPERATOR, bytes};
use crate::facade::auth::bearing;

/// Reads `response` as a problem, asserting its media type and that its
/// `status` member repeats the answer's status.
async fn problem(response: http::Response<Body>) -> Result<(Problem, String), Box<dyn Error>> {
    let status = response.status();
    let media = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    assert_eq!(Some(MEDIA_TYPE), media.as_deref(), "{status}");
    let text = String::from_utf8(bytes(response).await?)?;
    let read: Problem = serde_json::from_str(&text)?;
    assert_eq!(Some(status.as_u16()), read.status, "{text}");
    Ok((read, text))
}

#[tokio::test]
async fn the_gate_refuses_with_a_documented_problem_type() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let (missing, _) = problem(case.get("/operator/state", None).await?).await?;
    assert_eq!(Some("unauthenticated"), missing.slug());
    let (scope, _) = problem(
        case.get("/operator/state", Some("system/*.cruds openid"))
            .await?,
    )
    .await?;
    assert_eq!(Some("not-permitted"), scope.slug());
    assert!(scope.kind.starts_with(BASE), "{}", scope.kind);
    Ok(())
}

#[tokio::test]
async fn an_absent_file_and_an_unconfigured_save_name_their_types() -> Result<(), Box<dyn Error>> {
    let case = Case::new().await?;
    let (absent, _) = problem(
        case.get("/operator/mappings/fhirconnect/absent.yml", Some(OPERATOR))
            .await?,
    )
    .await?;
    assert_eq!(Some("not-found"), absent.slug());
    assert!(absent.detail.is_some(), "{absent:?}");
    let save = Request::put("/operator/mappings/fhirconnect/new.yml")
        .header(header::IF_NONE_MATCH, "*")
        .header("ferrobridge-reason", "synthetic")
        .body(Body::from("grammar: FHIRConnect/v1.0.0\n"))?;
    let response = case
        .app
        .clone()
        .oneshot(bearing(save, &case.token(OPERATOR)))
        .await?;
    assert_eq!(StatusCode::CONFLICT, response.status());
    let (unconfigured, _) = problem(response).await?;
    assert_eq!(Some("not-configured"), unconfigured.slug());
    Ok(())
}

#[tokio::test]
async fn no_problem_quotes_a_path_a_file_a_body_a_reason_or_a_token() -> Result<(), Box<dyn Error>>
{
    let case = Case::new().await?;
    let outside = case.tree.path().join("outside.yml");
    std::fs::write(&outside, "secret: outside the mapping tree\n")?;
    std::os::unix::fs::symlink(&outside, case.tree.path().join("mappings/link.yml"))?;
    let token = case.token(OPERATOR);
    let escaped = case
        .get("/operator/mappings/fhirconnect/link.yml", Some(OPERATOR))
        .await?;
    let (_, escaped) = problem(escaped).await?;
    let save = Request::put("/operator/mappings/fhirconnect/drafted.yml")
        .header(header::IF_NONE_MATCH, "*")
        .header("ferrobridge-reason", "a-reason-marker")
        .body(Body::from("grammar: FHIRConnect/v1.0.0\nbody-marker: 1\n"))?;
    let saved = case.app.clone().oneshot(bearing(save, &token)).await?;
    let (_, saved) = problem(saved).await?;
    for text in [escaped, saved] {
        for quoted in [
            "secret",
            "link.yml",
            "drafted.yml",
            case.tree.path().to_string_lossy().as_ref(),
            "body-marker",
            "a-reason-marker",
            token.as_str(),
            "panicked",
            "src/",
        ] {
            assert!(!text.contains(quoted), "{quoted} in {text}");
        }
    }
    Ok(())
}
