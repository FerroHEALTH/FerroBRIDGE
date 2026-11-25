// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A SMART App Launch 2.2.0 standalone patient launch through FerroSMART
//! with no browser: the authorization request with PKCE (RFC 7636 §4), the
//! upstream login answered by the stub provider, the callback, the consent
//! form and the code exchange (RFC 6749 §4.1).

use std::collections::BTreeMap;
use std::error::Error;

use ferrobridge_testkit::oidc::Provider;
use ferrobridge_testkit::oidc::TokenAnswer;
use http::StatusCode;
use http::header;
use url::Url;

/// The app's client id at FerroSMART.
pub(super) const APP: &str = "yellow-button";

/// The app's redirect URI, which no request ever reaches.
pub(super) const REDIRECT: &str = "http://127.0.0.1:9/callback";

/// FerroSMART's client id at the upstream provider.
pub(super) const UPSTREAM_CLIENT: &str = "ferrosmart";

/// The `acr` the upstream provider's logins carry, mapped to the eIDAS
/// level `substantial` in FerroSMART's `[ferrosmart.upstream.assurance]`.
pub(super) const UPSTREAM_ACR: &str = "urn:synthetic:loa:substantial";

/// The scope the app asks for.
pub(super) const SCOPE: &str = "openid fhirUser launch/patient patient/*.rs";

/// The PKCE verifier and its S256 challenge (RFC 7636 Appendix B).
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

/// The challenge of [`VERIFIER`].
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

/// A browser's cookie jar and its view of one FerroSMART.
struct Browser {
    /// The client, which follows no redirect.
    client: reqwest::Client,
    /// FerroSMART's issuer.
    issuer: String,
    /// The cookies FerroSMART set, by name.
    cookies: BTreeMap<String, String>,
}

impl Browser {
    /// Sends `request` with the cookies, keeps the cookies the answer sets,
    /// and returns the status, the `Location` and the body.
    async fn send(
        &mut self,
        request: reqwest::RequestBuilder,
    ) -> Result<(StatusCode, Option<Url>, String), Box<dyn Error>> {
        let cookie: Vec<String> = self
            .cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        let request = if cookie.is_empty() {
            request
        } else {
            request.header(header::COOKIE, cookie.join("; "))
        };
        let response = request.send().await?;
        for set in response.headers().get_all(header::SET_COOKIE) {
            let pair = set.to_str()?.split(';').next().unwrap_or_default();
            if let Some((name, value)) = pair.split_once('=') {
                self.cookies.insert(name.to_owned(), value.to_owned());
            }
        }
        let status = StatusCode::from_u16(response.status().as_u16())?;
        let location = match response.headers().get(header::LOCATION) {
            Some(value) => Some(Url::parse(&self.issuer)?.join(value.to_str()?)?),
            None => None,
        };
        Ok((status, location, response.text().await?))
    }
}

/// Returns `request` carrying `pairs` as an `application/x-www-form-urlencoded`
/// body.
fn form(request: reqwest::RequestBuilder, pairs: &[(&str, &str)]) -> reqwest::RequestBuilder {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    request
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
}

/// Returns the query parameter `name` of `url`.
fn param(url: &Url, name: &str) -> Result<String, Box<dyn Error>> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| format!("{url} carries no {name}").into())
}

/// Returns the location of a `303` answer, or an error naming `step`.
fn redirected(
    step: &str,
    (status, location, body): (StatusCode, Option<Url>, String),
) -> Result<Url, Box<dyn Error>> {
    match location {
        Some(location) if status == StatusCode::SEE_OTHER || status == StatusCode::FOUND => {
            Ok(location)
        }
        _ => Err(format!("{step} answered {status}: {body}").into()),
    }
}

