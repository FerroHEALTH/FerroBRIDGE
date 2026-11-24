// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SMART Health Links over the patient summary: a person creates, lists and
//! revokes their links, and a receiver resolves one as SMART Health Cards
//! and Links 1.0.0 (`links-specification`) has it, decrypting the file with
//! the key from the payload alone.

use std::error::Error;
use std::sync::Arc;

use aws_lc_rs::aead::AES_256_GCM;
use aws_lc_rs::aead::Aad;
use aws_lc_rs::aead::LessSafeKey;
use aws_lc_rs::aead::Nonce;
use aws_lc_rs::aead::UnboundKey;
use axum::Router;
use axum::body::Body;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrobridge_server::shl::Links;
use ferrobridge_server::shl::Settings;
use ferrobridge_server::shl::store::Answer;
use ferrobridge_server::shl::store::Ending;
use ferrobridge_server::shl::store::Fetched;
use ferrobridge_server::shl::store::FileEntry;
use ferrobridge_server::shl::store::Link;
use ferrobridge_server::shl::store::LinkStore;
use ferrobridge_server::shl::store::Resolution;
use http::Request;
use http::StatusCode;
use wiremock::MockServer;

use super::Harness;
use super::auth::Issuer;
use super::auth::authority;
use super::auth::bearing;
use super::auth::ec_key;
use super::auth::mint;
use super::call;
use super::raw;
use super::summary::UID;
use super::summary::composition;
use super::summary::harness;
use super::summary::mount_record;
use super::summary::mount_terminology;
use super::summary::patient_claims;

/// The public origin the links are served at.
const ORIGIN: &str = "https://bridge.example";

/// The content type of a link's FHIR file.
const FHIR_CONTENT: &str = "application/fhir+json;fhirVersion=4.0.1";

/// Everything one case drives.
struct Case {
    /// The facade's stub CDR and the facade, kept alive.
    _harness: Harness,
    /// The stub terminology server, kept alive.
    _term: MockServer,
    /// The authorization server, kept alive.
    issuer: Issuer,
    /// The key the tokens are signed with.
    signing: super::auth::SigningKey,
    /// The directory the link store lives in.
    dir: tempfile::TempDir,
    /// The application.
    app: Router,
}

/// Returns a case over the patient summary with the links lane on,
/// recording through `log`; `None` when the profile packages are not
/// fetched.
async fn case(
    log: Option<Arc<ferrobridge_server::audit::AccessLog>>,
) -> Result<Option<Case>, Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(None);
    };
    mount_terminology(&term).await;
    mount_record(&harness.cdr, &[composition()?]).await;
    let signing = ec_key("k1", jsonwebtoken::Algorithm::ES256);
    let issuer = Issuer::start(&[&signing]).await;
    let dir = tempfile::tempdir()?;
    let links = Links::open(settings(&dir), Arc::clone(&harness.facade), log.clone())?;
    let state = ferrobridge_server::state::AppState::with_health(
        ferrohealth_health::registry::Registry::default(),
    )
    .with_facade(Arc::clone(&harness.facade))
    .with_auth(authority(issuer.settings()).await)
    .with_links(Arc::new(links));
    let state = match log {
        Some(log) => state.with_access_log(log),
        None => state,
    };
    let app = ferrobridge_server::router(Arc::new(state), &super::server_settings());
    Ok(Some(Case {
        _harness: harness,
        _term: term,
        issuer,
        signing,
        dir,
        app,
    }))
}

/// The lane's settings over a store in `dir`.
fn settings(dir: &tempfile::TempDir) -> Settings {
    Settings {
        store: dir.path().join("links.redb"),
        base_url: String::from(ORIGIN),
        default_expiry: std::time::Duration::from_hours(24),
        max_expiry: std::time::Duration::from_hours(168),
        max_answers: 10,
        passcode_attempts: 5,
        location_lifetime: std::time::Duration::from_hours(1),
    }
}

impl Case {
    /// Returns a token of the person launched for `patient`.
    fn token(&self, patient: &str) -> String {
        mint(&self.signing, &patient_claims(&self.issuer, patient))
    }

