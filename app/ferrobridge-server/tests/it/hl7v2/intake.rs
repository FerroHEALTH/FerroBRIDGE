// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ADT patient triggers the face stores in the inbound intake (IHE ITI-8
//! and ITI-30): which triggers enter, the event's key and stamp, the
//! minimised patient an upsert carries, and the merge statement of A40.

use ferrobridge_hl7v2::decode::{self, Charset};
use ferrobridge_hl7v2::parse::Message;
use ferrobridge_server::hl7v2::intake::{Trigger, Unidentified, events, trigger};
use fhir_types::codec::Value;

/// The identifier system the registry settles the assigning authority in.
const SYSTEM: &str = "http://example.org/fhir/sid/northhosp-mrn";

/// Returns the message `segments` form, lexed.
fn message(segments: &[&str]) -> Message {
    let text = segments.join("\r");
    let decoded = decode::decode(text.as_bytes(), Charset::Ascii).expect("the message decodes");
    ferrobridge_hl7v2::parse::lex::lex(&decoded.text, decoded.charset)
        .expect("the message lexes")
        .message
}

/// Returns an ADT of trigger `event` with `rest` after its PID.
fn adt(event: &str, rest: &[&str]) -> Message {
    let header = format!(
        "MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTH|20260925090000+0200||ADT^{event}^ADT_A01|MSG-{event}|P|2.5.1"
    );
    let mut segments = vec![
        header.as_str(),
        "EVN||20260925085900+0200",
        "PID|1||PAT-0002^^^NORTHHOSP^MR||Doe^Anna^^^^^L||19920304|F",
    ];
    segments.extend_from_slice(rest);
    message(&segments)
}

/// Returns a Bundle holding one `Patient` per identifier value of `values`.
fn bundle(values: &[&str]) -> Value {
    let entries: Vec<serde_json::Value> = values
        .iter()
        .map(|value| {
            serde_json::json!({"resource": {
                "resourceType": "Patient",
                "identifier": [{"system": SYSTEM, "value": value}],
                "name": [{"family": "Doe"}],
                "birthDate": "1992-03-04",
                "gender": "female",
                "address": [{"city": "Utrecht"}]
            }})
        })
        .collect();
    serde_json::from_value(serde_json::json!({"resourceType": "Bundle", "entry": entries}))
        .expect("a legal Bundle")
}

#[test]
fn each_patient_trigger_enters_the_intake_and_no_other_message() {
    for event in ["A01", "A04", "A05", "A08", "A28", "A31"] {
        assert_eq!(Some(Trigger::Upsert), trigger(&adt(event, &[])), "{event}");
    }
    for event in ["A40", "A47"] {
        assert_eq!(Some(Trigger::Merge), trigger(&adt(event, &[])), "{event}");
    }
    for event in ["A02", "A03", "A11"] {
        assert_eq!(None, trigger(&adt(event, &[])), "{event}");
    }
    let oru = message(&[
        "MSH|^~\\&|LAB|NORTHLAB|EHR|SOUTH|20260925090000+0200||ORU^R01^ORU_R01|M-1|P|2.5.1",
    ]);
    assert_eq!(None, trigger(&oru));
}

#[test]
fn an_upsert_is_keyed_by_msh_10_stamped_by_evn_2_and_carries_the_minimised_patient()
-> Result<(), Box<dyn std::error::Error>> {
    let stored = events(&adt("A08", &[]), &bundle(&["PAT-0002"]), Trigger::Upsert)?;
    let [patient] = stored.as_slice() else {
        return Err(format!("one event, got {stored:?}").into());
    };
    assert_eq!(SYSTEM, patient.tenant);
    assert_eq!("PAT-0002", patient.event.entity);
    assert_eq!(Some("2026-09-25T06:59:00Z"), patient.event.stamp.as_deref());
    let again = events(&adt("A08", &[]), &bundle(&["PAT-0002"]), Trigger::Upsert)?;
    assert_eq!(
        again.first().map(|event| event.event.dedupe_key),
        Some(patient.event.dedupe_key),
        "one MSH-10, one key"
    );
    let body: serde_json::Value = serde_json::from_slice(patient.body.as_bytes())?;
    assert_eq!(None, body.get("address"), "the record is minimised: {body}");
    assert_eq!(serde_json::json!("1992-03-04"), body["birthDate"]);
    Ok(())
}

#[test]
fn an_event_without_evn_2_is_stamped_by_msh_7() -> Result<(), Box<dyn std::error::Error>> {
    let message = message(&[
        "MSH|^~\\&|ADMIT|NORTHHOSP|EHR|SOUTH|20260925090000+0200||ADT^A04^ADT_A01|MSG-1|P|2.5.1",
        "EVN|",
        "PID|1||PAT-0002^^^NORTHHOSP^MR",
    ]);
    let stored = events(&message, &bundle(&["PAT-0002"]), Trigger::Upsert)?;
    assert_eq!(
        Some("2026-09-25T07:00:00Z"),
        stored
            .first()
            .and_then(|event| event.event.stamp.as_deref())
    );
    Ok(())
}

#[test]
fn a_patient_without_a_system_is_unidentified_at_pid_3() {
    let bare: Value =
        serde_json::from_value(serde_json::json!({"resourceType": "Bundle", "entry": [
            {"resource": {"resourceType": "Patient", "identifier": [{"value": "PAT-0002"}]}}
        ]}))
        .expect("a legal Bundle");
    assert_eq!(
        Err(Unidentified {
            segment: "PID",
            field: 3
        }),
        events(&adt("A01", &[]), &bare, Trigger::Upsert)
    );
}

