// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where a message's compositions came from, as their `FEEDER_AUDIT` records
//! it (RM Common IM, `FEEDER_AUDIT`: "the origin of data that have been
//! transformed into openEHR form").
//!
//! The sending application (MSH-3) is the originating system and the bridge
//! the feeder. The placer and filler numbers of the orders (ORC-2, ORC-3,
//! OBR-2, OBR-3) are the "Identifiers used for the item in the originating
//! system, e.g. filler and placer ids", typed by HL7 table 0203 (`PLAC`,
//! `FILL`) and issued by the number's namespace. The message control id
//! (MSH-10) identifies what the bridge received, so it is the feeder's item
//! id. Every order number of the message goes on each of its compositions,
//! since the guide maps the message as one Bundle; that allocation is our own
//! design.

use ferrobridge_hl7v2::parse::Field;
use ferrobridge_hl7v2::parse::Message;
use fhirconnect::engine::origin::ItemId;
use fhirconnect::engine::origin::Party;

use crate::config::hl7v2::Sender;

/// The HL7 table 0203 code of a placer identifier.
const PLACER: &str = "PLAC";

/// The HL7 table 0203 code of a filler identifier.
const FILLER: &str = "FILL";

/// The order-number fields, by segment, with the identifier type of each:
/// ORC-2 and OBR-2 the placer, ORC-3 and OBR-3 the filler order number.
const ORDER_NUMBERS: [(&str, usize, &str); 4] = [
    ("ORC", 2, PLACER),
    ("ORC", 3, FILLER),
    ("OBR", 2, PLACER),
    ("OBR", 3, FILLER),
];

/// Returns the sending application MSH-3 names: its namespace id (HD.1), or
/// its universal id (HD.2) when it names none.
#[must_use]
pub fn sending_system(message: &Message) -> Option<String> {
    let application = message.header()?.field(3)?.repetitions().first()?;
    application
        .component(1)
        .and_then(|component| component.subcomponent(1))
        .or_else(|| {
            application
                .component(2)
                .and_then(|component| component.subcomponent(1))
        })
        .map(str::to_owned)
}

/// Returns the placer and filler numbers of every order in `message`, each
/// once, in message order.
#[must_use]
pub fn order_numbers(message: &Message) -> Vec<ItemId> {
    let mut found: Vec<ItemId> = Vec::new();
    for segment in message.segments() {
        for (id, position, kind) in ORDER_NUMBERS {
            if segment.id() != id {
                continue;
            }
            let Some(item) = segment
                .field(position)
                .and_then(|field| entity(field, kind))
            else {
                continue;
            };
            if !found.contains(&item) {
                found.push(item);
            }
        }
    }
    found
}

/// Returns the item id of the message the bridge received: its control id
/// (MSH-10), typed by its message type and issued by the sending system.
#[must_use]
pub fn control_item(control: &str, message_type: &str, sending: Option<&str>) -> ItemId {
    ItemId {
        id: control.to_owned(),
        kind: message_type.to_owned(),
        issuer: sending.map(str::to_owned),
    }
}

/// Returns the entity identifier (EI) `field` holds as an item id of `kind`:
/// EI.1 the id, and EI.2 the namespace, or EI.3 the universal id, its issuer.
fn entity(field: &Field, kind: &str) -> Option<ItemId> {
    let repetition = field.repetitions().first()?;
    let part = |position: usize| {
        repetition
            .component(position)
            .and_then(|component| component.subcomponent(1))
    };
    let id = part(1)?;
    Some(ItemId {
        id: id.to_owned(),
        kind: kind.to_owned(),
        issuer: part(2).or_else(|| part(3)).map(str::to_owned),
    })
}

/// The health care facility a message names, with the field it was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facility {
    /// The facility as a party: its name and its identifiers.
    pub party: Party,
    /// The facility as the senders' list names one: its namespace id, and its
    /// universal id with its type.
    pub names: Vec<Sender>,
    /// The segment and field it was read from.
    pub field: (&'static str, usize),
}

impl Facility {
    /// Returns how an acknowledgment names the facility.
    #[must_use]
    pub fn display(&self) -> String {
        let id = self.party.identifiers.first().map(|item| item.id.as_str());
        match (self.party.name.as_deref(), id) {
            (Some(name), Some(id)) => format!("{name} ({id})"),
            (Some(text), None) | (None, Some(text)) => text.to_owned(),
            (None, None) => String::from("an unnamed facility"),
        }
    }

    /// Returns whether `senders` lists this facility; an empty list lists any.
    #[must_use]
    pub fn listed(&self, senders: &[Sender]) -> bool {
        senders.is_empty() || self.names.iter().any(|name| senders.contains(name))
    }
}