    /// Sends `request` and returns the status and the JSON body.
    async fn send(
        &self,
        request: Request<Body>,
    ) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
        call(self.app.clone(), request).await
    }

    /// Creates a link as the synthetic patient with `body`.
    async fn create(
        &self,
        body: &serde_json::Value,
    ) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
        let (status, created) = self
            .send(bearing(
                Request::post("/shl")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))?,
                &self.token(UID),
            ))
            .await?;
        if status == StatusCode::BAD_REQUEST || status == StatusCode::CREATED {
            Ok((status, created))
        } else {
            Err(format!("{status}: {created}").into())
        }
    }

    /// Sends a manifest request for `payload` naming `recipient`, with
    /// `passcode` when given.
    async fn manifest(
        &self,
        payload: &serde_json::Value,
        passcode: Option<&str>,
    ) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
        let mut body = serde_json::json!({"recipient": "Dr. Receiver"});
        if let Some(passcode) = passcode {
            body["passcode"] = serde_json::json!(passcode);
        }
        self.send(
            Request::post(path_of(payload["url"].as_str().ok_or("the url")?)?)
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))?,
        )
        .await
    }

    /// Fetches `url`, a file location or a direct file request.
    async fn fetch(&self, url: &str) -> Result<(StatusCode, String, String), Box<dyn Error>> {
        let response = raw(
            self.app.clone(),
            Request::get(path_of(url)?).body(Body::empty())?,
        )
        .await?;
        let status = response.status();
        let content_type = response
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await?;
        Ok((status, content_type, String::from_utf8(bytes.to_vec())?))
    }
}

/// Returns the path and query of `url`, an absolute URL under [`ORIGIN`].
fn path_of(url: &str) -> Result<String, Box<dyn Error>> {
    Ok(url
        .strip_prefix(ORIGIN)
        .ok_or_else(|| format!("{url} is not under {ORIGIN}"))?
        .to_owned())
}

/// Decodes a `shlink:/` URI into its payload.
fn decode(shlink: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    let encoded = shlink
        .strip_prefix("shlink:/")
        .ok_or("the shlink:/ prefix")?;
    Ok(serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded)?)?)
}

/// Opens the compact JWE `jwe` under the payload's `key`, returning its
/// protected header and its plaintext.
fn open(key: &str, jwe: &str) -> Result<(serde_json::Value, serde_json::Value), Box<dyn Error>> {
    let parts: Vec<&str> = jwe.split('.').collect();
    let [protected, encrypted_key, iv, ciphertext, tag] = parts.as_slice() else {
        return Err(format!("{} JWE parts", parts.len()).into());
    };
    assert!(encrypted_key.is_empty(), "alg dir carries no encrypted key");
    let header = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(protected)?)?;
    let key = LessSafeKey::new(
        UnboundKey::new(&AES_256_GCM, &URL_SAFE_NO_PAD.decode(key)?).map_err(|_e| "the key")?,
    );
    let nonce =
        Nonce::try_assume_unique_for_key(&URL_SAFE_NO_PAD.decode(iv)?).map_err(|_e| "the iv")?;
    let mut sealed = URL_SAFE_NO_PAD.decode(ciphertext)?;
    sealed.extend(URL_SAFE_NO_PAD.decode(tag)?);
    let opened = key
        .open_in_place(nonce, Aad::from(protected.as_bytes()), &mut sealed)
        .map_err(|_e| "the tag")?;
    Ok((header, serde_json::from_slice(opened)?))
}

