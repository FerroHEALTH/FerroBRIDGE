// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR facade as a SMART App Launch 2.x resource server: who calls, and
//! what the caller's access token permits.
//!
//! One bridge validates the access tokens of one authorization server. The
//! issuer and the audience are configured; the issuer's metadata (RFC 8414
//! §3, with `OpenID` Connect Discovery 1.0 §4 as the fallback location) names
//! the key set (RFC 7517) every token signature is checked against. A token
//! is an RFC 7519 JWT, read under the RFC 9068 access token profile unless
//! the deployment says otherwise. [`guard`] runs in front of every FHIR
//! route, refuses a missing or invalid token with `401` and an insufficient
//! scope with `403` (RFC 6750 §3.1), and writes the one audit line of each
//! request. [`compartment`] holds a `patient/` token to its patient.
//!
//! The bridge never forwards a caller's token to the CDR: an access token
//! names the resource server it is for (RFC 9068 §4), and the CDR is reached
//! with the bridge's own credential.

pub mod compartment;
mod discovery;
pub mod guard;
mod jwks;
pub mod scope;
pub mod smart;
mod token;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use http::StatusCode;
use jsonwebtoken::Algorithm;
use url::Url;

use crate::auth::discovery::ServerMetadata;
use crate::auth::jwks::KeySet;
use crate::auth::scope::Permission;
use crate::auth::scope::Reach;
use crate::auth::scope::Scopes;

/// The claims a token must carry, by profile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TokenProfile {
    /// The JWT access token profile of RFC 9068: `typ` is `at+jwt` and `iss`,
    /// `exp`, `aud`, `sub`, `client_id`, `iat` and `jti` are present (§2.1,
    /// §2.2, §4).
    #[default]
    Rfc9068,
    /// A JWT bearer token outside that profile: any `typ`, and `iss`, `exp`,
    /// `aud` and `sub` present. For an authorization server that does not
    /// issue RFC 9068 tokens; no specification governs the choice: our own
    /// design.
    Jwt,
}

/// What one deployment configured its resource server with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSettings {
    /// The issuer identifier every token's `iss` must equal (RFC 8414 §2).
    pub issuer: String,
    /// The audience every token's `aud` must contain (RFC 9068 §4).
    pub audience: String,
    /// The signature algorithms a token may use; asymmetric only.
    pub algorithms: Vec<Algorithm>,
    /// The claims a token must carry.
    pub profile: TokenProfile,
    /// The clock skew `exp`, `nbf` and `iat` are checked with.
    pub clock_skew: Duration,
    /// The shortest time between two reads of the key set an unknown `kid`
    /// triggers.
    pub jwks_refresh_interval: Duration,
    /// How long one call to the authorization server may take.
    pub timeout: Duration,
    /// The subjects a token with no clinical scope is granted system-wide
    /// access for.
    pub scopeless_subjects: BTreeSet<String>,
    /// The scope that guards the operator API, which SMART scopes do not cover,
    /// and admits the operations that commit nothing.
    pub operator_scope: String,
    /// The SMART capabilities the discovery document adds to the ones the
    /// resource server enforces.
    pub capabilities: Vec<String>,
}

impl AuthSettings {
    /// Returns settings for tokens of `issuer` addressed to `audience`.
    ///
    /// RS256 and ES256 are accepted under the RFC 9068 profile, the clock skew
    /// is 60 seconds, the key set is read again at most once a minute, a call
    /// to the authorization server may take 10 seconds, no subject is
    /// granted scopeless access, and the operator scope is
    /// `ferrobridge.operator`.
    #[must_use]
    pub fn new(issuer: impl Into<String>, audience: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into(),
            audience: audience.into(),
            algorithms: vec![Algorithm::RS256, Algorithm::ES256],
            profile: TokenProfile::Rfc9068,
            clock_skew: Duration::from_secs(60),
            jwks_refresh_interval: Duration::from_secs(60),
            timeout: Duration::from_secs(10),
            scopeless_subjects: BTreeSet::new(),
            operator_scope: String::from(OPERATOR_SCOPE),
            capabilities: Vec::new(),
        }
    }
}

/// The operator scope a deployment that names none uses.
///
/// SMART scopes cover FHIR resources only, so the operator API is guarded by
/// a scope of FerroBRIDGE's own (no specification governs it: our own
/// design).
pub const OPERATOR_SCOPE: &str = "ferrobridge.operator";

