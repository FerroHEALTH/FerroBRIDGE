// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An HTTP Basic caller (RFC 7617) of `[[auth.basic.users]]` beside the
//! bearer tokens: always a person, the user's configured scopes decide what
//! it reaches, and a refused credential is the generic `401`.

use std::error::Error;
use std::sync::Arc;

use argon2::Argon2;
use argon2::password_hash::PasswordHasher;
use ferrohealth_smart::basic::Credential;
use ferrohealth_smart::basic::PasswordStore;
use ferrohealth_smart::basic::StoreError;
use http::StatusCode;
use http::header;
use jsonwebtoken::Algorithm;
use secrecy::SecretString;

use super::Issuer;
use super::audit::audit_lines;
use super::audit::logged;
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

/// The Basic user every case authenticates as, a person.
const USER: &str = "clinician";

/// Returns the router of a facade that admits the Basic user [`USER`] with
/// `user/Condition.r` beside the issuer's tokens.
async fn app() -> Result<axum::Router, Box<dyn Error>> {
    app_granting("user/Condition.r").await
}

/// Returns the router of a facade that admits the Basic user [`USER`] with
/// `scope` beside the issuer's tokens, built past the configuration check.
async fn app_granting(scope: &str) -> Result<axum::Router, Box<dyn Error>> {
    let key = ec_key("k1", Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let mut settings = issuer.settings();
    settings.basic = Some(Arc::new(PasswordStore::new().with_user(
        USER,
        hash("synthetic-password")?,
        scope,
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
        basic(get(UNKNOWN), USER, "synthetic-password")?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, read.status());
    let create = raw(
        app,
        basic(post_condition(&condition()), USER, "synthetic-password")?,
    )
    .await?;
    assert_eq!(StatusCode::FORBIDDEN, create.status(), "no c on Condition");
    Ok(())
}

#[tokio::test]
async fn a_wrong_password_or_an_unknown_user_is_the_same_401() -> Result<(), Box<dyn Error>> {
    let app = app().await?;
    for (user, password) in [(USER, "another-password"), ("nobody", "synthetic-password")] {
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

#[test]
fn a_basic_caller_is_audited_as_a_person_never_a_system() -> Result<(), Box<dyn Error>> {
    // NOTE: RFC 6749 §4.4, a machine authenticates with the client credentials
    // grant, so a Basic credential is a person's whatever scope it is granted.
    let text = logged(async {
        let app = app().await.expect("the facade starts");
        let response = raw(
            app,
            basic(get(UNKNOWN), USER, "synthetic-password").expect("a legal credential"),
        )
        .await
        .expect("an answer");
        assert_eq!(StatusCode::NOT_FOUND, response.status());
    });
    let lines = audit_lines(&text);
    assert_eq!(1, lines.len(), "{text}");
    let line = &lines[0];
    assert_eq!(Some(USER), line["principal"].as_str(), "{line}");
    assert_eq!(Some("basic"), line["via"].as_str(), "{line}");
    assert_eq!(Some("person"), line["kind"].as_str(), "{line}");
    // NOTE: GHSA-jhg2-jwjp-hq74, a Basic user granted a `system/` scope never
    // reaches the facade: the store refuses it, naming the user and the scope.
    for (scopes, refused) in [
        ("system/Condition.r", "system/Condition.r"),
        ("user/Condition.r system/*.read", "system/*.read"),
    ] {
        let error = PasswordStore::new()
            .with_user(USER, hash("synthetic-password")?, scopes)
            .err()
            .ok_or("a Basic user with a system scope is refused")?;
        assert!(
            matches!(
                &error,
                StoreError::SystemScope { username, scope }
                    if username == USER && scope == refused
            ),
            "{scopes}: {error:?}"
        );
    }
    Ok(())
}