/// The resource types of `bundle`'s entries, in order.
fn kinds(bundle: &serde_json::Value) -> Vec<String> {
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["resource"]["resourceType"].as_str())
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn a_link_with_a_passcode_opens_once_in_a_receiver_and_yields_the_summary()
-> Result<(), Box<dyn Error>> {
    let Some(case) = case(None).await? else {
        return Ok(());
    };
    let (status, created) = case
        .create(&serde_json::json!({"passcode": "correct horse", "label": "For Dr. Receiver"}))
        .await?;
    assert_eq!(StatusCode::CREATED, status, "{created}");
    let payload = decode(created["shlink"].as_str().ok_or("the shlink")?)?;
    assert_eq!(payload, created["payload"]);
    assert_eq!("P", payload["flag"]);
    assert_eq!("For Dr. Receiver", payload["label"]);
    let url = payload["url"].as_str().ok_or("the url")?;
    assert!(url.len() <= 128, "{url}");
    let id = url.rsplit('/').next().ok_or("the id")?;
    assert_eq!(32, URL_SAFE_NO_PAD.decode(id)?.len(), "256 bits of entropy");
    let key = payload["key"].as_str().ok_or("the key")?;
    assert_eq!(43, key.len());
    let exp = payload["exp"].as_i64().ok_or("exp")?;
    let now = jiff::Timestamp::now().as_second();
    assert!((now + 24 * 3600 - exp).abs() < 60, "24 hours by default");

    let (status, manifest) = case.manifest(&payload, Some("correct horse")).await?;
    assert_eq!(StatusCode::OK, status, "{manifest}");
    assert_eq!("finalized", manifest["status"]);
    let file = &manifest["files"][0];
    assert_eq!(FHIR_CONTENT, file["contentType"]);
    assert!(file["embedded"].is_null(), "{file}");
    let location = file["location"].as_str().ok_or("the location")?;
    let (status, content_type, jwe) = case.fetch(location).await?;
    assert_eq!(StatusCode::OK, status, "{jwe}");
    assert_eq!("application/jose", content_type);
    let (header, document) = open(key, &jwe)?;
    assert_eq!(
        serde_json::json!({"alg": "dir", "enc": "A256GCM", "cty": FHIR_CONTENT}),
        header
    );
    let (status, served) = case
        .send(bearing(
            Request::get(format!("/fhir/Patient/{UID}/$summary")).body(Body::empty())?,
            &case.token(UID),
        ))
        .await?;
    assert_eq!(StatusCode::OK, status, "{served}");
    assert_eq!("document", document["type"]);
    assert_eq!(served["meta"], document["meta"]);
    assert_eq!(kinds(&served), kinds(&document));
    assert_eq!(served["entry"][1], document["entry"][1], "the same patient");
    super::summary::emit("server-shl-ips", &document)?;

    let (status, _, _) = case.fetch(location).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "a location is single-use");
    let (status, again) = case.manifest(&payload, Some("correct horse")).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "one answer by default: {again}"
    );
    assert_eq!("OperationOutcome", again["resourceType"]);
    Ok(())
}

#[tokio::test]
async fn wrong_passcodes_count_down_and_the_fifth_disables_the_link() -> Result<(), Box<dyn Error>>
{
    let Some(case) = case(None).await? else {
        return Ok(());
    };
    let (_, created) = case
        .create(&serde_json::json!({"passcode": "1234"}))
        .await?;
    let payload = &created["payload"];
    let (status, body) = case.manifest(payload, None).await?;
    assert_eq!(
        StatusCode::UNAUTHORIZED,
        status,
        "a missing passcode is a wrong one"
    );
    assert_eq!(4, body["remainingAttempts"]);
    for remaining in [3, 2, 1, 0] {
        let (status, body) = case.manifest(payload, Some("0000")).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, status);
        assert_eq!(remaining, body["remainingAttempts"]);
    }
    let (status, _) = case.manifest(payload, Some("1234")).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "a locked link is no longer active"
    );
    Ok(())
}

#[tokio::test]
async fn parallel_guesses_take_exactly_the_lifetime_attempts() -> Result<(), Box<dyn Error>> {
    let Some(case) = case(None).await? else {
        return Ok(());
    };
    let (_, created) = case
        .create(&serde_json::json!({"passcode": "1234"}))
        .await?;
    let payload = created["payload"].clone();
    let guesses = (0..20).map(|_| case.manifest(&payload, Some("0000")));
    let answers = futures_util::future::join_all(guesses).await;
    let mut remaining = Vec::new();
    let mut gone = 0;
    for answer in answers {
        let (status, body) = answer?;
        match status {
            StatusCode::UNAUTHORIZED => {
                remaining.push(body["remainingAttempts"].as_u64().ok_or("remaining")?);
            }
            StatusCode::NOT_FOUND => gone += 1,
            other => return Err(format!("{other}: {body}").into()),
        }
    }
    remaining.sort_unstable();
    assert_eq!(vec![0, 1, 2, 3, 4], remaining);
    assert_eq!(15, gone);
    Ok(())
}

