// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The event map picks the action, the tenant, the entity and the
//! de-duplication key from a verified event.

use ferrobridge_server::inbound::event::{Answered, Change, EventMap, Part, Reading, Unreadable};
use http::{HeaderMap, HeaderValue};
use serde_json::{Value, json};

// A map of the shape a webhook source with no event id declares: the action
// from the model and event type, the key over the fields that tell two
// notifications apart.
fn webhook_map() -> EventMap {
    serde_json::from_value(json!({
        "tenant": "/customerCode",
        "entity": "/id",
        "rules": [
            {"when": {"/modelType": "client", "/eventType": "CREATE"}, "action": "upsert"},
            {"when": {"/modelType": "client", "/eventType": "UPDATE"}, "action": "upsert"},
            {"when": {"/modelType": "client", "/eventType": "DELETE"}, "action": "remove"},
            {"when": {"/eventType": "NOP"}, "action": "probe"}
        ],
        "dedupe": {"pointers": ["/customerCode", "/modelType", "/eventType", "/id", "/timestamp"]},
        "stamp": "/timestamp"
    }))
    .expect("an event map")
}

fn notification(event_type: &str, timestamp: &str) -> Value {
    json!({"customerCode": "t1", "modelType": "client", "eventType": event_type, "id": 42, "timestamp": timestamp})
}

fn stored(reading: Reading) -> ferrobridge_server::inbound::event::Event {
    match reading {
        Reading::Store(event) => Some(event),
        Reading::Answer(_) => None,
    }
    .expect("a stored event")
}

#[test]
fn a_rule_picks_the_change_and_the_parts_are_read() {
    let map = webhook_map();
    let event = stored(
        map.read(
            &HeaderMap::new(),
            &notification("UPDATE", "2026-11-24T10:00:00Z"),
        )
        .expect("a reading"),
    );
    assert_eq!(Change::Upsert, event.change);
    assert_eq!(Some(String::from("t1")), event.tenant);
    assert_eq!("42", event.entity);
    assert_eq!(Some(String::from("2026-11-24T10:00:00Z")), event.stamp);

    let removed = stored(
        map.read(
            &HeaderMap::new(),
            &notification("DELETE", "2026-11-24T10:00:01Z"),
        )
        .expect("a reading"),
    );
    assert_eq!(Change::Remove, removed.change);
}

#[test]
fn a_probe_and_an_unmatched_event_are_answered_unstored() {
    let map = webhook_map();
    assert_eq!(
        Reading::Answer(Answered::Probe),
        map.read(&HeaderMap::new(), &json!({"eventType": "NOP"}))
            .expect("a reading")
    );
    assert_eq!(
        Reading::Answer(Answered::Ignored),
        map.read(
            &HeaderMap::new(),
            &json!({"customerCode": "t1", "modelType": "employee", "eventType": "UPDATE", "id": 7})
        )
        .expect("a reading")
    );
}

#[test]
fn the_same_event_keys_alike_and_another_keys_apart() {
    let map = webhook_map();
    let read = |body: &Value| stored(map.read(&HeaderMap::new(), body).expect("a reading"));
    let first = read(&notification("UPDATE", "2026-11-24T10:00:00Z"));
    let again = read(&notification("UPDATE", "2026-11-24T10:00:00Z"));
    let later = read(&notification("UPDATE", "2026-11-24T10:00:05Z"));
    let create = read(&notification("CREATE", "2026-11-24T10:00:00Z"));
    assert_eq!(first.dedupe_key, again.dedupe_key);
    assert_ne!(first.dedupe_key, later.dedupe_key);
    assert_ne!(first.dedupe_key, create.dedupe_key);
}

#[test]
fn an_event_id_header_keys_the_event() {
    let map: EventMap = serde_json::from_value(json!({
        "entity": "/data/patient",
        "otherwise": "upsert",
        "dedupe": {"headers": ["webhook-id"]}
    }))
    .expect("an event map");
    let mut headers = HeaderMap::new();
    headers.insert("webhook-id", HeaderValue::from_static("msg_1"));
    let body = json!({"data": {"patient": "p-1"}});
    let first = stored(map.read(&headers, &body).expect("a reading"));
    let changed = stored(
        map.read(
            &headers,
            &json!({"data": {"patient": "p-1", "extra": true}}),
        )
        .expect("a reading"),
    );
    assert_eq!(None, first.tenant);
    assert_eq!(first.dedupe_key, changed.dedupe_key);
    assert_eq!(
        Err(Unreadable::Missing(Part::Dedupe)),
        map.read(&HeaderMap::new(), &body)
    );
}

#[test]
fn a_stored_event_without_its_parts_is_unreadable() {
    let map = webhook_map();
    assert_eq!(
        Err(Unreadable::Missing(Part::Entity)),
        map.read(
            &HeaderMap::new(),
            &json!({"customerCode": "t1", "modelType": "client", "eventType": "UPDATE", "timestamp": "x"})
        )
    );
    assert_eq!(
        Err(Unreadable::NotScalar(Part::Tenant)),
        map.read(
            &HeaderMap::new(),
            &json!({"customerCode": ["t1"], "modelType": "client", "eventType": "UPDATE", "id": 1, "timestamp": "x"})
        )
    );
    assert_eq!(
        Err(Unreadable::Missing(Part::Stamp)),
        map.read(
            &HeaderMap::new(),
            &json!({"customerCode": "t1", "modelType": "client", "eventType": "UPDATE", "id": 1})
        )
    );
}

#[test]
fn an_unknown_key_or_action_is_refused() {
    for refused in [
        json!({"entity": "/id", "extra": 1}),
        json!({"entity": "/id", "otherwise": "merge"}),
        json!({"entity": "/id", "rules": [{"when": {}, "action": "upsert", "extra": 1}]}),
    ] {
        assert!(
            serde_json::from_value::<EventMap>(refused.clone()).is_err(),
            "{refused}"
        );
    }
}

#[test]
fn a_key_prints_as_hex() {
    let map = webhook_map();
    let event = stored(
        map.read(&HeaderMap::new(), &notification("UPDATE", "t"))
            .expect("a reading"),
    );
    let printed = format!("{:?}", event.dedupe_key);
    assert_eq!(64 + "DedupeKey()".len(), printed.len());
}
