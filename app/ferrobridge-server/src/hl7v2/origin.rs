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
