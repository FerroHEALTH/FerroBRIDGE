// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bridge as a SMART App Launch 2.x resource server, over
//! `ferrohealth-smart`.
//!
//! The crate reads each configured issuer's metadata and key set, validates
//! every access token, or under `[auth] mode = "introspection"` asks the
//! issuer's introspection endpoint (RFC 7662), checks the Basic users of
//! `[[auth.basic.users]]`, and builds the SMART discovery document; this
//! module holds what is the
//! bridge's own: the R4 resource types a clinical scope may name, the
//! operator scope, and the token endpoint the `CapabilityStatement` names.
//! The facade's route table and its `OperationOutcome` refusals are
//! `crate::facade::guard`'s, and the operator API's are
//! `crate::operator::routes`'s.
//!
//! The bridge never forwards a caller's token to the CDR: an access token
//! names the resource server it is for (RFC 9068 §4), and the CDR is reached
//! with the bridge's own credential (`crate::credentials`).

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::Router;
use axum::response::IntoResponse;
use axum::routing::get;
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
use ferrohealth_smart::resource::configuration::SMART_CONFIGURATION;
use ferrohealth_smart::resource::configuration::SmartOptions;
use ferrohealth_smart::resource::introspection::IntrospectionError;
use ferrohealth_smart::resource::introspection::IntrospectionSettings;
use ferrohealth_smart::resource::introspection::Introspector;
use fhir_types::r4::schema::SCHEMAS;

use crate::facade::BASE_PATH;
use crate::facade::capability::Security;

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
    /// The resource-server settings of each trusted issuer, the first of
    /// which the discovery document and the `CapabilityStatement` name.
    pub issuers: Vec<Settings>,
    /// The introspection check of `[auth] mode = "introspection"`, in place
    /// of the issuers.
    pub introspection: Option<IntrospectionSettings>,
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
            introspection: None,
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
    /// The first issuer whose JWT access tokens are checked against its key
    /// set.
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
    /// the introspection check cannot start, and
    /// [`SmartError::NoTokenEndpoint`] when the first issuer names no token
    /// endpoint.
    pub async fn discover(settings: SmartSettings) -> Result<Self, SmartError> {
        let (bearer, primary): (Arc<dyn Verifier>, Primary) =
            if let Some(introspection) = settings.introspection {
                let issuer = introspection.issuer.clone();
                let introspector = Introspector::discover(introspection)
                    .await
                    .map_err(|source| SmartError::Introspection {
                        issuer,
                        source: Box::new(source),
                    })?
                    .with_resource_types(|resource_type| SCHEMAS.is_resource(resource_type));
                let introspector = Arc::new(introspector);
                let held = Arc::clone(&introspector);
                let bearer: Arc<dyn Verifier> = held;
                (bearer, Primary::Introspection(introspector))
            } else {
                let (issuers, primary) = Self::issuers(settings.issuers).await?;
                (Arc::new(issuers), Primary::Jwt(primary))
            };
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
            token_endpoint,
            options: settings.options,
            operator_scope: settings.operator_scope,
        })
    }

    /// Reads the metadata and the key set of every issuer of `settings`, and
    /// returns the set of them and the first.
    async fn issuers(
        settings: Vec<Settings>,
    ) -> Result<(IssuerSet, Arc<ResourceServer>), SmartError> {
        let mut issuers = IssuerSet::new();
        let mut primary = None;
        for issuer in settings {
            let name = issuer.issuer.clone();
            let server = ResourceServer::discover(issuer)
                .await
                .map_err(|source| SmartError::Issuer {
                    issuer: name,
                    source: Box::new(source),
                })?
                .with_resource_types(|resource_type| SCHEMAS.is_resource(resource_type));
            let server = Arc::new(server);
            primary.get_or_insert_with(|| Arc::clone(&server));
            issuers = issuers.with(server);
        }
        let primary = primary.ok_or(SmartError::NoIssuer)?;
        Ok((issuers, primary))
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
    /// capability requires, only when `[auth] capabilities` names it.
    #[must_use]
    pub fn configuration(&self) -> serde_json::Value {
        let sign_on = self
            .options
            .capabilities
            .iter()
            .any(|capability| capability == SSO_OPENID_CONNECT);
        self.primary.smart_configuration(&SmartOptions {
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
            ..self.options.clone()
        })
    }

    /// Returns the `rest.security` endpoints of the `CapabilityStatement`.
    #[must_use]
    pub fn security(&self) -> Security {
        let metadata = self.primary.metadata();
        Security {
            authorize: metadata.authorization_endpoint.clone(),
            token: self.token_endpoint.clone(),
            register: metadata.registration_endpoint.clone(),
            introspect: metadata.introspection_endpoint.clone(),
            revoke: metadata.revocation_endpoint.clone(),
        }
    }
}

/// Returns the routes of the discovery document.
///
/// The document is served at `[base]/.well-known/smart-configuration`, the
/// location SMART App Launch 2.2.0, Conformance, §Using Well-Known, fixes,
/// and at the server root for a client that looks there (no specification
/// governs the root copy: our own design). It "SHALL be JSON, regardless of
/// `Accept` headers".
pub fn routes(smart: &Smart) -> Router {
    let document = Arc::new(smart.configuration());
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
