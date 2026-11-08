// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 v2 face: an MLLP listener whose messages enter the facade's ingest
//! service.
//!
//! Each frame runs through `ferrobridge_hl7v2::inbound` (decode, parse,
//! group) and `ferrobridge_hl7v2::map` (the v2-to-FHIR `ConceptMaps`) into
//! an R4 message `Bundle`. The Bundle's entries then go through
//! [`crate::facade::ingest::Ingest::ingest_bundle`] as one transaction: the
//! programs are selected as the facade selects them, every composition's
//! `FEEDER_AUDIT` names the message by MSH-10 and its type, and the rule for
//! an entry no program maps is the configured one. The acknowledgment is
//! written only once the ingest settled, so `AA` means the CDR holds the
//! message's compositions ([`answer`]). No specification governs the
//! hand-off from a message Bundle to a transaction: our own design.
//!
//! No byte of a message reaches a log line. The span of a message names its
//! MSH-10 and its type, and the events name the acknowledgment code and
//! counts. The usage report's window takes each message's latency and its
//! acknowledgment code, nothing of its content.

pub mod answer;
pub mod directory;
pub mod edit;
mod envelope;
pub mod origin;
mod stamped;

use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ferrobridge_hl7v2::ack::{self, Code, Header};
use ferrobridge_hl7v2::decode::Charset;
use ferrobridge_hl7v2::inbound::{self, Inbound, Received};
use ferrobridge_hl7v2::map::corpus::{Corpus, CorpusError};
use ferrobridge_hl7v2::map::translate::Translator;
use ferrobridge_hl7v2::mllp::listener::{self, Connection, Handler, Timeouts};
use ferrobridge_hl7v2::mllp::{Codec, Malformed};
use ferrobridge_hl7v2::parse::Message;
use ferrobridge_hl7v2::parse::structure::structure_for;
use fhirconnect::engine::origin::{MessageControlId, MessageType, SourceItem};
use tokio::net::TcpListener;
use tracing::Instrument;

use crate::audit::AccessLog;
use crate::config::Hl7v2Settings;
use crate::config::hl7v2::Sender;
use crate::facade::Facade;
use crate::facade::ingest::{Provenance, UnmappedEntries};
use crate::hl7v2::answer::{Settled, Tally};
use crate::hl7v2::stamped::Stamped;
use ferrohealth_usage_report::window::Window;

/// Loads the guide's `ConceptMaps`, the supplements `ferrobridge-hl7v2`
/// ships, and every supplement directory of the settings over them, in order.
///
/// # Errors
///
/// Returns the [`CorpusError`] of the first directory or file that does not
/// load.
pub fn load_corpus(settings: &Hl7v2Settings) -> Result<Corpus, CorpusError> {
    let mut corpus = Corpus::load(&settings.concept_maps)?.with_shipped_supplements()?;
    for supplement in &settings.supplements {
        corpus = corpus.supplement(supplement)?;
    }
    Ok(corpus)
}

/// The face: what one frame's answer is built from.
#[derive(Debug)]
pub struct Face {
    /// The facade whose ingest service the messages enter.
    facade: Arc<Facade>,
    /// The v2-to-FHIR `ConceptMaps`.
    corpus: Corpus,
    /// The directory of the maps, with the messages each one served.
    directory: Arc<directory::Directory>,
    /// The terminology server the table maps are translated on.
    terminology: Option<ferrobridge_term::client::Client>,
    /// The facade's settings, under the face's EHR policy.
    settings: crate::facade::Settings,
    /// The character set a message with an empty MSH-18 is read in.
    charset: Charset,
    /// What an entry no program maps does to the message.
    unmapped: UnmappedEntries,
    /// The profile each resource type claims when the run wrote none.
    profiles: BTreeMap<String, String>,
    /// Whether an `AE` or an `AR` also logs the counted outcomes.
    log_outcomes: bool,
    /// The sending facilities the face accepts; empty accepts any.
    senders: Vec<Sender>,
    /// The senders that are devices.
    devices: Vec<crate::config::hl7v2::Device>,
    /// The DNS name each sender's client certificate carries.
    subjects: Vec<crate::config::hl7v2::Subject>,
    /// The usage report's window each message and acknowledgment is counted
    /// in.
    usage: Arc<Window>,
    /// The access-record lane each committed message is recorded through,
    /// with the address the face listens at.
    access_log: Option<(Arc<AccessLog>, SocketAddr)>,
}

