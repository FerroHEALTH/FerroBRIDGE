// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XTN of a PID as R4 `ContactPoint`s through the shipped supplements: a
//! cell phone as a `phone` in `mobile` use (#1762) and a number with no
//! equipment type as a system with the data-absent-reason (#1763), each
//! `Patient` validated against the R4 core definitions. The table maps are
//! answered by a stubbed terminology server loaded with the guide's maps and
//! the crate's supplements.

use std::path::Path;

use ferrobridge_hl7v2::map::inverse;
use ferrobridge_hl7v2::map::single;
use ferrobridge_hl7v2::map::translate::Translator;
use ferrobridge_hl7v2::parse::Delimiters;
use ferrobridge_hl7v2::parse::structure::segment_of;
use ferrobridge_hl7v2::write::Writer;
use ferrobridge_validator::release::R4;
use ferrobridge_validator::set::PackageSet;
use ferrobridge_validator::validate::Validator;
use fhir_types::codec::Value;

use crate::support::Tables;
use crate::{fixtures, support};

const DELIMITERS: Delimiters = Delimiters {
    field: '|',
    component: '^',
    repetition: '~',
    escape: '\\',
    subcomponent: '&',
    truncation: None,
};

/// The guide's base and FerroBRIDGE's, for the maps the stub answers.
const SUPPLEMENT: &str = "https://ferrobridge.eu/fhir/v2mappings/ConceptMap/";

/// The answers a server loaded with the guide's table maps and the crate's
/// supplements gives for the codes these PIDs carry, read from those maps.
fn tables() -> Tables {
    let use_system = "http://hl7.org/fhir/contact-point-use";
    let system_system = "http://hl7.org/fhir/contact-point-system";
    let v2_0201 = "http://terminology.hl7.org/CodeSystem/v2-0201";
    let v2_0202 = "http://terminology.hl7.org/CodeSystem/v2-0202";
    Tables::default()
        .answer(
            "table-hl70001-to-administrative-gender",
            "F",
            "http://hl7.org/fhir/administrative-gender",
            "female",
            "Female",
        )
        .answer(
            "table-hl70201-to-contact-point-use",
            "PRN",
            use_system,
            "home",
            "Home",
        )
        .answer(
            "table-hl70201-to-contact-point-use",
            "WPN",
            use_system,
            "work",
            "Work",
        )
        .answer(
            "table-hl70201-to-contact-point-use",
            "PRS",
            use_system,
            "mobile",
            "Mobile",
        )
        .answer(
            "table-hl70201-to-contact-point-use",
            "home",
            v2_0201,
            "PRN",
            "Primary Residence Number",
        )
        .answer(
            "table-hl70201-to-contact-point-use",
            "mobile",
            v2_0201,
            "PRS",
            "Personal",
        )
        .answer(
            "table-hl70202-to-contact-point-system",
            "CP",
            system_system,
            "phone",
            "Phone",
        )
        .answer(
            "table-hl70202-to-contact-point-system",
            "PH",
            system_system,
            "phone",
            "Phone",
        )
        .answer_at(
            &format!("{SUPPLEMENT}table-hl70202-to-contact-point-use"),
            "CP",
            use_system,
            "mobile",
            "Mobile",
        )
        .answer_at(
            &format!("{SUPPLEMENT}table-contact-point-system-to-hl70202"),
            "phone",
            v2_0202,
            "PH",
            "Telephone",
        )
        .answer_at(
            &format!("{SUPPLEMENT}table-administrative-gender-to-hl70001"),
            "female",
            "http://terminology.hl7.org/CodeSystem/v2-0001",
            "F",
            "Female",
        )
}

/// The `Patient` the PID of an `ADT^A01` maps to.
async fn patient(pid: &[u8]) -> Value {
    let parsed = support::parsed(&fixtures::message(&[
        b"MSH|^~\\&|ADT|EAST|PIX|NORTH|20261025100000||ADT^A01^ADT_A01|M-1|P|2.5",
        b"EVN||20261025100000",
        pid,
        b"PV1|1|O",
    ]));
    let corpus = support::shipped();
    let (_server, client) = tables().serve().await;
    let mapped = single::segment(
        &parsed,
        2,
        "Patient",
        &corpus,
        Some(Translator::Server(&client)),
    )
    .await
    .expect("the PID maps");
    mapped.resource().expect("a Patient").resource.clone()
}