#[tokio::test]
async fn a_link_without_a_passcode_is_u_and_answers_one_direct_request()
-> Result<(), Box<dyn Error>> {
    let Some(case) = case(None).await? else {
        return Ok(());
    };
    let (status, created) = case.create(&serde_json::json!({})).await?;
    assert_eq!(StatusCode::CREATED, status, "{created}");
    let payload = &created["payload"];
    assert_eq!("U", payload["flag"]);
    let url = payload["url"].as_str().ok_or("the url")?;
    let (status, _, _) = case.fetch(url).await?;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        status,
        "a direct request names its recipient"
    );
    let (status, _) = case.manifest(payload, None).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "a U link answers no manifest request"
    );
    let (status, content_type, jwe) = case
        .fetch(&format!("{url}?recipient=Dr.%20Receiver"))
        .await?;
    assert_eq!(StatusCode::OK, status, "{jwe}");
    assert_eq!("application/jose", content_type);
    let (_, document) = open(payload["key"].as_str().ok_or("the key")?, &jwe)?;
    assert_eq!("Bundle", document["resourceType"]);
    let (status, _, _) = case
        .fetch(&format!("{url}?recipient=Dr.%20Receiver"))
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    Ok(())
}

#[tokio::test]
async fn a_link_gives_the_answers_it_was_created_with() -> Result<(), Box<dyn Error>> {
    let Some(case) = case(None).await? else {
        return Ok(());
    };
    let (_, created) = case
        .create(&serde_json::json!({"passcode": "1234", "answers": 2}))
        .await?;
    let payload = &created["payload"];
    for _ in 0..2 {
        let (status, manifest) = case.manifest(payload, Some("1234")).await?;
        assert_eq!(StatusCode::OK, status, "{manifest}");
        let location = manifest["files"][0]["location"]
            .as_str()
            .ok_or("location")?;
        assert_eq!(StatusCode::OK, case.fetch(location).await?.0);
    }
    let (status, _) = case.manifest(payload, Some("1234")).await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    Ok(())
}

#[tokio::test]
async fn a_small_enough_file_is_embedded() -> Result<(), Box<dyn Error>> {
    let Some(case) = case(None).await? else {
        return Ok(());
    };
    let (_, created) = case
        .create(&serde_json::json!({"passcode": "1234"}))
        .await?;
    let payload = &created["payload"];
    let (status, manifest) = case
        .send(
            Request::post(path_of(payload["url"].as_str().ok_or("url")?)?)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::json!({"recipient": "r", "passcode": "1234", "embeddedLengthMax": 10_000_000})
                        .to_string(),
                ))?,
        )
        .await?;
    assert_eq!(StatusCode::OK, status, "{manifest}");
    let file = &manifest["files"][0];
    assert!(file["location"].is_null(), "{file}");
    let (_, document) = open(
        payload["key"].as_str().ok_or("key")?,
        file["embedded"].as_str().ok_or("embedded")?,
    )?;
    assert_eq!("document", document["type"]);
    Ok(())
}

#[tokio::test]
async fn the_person_lists_and_revokes_only_their_own_links() -> Result<(), Box<dyn Error>> {
    let Some(case) = case(None).await? else {
        return Ok(());
    };
    let (_, created) = case
        .create(&serde_json::json!({"passcode": "1234", "label": "mine"}))
        .await?;
    let handle = created["handle"].as_str().ok_or("the handle")?;
    let list = |patient: &str| {
        bearing(
            Request::get("/shl").body(Body::empty()).unwrap_or_default(),
            &case.token(patient),
        )
    };
    let (status, listed) = case.send(list(UID)).await?;
    assert_eq!(StatusCode::OK, status, "{listed}");
    assert_eq!(handle, listed["links"][0]["handle"]);
    assert_eq!("mine", listed["links"][0]["label"]);
    assert_eq!(true, listed["links"][0]["passcode"]);
    let other = "a1b2c3d4-0000-4000-8000-0000000020ff";
    let (_, theirs) = case.send(list(other)).await?;
    assert_eq!(serde_json::json!([]), theirs["links"]);
    let revoke = |patient: &str| {
        bearing(
            Request::delete(format!("/shl/{handle}"))
                .body(Body::empty())
                .unwrap_or_default(),
            &case.token(patient),
        )
    };
    let (status, refused) = case.send(revoke(other)).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{refused}");
    assert_eq!("OperationOutcome", refused["resourceType"]);
    let (status, _) = case.send(revoke(UID)).await?;
    assert_eq!(StatusCode::NO_CONTENT, status);
    let (_, listed) = case.send(list(UID)).await?;
    assert_eq!(serde_json::json!([]), listed["links"]);
    let (status, _) = case.manifest(&created["payload"], Some("1234")).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "a revoked link is no longer active"
    );
    Ok(())
}