/// Logs the patient `patient` in at FerroSMART `issuer` for the FHIR base
/// `aud`, signed in upstream as `subject`, approves the scope, and returns
/// the token response.
pub(super) async fn launch(
    issuer: &str,
    upstream: &Provider,
    aud: &str,
    patient: &str,
    subject: &str,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut browser = Browser {
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        issuer: issuer.to_owned(),
        cookies: BTreeMap::new(),
    };
    let authorize = Url::parse_with_params(
        &format!("{issuer}/authorize"),
        [
            ("response_type", "code"),
            ("client_id", APP),
            ("redirect_uri", REDIRECT),
            ("scope", SCOPE),
            ("state", "journey-state"),
            ("aud", aud),
            ("code_challenge", CHALLENGE),
            ("code_challenge_method", "S256"),
            ("nonce", "journey-nonce"),
        ],
    )?;
    let answer = browser.send(browser.client.get(authorize)).await?;
    let at_upstream = redirected("the authorization endpoint", answer)?;
    if !at_upstream.as_str().starts_with(&upstream.issuer()) {
        return Err(format!("the browser went to {at_upstream}, not the provider").into());
    }
    answer_login(upstream, &param(&at_upstream, "nonce")?, patient, subject).await?;
    let callback = Url::parse_with_params(
        &format!("{issuer}/callback"),
        [
            ("code", "journey-upstream-code"),
            ("state", param(&at_upstream, "state")?.as_str()),
        ],
    )?;
    let answer = browser.send(browser.client.get(callback)).await?;
    let consent = redirected("the callback", answer)?;
    if consent.path() != "/consent" {
        return Err(format!("the callback sent the browser to {consent}").into());
    }
    let (status, _, page) = browser.send(browser.client.get(consent)).await?;
    if status != StatusCode::OK {
        return Err(format!("the consent page answered {status}: {page}").into());
    }
    let form_token = page
        .split("name=\"form_token\" value=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .ok_or_else(|| format!("the consent page carries no form token: {page}"))?
        .to_owned();
    let mut consent = vec![("form_token", form_token.as_str()), ("decision", "approve")];
    consent.extend(SCOPE.split(' ').map(|scope| ("scope", scope)));
    let answer = browser
        .send(form(
            browser.client.post(format!("{issuer}/consent")),
            &consent,
        ))
        .await?;
    let back = redirected("the consent form", answer)?;
    let code = param(&back, "code")?;
    let answer = browser
        .send(form(
            browser.client.post(format!("{issuer}/token")),
            &[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", REDIRECT),
                ("code_verifier", VERIFIER),
                ("client_id", APP),
            ],
        ))
        .await?;
    match answer {
        (StatusCode::OK, _, body) => Ok(serde_json::from_str(&body)?),
        (status, _, body) => Err(format!("the token endpoint answered {status}: {body}").into()),
    }
}

/// Makes the provider answer the next code exchange with an ID Token for the
/// person `subject` whose `fhir_user` is the patient `patient`, signed in at
/// [`UPSTREAM_ACR`] for the login that sent `nonce`.
async fn answer_login(
    upstream: &Provider,
    nonce: &str,
    patient: &str,
    subject: &str,
) -> Result<(), Box<dyn Error>> {
    let now = jiff::Timestamp::now().as_second();
    let claims = serde_json::json!({
        "iss": upstream.issuer(),
        "aud": UPSTREAM_CLIENT,
        "sub": subject,
        "iat": now,
        "exp": now + 300,
        "nonce": nonce,
        "acr": UPSTREAM_ACR,
        "name": "Synthetic Patient",
        "fhir_user": format!("Patient/{patient}"),
    });
    let id_token = upstream.sign(&Provider::header(), &claims)?;
    upstream.reset().await?;
    upstream.publish_metadata().await;
    let mut answer = TokenAnswer::bearer("journey-upstream-access");
    answer.expires_in = Some(300);
    answer.id_token = Some(id_token);
    upstream
        .answer_grant("grant_type=authorization_code", &answer)
        .await?;
    Ok(())
}
