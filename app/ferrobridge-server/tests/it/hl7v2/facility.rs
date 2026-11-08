// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The health care facility a message's compositions name (RM EHR IM,
//! `EVENT_CONTEXT.health_care_facility`, "the most specific workgroup or
//! delivery unit"), read from PV1-3, PV1-39, ORC-21 and MSH-4 in that order,
//! the refusal of one outside the senders' list, and the performing
//! organisation (OBX-23) as the originating system's provider, and the
//! facility of a composition whose template prohibits the context as the
//! originating system's location.

use std::error::Error;
use std::time::Duration;

use ferrobridge_server::config::hl7v2::Sender;
use openehr_its::opt14::types::{CAttribute, CSingleAttribute, Intervalofinteger};
use wiremock::ResponseTemplate;

use super::{CONTROL_ID, Running, face_settings, oru_r01, segments};
use crate::facade::EHR_ID;
use crate::facade::ehr_body;

/// Returns the ORU^R01 fixture with `edit` applied to its text.
fn edited(control_id: &str, edit: impl Fn(String) -> String) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(edit(String::from_utf8(oru_r01(control_id))?).into_bytes())
}

/// Adds a visit segment whose assigned location names the facility
/// `NORTHWARD`.
fn with_visit(text: &str) -> String {
    text.replace("\rORC|", "\rPV1|1|O|ROOM-1^^^NORTHWARD\rORC|")
}

/// Starts the face with the facilities the cases name listed beside the
/// sending facility, since every facility a composition names is a sender's.
async fn listing_facilities() -> Result<Running, Box<dyn Error>> {
    let mut lane = face_settings(true);
    lane.senders.extend([
        Sender::Namespace(String::from("NORTHWARD")),
        Sender::Namespace(String::from("ORDERDESK")),
    ]);
    let lookup = ResponseTemplate::new(200).set_body_string(ehr_body());
    Running::start_with(lane, lookup).await
}

/// Returns every composition of the one contribution the CDR received.
async fn compositions(running: &Running) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let committed = running
        .requests("POST", &format!("/ehr/{EHR_ID}/contribution"))
        .await;
    let body: serde_json::Value =
        serde_json::from_slice(committed.first().ok_or("one contribution")?)?;
    Ok(body["versions"]
        .as_array()
        .ok_or("the versions")?
        .iter()
        .map(|version| version["data"].clone())
        .collect())
}