/// A failure to reach or read the authorization server.
///
/// No variant carries a token.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AuthError {
    /// The HTTP engine could not be built.
    #[error("the HTTP engine of the authorization server client could not be built")]
    Build {
        /// What `reqwest` reported.
        #[source]
        source: reqwest::Error,
    },
    /// The configured issuer is not a URL.
    #[error("the configured issuer is not a URL")]
    IssuerUrl {
        /// What the URL parser reported.
        #[source]
        source: url::ParseError,
    },
    /// A request to the authorization server never completed.
    #[error("the request to {url} could not be completed")]
    Transport {
        /// The URL that was called.
        url: Url,
        /// What `reqwest` reported.
        #[source]
        source: reqwest::Error,
    },
    /// No metadata document answered at either discovery location.
    #[error("the authorization server metadata at {url} answered {status}")]
    Discovery {
        /// The last location tried.
        url: Url,
        /// The status it answered.
        status: StatusCode,
        /// The body it answered, as received.
        body: String,
    },
    /// The metadata is not the JSON object RFC 8414 §3.2 describes.
    #[error("the authorization server metadata at {url} is not a metadata document")]
    Metadata {
        /// The location that answered it.
        url: Url,
        /// What the JSON reader reported.
        #[source]
        source: serde_json::Error,
    },
    /// The metadata names another issuer, which RFC 8414 §3.3 requires a
    /// client to refuse.
    #[error("the authorization server metadata names issuer {found:?}, not {expected:?}")]
    IssuerMismatch {
        /// The configured issuer.
        expected: String,
        /// The issuer the metadata named.
        found: String,
    },
    /// The metadata names no member the resource server needs.
    #[error("the authorization server metadata names no {member}")]
    MissingMember {
        /// The member it lacks.
        member: &'static str,
    },
    /// The metadata names a `jwks_uri` that is not a URL.
    #[error("the authorization server metadata names a jwks_uri that is not a URL")]
    JwksUrl {
        /// What the URL parser reported.
        #[source]
        source: url::ParseError,
    },
    /// The key set location answered anything other than `200`.
    #[error("the key set at {url} answered {status}")]
    KeySetStatus {
        /// The location.
        url: Url,
        /// The status it answered.
        status: StatusCode,
        /// The body it answered, as received.
        body: String,
    },
    /// The key set is not the JSON object RFC 7517 §5 describes.
    #[error("the key set at {url} is not a JWK Set")]
    KeySet {
        /// The location.
        url: Url,
        /// What the JSON reader reported.
        #[source]
        source: serde_json::Error,
    },
}

/// The resource server: the configured authorization server, its metadata
/// and its key set.
#[derive(Debug)]
pub struct Authority {
    /// What the deployment configured.
    settings: AuthSettings,
    /// The authorization server's metadata, read at boot.
    metadata: ServerMetadata,
    /// The token endpoint the metadata names, which SMART discovery requires.
    token_endpoint: String,
    /// The signing keys, read at boot and again on an unknown `kid`.
    keys: KeySet,
}

impl Authority {
    /// Reads the metadata and the key set of the authorization server
    /// `settings` names.
    ///
    /// Both are read once here, so an authorization server that cannot be
    /// reached refuses the start rather than the first request.
    ///
    /// # Errors
    ///
    /// Returns every [`AuthError`]: the metadata or the key set could not be
    /// read, or the metadata names another issuer or no `jwks_uri` or
    /// `token_endpoint`.
    pub async fn discover(settings: AuthSettings) -> Result<Self, AuthError> {
        let http = reqwest::Client::builder()
            .timeout(settings.timeout)
            .build()
            .map_err(|source| AuthError::Build { source })?;
        let metadata = discovery::discover(&http, &settings.issuer).await?;
        let jwks_uri = metadata
            .jwks_uri
            .as_deref()
            .ok_or(AuthError::MissingMember { member: "jwks_uri" })?
            .parse::<Url>()
            .map_err(|source| AuthError::JwksUrl { source })?;
        let token_endpoint = metadata
            .token_endpoint
            .clone()
            .ok_or(AuthError::MissingMember {
                member: "token_endpoint",
            })?;
        let keys = KeySet::load(http, jwks_uri, settings.jwks_refresh_interval).await?;
        tracing::info!(
            issuer = %settings.issuer,
            keys = keys.len().await,
            "the FHIR facade validates access tokens of this issuer"
        );
        Ok(Self {
            settings,
            metadata,
            token_endpoint,
            keys,
        })
    }

    /// Returns what the deployment configured.
    #[must_use]
    pub const fn settings(&self) -> &AuthSettings {
        &self.settings
    }

    /// Returns the caller `token` authenticates.
    ///
    /// # Errors
    ///
    /// Returns [`Rejection::Invalid`] for a token whose signature, issuer,
    /// audience, lifetime or claims do not hold, and
    /// [`Rejection::Unavailable`] when the key set could not be read again.
    pub async fn authenticate(&self, token: &str) -> Result<Caller, Rejection> {
        token::validate(self, token).await
    }
}

/// Why a request carries no usable identity.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Rejection {
    /// No bearer token arrived.
    #[error("the request carries no bearer token")]
    Missing,
    /// The token does not hold.
    #[error("the access token is not valid: {reason}")]
    Invalid {
        /// What failed, for the debug log; never a claim value.
        reason: &'static str,
    },
    /// The key set could not be read to check the token.
    #[error("the authorization server's key set could not be read")]
    Unavailable {
        /// What failed.
        #[source]
        source: Arc<AuthError>,
    },
}

