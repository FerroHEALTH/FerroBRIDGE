// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PID-3 assigning authority settled through the identifier registry of
//! the terminology server: a registered authority names the system the
//! identity source is asked in, and an unregistered one refuses the message
//! before anything is committed.

use std::error::Error;
use std::time::Duration;

use super::{CONTROL_ID, REGISTERED, Running, oru_r01, segments};

/// The ORU^R01 fixture with PID-3 assigned by `authority`.
fn assigned_by(authority: &str) -> Vec<u8> {
    let message = String::from_utf8(oru_r01(CONTROL_ID)).expect("the fixture is UTF-8");
    message
        .replace("^^^&2.999.1&ISO^MR", &format!("^^^{authority}^MR"))
        .into_bytes()
}

#[tokio::test]
async fn a_registered_assigning_authority_names_the_system_the_identity_source_is_asked_in()
-> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    let ack = running.send(&assigned_by(REGISTERED)).await?;
    assert_eq!(
        vec![format!("MSA|AA|{CONTROL_ID}")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    let asked = running.requests("POST", crate::support::PIX_PATH).await;
    let asked = String::from_utf8(asked.first().cloned().unwrap_or_default())?;
    assert!(
        asked.contains(crate::support::PATIENT_SYSTEM) && asked.contains("synthetic-subject-0001"),
        "the identity source is asked in the registered system: {asked}"
    );
    assert_eq!(1, running.contributions().await, "one commit");
    running.stop().await
}

#[tokio::test]
async fn an_unregistered_assigning_authority_is_answered_ae_at_pid_3_naming_it()
-> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    let ack = running.send(&assigned_by("SOUTHHOSP")).await?;
    assert_eq!(
        vec![format!("MSA|AE|{CONTROL_ID}")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    let errors = segments(&ack, "ERR");
    assert_eq!(1, errors.len(), "{ack}");
    let error = errors.first().copied().unwrap_or_default();
    assert!(
        error.contains("PID^1^3") && error.contains("SOUTHHOSP") && error.contains("NamingSystem"),
        "the ERR names PID-3 and the missing registry entry: {error}"
    );
    assert!(
        running
            .requests("POST", crate::support::PIX_PATH)
            .await
            .is_empty(),
        "the identity source is never asked"
    );
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}
