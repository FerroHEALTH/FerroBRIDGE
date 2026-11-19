// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bridge as a SMART App Launch 2.x resource server, over
//! `ferrohealth-smart`.
//!
//! The crate reads each configured issuer's metadata and key set, validates
//! every access token, or for an issuer with `mode = "introspection"` asks
//! its introspection endpoint (RFC 7662), classifies every caller by its
//! issuer's `kind`, checks the Basic users of `[[auth.basic.users]]`, each a
//! person, picks the issuer the SMART discovery document describes
//! (`resource::described::describe`) and builds that document; this module
//! holds what is the bridge's own: the R4 resource types a clinical scope
//! may name, the operator scope, and the token endpoint the
//! `CapabilityStatement` names.
//! The facade's route table and its `OperationOutcome` refusals are
//! `crate::facade::guard`'s, and the operator API's are
//! `crate::operator::routes`'s.
//!
//! The bridge never forwards a caller's token to the CDR: an access token
//! names the resource server it is for (RFC 9068 §4). The CDR is reached with
//! the bridge's own credential (`crate::credentials`), or, under
//! `[cdr.endpoint.credentials.onward]`, with a token the caller's was exchanged for
//! (`crate::cdr::onward`).

use crate::mount::BASE_PATH;
use std::collections::BTreeSet;
use std::sync::Arc;

use axum::Router;
use axum::response::IntoResponse;
use axum::routing::get;
use ferrohealth_secret::secret_url::SecretUrl;
use ferrohealth_smart::basic::PasswordStore;
use ferrohealth_smart::guard::Gate;
use ferrohealth_smart::metadata::ServerMetadata;
use ferrohealth_smart::resource::IssuerSet;
use ferrohealth_smart::resource::ResourceError;
use ferrohealth_smart::resource::ResourceServer;
use ferrohealth_smart::resource::Settings;
use ferrohealth_smart::resource::Verifier;
use ferrohealth_smart::resource::configuration::OpenIdConnect;
use ferrohealth_smart::resource::configuration::PermissionCapability;
use ferrohealth_smart::resource::configuration::Purpose;
use ferrohealth_smart::resource::configuration::SMART_CONFIGURATION;
use ferrohealth_smart::resource::configuration::SmartOptions;
use ferrohealth_smart::resource::described::Candidate;
use ferrohealth_smart::resource::described::DescribeError;
use ferrohealth_smart::resource::described::describe;
use ferrohealth_smart::resource::introspection::IntrospectionError;
use ferrohealth_smart::resource::introspection::IntrospectionSettings;
use ferrohealth_smart::resource::introspection::Introspector;
use fhir_types::r4::schema::SCHEMAS;

/// The operator scope a deployment that names none uses.
///
/// SMART scopes cover FHIR resources only, so the operator API is guarded by
/// a scope of FerroBRIDGE's own, in the form SMART App Launch 2.2.0, Scopes
/// and Launch Context, §Extensibility, gives an extension scope: "any string
/// starting with `__`".
pub const OPERATOR_SCOPE: &str = "__ferrobridge.operator";

/// The capability of SMART's `OpenID` Connect profile (SMART App Launch
/// 2.2.0, Conformance, §Single Sign-on).
const SSO_OPENID_CONNECT: &str = "sso-openid-connect";

/// What the deployment configured its resource server with, resolved.
#[derive(Debug, Clone)]
pub struct SmartSettings {
    /// The resource-server settings of each trusted issuer whose JWT access
    /// tokens are checked against its key set.
    pub issuers: Vec<Settings>,
    /// The introspection check of each issuer with `mode =
    /// "introspection"`.
    pub introspection: Vec<IntrospectionSettings>,
    /// The issuer the discovery document and the `CapabilityStatement`
    /// name, `[auth] described_issuer`; without it `describe` picks the one
    /// issuer its rule allows.
    pub described_issuer: Option<SecretUrl>,
    /// The Basic users, when the surface admits Basic.
    pub basic: Option<Arc<PasswordStore>>,
    /// The realm every challenge names, when one is set.
    pub realm: Option<String>,
    /// What the discovery document adds: the capabilities and the scopes
    /// the bridge honours.
    pub options: SmartOptions,
    /// The scope that guards the operator API and admits the operations that
    /// commit nothing.
    pub operator_scope: String,
}

