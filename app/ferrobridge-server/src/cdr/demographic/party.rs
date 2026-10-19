// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How a Release 1.1.0 `PARTY` is read through a client generated over the
//! development line.
//!
//! RM Demographic IM Release 1.1.0 §2.2.1 gives `PARTY` the optional
//! `reverse_relationships: List<LOCATABLE_REF>`, "References to relationships
//! in which this Party takes part as target", and its invariant
//! `Reverse_relationships_validity` requires every entry to name a
//! `PARTY_RELATIONSHIP` the demographic repository holds whose `target` is
//! this party. The development line removed the attribute (SPECRM-124), and
//! the `openehr-its` client decodes a party into the `openehr_rm::v1_2`
//! generation, whose strict reader refuses a member it does not define.
//!
//! The bridge drops the member before the client decodes the body, and the
//! caller learns how many entries it dropped. Dropping loses no fact: by the
//! invariant, each entry restates a relationship the repository holds on the
//! source party. A member that breaks the invariant's shape (an empty list, or
//! an entry that is no `LOCATABLE_REF`) is left in place, so the strict reader
//! refuses it as Release 1.1.0 does.

use openehr_base::v1_3::base_types::identification::locatable_ref::LocatableRef;
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;

/// The `PARTY` attribute Release 1.1.0 defines and the development line
/// removed.
const REVERSE_RELATIONSHIPS: &str = "reverse_relationships";

/// Returns `body` without the `reverse_relationships` of the `PARTY` it
/// carries, with the number of entries dropped, or `None` when there is
/// nothing to drop.
///
/// `None` also answers a body that is no JSON object and a member whose shape
/// Release 1.1.0 refuses, so the generated client reads the body as it came
/// and reports its own refusal.
pub(crate) fn without_reverse_relationships(body: &[u8]) -> Option<(Vec<u8>, usize)> {
    // NOTE: no specification governs this: our own design; a body that is not
    // JSON is the generated client's to refuse, so it passes through untouched.
    let mut value: Value = serde_json::from_slice(body).ok()?;
    let party = value.as_object_mut()?;
    let dropped = droppable(party)?;
    party.remove(REVERSE_RELATIONSHIPS);
    // NOTE: no specification governs this: our own design; a value read from
    // JSON always writes back, and a failure leaves the body as it came.
    let rewritten = serde_json::to_vec(&value).ok()?;
    Some((rewritten, dropped))
}

/// Returns how many entries the `reverse_relationships` of `party` holds,
/// when the member is there in the shape Release 1.1.0 admits.
fn droppable(party: &Map<String, Value>) -> Option<usize> {
    let entries = party.get(REVERSE_RELATIONSHIPS)?.as_array()?;
    let admitted = !entries.is_empty() && entries.iter().all(is_locatable_ref);
    admitted.then_some(entries.len())
}

/// Answers whether `entry` reads as a `LOCATABLE_REF` in canonical JSON.
fn is_locatable_ref(entry: &Value) -> bool {
    LocatableRef::deserialize(entry).is_ok()
}
