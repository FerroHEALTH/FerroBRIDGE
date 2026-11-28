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
//! Each message is routed by MSH-3 and MSH-4 to its MLLP source in the state
//! store (HL7 v2.9 §2.14.9), whose document says how the message is read and
//! committed; the route must agree with the connection, its client
//! certificate's subject on the TLS listener or its peer's network on the
//! plain one, and a message no enabled route takes, or one that disagrees,
//! is answered `AR`.
//!
//! No byte of a message reaches a log line. The span of a message names its
//! MSH-10 and its type, and the events name the acknowledgment code and
//! counts. The usage report's window takes each message's latency and its
//! acknowledgment code, nothing of its content.

pub mod answer;
pub mod authorities;
pub mod directory;
pub mod edit;
mod envelope;
pub mod intake;
pub mod origin;
pub mod plain;
mod stamped;

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ferrobridge_hl7v2::ack::{self, Code, Header};
use ferrobridge_hl7v2::inbound::{self, Inbound, Received};
use ferrobridge_hl7v2::map::maps::Maps;
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
use crate::facade::ingest::Provenance;
use crate::hl7v2::answer::{Settled, Tally, unsettled};
use crate::hl7v2::plain::Lane;
use crate::hl7v2::stamped::Stamped;
use crate::sources::Routes;
use crate::sources::document::{Document, Mllp, Origin, Route};
use ferrohealth_usage_report::window::Window;

/// The face: what one frame's answer is built from.
#[derive(Debug, Clone)]
pub struct Face {
    /// The facade whose ingest service the messages enter.
    facade: Arc<Facade>,
    /// The directory of the maps, with the messages each one served.
    directory: Arc<directory::Directory>,
    /// The terminology server the table maps are translated on, with the
    /// assigning authorities it resolved.
    terminology: Option<ferrobridge_hl7v2::map::terminology::Terminology>,
    /// Where each message's MLLP source is found.
    routes: Arc<dyn Routes>,
    /// The listener this face answers on.
    lane: Lane,
    /// The usage report's window each message and acknowledgment is counted
    /// in.
    usage: Arc<Window>,
    /// The access-record lane each committed message is recorded through,
    /// with the address the face listens at.
    access_log: Option<(Arc<AccessLog>, SocketAddr)>,
    /// The inbound intake an ADT patient trigger's patient is stored in.
    inbox: Option<crate::inbound::store::InboxStore>,
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

/// Returns MSH-3 and MSH-4 of the message `bytes` holds, each as a route
/// names it: its namespace id, or `^<universal id>^<type>` (HL7 v2.5.1
/// chapter 2, `HD` data type); `None` when no header reads.
///
/// The route is read before the message is decoded, since the source names
/// the character set an empty MSH-18 is read in; MSH-3 and MSH-4 are read as
/// ASCII.
#[must_use]
pub fn route_of(bytes: &[u8]) -> Option<(String, String)> {
    let text = String::from_utf8_lossy(bytes);
    let segment = text
        .split(['\r', '\n'])
        .find(|segment| segment.starts_with("MSH"))?;
    let mut characters = segment.chars().skip(3);
    let field = characters.next()?;
    let component = characters.next()?;
    let fields: Vec<&str> = segment.split(field).collect();
    let hd = |value: &str| {
        let first = value.split('~').next().unwrap_or_default();
        let parts: Vec<&str> = first.split(component).collect();
        match parts.first() {
            Some(namespace) if !namespace.is_empty() => (*namespace).to_owned(),
            _ => format!(
                "^{}^{}",
                parts.get(1).copied().unwrap_or_default(),
                parts.get(2).copied().unwrap_or_default()
            ),
        }
    };
    Some((hd(fields.get(2)?), hd(fields.get(3)?)))
}

/// What a message is routed to: its source's MLLP document and the route
/// that names its sender.
#[derive(Debug, Clone)]
struct Routed {
    /// The source.
    source: Arc<crate::sources::Source>,
    /// The route of the sender.
    route: Route,
}

impl Routed {
    /// Returns the source's MLLP document.
    fn mllp(&self) -> &Mllp {
        match &self.source.parsed {
            Document::Mllp(mllp) => mllp,
            Document::Webhook(_) | Document::FhirSubscription(_) => &EMPTY,
        }
    }
}

/// Returns MSH-9.1 and MSH-9.2 of `inbound` as its message type, or its
/// structure's name when the parse let MSH-9 through empty.
fn message_type_of(inbound: &Inbound) -> Option<MessageType> {
    let message = inbound.parsed().message();
    let type_code = [message.message_type(1), message.message_type(2)]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<&str>>()
        .join("^");
    // NOTE: no specification governs this: our own design; MSH-9 is
    // required, so a type the parse let through empty is named by the structure.
    MessageType::new(type_code)
        .or_else(|_| MessageType::new(inbound.parsed().structure_name()))
        .ok()
}

/// The document of a source no route takes, which no message reaches.
static EMPTY: std::sync::LazyLock<Mllp> = std::sync::LazyLock::new(Mllp::default);

impl Face {
    /// Returns the face over `facade` and `terminology` routing each message
    /// through `routes`, running the v2-to-FHIR maps with FerroBRIDGE's
    /// corrections ([`Maps::corrected`]).
    #[must_use]
    pub fn new(
        facade: Arc<Facade>,
        terminology: Option<ferrobridge_hl7v2::map::terminology::Terminology>,
        routes: Arc<dyn Routes>,
    ) -> Self {
        let directory = Arc::new(directory::Directory::new(
            Maps::corrected(),
            jiff::Timestamp::now(),
        ));
        Self {
            facade,
            directory,
            terminology,
            routes,
            lane: Lane::Tls,
            usage: Arc::new(crate::usage_report::window()),
            access_log: None,
            inbox: None,
        }
    }

