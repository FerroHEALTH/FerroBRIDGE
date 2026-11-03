// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The record of one HL7 v2 message the face committed: a system import.
//!
//! ITI TF-2 Table 3.20.4.1.1.1-1 audits an import of PHI as "PHI-import",
//! the DICOM PS3.15 A.5.3 "Import" event, and no natural person sits behind
//! an MLLP message, so the bridge records it on its own behalf and writes no
//! Annex II 3.2 access record. An MLLP message is no `RESTful` interaction,
//! so the record claims no BALP `RESTful` pattern.

use ihe_atna::balp::DESTINATION_ROLE;
use ihe_atna::balp::DOMAIN_RESOURCE;
use ihe_atna::balp::Described;
use ihe_atna::balp::Direction;
use ihe_atna::balp::Entity;
use ihe_atna::balp::EventKind;
use ihe_atna::balp::Exchange;
use ihe_atna::balp::IMPORT;
use ihe_atna::balp::NetworkAddress;
use ihe_atna::balp::Outcome;
use ihe_atna::balp::Peer;
use ihe_atna::balp::SOURCE_ROLE;
use ihe_atna::balp::SYSTEM_OBJECT;
use ihe_atna::balp::What;
use ihe_atna::user::OnBehalfOf;
use secrecy::SecretString;

/// The namespace the openEHR version uids a record names its data by.
// NOTE: no specification governs this: our own design; an openEHR version uid is no FHIR
// resource, so it is an identifier (FHIR R4 references.html §2.3.0.3) in a namespace we name.
pub const OBJECT_ID_SYSTEM: &str = "urn:openehr:object-id";

/// The message one record is of.
#[derive(Debug, Clone, Copy)]
pub struct Message<'a> {
    /// The sending facility, MSH-4's namespace id, when it names one.
    pub sender: Option<&'a str>,
    /// The peer the message arrived from.
    pub peer: Option<std::net::SocketAddr>,
    /// The address the face listens at.
    pub listen: std::net::SocketAddr,
}

/// The person whose EHR an import wrote into, as the identity map names them.
#[derive(Debug, Clone, Copy)]
pub struct Person<'a> {
    /// The person's id.
    pub id: &'a str,
    /// The namespace the id is issued in.
    pub namespace: &'a str,
}

/// One composition version an import committed.
#[derive(Debug, Clone, Copy)]
pub struct Imported<'a> {
    /// The openEHR version uid.
    pub version: &'a str,
    /// The template the composition conforms to.
    pub template_id: &'a str,
}

/// Returns the import record of `message`, which committed `imported` into
/// the EHR of `person`, when the identity map names one.
#[must_use]
pub fn import<'a>(
    message: Message<'_>,
    imported: impl IntoIterator<Item = Imported<'a>>,
    person: Option<Person<'_>>,
) -> Exchange {
    let mut entities: Vec<Entity> = person
        .map(|person| Entity::Patient {
            system: None,
            value: SecretString::from(person.id.to_owned()),
            namespace: Some(person.namespace.to_owned()),
        })
        .into_iter()
        .collect();
    entities.extend(imported.into_iter().map(|committed| {
        Entity::Described(Described {
            what: Some(What::Identifier {
                system: Some(OBJECT_ID_SYSTEM.to_owned()),
                value: committed.version.to_owned(),
            }),
            kind: SYSTEM_OBJECT,
            role: Some(DOMAIN_RESOURCE),
            name: Some(committed.template_id.to_owned()),
            description: None,
            details: Vec::new(),
            security_labels: Vec::new(),
        })
    }));
    Exchange {
        kind: EventKind {
            profile: "",
            event_type: IMPORT,
            subtypes: Vec::new(),
            action: "C",
            client: SOURCE_ROLE,
            server: DESTINATION_ROLE,
        },
        recorded: jiff::Timestamp::now(),
        outcome: Outcome::Success,
        outcome_description: None,
        direction: Direction::Received {
            client: Peer {
                who: message.sender.unwrap_or("unknown").to_owned(),
                network: message.peer.map_or_else(
                    || NetworkAddress::MachineName(String::from("unknown")),
                    |peer| NetworkAddress::Ip(peer.ip()),
                ),
            },
            endpoint: NetworkAddress::Ip(message.listen.ip()),
        },
        on_behalf: OnBehalfOf::System,
        entities,
        security: Vec::new(),
    }
}
