// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The platform the journey runs on: the reference CDR (its `main` image),
//! FerroSMART on the host network with a stub upstream OpenID Provider, a
//! stub terminology server and Audit Record Repository, two PostgreSQL
//! databases, and the bridge's own configuration, which the journey serves
//! in process on a real listener. No specification governs the harness: our
//! own design.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;

use base64::Engine as _;
use ferrobridge_server::config::Config;
use ferrobridge_server::config::Process;
use ferrobridge_server::config::Settings;
use ferrobridge_testkit::containers;
use ferrobridge_testkit::containers::Cdr;
use ferrobridge_testkit::containers::Postgres;
use ferrobridge_testkit::ferrosmart;
use ferrobridge_testkit::ferrosmart::FerroSmart;
use ferrobridge_testkit::oidc::Provider;
use tokio::net::TcpListener;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::USAGE_REPORT_OFF;
use super::login::APP;
use super::login::REDIRECT;
use super::login::SCOPE;
use super::login::UPSTREAM_ACR;
use super::login::UPSTREAM_CLIENT;
use super::record::Patients;
use super::record::SUBJECT_SYSTEM;
use crate::facade::summary::NAMESPACE;
use crate::facade::summary::mount_terminology;
use crate::facade::summary::package_paths;

/// The mapping set: the `Patient` party context the summary reads its
/// patient through.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures_summary");

/// The bridge's client id at FerroSMART, for its own token and the exchange.
const BRIDGE_CLIENT: &str = "ferrobridge";

/// The `kid` of the bridge's assertion key.
const BRIDGE_KID: &str = "ferrobridge-1";

/// Whether the bridge reads the CDR under the caller's exchanged token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Onward {
    /// Under the bridge's own access to the CDR.
    Off,
    /// Under the token FerroSMART exchanges the caller's for
    /// (`[cdr.endpoint.credentials.onward]`, RFC 8693 §2.1).
    On,
}

/// The started platform and the bridge's settings over it.
pub(super) struct Stack {
    /// The reference CDR, kept for the run's length.
    _cdr: Cdr,
    /// FerroSMART.
    pub(super) smart: FerroSmart,
    /// FerroSMART's upstream provider.
    pub(super) upstream: Provider,
    /// The Audit Record Repository.
    pub(super) arr: MockServer,
    /// The patients written into the CDR.
    pub(super) patients: Patients,
    /// The bridge's origin, `http://127.0.0.1:<port>`.
    pub(super) origin: String,
    /// The bridge's settings.
    pub(super) settings: Settings,
    /// The bridge's listener, bound before FerroSMART learnt its port.
    pub(super) listener: Option<TcpListener>,
    /// The terminology server, kept for the run's length.
    _terminology: MockServer,
    /// FerroSMART's database, kept for the run's length.
    _smart_database: Postgres,
    /// The bridge's state store, kept for the run's length.
    _state: Postgres,
    /// The operator's Basic credential, user and password, whose
    /// `[basic_grants]` table grants the operator scope.
    pub(super) operator: (String, String),
}

impl Stack {
    /// Returns the facade's base.
    pub(super) fn facade(&self) -> String {
        format!("{}/fhir", self.origin)
    }