    /// Returns the face storing each ADT patient trigger's patient in
    /// `inbox`, the inbound intake's rows, when the intake runs.
    #[must_use]
    pub fn with_inbox(mut self, inbox: Option<crate::inbound::store::InboxStore>) -> Self {
        self.inbox = inbox;
        self
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
            let routed = self.route(message).await;
            let charset = match &routed {
                Ok(Some(routed)) => routed.mllp().charset(),
                Ok(None) | Err(()) => ferrobridge_hl7v2::decode::Charset::Ascii,
            };
            match inbound::receive(message, charset, structure_for, stamp) {
                Received::Unanswerable => {
                    tracing::warn!("the message names no readable header and is not answered");
                    None
                }
                Received::Answered { code, reply } => {
                    tracing::info!(ack = code.as_str(), "answered before mapping");
                    Some((code, reply))
                }
                Received::Parsed(inbound) => {
                    let routed = match routed {
                        Ok(Some(routed)) => routed,
                        Ok(None) => {
                            connection.refuse();
                            return Self::acknowledge(
                                &inbound,
                                &Settled::Sender,
                                &Tally::default(),
                                &stamped,
                                false,
                            );
                        }
                        Err(()) => {
                            return Self::acknowledge(
                                &inbound,
                                &Settled::Routes,
                                &Tally::default(),
                                &stamped,
                                false,
                            );
                        }
                    };
                    if let Some(settled) = self.unbound(&routed.route, connection) {
                        connection.refuse();
                        return Self::acknowledge(
                            &inbound,
                            &settled,
                            &Tally::default(),
                            &stamped,
                            false,
                        );
                    }
                    self.settle(&inbound, &stamped, connection.peer(), &routed)
                        .await
                }
            }
        }
        .instrument(span)
        .await;
        let code = answered.as_ref().map(|(code, _)| *code);
        crate::usage_report::record_message(&self.usage, started.elapsed(), code);
        answered.map(|(_, reply)| reply)
    }

