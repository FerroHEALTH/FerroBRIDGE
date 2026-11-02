// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The import record of each message the HL7 v2 face commits: the system
//! on its own behalf, the DICOM "Import" event ITI TF-2 Table
//! 3.20.4.1.1.1-1 audits "PHI-import" as, and no person; a message whose
//! record has no place is answered `AR` and never committed (HL7 v2.5.1
//! §2.9.2.2).

use std::error::Error;
use std::time::Duration;

use ehds_logging::map::CategoryMap;
use wiremock::ResponseTemplate;

use crate::facade::ehr_body;
use crate::hl7v2::Running;
use crate::hl7v2::face_settings;
use crate::hl7v2::oru_r01;

/// The CDR the record names as the facade's origin, which an import record
/// does not carry.
const CDR: &str = "http://cdr.invalid/";

/// Starts the face recording through a spool of at most `max_bytes`, and
/// returns it with the repository and the spool's directory.
async fn recorded(
    max_bytes: u64,
) -> Result<(Running, wiremock::MockServer, tempfile::TempDir), Box<dyn Error>> {
    let arr = super::repository().await;
    let spool = tempfile::tempdir()?;
    let log = super::access_log(
        &arr,
        spool.path(),
        &CDR.parse()?,
        CategoryMap::default(),
        max_bytes,
    )?;
    let running = Running::start_recorded(
        face_settings(true),
        ResponseTemplate::new(200).set_body_string(ehr_body()),
        Some(log),
    )
    .await?;
    Ok((running, arr, spool))
}

#[tokio::test]
async fn a_committed_message_is_recorded_as_a_system_import() -> Result<(), Box<dyn Error>> {
    let (running, arr, _spool) = recorded(64 << 20).await?;
    let ack = running.send(&oru_r01("MSG-V2-AUDIT-1")).await?;
    assert!(ack.contains("MSA|AA|MSG-V2-AUDIT-1"), "{ack}");
    let records = super::records(&arr, 1).await?;
    assert_eq!(1, records.len(), "one commit, one record");
    let record = &records[0];
    assert_eq!(Some("110107"), record["type"]["code"].as_str(), "{record}");
    assert_eq!(Some("C"), record["action"].as_str(), "{record}");
    let agents = record["agent"]
        .as_array()
        .ok_or("a record names its agents")?;
    assert_eq!(
        vec![Some("NORTHLAB"), Some("ferrobridge.test")],
        agents
            .iter()
            .map(|agent| agent["who"]["display"].as_str())
            .collect::<Vec<_>>(),
        "the sending facility and the bridge, and no person: {record}"
    );
    assert!(
        agents.iter().all(|agent| agent["requestor"] == false),
        "the system imports on its own behalf: {record}"
    );
    assert!(
        record
            .to_string()
            .contains("3c9d6a70-0000-4000-8000-00000000001e::ferrobridge.test::1"),
        "the committed version is the data entity: {record}"
    );
    running.stop().await
}

#[tokio::test]
async fn a_message_whose_record_has_no_place_is_answered_ar_and_not_committed()
-> Result<(), Box<dyn Error>> {
    let (running, arr, _spool) = recorded(1024).await?;
    let ack = running.send(&oru_r01("MSG-V2-AUDIT-2")).await?;
    assert!(ack.contains("MSA|AR|MSG-V2-AUDIT-2"), "{ack}");
    assert_eq!(0, running.contributions().await, "nothing is committed");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        arr.received_requests().await.unwrap_or_default().is_empty(),
        "no commit, no record"
    );
    running.stop().await
}