    /// Starts the platform with the onward lane `onward`.
    pub(super) async fn start(onward: Onward) -> Result<Self, Box<dyn Error>> {
        let cdr = containers::cdr_main().await?;
        let patients = super::record::seed(cdr.base_url(), cdr.extensions_url()).await?;
        let smart_database = containers::postgres().await?;
        let state = containers::postgres().await?;
        let upstream = Provider::start().await?;
        upstream.publish_metadata().await;
        let terminology = MockServer::start().await;
        mount_terminology(&terminology).await;
        let arr = MockServer::start().await;
        wiremock::Mock::given(matchers::method("POST"))
            .and(matchers::path("/AuditEvent"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&arr)
            .await;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let origin = format!("http://{}", listener.local_addr()?);
        let facade = format!("{origin}/fhir");
        let smart_port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let assertion = Assertion::new()?;
        let smart = ferrosmart::start(&smart_launch(
            smart_port,
            &smart_database,
            &upstream,
            &facade,
            cdr.base_url(),
            &assertion,
        )?)
        .await?;
        let password = crate::support::random_secret();
        let text = bridge_config(&BridgeConfig {
            origin: &origin,
            cdr: cdr.base_url(),
            smart: smart.issuer(),
            terminology: &terminology.uri(),
            arr: &arr.uri(),
            state: state.url(),
            operator: &password_hash(&password)?,
            onward: (onward == Onward::On).then_some(&assertion),
        })?;
        let settings =
            Config::from_sources(Some(&text), &BTreeMap::new())?.resolve(Process::Server)?;
        Ok(Self {
            _cdr: cdr,
            smart,
            upstream,
            arr,
            patients,
            origin,
            settings,
            listener: Some(listener),
            _terminology: terminology,
            _smart_database: smart_database,
            _state: state,
            operator: (String::from(OPERATOR), password),
        })
    }
}

/// The user-id of the operator who commits the `audit-categories` register.
const OPERATOR: &str = "journey-operator";

/// Returns the Argon2id PHC string of `password` under a fresh salt, as
/// `[[auth.basic.users]] password_hash` takes it.
fn password_hash(password: &str) -> Result<String, Box<dyn Error>> {
    use aws_lc_rs::rand::SecureRandom as _;
    let mut salt = [0_u8; 16];
    aws_lc_rs::rand::SystemRandom::new()
        .fill(&mut salt)
        .map_err(|_unspecified| "a salt")?;
    Ok(
        argon2::password_hash::PasswordHasher::hash_password_with_salt(
            &argon2::Argon2::default(),
            password.as_bytes(),
            &salt,
        )?
        .to_string(),
    )
}

/// The bridge's assertion key for `private_key_jwt` at FerroSMART (RFC 7523
/// §2.2), written where the bridge reads it, with its public JWK.
struct Assertion {
    /// The directory the key file lives in.
    directory: tempfile::TempDir,
    /// The public JWK, as FerroSMART registers it.
    jwk: serde_json::Value,
}

impl Assertion {
    /// Generates the key.
    fn new() -> Result<Self, Box<dyn Error>> {
        let pkcs8 = aws_lc_rs::signature::EcdsaKeyPair::generate_pkcs8(
            &aws_lc_rs::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
            &aws_lc_rs::rand::SystemRandom::new(),
        )
        .map_err(|_unspecified| "an assertion key")?;
        let encoding = jsonwebtoken::EncodingKey::from_ec_der(pkcs8.as_ref());
        let mut jwk = serde_json::to_value(jsonwebtoken::jwk::Jwk::from_encoding_key(
            &encoding,
            jsonwebtoken::Algorithm::ES256,
        )?)?;
        jwk["kid"] = serde_json::json!(BRIDGE_KID);
        jwk["use"] = serde_json::json!("sig");
        let pem = ferrosmart::pkcs8_pem(pkcs8.as_ref());
        let directory = tempfile::tempdir()?;
        std::fs::write(directory.path().join("bridge.pem"), pem)?;
        Ok(Self { directory, jwk })
    }

    /// Returns the key file's path.
    fn path(&self) -> String {
        self.directory
            .path()
            .join("bridge.pem")
            .display()
            .to_string()
    }
}

/// Returns FerroSMART's run on `port`: the facade and the CDR as its
/// resources, its own Backend Services client, the upstream provider with
/// `client_secret_basic` and its assurance levels, the app and the bridge as
/// clients.
fn smart_launch(
    port: u16,
    database: &Postgres,
    upstream: &Provider,
    facade: &str,
    openehr: &str,
    assertion: &Assertion,
) -> Result<ferrosmart::Launch, Box<dyn Error>> {
    let issuer = ferrosmart::issuer_on(port);
    let secret = crate::support::random_secret();
    let config = format!(
        "[ferrosmart]\nissuer = {issuer:?}\n\n\
         [ferrosmart.keys]\nprivate_key_files = [\"/etc/ferrosmart/signing.pem\"]\n\n\
         [ferrosmart.backend]\nalgorithm = \"ES384\"\n\n\
         [ferrosmart.backend.keys]\nprivate_key_file = \"/etc/ferrosmart/backend.pem\"\n\n\
         [[ferrosmart.resource]]\nbase = {facade:?}\nkind = \"fhir\"\n\n\
         [[ferrosmart.resource]]\nbase = {openehr:?}\nkind = \"openehr\"\n\
         subject_namespace = {NAMESPACE:?}\nfhir_base = {facade:?}\nsubject_system = {SUBJECT_SYSTEM:?}\n\n\
         [[ferrosmart.upstream]]\nname = \"eid\"\n\n\
         [ferrosmart.upstream.oidc]\nissuer = {:?}\nclient_id = {UPSTREAM_CLIENT:?}\n\
         fhir_user_claim = \"fhir_user\"\n\n\
         [ferrosmart.upstream.oidc.client_secret_basic]\nsecret_file = \"/etc/ferrosmart/upstream-secret\"\n\n\
         [ferrosmart.upstream.assurance]\nlevels = {{ {UPSTREAM_ACR:?} = \"substantial\" }}\n",
        upstream.issuer()
    );
    let app = serde_json::json!({
        "client_id": APP,
        "client_name": "Yellow Button journey",
        "token_endpoint_auth_method": "none",
        "redirect_uris": [REDIRECT],
        "scope": SCOPE,
        "resources": [facade],
        "grant_types": ["authorization_code"]
    });
    let bridge = serde_json::json!({
        "client_id": BRIDGE_CLIENT,
        "client_name": "FerroBRIDGE",
        "token_endpoint_auth_method": "private_key_jwt",
        "jwks": {"keys": [assertion.jwk]},
        "scope": "system/template-*.r system/composition-*.r system/aql-*.s patient/composition-*.r patient/aql-*.s",
        "resources": [facade, openehr],
        "grant_types": ["client_credentials", "urn:ietf:params:oauth:grant-type:token-exchange"]
    });
    Ok(ferrosmart::Launch {
        port,
        database_url: database.url().to_owned(),
        config,
        files: vec![
            (
                String::from("signing.pem"),
                ferrosmart::signing_key_pem()?.into_bytes(),
            ),
            (
                String::from("backend.pem"),
                ferrosmart::backend_key_pem()?.into_bytes(),
            ),
            (String::from("upstream-secret"), secret.into_bytes()),
        ],
        clients: vec![app, bridge],
    })
}

/// What the bridge's configuration names.
struct BridgeConfig<'a> {
    /// The bridge's origin.
    origin: &'a str,
    /// The CDR's openEHR base.
    cdr: &'a str,
    /// FerroSMART's issuer.
    smart: &'a str,
    /// The terminology server.
    terminology: &'a str,
    /// The Audit Record Repository.
    arr: &'a str,
    /// The state store's URL.
    state: &'a str,
    /// The Argon2id PHC string of the operator's password.
    operator: &'a str,
    /// The assertion key of the onward lane, when it is on.
    onward: Option<&'a Assertion>,
}