impl Sender {
    /// Returns whether MSH-4 of `message` names this facility.
    #[must_use]
    pub fn names(&self, message: &Message) -> bool {
        let facility = message
            .header()
            .and_then(|header| header.field(4))
            .and_then(|field| field.repetitions().first());
        let component = |position: usize| {
            facility
                .and_then(|facility| facility.component(position))
                .and_then(|component| component.subcomponent(1))
        };
        match self {
            Self::Namespace(namespace) => component(1) == Some(namespace.as_str()),
            Self::Universal { id, kind } => {
                component(2) == Some(id.as_str()) && component(3) == Some(kind.as_str())
            }
        }
    }
}

/// Returns the namespace id MSH-4 of `message` names, HD.1, when it names one.
fn sending_facility(message: &Message) -> Option<&str> {
    message
        .header()
        .and_then(|header| header.field(4))
        .and_then(|field| field.repetitions().first())
        .and_then(|facility| facility.component(1))
        .and_then(|component| component.subcomponent(1))
}

/// Returns whether the client certificate of `connection` names the subject
/// of the sender MSH-4 of `message` names.
///
/// A sender is tied to the DNS name its certificate carries, so a peer sends
/// only as the sender its certificate names (our own design).
#[must_use]
pub fn certified(
    subjects: &[crate::config::hl7v2::Subject],
    message: &Message,
    connection: &Connection,
) -> bool {
    let Some(certificate) = connection.certificate() else {
        return false;
    };
    subjects.iter().any(|subject| {
        subject.sender.names(message)
            && ferrobridge_hl7v2::mllp::tls::names(certificate, &subject.name)
    })
}

/// Returns whether `senders` accepts the sending facility of `message`; an
/// empty list accepts any.
#[must_use]
pub fn admitted(senders: &[Sender], message: &Message) -> bool {
    senders.is_empty() || senders.iter().any(|sender| sender.names(message))
}

impl Face {
    /// Returns the face over `facade`, `corpus` and `terminology` that
    /// `settings` describes.
    #[must_use]
    pub fn new(
        facade: Arc<Facade>,
        corpus: Corpus,
        terminology: Option<ferrobridge_term::client::Client>,
        settings: &Hl7v2Settings,
    ) -> Self {
        let mut ingest = facade.settings().clone();
        if let Some(policy) = settings.ehr_policy {
            ingest.ehr_policy = policy;
        }
        let directory = Arc::new(directory::Directory::new(
            &corpus,
            settings,
            jiff::Timestamp::now(),
        ));
        Self {
            facade,
            corpus,
            directory,
            terminology,
            settings: ingest,
            charset: settings.default_charset,
            unmapped: settings.unmapped,
            profiles: settings.profiles.clone(),
            log_outcomes: settings.log_outcomes,
            senders: settings.senders.clone(),
            devices: settings.devices.clone(),
            subjects: settings.subjects.clone(),
            usage: Arc::new(crate::usage_report::window()),
            access_log: None,
        }
    }

    /// Returns the directory of the face's maps, with the messages each one
    /// served.
    #[must_use]
    pub const fn directory(&self) -> &Arc<directory::Directory> {
        &self.directory
    }

    /// Returns the face counting its messages in `usage`, the window the
    /// server's usage report takes.
    #[must_use]
    pub fn with_usage(mut self, usage: Arc<Window>) -> Self {
        self.usage = usage;
        self
    }