    /// Returns the enabled MLLP source the sender of the message `bytes` is
    /// routed to, with its route; `None` when no enabled route takes it, and
    /// `Err` when the routes cannot be read.
    async fn route(&self, bytes: &[u8]) -> Result<Option<Routed>, ()> {
        let Some((application, facility)) = route_of(bytes) else {
            return Ok(None);
        };
        let source = match self.routes.routed(&application, &facility).await {
            Ok(source) => source,
            Err(error) => {
                tracing::error!(error = %error, "the MLLP routes could not be read");
                return Err(());
            }
        };
        let Some(source) = source.filter(|source| source.enabled) else {
            return Ok(None);
        };
        let Document::Mllp(mllp) = &source.parsed else {
            return Ok(None);
        };
        let route = mllp.routes.iter().find(|route| {
            route.sending_application == application && route.sending_facility == facility
        });
        Ok(route.cloned().map(|route| Routed {
            source: Arc::clone(&source),
            route,
        }))
    }

    /// Maps and ingests one parsed message and returns its acknowledgment.
    async fn settle(
        &self,
        inbound: &Inbound,
        stamped: &Stamped,
        peer: Option<SocketAddr>,
        routed: &Routed,
    ) -> Option<(Code, Vec<u8>)> {
        let (settled, tally) = self.ingest(inbound, stamped, peer, routed).await;
        Self::acknowledge(
            inbound,
            &settled,
            &tally,
            stamped,
            routed.mllp().log_outcomes,
        )
    }