#[test]
fn a40_states_the_prior_identifier_replaced_by_the_patient()
-> Result<(), Box<dyn std::error::Error>> {
    let merge = adt("A40", &["MRG|PAT-0001^^^NORTHHOSP^MR"]);
    let stored = events(&merge, &bundle(&["PAT-0002"]), Trigger::Merge)?;
    let [statement] = stored.as_slice() else {
        return Err(format!("one event, got {stored:?}").into());
    };
    assert_eq!("PAT-0001", statement.event.entity);
    assert_eq!(SYSTEM, statement.tenant);
    let body: serde_json::Value = serde_json::from_slice(statement.body.as_bytes())?;
    assert_eq!(
        serde_json::json!([{
            "other": {"identifier": {"system": SYSTEM, "value": "PAT-0002"}},
            "type": "replaced-by"
        }]),
        body["link"]
    );
    Ok(())
}

#[test]
fn a_prior_identifier_of_another_authority_is_unidentified_at_mrg_1() {
    let merge = adt("A40", &["MRG|PAT-0001^^^SOUTHHOSP^MR"]);
    assert_eq!(
        Err(Unidentified {
            segment: "MRG",
            field: 1
        }),
        events(&merge, &bundle(&["PAT-0002"]), Trigger::Merge)
    );
}

/// The inbox of a state store no server answers, sealing under a test key.
fn unreachable_inbox()
-> Result<ferrobridge_server::inbound::store::InboxStore, Box<dyn std::error::Error>> {
    use ferrobridge_server::store::seal::{KeyId, LookupKey, SealKeys, SealingKey};
    let key = SealingKey::from_base64("QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(300))
        .connect_lazy("postgres://bridge@127.0.0.1:1/state?sslmode=disable")?;
    Ok(
        ferrobridge_server::inbound::store::InboxStore::new(pool).with_seal(Some(SealKeys::new(
            (KeyId::new("test")?, key.clone()),
            Vec::new(),
            LookupKey::new(&key),
        ))),
    )
}

#[tokio::test]
async fn a_patient_trigger_the_intake_cannot_store_is_answered_ae_and_commits_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    let mut lane = super::face_settings(true);
    lane.inbox = Some(unreachable_inbox()?);
    let lookup = wiremock::ResponseTemplate::new(200).set_body_string(crate::facade::ehr_body());
    let running = super::Running::start_with(lane, lookup).await?;
    let ack = running.send(&super::adt_a01("MSG-ADT-STORE")).await?;
    assert_eq!(
        vec!["MSA|AE|MSG-ADT-STORE"],
        super::segments(&ack, "MSA"),
        "{ack}"
    );
    assert!(ack.contains("send the message again"), "{ack}");
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}

#[tokio::test]
async fn a_message_that_is_no_patient_trigger_never_touches_the_intake()
-> Result<(), Box<dyn std::error::Error>> {
    let mut lane = super::face_settings(true);
    lane.inbox = Some(unreachable_inbox()?);
    let lookup = wiremock::ResponseTemplate::new(200).set_body_string(crate::facade::ehr_body());
    let running = super::Running::start_with(lane, lookup).await?;
    let ack = running.send(&super::oru_r01("MSG-ORU-STORE")).await?;
    assert_eq!(
        vec!["MSA|AA|MSG-ORU-STORE"],
        super::segments(&ack, "MSA"),
        "{ack}"
    );
    running.stop().await
}

#[tokio::test]
async fn each_patient_trigger_is_stored_and_answered_aa_after_the_commit()
-> Result<(), Box<dyn std::error::Error>> {
    let Some((_postgres, store)) = crate::store::sealed_store().await? else {
        return Ok(());
    };
    let secrets = tempfile::tempdir()?;
    let sources = std::sync::Arc::new(crate::inbound::store::sources(&store, secrets.path())?);
    crate::inbound::store::create(
        &sources,
        &ferrobridge_operator_api::sources::Write {
            name: String::from("northlab-adt"),
            kind: ferrobridge_operator_api::sources::Kind::Mllp,
            enabled: true,
            document: serde_json::json!({
                "routes": [super::route("ADT", "NORTHLAB", Some(super::NORTHLAB_SUBJECT), &[])],
                "profiles": {"Observation": super::PROFILE}
            }),
        },
    )
    .await?;
    let inbox = ferrobridge_server::inbound::store::InboxStore::new(store.pool().clone())
        .with_seal(store.seal().cloned());
    let lookup = wiremock::ResponseTemplate::new(200).set_body_string(crate::facade::ehr_body());
    let running = Box::pin(super::Running::start_wired(
        (super::listeners(), sources, Some(inbox.clone())),
        lookup,
        None,
        ferrobridge_testkit::fixtures::DIAGNOSE_OPT,
    ))
    .await?;
    let triggers = [
        ("A01", "ADT_A01"),
        ("A04", "ADT_A01"),
        ("A05", "ADT_A05"),
        ("A08", "ADT_A01"),
        ("A28", "ADT_A05"),
        ("A31", "ADT_A05"),
    ];
    for (index, (event, structure)) in triggers.iter().enumerate() {
        let control = format!("MSG-ADT-{index}");
        let message = String::from_utf8(super::adt_a01(&control))?
            .replace("ADT^A01^ADT_A01", &format!("ADT^{event}^{structure}"))
            .replace(
                "EVN||20260925143000+0200",
                &format!("EVN||202609251430{index:02}+0200"),
            );
        let ack = running.send(message.as_bytes()).await?;
        assert_eq!(
            vec![format!("MSA|AA|{control}")],
            super::segments(&ack, "MSA"),
            "{event}: {ack}"
        );
    }
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM inbound_event WHERE body IS NOT NULL AND NOT superseded",
    )
    .fetch_one(inbox.pool())
    .await?;
    assert_eq!(6, stored, "one stored patient event per trigger");
    running.stop().await
}
