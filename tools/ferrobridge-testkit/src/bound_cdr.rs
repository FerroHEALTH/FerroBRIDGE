// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reference CDR with authentication on, reached by a Basic user bound
//! to a professional `PERSON`, the posture a deployment of it runs in.
//!
//! [`bound`] boots the CDR twice on one database. The first boot runs with
//! authentication off and uploads the professional `PERSON` and `ROLE`
//! definitions and the given templates. The second boot trusts one
//! `machines` issuer for its SCIM client and one `persons` issuer the Basic
//! user binds under, both with the key set of the given provider, and maps
//! the Basic user to the binding key. The harness then provisions that
//! `PERSON` through `/scim/v2/Users` (RFC 7644 §3.3), so the CDR writes the
//! committer of every commit from the bound party. No specification governs
//! the harness: our own design, after the reference CDR's quickstart.

use crate::containers::{CDR_POSTGRES, Cdr, HarnessError, cdr_request};
use crate::facility_cdr::{database, serve};
use crate::oidc::{Provider, ProviderError};
use testcontainers::ImageExt;

/// Where the configuration file is copied in the CDR container.
const CONFIG_PATH: &str = "/etc/ferroehr/bound.toml";

/// Where the issuers' key set is copied in the CDR container.
const JWKS_PATH: &str = "/etc/ferroehr/bound.jwks.json";

/// The Basic user the bridge reaches the CDR as.
pub const BASIC_USER: &str = "ferrobridge";

/// A bound CDR and the password of [`BASIC_USER`] at it, drawn for the run.
pub struct Bound {
    /// The CDR.
    cdr: Cdr,
    /// The Basic user's password.
    password: String,
}

impl Bound {
    /// Returns the CDR.
    #[must_use]
    pub const fn cdr(&self) -> &Cdr {
        &self.cdr
    }

    /// Returns the password of [`BASIC_USER`].
    #[must_use]
    pub fn password(&self) -> &str {
        &self.password
    }
}

impl std::fmt::Debug for Bound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bound")
            .field("cdr", &self.cdr)
            .finish_non_exhaustive()
    }
}

/// The issuer the reference CDR names a Basic user under in the committer
/// placeholder it accepts, which `[cdr] committer_issuer` states.
pub const COMMITTER_ISSUER: &str = "ferroehr";

/// The name the SCIM client provisions the professional `PERSON` with.
pub const PERSON_NAME: &str = "Synthetic Professional";

/// The issuer of the SCIM client's tokens.
const MACHINES: &str = "https://machines.ferrobridge.test";

/// The `persons` issuer the Basic user binds under; it signs nothing.
const BASIC_USERS: &str = "https://basic-users.ferrobridge.test";

/// The identifier issuer of the binding keys.
const BINDING_ISSUER: &str = "urn:ferrobridge:basic-user";

/// The SCIM client's id and the scope its token holds.
const SCIM_CLIENT: (&str, &str) = ("ferrobridge-scim", "ferroehr/scim");

/// The audience the CDR accepts the SCIM client's tokens for.
const SCIM_AUDIENCE: &str = "ferroehr-scim";

/// The professional `PERSON` template the party mapping writes.
const PERSON_TEMPLATE: &str = crate::fixtures::FERROEHR_PROFESSIONAL_PERSON_ID;

/// The professional `ROLE` template the party mapping writes.
const ROLE_TEMPLATE: &str = crate::fixtures::FERROEHR_PROFESSIONAL_ROLE_ID;