    /// Returns the face recording each message it commits through `log`, when
    /// one is on, naming `listen` as the address the messages arrive at.
    #[must_use]
    pub fn with_access_log(mut self, log: Option<Arc<AccessLog>>, listen: SocketAddr) -> Self {
        self.access_log = log.map(|log| (log, listen));
        self
    }

    /// Answers one frame's bytes: the acknowledgment's bytes, or `None` when
    /// no header can be read to address one.
    ///
    /// A frame that opens with FHS or BHS is a batch file
    /// ([`ferrobridge_hl7v2::batch`]): each message is answered as a message
    /// alone, and the answers travel back in one response batch. A message
    /// from a sending facility the face does not accept is counted as
    /// refused on `connection` and is never mapped.
    pub async fn answer(&self, message: &[u8], connection: &Connection) -> Option<Vec<u8>> {
        if ferrobridge_hl7v2::batch::is_batch(message) {
            return self.answer_batch(message, connection).await;
        }
        self.answer_message(message, connection).await
    }

    /// Answers one message's bytes: the acknowledgment's bytes, or `None`
    /// when no header can be read to address one.
    async fn answer_message(&self, message: &[u8], connection: &Connection) -> Option<Vec<u8>> {
        let header = Header::from_bytes(message);
        let span = tracing::info_span!(
            "hl7v2_message",
            control_id = header.as_ref().and_then(Header::control_id),
            message_type = header.as_ref().and_then(Header::type_code),
        );
        let started = Instant::now();
        let answered = async {
            let stamped = Stamped::now();
            let stamp = stamped.stamp();
            match inbound::receive(message, self.charset, structure_for, stamp) {
                Received::Unanswerable => {
                    tracing::warn!("the message names no readable header and is not answered");
                    None
                }
                Received::Answered { code, reply } => {
                    tracing::info!(ack = code.as_str(), "answered before mapping");
                    Some((code, reply))
                }
                Received::Parsed(inbound) => {
                    if !admitted(&self.senders, inbound.parsed().message()) {
                        connection.refuse();
                        return self.acknowledge(
                            &inbound,
                            &Settled::Sender,
                            &Tally::default(),
                            &stamped,
                        );
                    }
                    if !certified(&self.subjects, inbound.parsed().message(), connection) {
                        connection.refuse();
                        return self.acknowledge(
                            &inbound,
                            &Settled::Certificate,
                            &Tally::default(),
                            &stamped,
                        );
                    }
                    self.settle(&inbound, &stamped, connection.peer()).await
                }
            }
        }
        .instrument(span)
        .await;
        let code = answered.as_ref().map(|(code, _)| *code);
        crate::usage_report::record_message(&self.usage, started.elapsed(), code);
        answered.map(|(_, reply)| reply)
    }

    /// Maps and ingests one parsed message and returns its acknowledgment.
    async fn settle(
        &self,
        inbound: &Inbound,
        stamped: &Stamped,
        peer: Option<SocketAddr>,
    ) -> Option<(Code, Vec<u8>)> {
        let (settled, tally) = self.ingest(inbound, stamped, peer).await;
        self.acknowledge(inbound, &settled, &tally, stamped)
    }

    /// Returns the acknowledgment `settled` owes with its code, logging the
    /// code and the counts.
    fn acknowledge(
        &self,
        inbound: &Inbound,
        settled: &Settled,
        tally: &Tally,
        stamped: &Stamped,
    ) -> Option<(Code, Vec<u8>)> {
        let (code, mut errors) = answer::acknowledgment(settled);
        for error in &mut errors {
            error.text = printable(&error.text);
        }
        tracing::info!(
            ack = code.as_str(),
            committed = tally.committed,
            skipped = tally.skipped,
            errors = errors.len(),
            "message acknowledged"
        );
        if self.log_outcomes && code != Code::Accept {
            for (kind, count) in &tally.outcomes {
                tracing::debug!(outcome = kind, count, "counted outcome of the run");
            }
        }
        match inbound.answer(code, &errors, stamped.stamp()) {
            Ok(reply) => Some((code, reply)),
            Err(error) => {
                tracing::warn!(error = %error, "the acknowledgment cannot be encoded");
                None
            }
        }
    }