impl SmartSettings {
    /// Returns the settings of a resource server trusting `issuer` alone,
    /// with no Basic user, no realm, no added capability or listed scope,
    /// and the default operator scope.
    #[must_use]
    pub fn new(issuer: Settings) -> Self {
        Self {
            issuers: vec![issuer],
            introspection: Vec::new(),
            described_issuer: None,
            basic: None,
            realm: None,
            options: SmartOptions::default(),
            operator_scope: String::from(OPERATOR_SCOPE),
        }
    }
}

/// A resource server the bridge cannot start with.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SmartError {
    /// The settings name no issuer.
    #[error("the resource server names no issuer")]
    NoIssuer,
    /// An issuer's metadata or key set could not be read, or breaks a rule.
    #[error("the authorization server {issuer} could not be read")]
    Issuer {
        /// The issuer identifier.
        issuer: String,
        /// What the resource server reported.
        #[source]
        source: Box<ResourceError>,
    },
    /// The introspection check could not start: the issuer's metadata could
    /// not be read, or it names no usable introspection endpoint.
    #[error("the introspection check of {issuer} could not start")]
    Introspection {
        /// The issuer identifier.
        issuer: String,
        /// What the check reported.
        #[source]
        source: Box<IntrospectionError>,
    },
    /// No issuer can be described: `[auth] described_issuer` names an issuer
    /// the rule refuses, or without it no single issuer is eligible.
    #[error("the SMART discovery document can describe no issuer (auth.described_issuer)")]
    Described {
        /// Why `describe` refused.
        #[source]
        source: DescribeError,
    },
    /// The issuer the discovery document names publishes no token endpoint,
    /// which the document requires (SMART App Launch 2.2.0, Conformance,
    /// §Metadata, `token_endpoint`: REQUIRED).
    #[error("the authorization server metadata of {issuer} names no token_endpoint")]
    NoTokenEndpoint {
        /// The issuer identifier.
        issuer: String,
    },
}

/// The check whose authorization server the two discovery documents name.
#[derive(Debug)]
enum Primary {
    /// An issuer whose JWT access tokens are checked against its key set.
    Jwt(Arc<ResourceServer>),
    /// The issuer whose tokens its introspection endpoint checks.
    Introspection(Arc<Introspector>),
}

impl Primary {
    /// Returns the authorization server's metadata.
    fn metadata(&self) -> &ServerMetadata {
        match self {
            Self::Jwt(server) => server.metadata(),
            Self::Introspection(introspector) => introspector.metadata(),
        }
    }

    /// Returns the SMART discovery document under `options`.
    fn smart_configuration(&self, options: &SmartOptions) -> serde_json::Value {
        match self {
            Self::Jwt(server) => server.smart_configuration(options),
            Self::Introspection(introspector) => introspector.smart_configuration(options),
        }
    }
}

/// The started resource server: the gate in front of every guarded route and
/// what the two discovery documents name.
#[derive(Debug)]
pub struct Smart {
    /// The credentials every guarded route admits.
    gate: Gate,
    /// The check whose issuer the discovery document and the
    /// `CapabilityStatement` name.
    primary: Primary,
    /// What the two documents describe that issuer for.
    purpose: Purpose,
    /// The token endpoint that issuer's metadata names.
    token_endpoint: String,
    /// What the discovery document adds.
    options: SmartOptions,
    /// The operator scope.
    operator_scope: String,
}

