// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 v2 face over MLLP: a framed message in, the acknowledgment and
//! the CDR calls out.
//!
//! Every case starts the listener on an ephemeral port over a `wiremock` CDR
//! and a `wiremock` terminology server, sends a synthetic ORU^R01 over TLS with the
//! sender's client certificate, and asserts the answer (HL7 v2.5.1 chapter 2 §2.9.2.2 for the
//! codes) and what the CDR received. The mapping set is one synthetic
//! Observation context under `hl7v2_fixtures/`, which the face's configured
//! profile points the message's Observation at. Behind `FERROBRIDGE_E2E` the
//! same message goes into the reference CDR.

use crate::facade::EHR_ID;
use crate::facade::client;
use crate::facade::ehr_body;
use crate::facade::handle;
use crate::facade::settings;
use crate::facade::stub::Echo;
use crate::facade::stub::mount_echo;
use ferrobridge_hl7v2::decode::Charset;
use ferrobridge_hl7v2::mllp::listener::Timeouts;
use ferrobridge_server::audit::AccessLog;
use ferrobridge_server::config::Hl7v2Settings;
use ferrobridge_server::config::hl7v2::{Sender, Subject, Tls};
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::ingest::UnmappedEntries;
use ferrobridge_server::facade::programs;
use ferrobridge_server::hl7v2::Face;
use ferrobridge_server::hl7v2::Listener;
use ferrobridge_server::hl7v2::Listening;
use ferrobridge_testkit::mllp_tls::{ClientIdentity, Pki};
use ferrobridge_testkit::stubs::terminology;
use ferrohealth_usage_report::window::{Counts, Window};
use std::collections::BTreeMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers;

/// The mapping set the face writes through: one Observation context.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/hl7v2_fixtures");

/// The profile the Observation context claims.
const PROFILE: &str = "http://example.org/fhir/StructureDefinition/ferrobridge-hl7v2-observation";

/// The containers the stub CDR hands out, in commit order.
const CONTAINERS: &[&str] = &[
    "3c9d6a70-0000-4000-8000-00000000001e",
    "3c9d6a70-0000-4000-8000-00000000001f",
];

/// The control id of the synthetic message.
const CONTROL_ID: &str = "MSG-V2-0001";

/// A laboratory result, ORU^R01 from a 2.5.1 sender with an empty MSH-18: one
/// patient, one order with its ordering provider and one numeric result, every value invented for the
/// test.
pub(crate) fn oru_r01(control_id: &str) -> Vec<u8> {
    let segments = [
        format!(
            "MSH|^~\\&|LAB|NORTHLAB|EHR|SOUTHCLINIC|20260925143000+0200||ORU^R01^ORU_R01|{control_id}|P|2.5.1"
        ),
        String::from(
            "PID|1||synthetic-subject-0001^^^NORTHLAB^MR||Synthetic^Sam^^^^^L||19800101|M",
        ),
        String::from("ORC|RE|PLC-1|FIL-1"),
        String::from(
            "OBR|1|PLC-1|FIL-1|2345-7^Glucose^LN|||20260925120000+0200|||||||||DR-1^Doctor^Dana^^^^^^NORTHLAB",
        ),
        String::from(
            "OBX|1|NM|2345-7^Glucose^LN^^^^^^Synthetic glucose||5.4|mmol/L^mmol/L^UCUM|3.9-5.8|N|||F|||20260925130000+0200",
        ),
    ];
    let mut bytes = Vec::new();
    for segment in segments {
        bytes.extend_from_slice(segment.as_bytes());
        bytes.push(b'\r');
    }
    bytes
}

/// An admission, ADT^A01 from a 2.5.1 sender: one patient, one inpatient
/// visit and one observation made at admission, every value invented for the
/// test.
fn adt_a01(control_id: &str) -> Vec<u8> {
    let segments = [
        format!(
            "MSH|^~\\&|ADT|NORTHLAB|EHR|SOUTHCLINIC|20260925143000+0200||ADT^A01^ADT_A01|{control_id}|P|2.5.1"
        ),
        String::from("EVN||20260925143000+0200|||OP-1^Operator^Olive^^^^^^NORTHLAB"),
        String::from(
            "PID|1||synthetic-subject-0001^^^NORTHLAB^MR||Synthetic^Sam^^^^^L||19800101|M",
        ),
        String::from("PV1|1|I|WARD-1^ROOM-1^BED-1"),
        String::from(
            "OBX|1|NM|2345-7^Glucose^LN^^^^^^Synthetic glucose||5.4|mmol/L^mmol/L^UCUM|3.9-5.8|N|||F|||20260925130000+0200",
        ),
    ];
    let mut bytes = Vec::new();
    for segment in segments {
        bytes.extend_from_slice(segment.as_bytes());
        bytes.push(b'\r');
    }
    bytes
}