/// Returns the health care facility `message` names.
///
/// It is read from the first field that names one: the assigned location's facility (PV1-3.4) or the servicing
/// facility (PV1-39) of an encounter, the ordering facility (ORC-21) of an
/// order, and the sending facility (MSH-4) last.
///
/// RM EHR IM, `EVENT_CONTEXT.health_care_facility`: "The health care facility
/// under whose care the event took place", "the most specific workgroup or
/// delivery unit", so the most specific field the message holds wins; the
/// order is our own design.
#[must_use]
pub fn facility(message: &Message) -> Option<Facility> {
    let first = |id: &str, position: usize| {
        message
            .segments()
            .iter()
            .find(|segment| segment.id() == id)
            .and_then(|segment| segment.field(position))
            .and_then(|field| field.repetitions().first())
    };
    if let Some(found) = first("PV1", 3)
        .and_then(|location| location.component(4))
        .and_then(|hd| hierarchic(|position| hd.subcomponent(position), ("PV1", 3)))
    {
        return Some(found);
    }
    if let Some(found) = first("PV1", 39).and_then(|servicing| {
        hierarchic(
            |position| {
                (position == 1)
                    .then(|| servicing.component(1).and_then(|c| c.subcomponent(1)))
                    .flatten()
            },
            ("PV1", 39),
        )
    }) {
        return Some(found);
    }
    if let Some(found) = first("ORC", 21).and_then(organisation_facility) {
        return Some(found);
    }
    let sending = message.header()?.field(4)?.repetitions().first()?;
    hierarchic(
        |position| {
            sending
                .component(position)
                .and_then(|component| component.subcomponent(1))
        },
        ("MSH", 4),
    )
}

/// Returns the performing organisation of the first result that names one
/// (OBX-23, `XON`): its name (XON.1) and its identifier (XON.10).
#[must_use]
pub fn performing_organisation(message: &Message) -> Option<Party> {
    message
        .segments()
        .iter()
        .filter(|segment| segment.id() == "OBX")
        .filter_map(|segment| segment.field(23)?.repetitions().first())
        .find_map(|xon| organisation(xon).map(|(party, _)| party))
}

/// Returns the facility an ordering facility (ORC-21, `XON`) names.
fn organisation_facility(xon: &ferrobridge_hl7v2::parse::Repetition) -> Option<Facility> {
    let (party, names) = organisation(xon)?;
    Some(Facility {
        party,
        names,
        field: ("ORC", 21),
    })
}

/// Returns the party an `XON` names, its name (XON.1) and its identifier
/// (XON.10, or XON.3 where an older sender puts it), with the facility names
/// the identifier gives.
fn organisation(xon: &ferrobridge_hl7v2::parse::Repetition) -> Option<(Party, Vec<Sender>)> {
    let part = |position: usize| {
        xon.component(position)
            .and_then(|component| component.subcomponent(1))
    };
    let name = part(1).map(str::to_owned);
    let id = part(10).or_else(|| part(3));
    if name.is_none() && id.is_none() {
        return None;
    }
    let identifiers = id
        .map(|id| ItemId {
            id: id.to_owned(),
            kind: String::from(NAMESPACE),
            issuer: None,
        })
        .into_iter()
        .collect();
    let names = id
        .map(|id| Sender::Namespace(id.to_owned()))
        .into_iter()
        .collect();
    Some((Party { name, identifiers }, names))
}

/// Returns the facility an `HD` names through `part`, its namespace id (HD.1)
/// and its universal id (HD.2) of the type HD.3.
fn hierarchic<'a>(
    part: impl Fn(usize) -> Option<&'a str>,
    field: (&'static str, usize),
) -> Option<Facility> {
    let namespace = part(1);
    let universal = part(2).zip(part(3));
    if namespace.is_none() && universal.is_none() {
        return None;
    }
    let mut identifiers = Vec::new();
    let mut names = Vec::new();
    if let Some(namespace) = namespace {
        identifiers.push(ItemId {
            id: namespace.to_owned(),
            kind: String::from(NAMESPACE),
            issuer: None,
        });
        names.push(Sender::Namespace(namespace.to_owned()));
    }
    if let Some((id, kind)) = universal {
        identifiers.push(ItemId {
            id: id.to_owned(),
            kind: kind.to_owned(),
            issuer: None,
        });
        names.push(Sender::Universal {
            id: id.to_owned(),
            kind: kind.to_owned(),
        });
    }
    Some(Facility {
        party: Party {
            name: namespace.map(str::to_owned),
            identifiers,
        },
        names,
        field,
    })
}

/// The `DV_IDENTIFIER.type` of a facility's namespace id; no specification
/// names one, so the bridge spells it (our own design).
const NAMESPACE: &str = "namespace";
