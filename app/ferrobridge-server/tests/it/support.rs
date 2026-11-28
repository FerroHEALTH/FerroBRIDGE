// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a capturing log writer, test settings, a test state and a
//! terminology translator.

use ferrobridge_server::state::AppState;
use ferrobridge_server::terminology::Translator;
use ferrobridge_term::client::Client;
use ferrobridge_term::config::{Config, RetryPolicy, WireVersion};
use ferrohealth_health::registry::Registry;
use ferrohealth_server::section::ServerSection;
use ferrohealth_telemetry::section::Rendering;
use std::error::Error;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::fmt::MakeWriter;
use wiremock::MockServer;

/// Returns a synthetic secret drawn at random for this run, so no credential
/// literal sits in the tree.
pub(crate) fn random_secret() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Returns the `Authorization` value of the Basic credential of `user` and
/// `password`: RFC 7617 §2, `Basic` and the base64 of `user:password`.
pub(crate) fn basic_authorization(user: &str, password: &str) -> String {
    use base64::Engine as _;
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
    )
}

/// The smallest `[identity_source]` table, which an enabled facade needs.
pub(crate) const IDENTITY_SOURCE: &str = "[identity_source]\npseudonym_domain = \"urn:oid:2.999.9\"\nsource_domains = [\"urn:oid:2.999.1\"]\n\n[identity_source.endpoint]\nurl = \"http://127.0.0.1:9/fhir\"\n\n";

/// The identifier system the test patients' source identifiers are in.
pub(crate) const SOURCE_DOMAIN: &str = "urn:oid:2.999.1";

/// The identifier system of the testkit's synthetic patients.
pub(crate) const PATIENT_SYSTEM: &str = "http://example.org/fhir/sid/ferrobridge-patient";

/// The identifier system the testkit's synthetic resources name their
/// subject in.
pub(crate) const SUBJECT_SYSTEM: &str = "http://example.org/fhir/sid/ferrobridge-subject";

/// Returns the identity source at `base` whose pseudonyms are in `domain`,
/// accepting source identifiers in [`SOURCE_DOMAIN`], [`PATIENT_SYSTEM`],
/// [`SUBJECT_SYSTEM`] and `domain`, in that order.
pub(crate) fn identity_source(
    base: &str,
    domain: &str,
) -> Arc<ferrobridge_server::identity_source::IdentitySource> {
    identity_source_accepting(
        base,
        domain,
        &[SOURCE_DOMAIN, PATIENT_SYSTEM, SUBJECT_SYSTEM, domain],
    )
}

/// Returns the identity source at `base` whose pseudonyms are in `domain`,
/// accepting source identifiers in `accepted`, in that order.
pub(crate) fn identity_source_accepting(
    base: &str,
    domain: &str,
    accepted: &[&str],
) -> Arc<ferrobridge_server::identity_source::IdentitySource> {
    let section = ferrobridge_server::config::section::identity_source::IdentitySource {
        cache: None,
        endpoint: ferrohealth_smart::config::endpoint::EndpointSection {
            url: ferrohealth_secret::secret_url::SecretUrl::new(base),
            ..ferrohealth_smart::config::endpoint::EndpointSection::default()
        },
        pseudonym_domain: String::from(domain),
        source_domains: accepted.iter().copied().map(String::from).collect(),
    };
    Arc::new(
        ferrobridge_server::config::identity_source::resolve(&section)
            .expect("a legal identity source"),
    )
}

/// Returns the subject link whose pseudonyms are in `namespace`, over an
/// identity source no case reaches.
pub(crate) fn subject_link(namespace: &str) -> ferrobridge_server::identity::subject::SubjectLink {
    subject_link_at("http://127.0.0.1:9/fhir", namespace)
}

/// Returns the subject link whose pseudonyms are in `namespace`, over the
/// identity source at `base`.
pub(crate) fn subject_link_at(
    base: &str,
    namespace: &str,
) -> ferrobridge_server::identity::subject::SubjectLink {
    ferrobridge_server::identity::subject::SubjectLink::new(identity_source(base, namespace))
}

