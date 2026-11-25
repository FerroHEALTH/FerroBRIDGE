// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! R4 Subscription notifications read from the Subscriptions R5 Backport IG
//! STU 1.1.0 examples (`docs/specs/subscriptions-backport/`), and the rules
//! of the R4 status profile a Bundle may break.

use std::error::Error;

use ferrobridge_server::inbound::subscription::{
    Malformed, NotificationEvent, NotificationType, patient_of, read,
};
use serde_json::{Value, json};

/// The directory of the vendored examples.
const EXAMPLES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/subscriptions-backport"
);

/// The example `Bundle-r4-notification-<name>.json`.
fn example(name: &str) -> Result<Value, Box<dyn Error>> {
    let text = std::fs::read_to_string(format!("{EXAMPLES}/Bundle-r4-notification-{name}.json"))?;
    Ok(serde_json::from_str(&text)?)
}

/// The subscription every example names.
const ADMISSION: &str = "https://example.org/fhir/Subscription/admission";

/// The encounter the event examples focus on.
const ENCOUNTER: &str = "https://example.org/fhir/Encounter/86009987-eabe-42bf-8c02-b112b18cb616";

#[test]
fn a_handshake_names_its_subscription_at_event_zero() -> Result<(), Box<dyn Error>> {
    let handshake = read(&example("handshake")?)?;
    assert_eq!(ADMISSION, handshake.subscription);
    assert_eq!(NotificationType::Handshake, handshake.kind);
    assert_eq!("requested", handshake.status);
    assert_eq!(Some(0), handshake.events_since_start);
    assert_eq!(Vec::<NotificationEvent>::new(), handshake.events);
    Ok(())
}

#[test]
fn a_heartbeat_carries_the_count_a_gap_is_found_by() -> Result<(), Box<dyn Error>> {
    let heartbeat = read(&example("heartbeat")?)?;
    assert_eq!(NotificationType::Heartbeat, heartbeat.kind);
    assert_eq!(Some(2), heartbeat.events_since_start);
    assert_eq!(Vec::<NotificationEvent>::new(), heartbeat.events);
    Ok(())
}

#[test]
fn an_id_only_event_names_its_focus_and_carries_no_resource() -> Result<(), Box<dyn Error>> {
    let notification = read(&example("id-only")?)?;
    assert_eq!(NotificationType::EventNotification, notification.kind);
    assert_eq!(
        vec![NotificationEvent {
            number: 2,
            focus: Some(String::from(ENCOUNTER)),
            resource: None,
        }],
        notification.events
    );
    Ok(())
}

#[test]
fn a_full_resource_event_carries_the_resource_it_focuses_on() -> Result<(), Box<dyn Error>> {
    let notification = read(&example("full-resource")?)?;
    let event = notification.events.first().ok_or("one event")?;
    assert_eq!(Some(ENCOUNTER), event.focus.as_deref());
    let resource = event.resource.as_ref().ok_or("the carried encounter")?;
    assert_eq!(Some("Encounter"), resource["resourceType"].as_str());
    Ok(())
}

#[test]
fn an_empty_event_has_a_number_and_no_focus() -> Result<(), Box<dyn Error>> {
    let notification = read(&example("empty")?)?;
    let event = notification.events.first().ok_or("one event")?;
    assert_eq!(2, event.number);
    assert_eq!(None, event.focus);
    Ok(())
}

#[test]
fn the_patient_is_the_focus_or_the_carried_subject() -> Result<(), Box<dyn Error>> {
    let patient = NotificationEvent {
        number: 1,
        focus: Some(String::from("https://example.org/fhir/Patient/p1")),
        resource: None,
    };
    assert_eq!(
        Some("https://example.org/fhir/Patient/p1"),
        patient_of(&patient).as_deref()
    );
    let full = read(&example("full-resource")?)?;
    let encounter = full.events.first().ok_or("one event")?;
    let subject = encounter
        .resource
        .as_ref()
        .and_then(|resource| resource.pointer("/subject/reference"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    assert_eq!(subject, patient_of(encounter));
    let id_only = read(&example("id-only")?)?;
    assert_eq!(None, patient_of(id_only.events.first().ok_or("one event")?));
    Ok(())
}

#[test]
fn a_bundle_breaking_the_status_profile_is_refused() -> Result<(), Box<dyn Error>> {
    let mut no_history = example("heartbeat")?;
    no_history["type"] = json!("transaction");
    assert_eq!(Err(Malformed::NotHistory), read(&no_history));

    let mut no_status = example("heartbeat")?;
    no_status["entry"] = json!([]);
    assert_eq!(Err(Malformed::NoStatus), read(&no_status));

    let mut twice = example("heartbeat")?;
    let parameters = twice["entry"][0]["resource"]["parameter"]
        .as_array_mut()
        .ok_or("parameters")?;
    parameters.push(json!({"name": "type", "valueCode": "heartbeat"}));
    assert_eq!(Err(Malformed::Parameter("type")), read(&twice));

    let mut unknown = example("heartbeat")?;
    unknown["entry"][0]["resource"]["parameter"][3]["valueCode"] = json!("ping");
    assert_eq!(Err(Malformed::Value("type")), read(&unknown));

    let mut count = example("heartbeat")?;
    count["entry"][0]["resource"]["parameter"][4]["valueString"] = json!("two");
    assert_eq!(
        Err(Malformed::Value("events-since-subscription-start")),
        read(&count)
    );
    Ok(())
}