/// Returns the facility name of every composition.
async fn facilities(running: &Running) -> Result<Vec<Option<String>>, Box<dyn Error>> {
    Ok(compositions(running)
        .await?
        .iter()
        .map(|composition| {
            composition
                .pointer("/context/health_care_facility/name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .collect())
}

#[tokio::test]
async fn the_visit_location_s_facility_is_named_before_any_other() -> Result<(), Box<dyn Error>> {
    let running = listing_facilities().await?;
    running
        .send(&edited("MSG-FAC-PV1", |text| {
            with_visit(&text).replace(
                "ORC|RE|PLC-1|FIL-1",
                "ORC|RE|PLC-1|FIL-1||||||||||||||||||Synthetic order desk^^^^^^^^^ORDERDESK",
            )
        })?)
        .await?;
    let named = facilities(&running).await?;
    assert!(!named.is_empty(), "one composition");
    assert!(
        named
            .iter()
            .all(|name| name.as_deref() == Some("NORTHWARD")),
        "PV1-3.4 wins over ORC-21 and MSH-4: {named:?}"
    );
    running.stop().await
}

#[tokio::test]
async fn the_servicing_facility_is_named_when_the_location_names_none() -> Result<(), Box<dyn Error>>
{
    let running = listing_facilities().await?;
    let servicing = format!("PV1|1|O|ROOM-1{}NORTHWARD", "|".repeat(36));
    running
        .send(&edited("MSG-FAC-PV139", |text| {
            text.replace("\rORC|", &format!("\r{servicing}\rORC|"))
                .replace(
                    "ORC|RE|PLC-1|FIL-1",
                    "ORC|RE|PLC-1|FIL-1||||||||||||||||||Synthetic order desk^^^^^^^^^ORDERDESK",
                )
        })?)
        .await?;
    let named = facilities(&running).await?;
    assert!(!named.is_empty(), "one composition");
    assert!(
        named
            .iter()
            .all(|name| name.as_deref() == Some("NORTHWARD")),
        "PV1-39 wins over ORC-21 and MSH-4: {named:?}"
    );
    running.stop().await
}

#[tokio::test]
async fn the_ordering_facility_is_named_when_no_visit_names_one() -> Result<(), Box<dyn Error>> {
    let running = listing_facilities().await?;
    running
        .send(&edited("MSG-FAC-ORC", |text| {
            text.replace(
                "ORC|RE|PLC-1|FIL-1",
                "ORC|RE|PLC-1|FIL-1||||||||||||||||||Synthetic order desk^^^^^^^^^ORDERDESK",
            )
        })?)
        .await?;
    let named = facilities(&running).await?;
    assert!(
        named
            .iter()
            .all(|name| name.as_deref() == Some("Synthetic order desk")),
        "ORC-21 wins over MSH-4: {named:?}"
    );
    running.stop().await
}

#[tokio::test]
async fn the_sending_facility_is_named_last() -> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    running.send(&oru_r01(CONTROL_ID)).await?;
    let named = facilities(&running).await?;
    assert!(
        named.iter().all(|name| name.as_deref() == Some("NORTHLAB")),
        "MSH-4 names the facility when nothing else does: {named:?}"
    );
    running.stop().await
}

#[tokio::test]
async fn a_facility_outside_the_senders_list_is_answered_ae_naming_it() -> Result<(), Box<dyn Error>>
{
    let mut lane = face_settings(true);
    lane.senders = vec![Sender::Namespace(String::from("NORTHLAB"))];
    let lookup = ResponseTemplate::new(200).set_body_string(ehr_body());
    let running = Running::start_with(lane, lookup).await?;
    let ack = running
        .send(&edited("MSG-FAC-OUT", |text| with_visit(&text))?)
        .await?;
    assert_eq!(
        vec![String::from("MSA|AE|MSG-FAC-OUT")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    let errors = segments(&ack, "ERR");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("PV1^1^3") && error.contains("NORTHWARD")),
        "the ERR names the facility and its field: {ack}"
    );
    assert_eq!(0, running.contributions().await, "nothing is committed");
    running.stop().await
}

#[tokio::test]
async fn the_performing_organisation_is_the_provider_and_never_the_facility()
-> Result<(), Box<dyn Error>> {
    let running = Running::start(true, Duration::ZERO).await?;
    running
        .send(&edited("MSG-FAC-OBX23", |text| {
            text.replace(
                "|||20260925130000+0200",
                "|||20260925130000+0200|||||||||Synthetic performing lab^^^^^^^^^PERFLAB",
            )
        })?)
        .await?;
    for composition in compositions(&running).await? {
        assert_eq!(
            Some("Synthetic performing lab"),
            composition
                .pointer("/feeder_audit/originating_system_audit/provider/name")
                .and_then(serde_json::Value::as_str),
            "OBX-23 is the provider: {composition}"
        );
        assert_eq!(
            Some("NORTHLAB"),
            composition
                .pointer("/context/health_care_facility/name")
                .and_then(serde_json::Value::as_str),
            "OBX-23 is never the facility: {composition}"
        );
        let composer = composition["composer"].to_string();
        assert!(
            !composer.contains("performing"),
            "OBX-23 is never the composer: {composer}"
        );
    }
    running.stop().await
}

/// Returns the testkit's diagnosis template with its `COMPOSITION.context`
/// prohibited (existence `0..0`), which the RM allows since the attribute is
/// `0..1` (RM EHR IM, `COMPOSITION.context`).
fn without_context() -> Result<String, Box<dyn Error>> {
    let mut opt = openehr_its::opt14::from_xml(ferrobridge_testkit::fixtures::DIAGNOSE_OPT)?;
    let prohibited = Intervalofinteger {
        lower_included: Some(true),
        upper_included: Some(true),
        lower_unbounded: false,
        upper_unbounded: false,
        lower: Some(0),
        upper: Some(0),
    };
    opt.definition
        .attributes
        .retain(|attribute| match attribute {
            CAttribute::CSingleAttribute(single) => single.rm_attribute_name != "context",
            CAttribute::CMultipleAttribute(multiple) => multiple.rm_attribute_name != "context",
        });
    opt.definition
        .attributes
        .push(CAttribute::CSingleAttribute(CSingleAttribute {
            rm_attribute_name: String::from("context"),
            existence: prohibited,
            children: Vec::new(),
        }));
    Ok(openehr_its::opt14::to_xml(&opt)?)
}

#[tokio::test]
async fn a_composition_without_context_names_the_facility_as_the_originating_location()
-> Result<(), Box<dyn Error>> {
    let lookup = ResponseTemplate::new(200).set_body_string(ehr_body());
    let running =
        Running::start_serving(face_settings(true), lookup, None, &without_context()?).await?;
    let ack = running.send(&oru_r01("MSG-FAC-NOCTX")).await?;
    assert_eq!(
        vec![String::from("MSA|AA|MSG-FAC-NOCTX")],
        segments(&ack, "MSA"),
        "{ack}"
    );
    let compositions = compositions(&running).await?;
    assert!(!compositions.is_empty(), "one composition");
    for composition in &compositions {
        assert!(
            composition.get("context").is_none(),
            "a template that prohibits the context gets none: {composition}"
        );
        assert_eq!(
            Some("NORTHLAB"),
            composition
                .pointer("/feeder_audit/originating_system_audit/location/name")
                .and_then(serde_json::Value::as_str),
            "RM Common IM, FEEDER_AUDIT_DETAILS.location holds the facility: {composition}"
        );
    }
    running.stop().await
}