/// The pseudonym the stub identity source answers every patient with.
pub(crate) const PSEUDONYM: &str = "synthetic-pseudonym-1";

/// The path of the stub identity source's `$ihe-pix` operation.
pub(crate) const PIX_PATH: &str = "/pix/Patient/$ihe-pix";

/// Returns the ITI-83 answer naming `pseudonym` in `domain`
/// (`PIXm` 3.1.0 §2:3.83.4.2.2.1).
pub(crate) fn pseudonym_answer(domain: &str, pseudonym: &str) -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(200)
        .insert_header("Content-Type", "application/fhir+json")
        .set_body_json(serde_json::json!({
            "resourceType": "Parameters",
            "parameter": [
                {
                    "name": "targetIdentifier",
                    "valueIdentifier": {"system": domain, "value": pseudonym}
                },
                {"name": "targetId", "valueReference": {"reference": format!("Patient/{pseudonym}")}}
            ]
        }))
}

/// Returns `settings` asking the identity source served by `server` under
/// `/pix`, which answers every patient with [`PSEUDONYM`].
pub(crate) async fn served_identity(
    server: &MockServer,
    settings: ferrobridge_server::facade::Settings,
) -> ferrobridge_server::facade::Settings {
    let domain = settings.subject.namespace().to_owned();
    served_identity_accepting(
        server,
        settings,
        &[
            SOURCE_DOMAIN,
            PATIENT_SYSTEM,
            SUBJECT_SYSTEM,
            domain.as_str(),
        ],
    )
    .await
}

/// Mounts on `server` an identity source under `/pix` that answers every
/// patient with [`PSEUDONYM`] in `domain`.
pub(crate) async fn mount_pseudonym(server: &MockServer, domain: &str) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(PIX_PATH))
        .respond_with(pseudonym_answer(domain, PSEUDONYM))
        .with_priority(10)
        .mount(server)
        .await;
}

/// Returns `settings` asking the identity source served by `server` under
/// `/pix`, which answers every patient with [`PSEUDONYM`] and accepts source
/// identifiers in `accepted`.
pub(crate) async fn served_identity_accepting(
    server: &MockServer,
    settings: ferrobridge_server::facade::Settings,
    accepted: &[&str],
) -> ferrobridge_server::facade::Settings {
    let domain = settings.subject.namespace().to_owned();
    mount_pseudonym(server, &domain).await;
    let base = format!("{}/pix", server.uri());
    ferrobridge_server::facade::Settings {
        subject: ferrobridge_server::identity::subject::SubjectLink::new(
            identity_source_accepting(&base, &domain, accepted),
        ),
        ..settings
    }
}

/// The domain the per-identifier stub identity source mints pseudonyms in.
pub(crate) const PSEUDONYM_DOMAIN: &str = "urn:oid:2.999.9";

/// Returns the pseudonym the per-identifier stub identity source answers the
/// source identifier `system` and `value` with, for a value no FHIR search
/// escape changes.
pub(crate) fn pseudonym_of(system: &str, value: &str) -> String {
    token_pseudonym(&format!("{system}|{value}"))
}

/// Returns the pseudonym of the `sourceIdentifier` token `token`: distinct
/// tokens get distinct pseudonyms.
fn token_pseudonym(token: &str) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(token.as_bytes());
    let mut head = [0_u8; 16];
    head.copy_from_slice(&digest[..16]);
    format!("pseudonym-{:032x}", u128::from_be_bytes(head))
}

/// A stub `$ihe-pix` that answers each `sourceIdentifier` of an ITI-83 `POST`
/// with its own pseudonym in `domain`.
struct PerIdentifier {
    /// The pseudonym domain.
    domain: String,
}