impl Smart {
    /// Reads the metadata and the key set of every issuer `settings` names.
    ///
    /// Each is read once here, so an authorization server that cannot be
    /// reached refuses the start rather than the first request.
    ///
    /// # Errors
    ///
    /// Returns [`SmartError::NoIssuer`] for settings without an issuer,
    /// [`SmartError::Issuer`] for an issuer whose metadata or key set could
    /// not be read or that breaks a rule, [`SmartError::Introspection`] when
    /// an introspection check cannot start, [`SmartError::Described`] when no
    /// issuer can be described, and
    /// [`SmartError::NoTokenEndpoint`] when the issuer the documents name
    /// names no token endpoint.
    pub async fn discover(settings: SmartSettings) -> Result<Self, SmartError> {
        let (issuers, candidates) = Self::issuers(settings.issuers, settings.introspection).await?;
        let described = describe(candidates, settings.described_issuer.as_ref())
            .map_err(|source| SmartError::Described { source })?;
        let purpose = described.purpose();
        let primary = described.into_check();
        let bearer: Arc<dyn Verifier> = Arc::new(issuers);
        let token_endpoint = primary.metadata().token_endpoint.clone().ok_or_else(|| {
            SmartError::NoTokenEndpoint {
                issuer: primary.metadata().issuer.clone(),
            }
        })?;
        let mut gate = Gate::open().with_bearer(bearer);
        if let Some(store) = settings.basic {
            gate = gate.with_basic(store);
        }
        if let Some(realm) = settings.realm {
            gate = gate.with_realm(realm);
        }
        Ok(Self {
            gate,
            primary,
            purpose,
            token_endpoint,
            options: settings.options,
            operator_scope: settings.operator_scope,
        })
    }

    /// Reads the metadata and the key set of every issuer of `jwt`, and the
    /// metadata of every issuer of `introspected`, and returns the set of
    /// them with each offered to the documents as a candidate.
    async fn issuers(
        jwt: Vec<Settings>,
        introspected: Vec<IntrospectionSettings>,
    ) -> Result<(IssuerSet, Vec<Candidate<Primary>>), SmartError> {
        if jwt.is_empty() && introspected.is_empty() {
            return Err(SmartError::NoIssuer);
        }
        let mut issuers = IssuerSet::new();
        let mut candidates = Vec::new();
        for issuer in jwt {
            let name = issuer.issuer.clone();
            let server = ResourceServer::discover(issuer)
                .await
                .map_err(|source| SmartError::Issuer {
                    issuer: name,
                    source: Box::new(source),
                })?
                .with_resource_types(|resource_type| SCHEMAS.is_resource(resource_type));
            let server = Arc::new(server);
            candidates.push(Candidate::of_server(
                &server,
                Primary::Jwt(Arc::clone(&server)),
            ));
            issuers = issuers.with(server);
        }
        for introspection in introspected {
            let name = introspection.issuer.clone();
            let introspector = Introspector::discover(introspection)
                .await
                .map_err(|source| SmartError::Introspection {
                    issuer: name,
                    source: Box::new(source),
                })?
                .with_resource_types(|resource_type| SCHEMAS.is_resource(resource_type));
            let introspector = Arc::new(introspector);
            candidates.push(Candidate::of_introspector(
                &introspector,
                Primary::Introspection(Arc::clone(&introspector)),
            ));
            issuers = issuers.with_introspector(introspector);
        }
        Ok((issuers, candidates))
    }

    /// Returns the credentials every guarded route admits.
    #[must_use]
    pub const fn gate(&self) -> &Gate {
        &self.gate
    }

    /// Returns the operator scope.
    #[must_use]
    pub fn operator_scope(&self) -> &str {
        &self.operator_scope
    }