/// The identity one valid access token carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// The token's `sub`.
    subject: String,
    /// The token's `client_id`, when it carries one.
    client_id: Option<String>,
    /// The clinical scopes the token grants.
    scopes: Scopes,
    /// The `patient` launch context the token carries.
    patient: Option<String>,
    /// Whether the subject is granted system-wide access without a clinical
    /// scope.
    scopeless: bool,
    /// Whether the token carries the operator scope.
    operator: bool,
}

impl Caller {
    /// Returns the caller of a token whose claims say so.
    ///
    /// `scopeless` is whether the deployment grants the subject system-wide
    /// access when the token names no clinical scope.
    #[must_use]
    pub fn new(
        subject: String,
        client_id: Option<String>,
        scope: &str,
        patient: Option<String>,
        scopeless: bool,
    ) -> Self {
        Self {
            subject,
            client_id,
            scopes: Scopes::parse(scope),
            patient,
            scopeless,
            operator: false,
        }
    }

    /// Returns this caller with `operator` as whether its token carries the
    /// operator scope.
    #[must_use]
    pub const fn with_operator(mut self, operator: bool) -> Self {
        self.operator = operator;
        self
    }

    /// Whether the token carries the operator scope, which admits the operator
    /// API and the operations that commit nothing.
    #[must_use]
    pub const fn is_operator(&self) -> bool {
        self.operator
    }

    /// Returns the token's subject.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Returns the token's client, when it names one.
    #[must_use]
    pub fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    /// Returns which data `permission` on `resource_type` reaches, or `None`
    /// when the token does not grant it.
    ///
    /// A token with no clinical scope reaches everything when its subject is
    /// on the deployment's allowlist and nothing otherwise. A `patient/`
    /// scope reaches the patient in the `patient` claim, and nothing when the
    /// token carries no such claim.
    #[must_use]
    pub fn reach(&self, permission: Permission, resource_type: &str) -> Option<Compartment> {
        if self.scopes.is_empty() {
            return self.scopeless.then_some(Compartment::Any);
        }
        match self.scopes.reach(permission, resource_type)? {
            Reach::Any => Some(Compartment::Any),
            Reach::Patient => self.patient.clone().map(Compartment::Patient),
        }
    }
}

/// Which resources one permission reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compartment {
    /// Every resource of the type.
    Any,
    /// Only the compartment of the patient with this FHIR id.
    Patient(String),
}

/// What a request may do, as the guard established it.
///
/// Every request through the guard carries one as an extension, so a handler
/// that finds none is outside the guard, which fails the request.
#[derive(Debug, Clone)]
pub enum Access {
    /// Inbound authentication is off: the facade is open.
    Open,
    /// A caller with a valid token.
    Caller(Arc<Caller>),
}

impl Access {
    /// Whether the caller carries the operator scope; an open facade has no
    /// caller and so no operator.
    #[must_use]
    pub fn is_operator(&self) -> bool {
        match self {
            Self::Open => false,
            Self::Caller(caller) => caller.is_operator(),
        }
    }

    /// Returns which data `permission` on `resource_type` reaches.
    #[must_use]
    pub fn reach(&self, permission: Permission, resource_type: &str) -> Option<Compartment> {
        match self {
            Self::Open => Some(Compartment::Any),
            Self::Caller(caller) => caller.reach(permission, resource_type),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Caller, Compartment};
    use crate::auth::scope::Permission;

    #[test]
    fn a_scopeless_token_reaches_everything_only_when_allowlisted() {
        let allowed = Caller::new(String::from("svc"), None, "openid", None, true);
        assert_eq!(
            Some(Compartment::Any),
            allowed.reach(Permission::Create, "Condition")
        );
        let other = Caller::new(String::from("svc"), None, "", None, false);
        assert_eq!(None, other.reach(Permission::Read, "Condition"));
    }

    #[test]
    fn an_allowlisted_subject_with_scopes_gets_only_its_scopes() {
        let caller = Caller::new(String::from("svc"), None, "system/Condition.r", None, true);
        assert_eq!(None, caller.reach(Permission::Create, "Condition"));
    }

    #[test]
    fn a_patient_scope_without_a_patient_claim_reaches_nothing() {
        let without = Caller::new(String::from("app"), None, "patient/*.rs", None, false);
        assert_eq!(None, without.reach(Permission::Read, "Condition"));
        let with = Caller::new(
            String::from("app"),
            None,
            "patient/*.rs",
            Some(String::from("123")),
            false,
        );
        assert_eq!(
            Some(Compartment::Patient(String::from("123"))),
            with.reach(Permission::Read, "Condition")
        );
    }
}