impl wiremock::Respond for PerIdentifier {
    fn respond(&self, request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let token = serde_json::from_slice::<serde_json::Value>(&request.body)
            .ok()
            .and_then(|parameters| {
                parameters["parameter"]
                    .as_array()?
                    .iter()
                    .find(|parameter| parameter["name"] == "sourceIdentifier")?["valueString"]
                    .as_str()
                    .map(str::to_owned)
            });
        match token {
            Some(token) => pseudonym_answer(&self.domain, &token_pseudonym(&token)),
            None => wiremock::ResponseTemplate::new(400),
        }
    }
}

/// Returns a stub identity source and the subject link over it, minting in
/// [`PSEUDONYM_DOMAIN`] and accepting [`SOURCE_DOMAIN`], [`PATIENT_SYSTEM`]
/// and [`SUBJECT_SYSTEM`] then `more`: each source identifier gets its own
/// pseudonym ([`pseudonym_of`]), so each subject keeps its own EHR.
pub(crate) async fn per_identifier(
    more: &[&str],
) -> (
    MockServer,
    ferrobridge_server::identity::subject::SubjectLink,
) {
    let server = MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(PIX_PATH))
        .respond_with(PerIdentifier {
            domain: String::from(PSEUDONYM_DOMAIN),
        })
        .mount(&server)
        .await;
    let mut accepted = vec![SOURCE_DOMAIN, PATIENT_SYSTEM, SUBJECT_SYSTEM];
    accepted.extend_from_slice(more);
    let link = ferrobridge_server::identity::subject::SubjectLink::new(identity_source_accepting(
        &format!("{}/pix", server.uri()),
        PSEUDONYM_DOMAIN,
        &accepted,
    ));
    (server, link)
}

/// A `tracing` writer that keeps every line in memory.
#[derive(Debug, Clone, Default)]
pub(crate) struct Logs(Arc<Mutex<Vec<u8>>>);

impl Logs {
    /// Returns everything written so far.
    pub(crate) fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

/// The writer [`Logs`] hands out.
#[derive(Debug)]
pub(crate) struct LogsWriter(Arc<Mutex<Vec<u8>>>);

impl Write for LogsWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Logs {
    type Writer = LogsWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogsWriter(Arc::clone(&self.0))
    }
}

/// Returns server settings a test drives the middleware with: a 5 s request
/// timeout and drain, a 1024-byte ceiling and no compression.
pub(crate) fn settings() -> ServerSection {
    ServerSection {
        listen: "127.0.0.1:0".parse().expect("a socket address"),
        request_timeout: ferrohealth_config::units::Duration::from_millis(5_000),
        shutdown_timeout: ferrohealth_config::units::Duration::from_millis(5_000),
        body_limit: ferrohealth_config::units::ByteSize::from_bytes(1024),
        compression: false,
        ..ServerSection::default()
    }
}

/// Returns a subscriber that writes the console of `rendering`, filtered by
/// `filter`, to `writer`, as the family's console renders it.
pub(crate) fn subscriber<W>(
    rendering: Rendering,
    filter: &str,
    ansi: bool,
    writer: W,
) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    use tracing_subscriber::layer::SubscriberExt as _;
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(filter))
        .with(ferrohealth_telemetry::console::layer(
            rendering, ansi, writer,
        ))
}

/// Returns a state with no indicator and no logged query parameter.
pub(crate) fn state() -> Arc<AppState> {
    Arc::new(AppState::with_health(Registry::default()))
}

/// Returns a translator over the R4 terminology server `server` mocks.
pub(crate) fn translator(server: &MockServer) -> Result<Translator, Box<dyn Error>> {
    let config = Config::new(format!("{}/r4", server.uri()).parse()?, WireVersion::R4)
        .with_timeout(Duration::from_secs(5))
        .with_retry(RetryPolicy {
            max_attempts: 1,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
        });
    Ok(Translator::new(Some(Client::new(config)?)))
}

/// Returns the count `counts` holds under the counter `name`, 0 when none.
pub(crate) fn counter(counts: &ferrohealth_usage_report::window::Counts, name: &str) -> u64 {
    counts
        .counters
        .iter()
        .find(|(key, _)| key.as_str() == name)
        .map_or(0, |(_, count)| *count)
}
