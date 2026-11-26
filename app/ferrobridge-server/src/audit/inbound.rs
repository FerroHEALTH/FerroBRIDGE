// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The records of the inbound engine's own work: a patient identity imported
//! from a source and fed to the identity source, and an EHR created for it.
//!
//! The engine acts on its own behalf, so, as for an HL7 v2 import, it
//! records each step as the system and writes no Annex II 3.2 access record.
//! The feed is an import of PHI, the DICOM PS3.15 A.5.3 "Import" event (ITI
//! TF-2 Table 3.20.4.1.1.1-1); the EHR is a create on the CDR over ITS-REST.

use ihe_atna::balp::CREATE;
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
use ihe_atna::balp::REST;
use ihe_atna::balp::SOURCE_ROLE;
use ihe_atna::balp::SYSTEM_OBJECT;
use ihe_atna::balp::What;
use ihe_atna::user::OnBehalfOf;
use secrecy::SecretString;

use super::hl7v2::OBJECT_ID_SYSTEM;

/// The patient one record is about, by an identifier.
#[derive(Debug, Clone, Copy)]
pub struct Patient<'a> {
    /// The identifier's system.
    pub system: &'a str,
    /// The identifier's value.
    pub value: &'a str,
}

/// Returns the record of the patient `patient` imported from the source
/// named `source` and fed to the identity source, received at `endpoint`.
#[must_use]
pub fn imported(source: &str, endpoint: NetworkAddress, patient: Patient<'_>) -> Exchange {
    exchange(
        (IMPORT, Vec::new()),
        Direction::Received {
            client: Peer {
                who: source.to_owned(),
                network: NetworkAddress::MachineName(source.to_owned()),
            },
            endpoint,
        },
        vec![entity_of(patient)],
    )
}

/// Returns the record of the EHR `ehr_id` created for `person` in the CDR
/// at `cdr`.
#[must_use]
pub fn ehr_created(cdr: NetworkAddress, person: Patient<'_>, ehr_id: &str) -> Exchange {
    exchange(
        (REST, vec![CREATE]),
        Direction::Sent {
            server: Peer {
                who: cdr.address(),
                network: cdr,
            },
        },
        vec![
            entity_of(person),
            Entity::Described(Described {
                what: Some(What::Identifier {
                    system: Some(OBJECT_ID_SYSTEM.to_owned()),
                    value: ehr_id.to_owned(),
                }),
                kind: SYSTEM_OBJECT,
                role: Some(DOMAIN_RESOURCE),
                name: Some(String::from("EHR")),
                description: None,
                details: Vec::new(),
                security_labels: Vec::new(),
            }),
        ],
    )
}

/// Returns the patient entity of `patient`.
fn entity_of(patient: Patient<'_>) -> Entity {
    Entity::Patient {
        system: Some(patient.system.to_owned()),
        value: SecretString::from(patient.value.to_owned()),
        namespace: None,
    }
}

/// Returns the successful create the system made, of `event` and its
/// subtypes, in `direction`, about `entities`.
fn exchange(
    (event_type, subtypes): (ihe_atna::balp::Coded, Vec<ihe_atna::balp::Coded>),
    direction: Direction,
    entities: Vec<Entity>,
) -> Exchange {
    Exchange {
        kind: EventKind {
            profile: "",
            event_type,
            subtypes,
            action: "C",
            client: SOURCE_ROLE,
            server: DESTINATION_ROLE,
        },
        recorded: jiff::Timestamp::now(),
        outcome: Outcome::Success,
        outcome_description: None,
        direction,
        on_behalf: OnBehalfOf::System,
        entities,
        security: Vec::new(),
    }
}