    /// Returns who is responsible for `message`'s compositions: the facility
    /// it names, its composer, and the device it is an act of when its sender
    /// is a device.
    ///
    /// # Errors
    ///
    /// Returns [`Settled::Facility`] for a facility no listed sender names,
    /// and [`Settled::NoClinician`] for a result naming no clinician from a
    /// sender that is no device.
    fn responsible(&self, message: &Message) -> Result<origin::Responsible, Settled> {
        let facility = origin::facility(message);
        if let Some(facility) = facility.as_ref()
            && !facility.listed(&self.senders)
        {
            return Err(Settled::Facility(Box::new(facility.clone())));
        }
        let device = self
            .devices
            .iter()
            .find(|device| device.sender.names(message));
        let composer = match device {
            Some(device) => origin::device(message, &device.identifier_issuer),
            None => origin::clinician(message),
        };
        if composer.is_none() && origin::carries_results(message) {
            return Err(Settled::NoClinician);
        }
        let device_id = device
            .and(composer.as_ref())
            .and_then(|composer| composer.id.as_ref())
            .map(|id| id.id.clone());
        Ok(origin::Responsible {
            facility,
            composer,
            device_id,
        })
    }

    /// Runs one parsed message through the guide and the ingest service.
    async fn ingest(
        &self,
        inbound: &Inbound,
        stamped: &Stamped,
        peer: Option<SocketAddr>,
    ) -> (Settled, Tally) {
        let mut tally = Tally::default();
        let message = inbound.parsed().message();
        let Some(control) = message
            .control_id()
            .and_then(|control| MessageControlId::new(control).ok())
        else {
            return (Settled::Unkeyed, tally);
        };
        let mapped = match ferrobridge_hl7v2::map::map(
            inbound.parsed(),
            &self.corpus,
            self.terminology.as_ref().map(Translator::Server),
        )
        .await
        {
            Ok(mapped) => mapped,
            Err(error) => return (Settled::Unmapped(error), tally),
        };
        self.directory.served(mapped.applied());
        tally.outcomes = mapped.counts();
        let type_code = [message.message_type(1), message.message_type(2)]
            .into_iter()
            .flatten()
            .filter(|part| !part.is_empty())
            .collect::<Vec<&str>>()
            .join("^");
        // NOTE: no specification governs this: our own design; MSH-9 is
        // required, so a type the parse let through empty is named by the structure.
        let message_type = MessageType::new(type_code)
            .or_else(|_| MessageType::new(inbound.parsed().structure_name()));
        let Ok(message_type) = message_type else {
            return (Settled::Unkeyed, tally);
        };
        let mut bundle = mapped.into_bundle();
        edit::claim_profiles(&mut bundle, &self.profiles);
        edit::link_subjects(&mut bundle);
        let sending = origin::sending_system(message);
        let control_item =
            origin::control_item(control.as_str(), message_type.as_str(), sending.as_deref());
        let orders = origin::order_numbers(message);
        let origin::Responsible {
            facility,
            composer,
            device_id,
        } = match self.responsible(message) {
            Ok(found) => found,
            Err(settled) => return (settled, tally),
        };
        let provenance = Provenance::Item(SourceItem::message(control, message_type));
        let client = match crate::cdr::ids::RequestId::new(&stamped.control_id) {
            Ok(id) => self.facade.client().with_request_id(id),
            Err(_) => self.facade.client().clone(),
        };
        let place = match self.access_log.as_ref() {
            Some((log, _)) => match log.recorder().reserve().await {
                Ok(place) => place,
                Err(error) => {
                    tracing::warn!(error = %ferrohealth_server::run::chain(&error), "a message was refused: its import record has no place");
                    return (Settled::Unrecorded, tally);
                }
            },
            None => None,
        };
        let mut ingest = self
            .facade
            .ingest_under(client, &self.settings)
            .with_originating_system(device_id.or(sending))
            .with_item_ids(orders, vec![control_item])
            .with_parties(
                facility.map(|facility| facility.party),
                origin::performing_organisation(message),
            );
        if let Some(composer) = composer {
            ingest = ingest.with_composer(composer.name, composer.id);
        }
        let outcome = ingest
            .ingest_bundle(&bundle, self.unmapped, &provenance)
            .await;
        match outcome {
            Ok(ingested) => {
                tally.committed = ingested.committed().count();
                tally.skipped = ingested.skipped().count();
                if let Err(error) = self.record(place, message, peer, &ingested).await {
                    tracing::warn!(error = %ferrohealth_server::run::chain(&error), "a committed message is not acknowledged: its import record could not be stored");
                    return (Settled::Unrecorded, tally);
                }
                (Settled::Ingested(Box::new(ingested)), tally)
            }
            Err(refused) => (Settled::Refused(refused), tally),
        }
    }