#[tokio::test]
async fn a_create_outside_the_limits_or_without_a_person_is_refused() -> Result<(), Box<dyn Error>>
{
    let Some(case) = case(None).await? else {
        return Ok(());
    };
    let now = jiff::Timestamp::now().as_second();
    for body in [
        serde_json::json!({"exp": now - 1}),
        serde_json::json!({"exp": now + 8 * 24 * 3600}),
        serde_json::json!({"answers": 0}),
        serde_json::json!({"answers": 11}),
        serde_json::json!({"label": "x".repeat(81)}),
        serde_json::json!({"passcode": ""}),
        serde_json::json!({"unknown": true}),
    ] {
        let (status, refused) = case.create(&body).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{body}: {refused}");
        assert_eq!("OperationOutcome", refused["resourceType"]);
    }
    let (status, refused) = case.send(Request::get("/shl").body(Body::empty())?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "{refused}");
    assert_eq!("OperationOutcome", refused["resourceType"]);
    let machine = mint(&case.signing, &case.issuer.claims("patient/*.rs"));
    let (status, refused) = case
        .send(bearing(Request::get("/shl").body(Body::empty())?, &machine))
        .await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{refused}");
    Ok(())
}

#[tokio::test]
async fn neither_the_store_nor_the_log_holds_the_key_the_document_or_the_passcode()
-> Result<(), Box<dyn Error>> {
    let logs = crate::support::Logs::default();
    let _capture = tracing::subscriber::set_default(crate::support::subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "trace",
        false,
        logs.clone(),
    ));
    let Some(case) = case(None).await? else {
        return Ok(());
    };
    let (_, created) = case
        .create(&serde_json::json!({"passcode": "correct horse"}))
        .await?;
    let key = created["payload"]["key"].as_str().ok_or("the key")?;
    let held = std::fs::read(case.dir.path().join("links.redb"))?;
    let text = String::from_utf8_lossy(&held);
    let logged = logs.text();
    assert!(logged.contains("/shl"), "the log of the request is read");
    for (what, secret) in [
        ("the key", key),
        ("the passcode", "correct horse"),
        ("the identifier", "SYN-PAT-2014"),
        ("the name", "Synthetic Subject"),
    ] {
        assert!(!text.contains(secret), "the store holds {what}");
        assert!(!logged.contains(secret), "the log holds {what}");
    }
    assert!(
        !text.contains("\"resourceType\""),
        "the store holds a document"
    );
    assert!(
        text.contains("$argon2id$"),
        "the passcode is held as its hash"
    );
    Ok(())
}

