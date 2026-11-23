// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The BALP `AuditEvent` of each event in a link's life.
//!
//! A link is an export of the person's record (DICOM PS3.15 A.5.3 "Export",
//! `110106`), so every record is of that event: its creation (`C`), each
//! answer to a receiver with the `recipient` it named (`R`), each refused
//! passcode (`R`, a minor failure), its revocation (`D`) and its end by
//! expiry, lockout or last use (`D`, by the system). A record names the
//! person by the patient id of their token and the link by its handle; never
//! the manifest id, the key, a file or a passcode. No BALP pattern describes
//! a link, so a record claims no profile (no specification governs these
//! records: our own design).

use ihe_atna::balp::DESTINATION_ROLE;
use ihe_atna::balp::Described;
use ihe_atna::balp::Detail;
use ihe_atna::balp::Direction;
use ihe_atna::balp::EXPORT;
use ihe_atna::balp::Entity;
use ihe_atna::balp::EventKind;
use ihe_atna::balp::Exchange;
use ihe_atna::balp::NetworkAddress;
use ihe_atna::balp::Outcome;
use ihe_atna::balp::Peer;
use ihe_atna::balp::SOURCE_ROLE;
use ihe_atna::balp::SYSTEM_OBJECT;
use ihe_atna::balp::What;
use ihe_atna::user::OnBehalfOf;
use ihe_atna::user::User;
use secrecy::SecretString;

use super::store::Ending;

/// The identifier system a link's handle is written under.
// NOTE: no specification governs this: our own design; a link is no FHIR resource, so its handle
// is an identifier (FHIR R4 references.html §2.3.0.3) in a namespace we name.
pub const HANDLE_SYSTEM: &str = "urn:ferrobridge:shl";

/// The `entity.detail` the recipient is written to.
pub const RECIPIENT: &str = "recipient";

/// The `entity.detail` the end of a link is written to.
pub const ENDING: &str = "ending";

/// What happened to a link.
#[derive(Debug, Clone, Copy)]
pub enum Event<'a> {
    /// The person created it.
    Created,
    /// A receiver was answered, naming `recipient`.
    Answered {
        /// The `recipient` the request named.
        recipient: &'a str,
    },
    /// A receiver named `recipient` gave a missing or wrong passcode.
    Refused {
        /// The `recipient` the request named.
        recipient: &'a str,
    },
    /// The person revoked it.
    Revoked,
    /// It ended by expiry, lockout or last use.
    Ended(Ending),
}

/// Who the record is made for: the person by their token, or the system.
#[derive(Debug, Clone)]
pub enum Actor {
    /// The person, by their token's issuer, subject and client.
    Person {
        /// The token's `iss`.
        issuer: Option<String>,
        /// The token's `sub`.
        subject: String,
        /// The token's `client_id`.
        client_id: Option<String>,
    },
    /// A receiver, or the system itself.
    System,
}

/// Returns the record of `event` on the link `handle` the person `owner`
/// shares, received at `endpoint`.
#[must_use]
pub fn exchange(
    event: Event<'_>,
    (handle, owner): (&str, &str),
    actor: &Actor,
    endpoint: &NetworkAddress,
) -> Exchange {
    let (action, outcome) = match event {
        Event::Created => ("C", Outcome::Success),
        Event::Answered { .. } => ("R", Outcome::Success),
        Event::Refused { .. } => ("R", Outcome::MinorFailure),
        Event::Revoked | Event::Ended(_) => ("D", Outcome::Success),
    };
    let mut details = Vec::new();
    let who = match event {
        Event::Answered { recipient } | Event::Refused { recipient } => {
            details.push(Detail::new(RECIPIENT, recipient));
            recipient.to_owned()
        }
        Event::Ended(ending) => {
            details.push(Detail::new(ENDING, ending_code(ending)));
            String::from("system")
        }
        Event::Created | Event::Revoked => match actor {
            Actor::Person {
                subject, client_id, ..
            } => client_id.clone().unwrap_or_else(|| subject.clone()),
            Actor::System => String::from("system"),
        },
    };
    let on_behalf = match actor {
        Actor::Person {
            issuer,
            subject,
            client_id,
        } => OnBehalfOf::User(
            User::local(subject.clone())
                .with_issuer(issuer.clone())
                .with_client_id(client_id.clone()),
        ),
        Actor::System => OnBehalfOf::System,
    };
    Exchange {
        kind: EventKind {
            profile: "",
            event_type: EXPORT,
            subtypes: Vec::new(),
            action,
            client: SOURCE_ROLE,
            server: DESTINATION_ROLE,
        },
        recorded: jiff::Timestamp::now(),
        outcome,
        outcome_description: None,
        direction: Direction::Received {
            client: Peer {
                who,
                network: NetworkAddress::MachineName(String::from("unknown")),
            },
            endpoint: endpoint.clone(),
        },
        on_behalf,
        entities: vec![
            Entity::Patient {
                system: None,
                value: SecretString::from(owner.to_owned()),
                namespace: None,
            },
            Entity::Described(Described {
                what: Some(What::Identifier {
                    system: Some(HANDLE_SYSTEM.to_owned()),
                    value: handle.to_owned(),
                }),
                kind: SYSTEM_OBJECT,
                role: None,
                name: Some(String::from("SMART Health Link")),
                description: None,
                details,
                security_labels: Vec::new(),
            }),
        ],
        security: Vec::new(),
    }
}

/// Returns the code a record names `ending` by.
const fn ending_code(ending: Ending) -> &'static str {
    match ending {
        Ending::Expired => "expired",
        Ending::Revoked => "revoked",
        Ending::Locked => "locked",
        Ending::Used => "used",
    }
}