/// Returns the bridge's configuration file.
fn bridge_config(config: &BridgeConfig<'_>) -> Result<String, Box<dyn Error>> {
    let BridgeConfig {
        origin,
        cdr,
        smart,
        terminology,
        arr,
        state,
        operator,
        onward,
    } = config;
    let packages: Vec<String> = package_paths()?
        .ok_or("the profile packages are not fetched; run scripts/vendor/profile-packages.sh")?
        .iter()
        .map(|path| format!("{:?}", path.display().to_string()))
        .collect();
    let seal = key32();
    let lookup = key32();
    let mut text = format!(
        "[server]\nlisten = {listen:?}\n\n\
         {USAGE_REPORT_OFF}\n\
         [operations]\nenabled = false\n\n\
         [mappings]\ndirectory = {FIXTURES:?}\n\n\
         [cdr]\nendpoint.url = \"{cdr}/\"\ndemographic = true\n\n\
         [terminology]\nendpoint.url = \"{terminology}/r4\"\n\n\
         [facade]\nenabled = true\nbase_url = \"{origin}/fhir\"\n\
         subject_namespace = {NAMESPACE:?}\nsubject_id_form = \"logical_id\"\nsubject_type = \"PERSON\"\n\
         composition_language = \"en\"\ncomposition_territory = \"NL\"\n\n\
         [facade.summary]\nenabled = true\noperator = \"Synthetic operator\"\npackages = [{packages}]\n\n\
         [auth.issuer.ferrosmart]\nissuer = {smart:?}\n\n\
         [[auth.basic.users]]\nuser = {OPERATOR:?}\npassword_hash = {operator:?}\n\n\
         [basic_grants.{OPERATOR}]\nscopes = [\"__ferrobridge.operator\"]\n\n\
         [operator]\nenabled = true\n\n\
         [state]\nurl = \"{state}\"\n\n\
         [state.seal]\nlookup_key = {lookup:?}\n\n\
         [[state.seal.keys]]\nid = \"journey\"\nkey = {seal:?}\n\n\
         [shl]\nenabled = true\nbase_url = {origin:?}\n\n\
         [audit]\nenabled = true\ncleartext = true\nendpoint.url = {arr:?}\n",
        listen = origin.trim_start_matches("http://"),
        packages = packages.join(", "),
    );
    if let Some(assertion) = onward {
        writeln!(
            text,
            "\n[cdr.endpoint.credentials.oauth]\ntoken_endpoint = \"{smart}/token\"\n\
             client_id = {BRIDGE_CLIENT:?}\nclient_auth = \"private_key_jwt\"\n\
             private_key_file = {:?}\nkid = {BRIDGE_KID:?}\nresource = {cdr:?}\n\
             scope = \"system/template-*.r system/composition-*.r system/aql-*.s\"\n\n\
             [cdr.endpoint.credentials.onward]\nissuer = {smart:?}\ntoken_endpoint = \"{smart}/token\"\n\
             audiences = [{cdr:?}]\nindicators = [{cdr:?}]\n\n\
             [cdr.endpoint.credentials.onward.profile]\nmax_onward_lifetime = \"5min\"\n",
            assertion.path()
        )?;
    }
    Ok(text)
}

/// Returns the base64 of 32 random bytes, a key of `[state.seal]`.
fn key32() -> String {
    let bytes: Vec<u8> = [uuid::Uuid::new_v4(), uuid::Uuid::new_v4()]
        .iter()
        .flat_map(|uuid| *uuid.as_bytes())
        .collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