    /// Returns the SMART discovery document, listing as `scopes_supported`
    /// the scopes `[auth] scopes_supported` names, which "the server SHALL
    /// support" (SMART App Launch 2.2.0, Conformance, §Metadata).
    ///
    /// The document claims the permission capabilities the route table and
    /// the patient compartment enforce: `patient/` and `user/` scopes in
    /// the v1 syntax beside v2 (Conformance, §Permissions). The bridge
    /// evaluates no granular scope, so it claims no `permission-v2`. It
    /// advertises `sso-openid-connect`, and writes the `issuer` that
    /// capability requires, only when `[auth] capabilities` names it. A
    /// `machines` issuer is described for SMART Backend Services alone, with
    /// no app-launch member (SMART App Launch 2.2.0, Backend Services,
    /// §Retrieve `.well-known/smart-configuration`). It lists `smart-app-state`
    /// exactly when `app_state` says the facade serves App State, whatever
    /// `[auth] capabilities` names (SMART App Launch 2.2.0, Persisting App
    /// State, §Discovery).
    #[must_use]
    pub fn configuration(&self, app_state: bool) -> serde_json::Value {
        let mut capabilities: Vec<String> = self
            .options
            .capabilities
            .iter()
            .filter(|capability| capability.as_str() != crate::facade::app_state::CAPABILITY)
            .cloned()
            .collect();
        if app_state {
            capabilities.push(String::from(crate::facade::app_state::CAPABILITY));
        }
        let sign_on = self
            .options
            .capabilities
            .iter()
            .any(|capability| capability == SSO_OPENID_CONNECT);
        self.primary.smart_configuration(&SmartOptions {
            purpose: self.purpose,
            permissions: BTreeSet::from([
                PermissionCapability::V1,
                PermissionCapability::Patient,
                PermissionCapability::User,
            ]),
            openid_connect: if sign_on {
                OpenIdConnect::Advertised
            } else {
                OpenIdConnect::Withheld
            },
            capabilities,
            ..self.options.clone()
        })
    }

    /// Returns the `rest.security` endpoints of the `CapabilityStatement`,
    /// with no `authorize` for an issuer described for Backend Services
    /// alone, where no user launch starts.
    #[must_use]
    pub fn security(&self) -> Security {
        let metadata = self.primary.metadata();
        let launch = self.purpose == Purpose::AppLaunch;
        Security {
            authorize: metadata
                .authorization_endpoint
                .clone()
                .filter(|_endpoint| launch),
            token: self.token_endpoint.clone(),
            register: metadata.registration_endpoint.clone(),
            introspect: metadata.introspection_endpoint.clone(),
            revoke: metadata.revocation_endpoint.clone(),
        }
    }
}

/// The authorization server that guards the facade, as `rest.security`
/// states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Security {
    /// The authorization endpoint, when the server has one.
    pub authorize: Option<String>,
    /// The token endpoint.
    pub token: String,
    /// The dynamic registration endpoint, when the server has one.
    pub register: Option<String>,
    /// The token introspection endpoint, when the server has one.
    pub introspect: Option<String>,
    /// The token revocation endpoint, when the server has one.
    pub revoke: Option<String>,
}

/// Returns the routes of the discovery document.
///
/// The document is served at `[base]/.well-known/smart-configuration`, the
/// location SMART App Launch 2.2.0, Conformance, §Using Well-Known, fixes,
/// and at the server root for a client that looks there (no specification
/// governs the root copy: our own design). It "SHALL be JSON, regardless of
/// `Accept` headers". `app_state` says whether the facade serves SMART App
/// State, which the document then advertises.
pub fn routes(smart: &Smart, app_state: bool) -> Router {
    let document = Arc::new(smart.configuration(app_state));
    let answer = move || {
        let document = Arc::clone(&document);
        async move { axum::Json(document.as_ref().clone()).into_response() }
    };
    Router::new()
        .route(
            &format!("{BASE_PATH}{SMART_CONFIGURATION}"),
            get(answer.clone()),
        )
        .route(SMART_CONFIGURATION, get(answer))
}