/// Wraps a message in the MLLP envelope.
fn frame(message: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x0B];
    bytes.extend_from_slice(message);
    bytes.extend_from_slice(&[0x1C, 0x0D]);
    bytes
}

/// Reads one framed acknowledgment and returns its text.
async fn read_ack(stream: &mut (impl AsyncRead + Unpin)) -> Result<String, Box<dyn Error>> {
    let mut buffer = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let read = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut byte)).await??;
        if read == 0 {
            return Err("the connection closed before the acknowledgment ended".into());
        }
        buffer.extend_from_slice(&byte);
        if buffer.ends_with(&[0x1C, 0x0D]) {
            break;
        }
    }
    let inner = buffer
        .get(1..buffer.len().saturating_sub(2))
        .ok_or("a framed answer")?;
    Ok(String::from_utf8(inner.to_vec())?)
}

/// The segments of an acknowledgment that open with `id`.
fn segments<'a>(ack: &'a str, id: &str) -> Vec<&'a str> {
    ack.split('\r')
        .filter(|segment| segment.starts_with(&format!("{id}|")))
        .collect()
}

/// A terminology server loaded with the one table value the Observation
/// needs: OBX-11 `F` through `table-hl70085-to-observation-status`. Every
/// other translation states no match.
#[derive(Debug, Clone, Copy)]
struct Tables;

impl Respond for Tables {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap_or_default();
        let parameter = |name: &str, kind: &str| {
            body["parameter"]
                .as_array()
                .and_then(|parameters| {
                    parameters
                        .iter()
                        .find(|parameter| parameter["name"] == name)
                        .and_then(|parameter| parameter[kind].as_str())
                })
                .map(String::from)
                .unwrap_or_default()
        };
        let url = parameter("url", "valueUri");
        let code = parameter("code", "valueCode");
        if url.ends_with("/table-hl70085-to-observation-status") && code == "F" {
            return terminology::translate(
                "equivalent",
                "http://hl7.org/fhir/observation-status",
                "final",
                "Final",
            );
        }
        terminology::translate_no_match("no mapping for the code")
    }
}

/// Returns a terminology client over a started stub.
async fn terminology_stub() -> (MockServer, ferrobridge_term::client::Client) {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .respond_with(Tables)
        .mount(&server)
        .await;
    let base = format!("{}/r4", server.uri()).parse().expect("a base url");
    let config =
        ferrobridge_term::config::Config::new(base, ferrobridge_term::config::WireVersion::R4)
            .with_timeout(Duration::from_secs(5))
            .with_retry(ferrobridge_term::config::RetryPolicy {
                max_attempts: 1,
                initial_backoff: Duration::from_millis(1),
                max_backoff: Duration::from_millis(1),
            });
    let client = ferrobridge_term::client::Client::new(config).expect("a terminology client");
    (server, client)
}

/// The TLS material every case serves and sends with, generated once.
pub(crate) static PKI: LazyLock<Pki> = LazyLock::new(|| Pki::generate().expect("the TLS material"));

/// The DNS name the sending facility NORTHLAB's client certificate carries.
const NORTHLAB_SUBJECT: &str = "northlab.example.org";

/// The client certificate of the sending facility NORTHLAB.
static NORTHLAB: LazyLock<ClientIdentity> =
    LazyLock::new(|| PKI.client(NORTHLAB_SUBJECT).expect("a client certificate"));

/// Connects to `address` over TLS as NORTHLAB.
pub(crate) async fn connect(
    address: SocketAddr,
) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>, Box<dyn Error>> {
    let connector = PKI.connector(Some(&NORTHLAB))?;
    Ok(ferrobridge_testkit::mllp_tls::connect(address, &connector).await?)
}

