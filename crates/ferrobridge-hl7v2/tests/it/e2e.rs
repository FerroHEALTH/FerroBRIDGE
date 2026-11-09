// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The map against the reference terminology server in a container, loaded
//! with HL7 Terminology, the v2-to-FHIR guide and FerroBRIDGE's value
//! package, as an operator loads it: every table value of a PID translated
//! by `ConceptMap/$translate` there, forward and back. The test runs only
//! when `FERROBRIDGE_E2E=1` admits the container harness.

use std::error::Error;

use ferrobridge_hl7v2::map::maps::Maps;
use ferrobridge_hl7v2::map::{Outcome, inverse, single};
use ferrobridge_hl7v2::parse::Delimiters;
use ferrobridge_hl7v2::parse::structure::segment_of;
use ferrobridge_hl7v2::write::Writer;
use ferrobridge_term::client::Client;
use ferrobridge_term::config::{Config, WireVersion};
use ferrobridge_testkit::containers;

use crate::{fixtures, support};

const DELIMITERS: Delimiters = Delimiters {
    field: '|',
    component: '^',
    repetition: '~',
    escape: '\\',
    subcomponent: '&',
    truncation: None,
};

#[tokio::test]
async fn a_pid_maps_and_writes_back_through_the_reference_terminology_server()
-> Result<(), Box<dyn Error>> {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = containers::v2_terminology().await?;
    let client = Client::new(Config::new(server.base_url("r4").parse()?, WireVersion::R4))?;
    let pid: &[u8] = b"PID|1||PAT-1^^^EAST||Doe^Ann^^^^^L||19800229|F|||||^PRS^CP^^^^^^^^^+31600000000~^WPN^PH^^^^^^^^^+31207654321";
    let parsed = support::parsed(&fixtures::message(&[
        b"MSH|^~\\&|ADT|EAST|PIX|NORTH|20261025100000||ADT^A01^ADT_A01|M-1|P|2.5",
        b"EVN||20261025100000",
        pid,
        b"PV1|1|O",
    ]));
    let maps = Maps::corrected();
    let forward = single::segment(&parsed, 2, "Patient", maps, Some(&client)).await?;
    assert!(
        !forward.outcomes().iter().any(|outcome| matches!(
            outcome,
            Outcome::Untranslated { .. } | Outcome::NoTerminology { .. }
        )),
        "{:?}",
        forward.outcomes()
    );
    let patient = &forward.resource().ok_or("a Patient")?.resource;
    assert_eq!(
        Some("female"),
        patient.get("gender").and_then(|gender| gender.as_str())
    );
    let points: Vec<String> = patient
        .get("telecom")
        .and_then(|telecom| telecom.as_array())
        .ok_or("a telecom")?
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<_, _>>()?;
    assert_eq!(
        points,
        [
            r#"{"system":"phone","use":"mobile","value":"+31600000000"}"#,
            r#"{"system":"phone","use":"work","value":"+31207654321"}"#,
        ]
    );
    let structure = hl7v2_types::legacy::find("ADT_A01", "2.5").ok_or("the 2.5 ADT_A01")?;
    let definition = segment_of(structure, "PID").ok_or("ADT_A01 places a PID")?;
    let inverted = inverse::segment(patient, definition, maps, Some(&client)).await?;
    assert_eq!(
        Writer::new(DELIMITERS).segment(inverted.segment()),
        "PID|||PAT-1||Doe^Ann^^^^^L||19800229|F|||||^PRS^PH^^^^^^^^^+31600000000|^WPN^PH^^^^^^^^^+31207654321",
        "{:?}",
        inverted.outcomes()
    );
    Ok(())
}