/// A bound CDR could not be booted.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BoundError {
    /// A container could not be started or did not become ready.
    #[error(transparent)]
    Harness(#[from] HarnessError),
    /// The key set could not be written.
    #[error("the key set could not be written")]
    Keys(#[source] serde_json::Error),
    /// The SCIM client's token could not be signed.
    #[error("the SCIM client's token could not be signed")]
    Token(#[source] ProviderError),
    /// A request to the CDR could not be sent.
    #[error("the request for {what} could not be sent")]
    Send {
        /// What the request was for.
        what: String,
        /// What the HTTP stack reported.
        #[source]
        source: reqwest::Error,
    },
    /// The CDR refused a request.
    #[error("the CDR answered {status} to {what}: {body}")]
    Refused {
        /// What the request was for.
        what: String,
        /// The status it answered.
        status: reqwest::StatusCode,
        /// The body it answered.
        body: String,
    },
    /// The Basic user's password could not be drawn.
    #[error("the Basic user's password could not be drawn")]
    Random(#[source] aws_lc_rs::error::Unspecified),
    /// The Basic user's password could not be hashed.
    #[error("the Basic user's password could not be hashed")]
    Hash(#[source] argon2::password_hash::Error),
    /// The first boot's server could not be removed.
    #[error("the first boot's CDR could not be removed")]
    Remove(#[source] testcontainers::TestcontainersError),
}

/// Returns the configuration of the second boot.
fn configuration(public_base_url: &str, password_hash: &str) -> String {
    let (client, scope) = SCIM_CLIENT;
    let person =
        format!("{{ \"{PERSON_TEMPLATE}\" = \"/identities[id4]/details[id5]/items[id6]\" }}");
    format!(
        r#"[auth]
enabled = true

[[auth.issuer]]
issuer = "{BASIC_USERS}"
kind = "persons"
audience = ["ferroehr-basic-users"]
jwks_file = "{JWKS_PATH}"

[[auth.issuer]]
issuer = "{MACHINES}"
kind = "machines"
audience = ["{SCIM_AUDIENCE}"]
jwks_file = "{JWKS_PATH}"
token_types = ["at+jwt"]

[[auth.party]]
issuer = "{BASIC_USERS}"
identifier_issuer = "{BINDING_ISSUER}"
identifier_type = "login"

[[auth.basic.users]]
user = "{BASIC_USER}"
password_hash = "{password_hash}"
scopes = ["user/composition-*.cruds", "user/template-*.cruds", "user/aql-*.cruds", "user/ehr-*.cruds", "__ferroehr.platform"]
roles = ["ADMIN", "USER"]

[auth.scim]
public_base_url = "{public_base_url}"

[[auth.scim.client]]
issuer = "{MACHINES}"
client_id = "{client}"
scope = "{scope}"
binding_issuer = "{BINDING_ISSUER}"

[authz.party_binding]
provider = {{ issuer = "urn:oid:2.999.7.9", id = "ferrobridge-e2e" }}

[[authz.party_binding.basic_user]]
user = "{BASIC_USER}"
key_issuer = "{BINDING_ISSUER}"
key = "{BASIC_USER}"

[[authz.party_binding.person.template]]
template_id = "{PERSON_TEMPLATE}"
default = true
key = {{ path = "/details[id2]/items[id3]", type = "login" }}
user_name = {{ path = "/details[id2]/items[id3]", type = "scim_user_name" }}
external_id = {{ path = "/details[id2]/items[id3]", type = "scim_external_id" }}

[[authz.party_binding.person.attribute]]
scim = "name.formatted"
name = true
path = {person}

[authz.party_binding.professional_role]
template_id = "{ROLE_TEMPLATE}"

[authz.rbac]
enabled = false
"#
    )
}

/// Starts the reference CDR with authentication on and [`BASIC_USER`] bound.
///
/// `templates` are stored first, each an OPT 1.4 document by its template
/// id. The CDR trusts `provider`'s key set, and the Basic user binds to a
/// professional `PERSON` named [`PERSON_NAME`] the harness provisions.
///
/// # Errors
///
/// Returns [`BoundError`] when a container cannot be started, a definition
/// or the provisioning is refused, or a request cannot be sent.
pub async fn bound(provider: &Provider, templates: &[(&str, &str)]) -> Result<Bound, BoundError> {
    let (password, password_hash) = password()?;
    let keys = serde_json::to_vec(&provider.jwks()).map_err(BoundError::Keys)?;
    let (database, placement) = database(CDR_POSTGRES).await?;
    let first = serve(
        database,
        cdr_request(placement.dsn.clone(), placement.network.clone()),
    )
    .await?;
    let http = reqwest::Client::new();
    for (id, adl) in [
        (
            PERSON_TEMPLATE,
            crate::fixtures::FERROEHR_PROFESSIONAL_PERSON_ADL2,
        ),
        (
            ROLE_TEMPLATE,
            crate::fixtures::FERROEHR_PROFESSIONAL_ROLE_ADL2,
        ),
    ] {
        let request = http
            .post(format!("{}/definition/template/adl2", first.base_url()))
            .header(reqwest::header::CONTENT_TYPE, "text/plain")
            .body(adl);
        expect(request, id, reqwest::StatusCode::CREATED).await?;
    }
    for (id, opt) in templates {
        let request = http
            .post(format!("{}/definition/template/adl1.4", first.base_url()))
            .header(reqwest::header::CONTENT_TYPE, "application/xml")
            .body(String::from(*opt));
        expect(request, id, reqwest::StatusCode::CREATED).await?;
    }
    let Cdr {
        server, database, ..
    } = first;
    server.rm().await.map_err(BoundError::Remove)?;
    let request = cdr_request(placement.dsn, placement.network)
        .with_env_var("FERROEHR__AUTH__ENABLED", "true")
        .with_env_var("FERROEHR_CONFIG", CONFIG_PATH)
        .with_copy_to(
            CONFIG_PATH,
            configuration("http://localhost:8080", &password_hash).into_bytes(),
        )
        .with_copy_to(JWKS_PATH, keys);
    let cdr = serve(database, request).await?;
    provision(&http, provider, cdr.base_url()).await?;
    Ok(Bound { cdr, password })
}

/// Draws a password for the run and returns it with its Argon2id PHC string.
fn password() -> Result<(String, String), BoundError> {
    use aws_lc_rs::rand::SecureRandom as _;
    use base64::Engine as _;
    let mut bytes = [0_u8; 24];
    aws_lc_rs::rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(BoundError::Random)?;
    let password = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let mut salt = [0_u8; 16];
    aws_lc_rs::rand::SystemRandom::new()
        .fill(&mut salt)
        .map_err(BoundError::Random)?;
    let hash = argon2::password_hash::PasswordHasher::hash_password_with_salt(
        &argon2::Argon2::default(),
        password.as_bytes(),
        &salt,
    )
    .map_err(BoundError::Hash)?
    .to_string();
    Ok((password, hash))
}

/// Provisions the professional `PERSON` [`BASIC_USER`] binds to through the
/// CDR's SCIM endpoint, under a token of the SCIM client.
async fn provision(
    http: &reqwest::Client,
    provider: &Provider,
    base_url: &str,
) -> Result<(), BoundError> {
    let (client, scope) = SCIM_CLIENT;
    let now = jiff::Timestamp::now().as_second();
    let mut header = Provider::header();
    header.typ = Some(String::from("at+jwt"));
    let token = provider
        .sign(
            &header,
            &serde_json::json!({
                "iss": MACHINES,
                "aud": SCIM_AUDIENCE,
                "sub": client,
                "client_id": client,
                "iat": now,
                "exp": now.saturating_add(300),
                "jti": "synthetic-scim-access",
                "scope": scope,
            }),
        )
        .map_err(BoundError::Token)?;
    let origin = base_url
        .strip_suffix("/ferroehr/rest/openehr/v1")
        .unwrap_or(base_url);
    let request = http
        .post(format!("{origin}/scim/v2/Users"))
        .bearer_auth(token)
        .header(reqwest::header::CONTENT_TYPE, "application/scim+json")
        .body(
            serde_json::json!({
                "schemas": [
                    "urn:ietf:params:scim:schemas:core:2.0:User",
                    "urn:ietf:params:scim:schemas:extension:ferrohealth:2.0:User"
                ],
                "userName": BASIC_USER,
                "name": { "formatted": PERSON_NAME },
                "urn:ietf:params:scim:schemas:extension:ferrohealth:2.0:User": {
                    "loginIdentifier": BASIC_USER
                }
            })
            .to_string(),
        );
    expect(
        request,
        "the SCIM provisioning",
        reqwest::StatusCode::CREATED,
    )
    .await
}

/// Sends `request` for `what` and refuses any answer but `status`.
async fn expect(
    request: reqwest::RequestBuilder,
    what: &str,
    status: reqwest::StatusCode,
) -> Result<(), BoundError> {
    let response = request.send().await.map_err(|source| BoundError::Send {
        what: what.to_owned(),
        source,
    })?;
    if response.status() == status {
        return Ok(());
    }
    let answered = response.status();
    Err(BoundError::Refused {
        what: what.to_owned(),
        status: answered,
        body: response
            .text()
            .await
            .unwrap_or_else(|error| format!("(the body could not be read: {error})")),
    })
}

#[cfg(test)]
mod tests {
    use super::{BASIC_USER, configuration};

    #[test]
    fn the_configuration_binds_the_basic_user_to_a_scim_provisioned_person() {
        let written: toml::Table = toml::from_str(&configuration(
            "http://localhost:8080",
            "$argon2id$synthetic",
        ))
        .unwrap();
        assert_eq!(
            BASIC_USER,
            written["authz"]["party_binding"]["basic_user"][0]["user"]
                .as_str()
                .unwrap()
        );
        assert_eq!(
            "machines",
            written["auth"]["issuer"][1]["kind"].as_str().unwrap()
        );
        assert_eq!(
            "ferrobridge-scim",
            written["auth"]["scim"]["client"][0]["client_id"]
                .as_str()
                .unwrap()
        );
    }
}