/// Returns the face's settings, claiming [`PROFILE`] for an Observation when
/// `claim` is set.
pub(crate) fn face_settings(claim: bool) -> Hl7v2Settings {
    let profiles = if claim {
        BTreeMap::from([(String::from("Observation"), String::from(PROFILE))])
    } else {
        BTreeMap::new()
    };
    Hl7v2Settings {
        listen: "127.0.0.1:0".parse().expect("a socket address"),
        default_charset: Charset::Ascii,
        unmapped: UnmappedEntries::SkipAndCount,
        ehr_policy: None,
        profiles,
        timeouts: Timeouts::default(),
        frame_limit: 1024 * 1024,
        log_outcomes: true,
        senders: vec![Sender::Namespace(String::from("NORTHLAB"))],
        devices: Vec::new(),
        subjects: vec![Subject {
            sender: Sender::Namespace(String::from("NORTHLAB")),
            name: String::from(NORTHLAB_SUBJECT),
        }],
        tls: Tls {
            acceptor: ferrobridge_hl7v2::mllp::tls::acceptor(
                PKI.server_certificate().as_bytes(),
                PKI.server_key().as_bytes(),
                PKI.ca().as_bytes(),
            )
            .expect("the acceptor builds"),
        },
    }
}

/// A face serving on an ephemeral port, with its terminology stub.
struct Serving {
    /// The stub terminology server, held so it keeps answering.
    _terminology: MockServer,
    /// Where the listener accepts.
    address: SocketAddr,
    /// Whether the listener reads up.
    listening: Listening,
    /// The usage report's window the face counts its messages in.
    usage: Arc<Window>,
    /// The face's `ConceptMap` directory.
    directory: Arc<ferrobridge_server::hl7v2::directory::Directory>,
    /// The switch that stops it.
    stop: Option<oneshot::Sender<()>>,
    /// The listener task.
    task: JoinHandle<std::io::Result<()>>,
}

impl Serving {
    /// Starts the face over `facade` under `lane`, recording each commit
    /// through `log` when one is given.
    async fn start(
        facade: Arc<Facade>,
        lane: Hl7v2Settings,
        log: Option<Arc<AccessLog>>,
    ) -> Result<Self, Box<dyn Error>> {
        let (terminology_server, terminology) = terminology_stub().await;
        let usage = Arc::new(ferrobridge_server::usage_report::window());
        let face = Arc::new(
            Face::new(facade, Some(terminology), &lane)
                .with_usage(Arc::clone(&usage))
                .with_access_log(log, lane.listen),
        );
        let directory = Arc::clone(face.directory());
        let socket = TcpListener::bind("127.0.0.1:0").await?;
        let address = socket.local_addr()?;
        let listening = Listening::default();
        let (stop, stopped) = oneshot::channel::<()>();
        let task = tokio::spawn(ferrobridge_server::hl7v2::serve_until(
            socket,
            face,
            Listener::of(&lane, Duration::from_secs(10)),
            listening.clone(),
            async {
                let _signal = stopped.await;
            },
        ));
        Ok(Self {
            _terminology: terminology_server,
            address,
            listening,
            usage,
            directory,
            stop: Some(stop),
            task,
        })
    }

    /// Sends one framed message on a new connection and reads the answer.
    async fn send(&self, message: &[u8]) -> Result<String, Box<dyn Error>> {
        let mut stream = connect(self.address).await?;
        stream.write_all(&frame(message)).await?;
        read_ack(&mut stream).await
    }

    /// Takes what the face counted for the usage report so far.
    fn usage(&self) -> Counts {
        self.usage.take()
    }

    /// Signals the stop without waiting for the listener to end.
    fn signal(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(stop) = self.stop.take() {
            stop.send(()).map_err(|()| "the listener is gone")?;
        }
        Ok(())
    }

    /// Signals the stop and waits for the listener to end.
    async fn stop(mut self) -> Result<(), Box<dyn Error>> {
        self.signal()?;
        tokio::time::timeout(Duration::from_secs(10), self.task).await???;
        Ok(())
    }
}

/// A running face over stub upstreams.
pub(crate) struct Running {
    /// The stub CDR.
    cdr: MockServer,
    /// The face.
    serving: Serving,
}

