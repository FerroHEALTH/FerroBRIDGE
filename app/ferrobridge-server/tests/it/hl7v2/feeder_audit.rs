// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a committed message's compositions record in `FEEDER_AUDIT` (RM
//! Common IM, `FEEDER_AUDIT`): the sending application as the originating
//! system, the bridge as the feeder, the placer and filler numbers as the
//! originating system's item ids ("e.g. filler and placer ids"), and the
//! message control id as the feeder's.

use std::error::Error;
use std::time::Duration;

use super::{CONTROL_ID, Running, oru_r01};
use crate::facade::EHR_ID;

/// Returns the `FEEDER_AUDIT` of every version of the one contribution the
/// CDR received.
async fn audits(running: &Running) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let committed = running
        .requests("POST", &format!("/ehr/{EHR_ID}/contribution"))
        .await;
    let body: serde_json::Value =
        serde_json::from_slice(committed.first().ok_or("one contribution")?)?;
    Ok(body["versions"]
        .as_array()
        .ok_or("the versions")?
        .iter()
        .map(|version| version["data"]["feeder_audit"].clone())
        .collect())
}

/// Returns the `(type, id, issuer)` of every entry of `list` in `audit`.
fn items(audit: &serde_json::Value, list: &str) -> Vec<(String, String, Option<String>)> {
    audit[list]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| {
                    (
                        entry["type"].as_str().unwrap_or_default().to_owned(),
                        entry["id"].as_str().unwrap_or_default().to_owned(),
                        entry["issuer"].as_str().map(str::to_owned),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn an_oru_records_its_sender_orders_and_control_id_in_feeder_audit()
-> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    running.send(&oru_r01(CONTROL_ID)).await?;

    let audits = audits(&running).await?;
    assert!(!audits.is_empty(), "the message committed a composition");
    for audit in &audits {
        assert_eq!(
            Some("LAB"),
            audit["originating_system_audit"]["system_id"].as_str(),
            "MSH-3 is the originating system: {audit}"
        );
        assert_eq!(
            Some("ferrobridge.test"),
            audit["feeder_system_audit"]["system_id"].as_str(),
            "the bridge is the feeder: {audit}"
        );
        // ORC-2 and OBR-2 carry the same placer number, ORC-3 and OBR-3 the
        // same filler number, each recorded once (HL7 table 0203 PLAC, FILL).
        assert_eq!(
            vec![
                (String::from("PLAC"), String::from("PLC-1"), None),
                (String::from("FILL"), String::from("FIL-1"), None),
            ],
            items(audit, "originating_system_item_ids"),
            "{audit}"
        );
        let feeder = items(audit, "feeder_system_item_ids");
        assert_eq!(
            Some(&(
                String::from("ORU^R01"),
                String::from(CONTROL_ID),
                Some(String::from("LAB"))
            )),
            feeder.first(),
            "MSH-10 is the feeder's item id: {audit}"
        );
    }
    running.stop().await
}

#[tokio::test]
async fn a_message_without_order_numbers_records_none() -> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    let message = String::from_utf8(oru_r01("MSG-V2-NO-ORDERS"))?
        .replace("ORC|RE|PLC-1|FIL-1", "ORC|RE")
        .replace("OBR|1|PLC-1|FIL-1|", "OBR|1|||");
    running.send(message.as_bytes()).await?;

    for audit in audits(&running).await? {
        assert_eq!(
            Vec::<(String, String, Option<String>)>::new(),
            items(&audit, "originating_system_item_ids"),
            "no order number is invented: {audit}"
        );
        assert_eq!(
            Some("MSG-V2-NO-ORDERS"),
            audit["feeder_system_item_ids"][0]["id"].as_str(),
            "{audit}"
        );
    }
    running.stop().await
}