    /// Returns the acknowledgment `settled` owes with its code, logging the
    /// code and the counts.
    fn acknowledge(
        inbound: &Inbound,
        settled: &Settled,
        tally: &Tally,
        stamped: &Stamped,
        log_outcomes: bool,
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
        if log_outcomes && code != Code::Accept {
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
    /// Returns [`Settled::Facility`] for a facility no route of the source
    /// names, and [`Settled::NoClinician`] for a result naming no clinician
    /// from a source that is no device.
    fn responsible(message: &Message, mllp: &Mllp) -> Result<origin::Responsible, Settled> {
        let facility = origin::facility(message, mllp.facility_identifier_issuer.as_deref());
        let senders: Vec<Sender> = mllp
            .routes
            .iter()
            .map(|route| Sender::of(&route.sending_facility))
            .collect();
        if let Some(facility) = facility.as_ref()
            && !facility.listed(&senders)
        {
            return Err(Settled::Facility(Box::new(facility.clone())));
        }
        let device = match (mllp.origin, mllp.device_identifier_issuer.as_deref()) {
            (Origin::Device, Some(issuer)) => Some(issuer),
            _ => None,
        };
        let composer = match device {
            Some(issuer) => origin::device(message, issuer),
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
        routed: &Routed,
    ) -> (Settled, Tally) {
        let mllp = routed.mllp();
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
            Maps::corrected(),
            self.terminology.as_ref(),
        )
        .await
        {
            Ok(mapped) => mapped,
            Err(error) => return (Settled::Unmapped(error), tally),
        };
        self.directory.served(mapped.applied());
        tally.outcomes = mapped.counts();
        let unsettled = unsettled(mapped.outcomes());
        if !unsettled.is_empty() {
            return (Settled::Unregistered(unsettled), tally);
        }
        let Some(message_type) = message_type_of(inbound) else {
            return (Settled::Unkeyed, tally);
        };
        let mut bundle = mapped.into_bundle();
        if let Err(settled) = self.store_patient(message, &bundle, routed).await {
            return (settled, tally);
        }
        edit::claim_profiles(&mut bundle, &mllp.profiles);
        edit::link_subjects(&mut bundle);
        let sending = origin::sending_system(message);
        let control_item =
            origin::control_item(control.as_str(), message_type.as_str(), sending.as_deref());
        let orders = origin::order_numbers(message);
        let origin::Responsible {
            facility,
            composer,
            device_id,
        } = match Self::responsible(message, mllp) {
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
        let mut settings = self.facade.settings().clone();
        if let Some(policy) = mllp.policy() {
            settings.ehr_policy = policy;
        }
        let mut ingest = self
            .facade
            .ingest_under(client, &settings)
            .with_originating_system(device_id.or(sending))
            .with_item_ids(orders, vec![control_item])
            .with_parties(
                facility.map(|facility| facility.party),
                origin::performing_organisation(message),
            );
        if let Some(composer) = composer {
            ingest = ingest.with_composer(composer);
        }
        let outcome = ingest
            .ingest_bundle(&bundle, mllp.unmapped_entries.into(), &provenance)
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

    /// Stores the patient events of `message`, an ADT patient trigger whose
    /// maps wrote `bundle`, in the inbound intake before the message is
    /// answered; any other message, or a face with no intake, stores none.
    async fn store_patient(
        &self,
        message: &Message,
        bundle: &fhir_types::codec::Value,
        routed: &Routed,
    ) -> Result<(), Settled> {
        let (Some(inbox), Some(trigger)) = (self.inbox.as_ref(), intake::trigger(message)) else {
            return Ok(());
        };
        let events = intake::events(message, bundle, trigger).map_err(Settled::Unidentified)?;
        for patient in &events {
            if let Err(error) = inbox
                .accept_with_body(
                    routed.source.id,
                    &patient.tenant,
                    &patient.event,
                    &patient.body,
                )
                .await
            {
                tracing::warn!(error = %ferrohealth_server::run::chain(&error), "a patient event could not be stored");
                return Err(Settled::Unstored);
            }
        }
        Ok(())
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
        let person = match self.facade.store().person_of(ingested.ehr_id()).await {
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

    fn admits(&self, _peer: SocketAddr) -> bool {
        true
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
    /// The TLS the listener serves, requiring a client certificate; `None`
    /// when every sender is plain.
    pub tls: Option<crate::config::hl7v2::Tls>,
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

/// Serves `face` on an already-bound listener, over TLS or plain as its
/// [`Lane`] says, until `shutdown` completes,
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
/// Returns the I/O error of a failed `accept`, and an `InvalidInput` error
/// for a face on the TLS lane whose listener has no TLS.
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
    let acceptor = match face.lane() {
        Lane::Tls => Some(
            listener
                .tls
                .as_ref()
                .map(|tls| tls.acceptor.clone())
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "the TLS listener has no TLS configured",
                    )
                })?,
        ),
        Lane::Plain => None,
    };
    let signalled = Arc::new(tokio::sync::Notify::new());
    let inner = Arc::clone(&signalled);
    let flag = listening.clone();
    listening.set(true);
    let stopping = async move {
        shutdown.await;
        flag.set(false);
        inner.notify_one();
    };
    let server: std::pin::Pin<Box<dyn Future<Output = std::io::Result<()>> + Send>> = match acceptor
    {
        Some(acceptor) => Box::pin(listener::serve_tls(
            socket,
            acceptor,
            face,
            listener.codec,
            listener.timeouts,
            stopping,
        )),
        None => Box::pin(listener::serve_with(
            socket,
            face,
            listener.codec,
            listener.timeouts,
            stopping,
        )),
    };
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
    use super::{printable, route_of};

    #[test]
    fn a_route_reads_msh_3_and_msh_4_by_namespace_id_or_universal_id() {
        let message = b"MSH|^~\\&|MONITOR|WARD-1|EHR|SOUTH|20260925120000+0200||ORU^R01^ORU_R01|MSG-1|P|2.5.1\r";
        assert_eq!(
            Some((String::from("MONITOR"), String::from("WARD-1"))),
            route_of(message)
        );
        let universal =
            b"MSH|^~\\&|^1.2.3^ISO|^4.5.6^ISO|EHR|SOUTH|20260925120000||ORU^R01|MSG-2|P|2.5.1\r";
        assert_eq!(
            Some((String::from("^1.2.3^ISO"), String::from("^4.5.6^ISO"))),
            route_of(universal)
        );
        assert_eq!(None, route_of(b"PID|1"));
    }

    #[test]
    fn an_err_text_keeps_printable_ascii_only() {
        assert_eq!("M?ller: no mapping", printable("M\u{fc}ller: no mapping"));
        assert_eq!("a?b", printable("a\rb"));
    }
}