impl Running {
    /// Starts the face whose settings claim the profile when `claim` is set,
    /// over a CDR whose EHR lookup answers after `delay`.
    async fn start(claim: bool, delay: Duration) -> Result<Self, Box<dyn Error>> {
        let lookup = ResponseTemplate::new(200)
            .set_body_string(ehr_body())
            .set_delay(delay);
        Self::start_with(face_settings(claim), lookup).await
    }

    /// Starts the face under `lane` over a CDR whose EHR lookup answers
    /// `lookup`.
    pub(crate) async fn start_with(
        lane: Hl7v2Settings,
        lookup: ResponseTemplate,
    ) -> Result<Self, Box<dyn Error>> {
        Self::start_recorded(lane, lookup, None).await
    }

    /// Starts the face under `lane` over a CDR whose EHR lookup answers
    /// `lookup`, recording each commit through `log` when one is given.
    pub(crate) async fn start_recorded(
        lane: Hl7v2Settings,
        lookup: ResponseTemplate,
        log: Option<Arc<AccessLog>>,
    ) -> Result<Self, Box<dyn Error>> {
        Self::start_serving(
            lane,
            lookup,
            log,
            ferrobridge_testkit::fixtures::DIAGNOSE_OPT,
        )
        .await
    }

    /// Starts the face under `lane` over a CDR whose EHR lookup answers
    /// `lookup` and whose `ferrobridge.diagnose.v1` is `opt`.
    pub(crate) async fn start_serving(
        lane: Hl7v2Settings,
        lookup: ResponseTemplate,
        log: Option<Arc<AccessLog>>,
        opt: &str,
    ) -> Result<Self, Box<dyn Error>> {
        let cdr = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path(
                "/definition/template/adl1.4/ferrobridge.diagnose.v1",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "application/xml")
                    .set_body_string(opt),
            )
            .mount(&cdr)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/ehr"))
            .respond_with(lookup)
            .mount(&cdr)
            .await;
        mount_echo(&cdr, CONTAINERS, Echo::Sent).await;
        let set = programs::read_set(std::path::Path::new(FIXTURES))?;
        let templates = programs::fetch_templates(&set, &client(&cdr)).await?;
        let compiled = programs::compile_set(&set, &templates)?;
        let store = Arc::new(MemoryStore::default());
        let facade = Arc::new(Facade::new(
            compiled,
            handle(&store),
            client(&cdr),
            settings(),
        ));
        let serving = Serving::start(facade, lane, log).await?;
        Ok(Self { cdr, serving })
    }

    /// Sends one framed message on a new connection and reads the answer.
    pub(crate) async fn send(&self, message: &[u8]) -> Result<String, Box<dyn Error>> {
        self.serving.send(message).await
    }

    /// Counts the contribution commits the CDR received.
    pub(crate) async fn contributions(&self) -> usize {
        self.requests("POST", &format!("/ehr/{EHR_ID}/contribution"))
            .await
            .len()
    }

    /// Returns the bodies of the requests the CDR received on `path`.
    pub(crate) async fn requests(&self, method: &str, path: &str) -> Vec<Vec<u8>> {
        self.cdr
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|request| request.method.as_str() == method && request.url.path() == path)
            .map(|request| request.body)
            .collect()
    }

    /// Signals the stop and waits for the listener to end.
    pub(crate) async fn stop(self) -> Result<(), Box<dyn Error>> {
        self.serving.stop().await
    }
}

/// Fails unless `counts` holds one HL7 v2 message, answered `(aa, ae, ar)`,
/// and `mapping_failures` engine refusals.
fn assert_counted(counts: &Counts, (aa, ae, ar): (u64, u64, u64), mapping_failures: u64) {
    let messages = counts
        .groups
        .iter()
        .find(|(key, _)| key.as_str() == ferrobridge_server::usage_report::HL7V2_MESSAGE)
        .map(|(_, group)| group.total());
    assert_eq!(Some(1), messages, "one message is timed");
    assert_eq!(
        (aa, ae, ar, mapping_failures),
        (
            crate::support::counter(counts, "hl7v2_aa"),
            crate::support::counter(counts, "hl7v2_ae"),
            crate::support::counter(counts, "hl7v2_ar"),
            crate::support::counter(counts, "mapping_failures")
        ),
        "the acknowledgment codes and the mapping failures"
    );
}

mod answers;
mod certificate;
mod clinician;
mod facility;
mod feeder_audit;