/// The `telecom` of `patient`.
fn telecom(patient: &Value) -> &[Value] {
    patient
        .get("telecom")
        .and_then(Value::as_array)
        .unwrap_or_default()
}

/// Whether `patient` holds every rule of the R4 core definitions the
/// validator evaluates, the required bindings and `cpt-2` among them, with
/// the issues when it does not.
fn validity(patient: &Value) -> (bool, String) {
    let core = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/fhir-codegen/vendor/hl7.fhir.r4.core");
    let set = PackageSet::<R4>::load([core]).expect("the R4 core package loads");
    let report = Validator::new(&set)
        .validate(patient)
        .expect("the Patient validates");
    (report.is_valid(), format!("{:#?}", report.issues()))
}

/// Asserts that `patient` is valid R4.
fn assert_valid(patient: &Value) {
    let (valid, issues) = validity(patient);
    assert!(valid, "{issues}");
}

#[test]
fn the_validator_refuses_what_the_guide_alone_wrote() {
    for telecom in [
        r#"{"system": "mobile", "use": "mobile", "value": "+31600000000"}"#,
        r#"{"use": "home", "value": "+31200000003"}"#,
    ] {
        let patient = Value::from_serde_json(
            serde_json::from_str(&format!(
                r#"{{"resourceType": "Patient", "telecom": [{telecom}]}}"#
            ))
            .expect("a JSON literal"),
        );
        let (valid, issues) = validity(&patient);
        assert!(!valid, "{telecom} should be refused: {issues}");
    }
}

#[tokio::test]
async fn a_cell_phone_is_a_phone_in_mobile_use() {
    let patient = patient(
        b"PID|1||PAT-1^^^EAST||Doe^Ann||19800229|F|||||^^CP^^^^^^^^^+31600000000~^PRN^CP^^^^^^^^^+31600000001|^WPN^PH^^^^^^^^^+31207654321",
    )
    .await;
    let points: Vec<String> = telecom(&patient)
        .iter()
        .map(|point| serde_json::to_string(point).expect("a ContactPoint serializes"))
        .collect();
    assert_eq!(
        points,
        [
            r#"{"system":"phone","use":"mobile","value":"+31600000000"}"#,
            r#"{"system":"phone","use":"home","value":"+31600000001"}"#,
            r#"{"system":"phone","use":"work","value":"+31207654321"}"#,
        ]
    );
    assert_valid(&patient);
}

#[tokio::test]
async fn a_number_with_no_equipment_type_keeps_a_system_that_is_absent() {
    let patient =
        patient(b"PID|1||PAT-1^^^EAST||Doe^Ann||19800229|F|||||^PRN^^^^^^^^^^+31200000003").await;
    let points: Vec<String> = telecom(&patient)
        .iter()
        .map(|point| serde_json::to_string(point).expect("a ContactPoint serializes"))
        .collect();
    assert_eq!(
        points,
        [
            r#"{"_system":{"extension":[{"url":"http://hl7.org/fhir/StructureDefinition/data-absent-reason","valueCode":"unknown"}]},"use":"home","value":"+31200000003"}"#
        ]
    );
    assert_valid(&patient);
}

#[tokio::test]
async fn both_write_back_through_the_inverse() {
    let patient = patient(
        b"PID|1||PAT-1^^^EAST||Doe^Ann||19800229|F|||||^PRS^CP^^^^^^^^^+31600000000~^PRN^^^^^^^^^^+31200000003",
    )
    .await;
    let corpus = support::shipped();
    let (_server, client) = tables().serve().await;
    let structure = hl7v2_types::legacy::find("ADT_A01", "2.5").expect("the 2.5 ADT_A01");
    let definition = segment_of(structure, "PID").expect("ADT_A01 places a PID");
    let inverted = inverse::segment(
        &patient,
        definition,
        &corpus,
        Some(Translator::Server(&client)),
    )
    .await
    .expect("the Patient writes back");
    assert_eq!(
        Writer::new(DELIMITERS).segment(inverted.segment()),
        "PID|||PAT-1||Doe^Ann||19800229|F|||||^PRS^PH^^^^^^^^^+31600000000~^PRN^^^^^^^^^^+31200000003",
        "{:?}",
        inverted.outcomes()
    );
    assert!(inverted.outcomes().is_empty(), "{:?}", inverted.outcomes());
}