    /// Records the import of `message` from `peer`, which committed
    /// `ingested`, in `place` or, with no place, through the recorder.
    ///
    /// A message whose every source an earlier delivery committed commits
    /// nothing, so it imports nothing and leaves no record.
    async fn record(
        &self,
        place: Option<Box<dyn ihe_atna::balp::Reserved>>,
        message: &Message,
        peer: Option<SocketAddr>,
        ingested: &crate::facade::ingest::Ingested,
    ) -> Result<(), ihe_atna::balp::AuditError> {
        let Some((log, listen)) = self.access_log.as_ref() else {
            return Ok(());
        };
        if ingested.contribution().is_none() {
            return Ok(());
        }
        let person = match self.facade.store().person_of(ingested.ehr_id()) {
            Ok(person) => person,
            Err(error) => return Err(ihe_atna::balp::AuditError(Box::new(error))),
        };
        let exchange = crate::audit::hl7v2::import(
            crate::audit::hl7v2::Message {
                sender: sending_facility(message),
                peer,
                listen: *listen,
            },
            ingested
                .committed()
                .map(|committed| crate::audit::hl7v2::Imported {
                    version: committed.version.value(),
                    template_id: &committed.template_id,
                }),
            person.as_ref().map(|person| crate::audit::hl7v2::Person {
                id: person.id(),
                namespace: person.namespace(),
            }),
        );
        match place {
            Some(place) => place.record(exchange).await,
            None => log.recorder().record(exchange).await,
        }
    }
}

impl Handler for Face {
    async fn handle(&self, message: Vec<u8>, connection: &Connection) -> Option<Vec<u8>> {
        self.answer(&message, connection).await
    }

    fn malformed(&self, kind: Malformed, partial: &[u8]) -> Option<Vec<u8>> {
        let stamped = Stamped::now();
        let reply = ack::reject_frame(partial, &kind.to_string(), stamped.stamp());
        if reply.is_some() {
            crate::usage_report::record_acknowledgment(&self.usage, Code::Reject);
        }
        reply
    }
}

/// Returns `text` with every character outside printable ASCII replaced by
/// `?`, so an `ERR-8` encodes in every character set a message can declare.
fn printable(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                '?'
            }
        })
        .collect()
}

/// Whether the MLLP listener is accepting, for the readiness probe.
#[derive(Debug, Clone, Default)]
pub struct Listening(Arc<AtomicBool>);

