// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ADT patient triggers the face stores in the inbound intake.
//!
//! A01, A04, A05 and A08 (IHE ITI-8) and A28 and A31 (IHE ITI-30) store the
//! message's `Patient` as an upsert; A40 and A47 store the source's own
//! merge statement, the prior identifier of MRG-1 `replaced-by` the
//! identifier of PID-3, which the one handler forwards as Resolve Duplicate.
//! The de-duplication key is MSH-10 and the stamp EVN-2, or MSH-7 without
//! it. Which triggers enter the intake is our own design over the IHE
//! transactions.

use ferrobridge_hl7v2::parse::{Message, Segment};
use fhir_types::codec::{Object, Value};

use crate::inbound::event::{Change, DedupeKey, Event};
use crate::inbound::store::Body;
use crate::sources::document::Tenant;

/// The triggers whose `Patient` is stored as an upsert.
const UPSERTS: [&str; 6] = ["A01", "A04", "A05", "A08", "A28", "A31"];

/// The triggers that carry the source's own merge statement.
const MERGES: [&str; 2] = ["A40", "A47"];

/// What a patient trigger asks of the intake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// Register or update the patient.
    Upsert,
    /// The source merged the prior identifier into the patient.
    Merge,
}

/// One patient event of a message: the identifier system it is stored
/// under as the tenant, the event and the minimised record it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatientEvent {
    /// The identifier system of the patient's identifier.
    pub tenant: String,
    /// The event.
    pub event: Event,
    /// The record the worker applies.
    pub body: Body,
}

/// Why a patient trigger names no patient the intake can key, with the
/// field at fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{segment}-{field} names no identifier the patient event can be keyed by")]
pub struct Unidentified {
    /// The segment.
    pub segment: &'static str,
    /// The field.
    pub field: usize,
}

/// Returns what `message` asks of the intake, `None` for a message that is
/// no ADT patient trigger.
#[must_use]
pub fn trigger(message: &Message) -> Option<Trigger> {
    if message.message_type(1) != Some("ADT") {
        return None;
    }
    let event = message.message_type(2)?;
    if UPSERTS.contains(&event) {
        Some(Trigger::Upsert)
    } else if MERGES.contains(&event) {
        Some(Trigger::Merge)
    } else {
        None
    }
}

/// Returns the patient events of `message`, whose maps wrote `bundle`.
///
/// # Errors
/// Returns [`Unidentified`] when a patient has no identifier with a system,
/// or when MRG-1 names an assigning authority other than the PID-3 it is
/// paired with, so its system is not known.
pub fn events(
    message: &Message,
    bundle: &Value,
    trigger: Trigger,
) -> Result<Vec<PatientEvent>, Unidentified> {
    let control = message.control_id().unwrap_or_default();
    let stamp = stamp(message);
    let patients = patients(bundle);
    let event = |index: usize, tenant: String, entity: String, body: &Value| PatientEvent {
        event: Event {
            change: Change::Upsert,
            tenant: Some(tenant.clone()),
            entity,
            dedupe_key: DedupeKey::of(vec![
                serde_json::Value::from("hl7v2"),
                serde_json::Value::from(control),
                serde_json::Value::from(index),
            ]),
            stamp: stamp.clone(),
        },
        tenant,
        body: body_of(body),
    };
    match trigger {
        Trigger::Upsert => {
            let patient = patients.first().ok_or(PID_3)?;
            let (system, value) = identifier(patient).ok_or(PID_3)?;
            let tenant = tenant_of(&system);
            let kept = crate::inbound::apply::minimised(patient, &tenant, &value);
            Ok(vec![event(0, system, value, &kept)])
        }
        Trigger::Merge => {
            let pids = segments(message, "PID");
            let mrgs = segments(message, "MRG");
            if mrgs.is_empty() {
                return Err(MRG_1);
            }
            mrgs.iter()
                .enumerate()
                .map(|(index, mrg)| {
                    let patient = patients.get(index).ok_or(PID_3)?;
                    let (system, survivor) = identifier(patient).ok_or(PID_3)?;
                    let prior = mrg.field(1).and_then(|field| field.repetitions().first());
                    let value = prior
                        .and_then(|cx| cx.component(1))
                        .and_then(|component| component.subcomponent(1))
                        .filter(|value| !value.is_empty())
                        .ok_or(MRG_1)?;
                    let authority = |cx: Option<&ferrobridge_hl7v2::parse::Repetition>| {
                        cx.and_then(|cx| cx.component(4))
                            .map(|component| component.subcomponents().join("&"))
                    };
                    let survivor_cx = pids
                        .get(index)
                        .and_then(|pid| pid.field(3))
                        .and_then(|field| field.repetitions().first());
                    if authority(prior) != authority(survivor_cx) {
                        return Err(MRG_1);
                    }
                    let merged = merge_statement(&system, value, &survivor);
                    Ok(event(index, system, value.to_owned(), &merged))
                })
                .collect()
        }
    }
}