#[tokio::test]
async fn each_event_of_a_link_is_recorded_with_its_recipient() -> Result<(), Box<dyn Error>> {
    let Some((_postgres, store)) = crate::audit::state().await? else {
        return Ok(());
    };
    let arr = crate::audit::repository().await;
    let cdr: url::Url = "http://cdr.invalid/".parse()?;
    let log = crate::audit::access_log(
        &arr,
        &store,
        &cdr,
        ehds_logging::map::CategoryMap::default(),
    )?;
    let Some(case) = case(Some(log)).await? else {
        return Ok(());
    };
    let (_, created) = case
        .create(&serde_json::json!({"passcode": "1234"}))
        .await?;
    let payload = &created["payload"];
    case.manifest(payload, Some("0000")).await?;
    case.manifest(payload, Some("1234")).await?;
    let handle = created["handle"].as_str().ok_or("handle")?;
    let records = crate::audit::records(&arr, 3).await?;
    let text = serde_json::to_string(&records)?;
    assert!(text.contains(handle), "the records name the link: {text}");
    assert!(
        text.contains("Dr. Receiver"),
        "the records name the recipient"
    );
    assert!(
        !text.contains(payload["key"].as_str().ok_or("key")?),
        "never the key"
    );
    let actions: Vec<(&str, &str)> = records
        .iter()
        .map(|record| {
            (
                record["action"].as_str().unwrap_or_default(),
                record["outcome"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    for expected in [("C", "0"), ("R", "4"), ("R", "0")] {
        assert!(actions.contains(&expected), "{expected:?} in {actions:?}");
    }
    Ok(())
}

#[tokio::test]
async fn the_capability_statement_names_where_links_are_created() -> Result<(), Box<dyn Error>> {
    let Some(case) = case(None).await? else {
        return Ok(());
    };
    let (status, statement) = case
        .send(Request::get("/fhir/metadata").body(Body::empty())?)
        .await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(
        serde_json::json!([{
            "url": "urn:ferrobridge:extension:smart-health-links",
            "valueUrl": format!("{ORIGIN}/shl")
        }]),
        statement["rest"][0]["extension"]
    );
    Ok(())
}

/// A link of one sealed file, resolved by passcode, as the store holds it.
fn stored(expires: i64) -> Link {
    Link {
        handle: String::from("h-1"),
        owner: String::from(UID),
        created: 0,
        expires,
        label: None,
        resolution: Resolution::Passcode,
        passcode: Some(String::from(
            "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaA",
        )),
        attempts_left: 5,
        answers_left: 1,
        files: vec![FileEntry {
            content_type: String::from(FHIR_CONTENT),
            last_updated: String::from("1970-01-01T00:00:00Z"),
        }],
        outstanding: 0,
    }
}

#[test]
fn an_expired_link_and_a_lapsed_location_answer_inactive() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let store = LinkStore::open(&dir.path().join("links.redb"))?;
    let mut link = stored(1000);
    link.passcode = None;
    link.resolution = Resolution::Direct;
    store.create("expired", &link, &[String::from("jwe")])?;
    assert_eq!(
        Fetched::Inactive {
            gone: Some(ferrobridge_server::shl::store::Gone {
                handle: String::from("h-1"),
                owner: String::from(UID),
                ending: Ending::Expired,
            })
        },
        store.direct("expired", 1000)?
    );
    let mut answered = stored(10_000);
    answered.passcode = None;
    answered.resolution = Resolution::Direct;
    answered.handle = String::from("h-2");
    store.create("by-location", &answered, &[String::from("jwe")])?;
    let swept = store.sweep(9_999)?;
    assert!(swept.is_empty(), "{swept:?}");
    let swept = store.sweep(10_000)?;
    assert_eq!(1, swept.len());
    assert_eq!(Ending::Expired, swept[0].ending);
    assert_eq!(
        Vec::<ferrobridge_server::shl::store::Listed>::new(),
        store.list(UID, 0)?
    );
    Ok(())
}

#[test]
fn a_location_lapses_after_its_lifetime() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let store = LinkStore::open(&dir.path().join("links.redb"))?;
    let mut link = stored(100_000);
    link.passcode = None;
    store.create("lapsing", &link, &[String::from("jwe")])?;
    let Answer::Manifest { files, used, .. } = store.answer("lapsing", None, None, (0, 3600))?
    else {
        return Err("a manifest".into());
    };
    assert!(!used, "the location is outstanding");
    let token = files[0].location.clone().ok_or("a location")?;
    let fetched = store.fetch(&token, 3600)?;
    let Fetched::Inactive { gone: Some(gone) } = fetched else {
        return Err(format!("{fetched:?}").into());
    };
    assert_eq!(Ending::Used, gone.ending, "the link's last location lapsed");
    assert_eq!(Fetched::Inactive { gone: None }, store.fetch(&token, 0)?);
    Ok(())
}

#[test]
fn two_seals_of_one_file_use_two_nonces_and_open_under_the_key() -> Result<(), Box<dyn Error>> {
    use secrecy::ExposeSecret as _;

    let key = ferrobridge_server::shl::seal::LinkKey::generate()?;
    let first = key.seal(FHIR_CONTENT, b"{\"resourceType\":\"Bundle\"}")?;
    let second = key.seal(FHIR_CONTENT, b"{\"resourceType\":\"Bundle\"}")?;
    assert_ne!(first.split('.').nth(2), second.split('.').nth(2));
    let encoded = key.encoded();
    assert_eq!(43, encoded.expose_secret().len());
    let (_, opened) = open(encoded.expose_secret(), &first)?;
    assert_eq!("Bundle", opened["resourceType"]);
    let token = ferrobridge_server::shl::seal::random_token()?;
    assert_eq!(32, URL_SAFE_NO_PAD.decode(&token)?.len());
    Ok(())
}

#[test]
fn a_passcode_hash_is_argon2id_and_matches_only_its_passcode() -> Result<(), Box<dyn Error>> {
    use ferrobridge_server::shl::passcode;

    let stored = passcode::hash(&secrecy::SecretString::from("1234"))?;
    assert!(stored.starts_with("$argon2id$"), "{stored}");
    assert!(passcode::matches("1234", &stored)?);
    assert!(!passcode::matches("4321", &stored)?);
    Ok(())
}