impl Listening {
    /// Returns whether the listener is accepting.
    #[must_use]
    pub fn is_up(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Records whether the listener is accepting.
    pub(crate) fn set(&self, up: bool) {
        self.0.store(up, Ordering::Release);
    }
}

/// How the MLLP listener runs.
#[derive(Debug, Clone)]
pub struct Listener {
    /// The frame codec, with its ceiling.
    pub codec: Codec,
    /// The TLS the listener serves, requiring a client certificate.
    pub tls: crate::config::hl7v2::Tls,
    /// How long a connection and a frame may wait on the peer.
    pub timeouts: Timeouts,
    /// How long the drain may take after the stop signal.
    pub drain: Duration,
}

impl Listener {
    /// Returns the listener `settings` describe, draining within `drain`.
    #[must_use]
    pub fn of(settings: &Hl7v2Settings, drain: Duration) -> Self {
        Self {
            codec: Codec::new(settings.frame_limit),
            tls: settings.tls.clone(),
            timeouts: settings.timeouts,
            drain,
        }
    }
}

/// Serves `face` on an already-bound listener until `shutdown` completes,
/// then lets every connection finish the message it is answering within the
/// drain.
///
/// A message in flight when the signal arrives is mapped, committed and
/// acknowledged; a connection still open when the drain elapses is dropped,
/// as the HTTP server's are ([`crate::serve_until`]). `listening` reads up
/// from the first accept until the signal.
///
/// # Errors
///
/// Returns the I/O error of a failed `accept`.
pub async fn serve_until<F>(
    socket: TcpListener,
    face: Arc<Face>,
    listener: Listener,
    listening: Listening,
    shutdown: F,
) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let signalled = Arc::new(tokio::sync::Notify::new());
    let inner = Arc::clone(&signalled);
    let flag = listening.clone();
    listening.set(true);
    let server = listener::serve_tls(
        socket,
        listener.tls.acceptor.clone(),
        face,
        listener.codec,
        listener.timeouts,
        async move {
            shutdown.await;
            flag.set(false);
            inner.notify_one();
        },
    );
    let mut server = std::pin::pin!(server);
    let result = tokio::select! {
        result = &mut server => Some(result),
        () = signalled.notified() => None,
    };
    if let Some(result) = result {
        listening.set(false);
        return result;
    }
    if let Ok(result) = tokio::time::timeout(listener.drain, server).await {
        return result;
    }
    tracing::warn!(
        drain_ms = listener.drain.as_millis(),
        "the MLLP drain did not finish in time; the remaining connections are dropped"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Sender, admitted, printable};

    /// Returns the lexed message whose MSH-4 is `facility`.
    fn from(facility: &str) -> ferrobridge_hl7v2::parse::Message {
        let text = format!(
            "MSH|^~\\&|LAB|{facility}|EHR|SOUTH|20260925120000+0200||ORU^R01^ORU_R01|MSG-1|P|2.5.1\r"
        );
        ferrobridge_hl7v2::parse::lex::lex(&text, ferrobridge_hl7v2::decode::Charset::Ascii)
            .expect("the header lexes")
            .message
    }

    #[test]
    fn an_empty_allow_list_admits_any_sender() {
        assert!(admitted(&[], &from("NORTHLAB")));
    }

    #[test]
    fn a_sender_is_admitted_by_its_namespace_id_or_its_universal_id() {
        let senders = [
            Sender::Namespace(String::from("NORTHLAB")),
            Sender::Universal {
                id: String::from("1.2.3.4"),
                kind: String::from("ISO"),
            },
        ];
        assert!(admitted(&senders, &from("NORTHLAB")));
        assert!(admitted(&senders, &from("^1.2.3.4^ISO")));
        assert!(!admitted(&senders, &from("SOUTHLAB")));
        assert!(
            !admitted(&senders, &from("^1.2.3.4^DNS")),
            "the type must match"
        );
        assert!(
            !admitted(&senders, &from("")),
            "an empty MSH-4 names no listed sender"
        );
    }

    #[test]
    fn an_err_text_keeps_printable_ascii_only() {
        assert_eq!("M?ller: no mapping", printable("M\u{fc}ller: no mapping"));
        assert_eq!("a?b", printable("a\rb"));
    }
}
