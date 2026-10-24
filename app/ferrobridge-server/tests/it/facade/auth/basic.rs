// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An HTTP Basic caller (RFC 7617) of `[[auth.basic.users]]` beside the
//! bearer tokens: the user's configured scopes decide what it reaches, and a
//! refused credential is the generic `401`.

use std::error::Error;
use std::sync::Arc;

use argon2::Argon2;
use argon2::password_hash::PasswordHasher;
use ferrohealth_smart::basic::Credential;
use ferrohealth_smart::basic::PasswordStore;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;
use secrecy::SecretString;

use super::Issuer;
use super::authority;
use super::challenge;
use super::ec_key;
use super::get;
use super::secured;
use crate::facade::condition;
use crate::facade::harness;
use crate::facade::post_condition;
use crate::facade::raw;

/// The read every case sends: an id the map does not know, so a request the
/// layer admits answers the handler's `404`.
const UNKNOWN: &str = "/fhir/Condition/never-written";

/// Returns the Argon2id PHC string of `password` under a fixed salt.
fn hash(password: &str) -> Result<SecretString, Box<dyn Error>> {
    Ok(SecretString::from(
        Argon2::default()
            .hash_password_with_salt(password.as_bytes(), b"synthetic-salt-16")
            .map_err(|error| error.to_string())?
            .to_string(),
    ))
}

/// Returns the router of a facade that admits the Basic user `etl` with
/// `system/Condition.r` beside the issuer's tokens.
async fn app() -> Result<axum::Router, Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let mut settings = issuer.settings();
    settings.basic = Some(Arc::new(PasswordStore::new().with_user(
        "etl",
        hash("synthetic-password")?,
        "system/Condition.r",
    )?));
    let harness = harness().await;
    Ok(secured(&harness, &authority(settings).await))
}

/// Returns `request` carrying the Basic credential of `user` and `password`.
fn basic(
    mut request: http::Request<axum::body::Body>,
    user: &str,
    password: &str,
) -> Result<http::Request<axum::body::Body>, Box<dyn Error>> {
    let credential = Credential::new(user, SecretString::from(password))?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, credential.header_value());
    Ok(request)
}

#[tokio::test]
async fn a_basic_user_reaches_what_its_scopes_grant() -> Result<(), Box<dyn Error>> {
    let app = app().await?;
    let read = raw(
        app.clone(),
        basic(get(UNKNOWN), "etl", "synthetic-password")?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, read.status());
    let create = raw(
        app,
        basic(post_condition(&condition()), "etl", "synthetic-password")?,
    )
    .await?;
    assert_eq!(StatusCode::FORBIDDEN, create.status(), "no c on Condition");
    Ok(())
}

#[tokio::test]
async fn a_wrong_password_or_an_unknown_user_is_the_same_401() -> Result<(), Box<dyn Error>> {
    let app = app().await?;
    for (user, password) in [
        ("etl", "another-password"),
        ("nobody", "synthetic-password"),
    ] {
        let response = raw(app.clone(), basic(get(UNKNOWN), user, password)?).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, response.status(), "{user}");
        // NOTE: RFC 7617 §2 and §2.1, the Basic challenge names a realm and
        // the UTF-8 charset.
        assert_eq!(
            Some("Basic realm=\"\", charset=\"UTF-8\""),
            challenge(&response).as_deref(),
            "{user}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_missing_credential_is_challenged_for_both_schemes() -> Result<(), Box<dyn Error>> {
    let response = raw(app().await?, get(UNKNOWN)).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, response.status());
    // NOTE: RFC 9110 §11.6.1, a 401 may carry one challenge per scheme.
    assert_eq!(
        Some("Bearer, Basic realm=\"\", charset=\"UTF-8\""),
        challenge(&response).as_deref()
    );
    Ok(())
}