/// PID-3, the patient identifier list.
const PID_3: Unidentified = Unidentified {
    segment: "PID",
    field: 3,
};

/// MRG-1, the prior patient identifier list.
const MRG_1: Unidentified = Unidentified {
    segment: "MRG",
    field: 1,
};

/// Returns the segments of `message` with id `id`, in message order.
fn segments<'a>(message: &'a Message, id: &str) -> Vec<&'a Segment> {
    message
        .segments()
        .iter()
        .filter(|segment| segment.id() == id)
        .collect()
}

/// Returns the `Patient` resources of `bundle`, in entry order.
fn patients(bundle: &Value) -> Vec<&Value> {
    let Some(Value::Array(entries)) = bundle.get("entry") else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| entry.get("resource"))
        .filter(|resource| resource.get("resourceType").and_then(Value::as_str) == Some("Patient"))
        .collect()
}

/// Returns the system and value of the first identifier of `patient` that
/// names both.
fn identifier(patient: &Value) -> Option<(String, String)> {
    let Some(Value::Array(identifiers)) = patient.get("identifier") else {
        return None;
    };
    identifiers.iter().find_map(|identifier| {
        let system = identifier.get("system").and_then(Value::as_str)?;
        let value = identifier.get("value").and_then(Value::as_str)?;
        (!system.is_empty() && !value.is_empty()).then(|| (system.to_owned(), value.to_owned()))
    })
}

/// Returns the tenant an MLLP patient event is stored under: its identifier
/// system.
fn tenant_of(system: &str) -> Tenant {
    Tenant {
        name: system.to_owned(),
        identifier_system: system.to_owned(),
        credentials: None,
        base: None,
        enabled: true,
    }
}

/// Returns the `Patient` stating that `prior` in `system` is
/// `replaced-by` `survivor` (R4 `link-type`).
fn merge_statement(system: &str, prior: &str, survivor: &str) -> Value {
    let identifier = |value: &str| {
        let mut identifier = Object::new();
        identifier.insert(String::from("system"), Value::String(system.to_owned()));
        identifier.insert(String::from("value"), Value::String(value.to_owned()));
        Value::Object(identifier)
    };
    let mut other = Object::new();
    other.insert(String::from("identifier"), identifier(survivor));
    let mut link = Object::new();
    link.insert(String::from("other"), Value::Object(other));
    link.insert(
        String::from("type"),
        Value::String(String::from("replaced-by")),
    );
    let mut patient = Object::new();
    patient.insert(
        String::from("resourceType"),
        Value::String(String::from("Patient")),
    );
    patient.insert(
        String::from("identifier"),
        Value::Array(vec![identifier(prior)]),
    );
    patient.insert(
        String::from("link"),
        Value::Array(vec![Value::Object(link)]),
    );
    Value::Object(patient)
}

/// Returns `patient` written as JSON, the body its event carries.
#[expect(
    clippy::expect_used,
    reason = "a codec Value has string keys and numbers read from JSON, so it always writes"
)]
fn body_of(patient: &Value) -> Body {
    Body::new(serde_json::to_vec(patient).expect("a codec Value should write as JSON"))
}

/// Returns the instant EVN-2, or MSH-7 without it, records, in RFC 3339 UTC;
/// `None` when neither reads as a `DTM` with a time and an offset.
fn stamp(message: &Message) -> Option<String> {
    let evn = message
        .segments()
        .iter()
        .find(|segment| segment.id() == "EVN")
        .and_then(|evn| evn.text(2));
    let msh = message.header().and_then(|header| header.text(7));
    let text = evn.filter(|text| !text.is_empty()).or(msh)?;
    // NOTE: our own design; a DTM without a time or an offset names no instant, so the event carries no stamp to order by.
    let converted = ferrobridge_hl7v2::map::convert::primitive("dateTime", text).ok()?;
    let instant: jiff::Timestamp = converted.as_str()?.parse().ok()?;
    Some(instant.to_string())
}
