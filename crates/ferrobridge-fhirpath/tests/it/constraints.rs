// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The invariants of the vendored FHIR packages
//! (`ElementDefinition.constraint.expression`, FHIR R4 conformance rules
//! <https://hl7.org/fhir/R4/conformance-rules.html#constraints>): every one
//! parses, every R4 core one passes the static check against the element it
//! constrains, and a sample evaluates on synthetic resources as the rule
//! reads.
//!
//! An invariant the packages publish with a defect is an expected refusal
//! below, each with the evidence from the package itself.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::LazyLock;

use ferrobridge_fhirpath::eval::{Environment, Focus};
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::model::path_type;
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::value::Collection;
use fhir_types::codec::Value;
use serde::Deserialize;

use super::support::{json, root, truth};

/// One invariant: its key, the element path it constrains, its expression.
type Constraint = (String, String, String);

/// The part of a `StructureDefinition` the invariants live in.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Definition {
    snapshot: Snapshot,
}

/// `StructureDefinition.snapshot`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Snapshot {
    element: Vec<Element>,
}

/// `ElementDefinition`, its path and constraints.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Element {
    path: String,
    constraint: Vec<Invariant>,
}

/// `ElementDefinition.constraint`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Invariant {
    key: String,
    expression: Option<String>,
}

/// Every distinct invariant of the snapshots of a vendored package.
fn constraints(package: &str) -> BTreeSet<Constraint> {
    let directory = root()
        .join("tools/fhir-codegen/vendor")
        .join(package)
        .join("package");
    let mut found = BTreeSet::new();
    for entry in std::fs::read_dir(&directory).expect("the package should be vendored") {
        let path = entry.expect("the entry should read").path();
        let is_json = path
            .extension()
            .is_some_and(|extension| extension == "json");
        let is_definition = is_json
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("StructureDefinition-"));
        if is_definition {
            collect(&path, &mut found);
        }
    }
    found
}

/// The invariants of one structure definition file.
fn collect(path: &Path, found: &mut BTreeSet<Constraint>) {
    let text = std::fs::read_to_string(path).expect("the definition should read");
    if !text.contains("\"constraint\"") {
        return;
    }
    let definition: Definition =
        serde_json::from_str(&text).expect("the definition should be JSON");
    for element in definition.snapshot.element {
        for invariant in element.constraint {
            if let Some(expression) = invariant.expression {
                found.insert((invariant.key, element.path.clone(), expression));
            }
        }
    }
}

/// The R4 core invariants, read once.
static R4_CORE: LazyLock<BTreeSet<Constraint>> = LazyLock::new(|| constraints("hl7.fhir.r4.core"));

/// The invariants the packages publish with a defect, by package, key and
/// the element they constrain, with the evidence from the package.
const DEFECTIVE: [(&str, &str, &str, &str); 4] = [
    (
        "hl7.fhir.r4.core",
        "cid-0",
        "ChargeItemDefinition",
        "R4 ChargeItemDefinition defines no `name` element (its snapshot has no ChargeItemDefinition.name), yet cid-0 reads name",
    ),
    (
        "hl7.fhir.r4.core",
        "inv-1",
        "Extension",
        "the allergyintolerance-substanceExposureRisk extension names its parts as `extension` slices, so `substanceExposureRisk` is no element of Extension",
    ),
    (
        "hl7.fhir.r5.core",
        "eld-11",
        "ElementDefinition",
        "eld-11 writes the string \":\" in double quotes, and the grammar's STRING is single-quoted",
    ),
    (
        "hl7.fhir.r6.core",
        "csd-6",
        "CodeSystem.concept",
        "csd-6 writes `.%resource`, and the grammar's invocation after `.` admits no external constant",
    ),
];

/// Whether an invariant of `package` is a recorded defect.
fn defective(package: &str, key: &str, path: &str) -> bool {
    DEFECTIVE
        .iter()
        .any(|(within, defect, on, _)| *within == package && *defect == key && *on == path)
}

/// The invariants of `package` that do not parse, with the recorded defects
/// that do.
fn unparsed(package: &str, found: &BTreeSet<Constraint>) -> (Vec<String>, Vec<String>) {
    let mut refused = Vec::new();
    let mut parsed_defects = Vec::new();
    for (key, path, expression) in found {
        match (Expression::parse(expression), defective(package, key, path)) {
            (Err(error), false) => refused.push(format!("{key} on {path}: {expression}: {error}")),
            (Ok(_), true) => parsed_defects.push(format!("{key} on {path}")),
            _ => {}
        }
    }
    (refused, parsed_defects)
}

/// Asserts every invariant of `package` parses, the recorded defects
/// excepted, and those are refused.
fn assert_parses(package: &str, found: &BTreeSet<Constraint>) {
    let (refused, parsed_defects) = unparsed(package, found);
    assert!(
        refused.is_empty(),
        "{package}: invariants that do not parse: {refused:#?}"
    );
    assert!(
        parsed_defects.is_empty(),
        "{package}: recorded defects that parse: {parsed_defects:#?}"
    );
}

#[test]
fn every_r4_core_constraint_parses() {
    let pairs: BTreeSet<(&str, &str)> = R4_CORE
        .iter()
        .map(|(key, _, expression)| (key.as_str(), expression.as_str()))
        .collect();
    assert_eq!(
        pairs.len(),
        248,
        "the R4 core package holds 248 distinct invariants"
    );
    let (refused, _) = unparsed("hl7.fhir.r4.core", &R4_CORE);
    assert!(
        refused.is_empty(),
        "invariants that do not parse: {refused:#?}"
    );
}

#[test]
fn every_r4_core_constraint_checks_against_its_element() {
    let mut checked = 0;
    let mut refused = Vec::new();
    let mut expected = Vec::new();
    for (key, path, expression) in R4_CORE.iter() {
        let Some(context) = path_type(&R4, path) else {
            continue;
        };
        let parsed = Expression::parse(expression).expect("the invariant should parse");
        checked += 1;
        match (
            parsed.check(&R4, context),
            defective("hl7.fhir.r4.core", key, path),
        ) {
            (Err(_), true) => expected.push(key.clone()),
            (Err(error), false) => refused.push(format!("{key} on {path}: {error}")),
            (Ok(()), true) => refused.push(format!(
                "{key} on {path}: recorded as defective, yet it checks"
            )),
            (Ok(()), false) => {}
        }
    }
    assert!(
        checked > 1000,
        "only {checked} invariants had a context type"
    );
    assert!(
        refused.is_empty(),
        "invariants the check refuses: {refused:#?}"
    );
    assert_eq!(
        expected,
        ["cid-0", "inv-1"],
        "the recorded defects are refused"
    );
}

#[test]
fn every_r4b_core_constraint_parses() {
    assert_parses("hl7.fhir.r4b.core", &constraints("hl7.fhir.r4b.core"));
}

#[test]
fn every_r5_core_constraint_parses() {
    assert_parses("hl7.fhir.r5.core", &constraints("hl7.fhir.r5.core"));
}

#[test]
fn every_r6_core_constraint_parses() {
    assert_parses("hl7.fhir.r6.core", &constraints("hl7.fhir.r6.core"));
}

#[test]
fn every_constraint_of_the_v2_mappings_and_terminology_packages_parses() {
    for package in ["hl7.fhir.uv.v2mappings", "hl7.terminology"] {
        assert_parses(package, &constraints(package));
    }
}

/// Evaluates an invariant of the R4 core package by key on `focus`.
fn invariant<'a>(key: &str, focus: Focus<'a>) -> Collection<'a> {
    let (_, _, expression) = R4_CORE
        .iter()
        .find(|(candidate, ..)| candidate == key)
        .expect("the invariant should exist");
    Expression::parse(expression)
        .expect("the invariant should parse")
        .evaluate(&Environment::new(&R4), focus)
        .expect("the invariant should evaluate")
}

#[test]
fn ait_1_needs_a_clinical_status_unless_entered_in_error() {
    let with_status = json(
        r#"{"resourceType": "AllergyIntolerance", "patient": {"reference": "Patient/1"},
            "clinicalStatus": {"coding": [{"system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical", "code": "active"}]}}"#,
    );
    let without =
        json(r#"{"resourceType": "AllergyIntolerance", "patient": {"reference": "Patient/1"}}"#);
    let in_error = json(
        r#"{"resourceType": "AllergyIntolerance", "patient": {"reference": "Patient/1"},
            "verificationStatus": {"coding": [{"system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-verification", "code": "entered-in-error"}]}}"#,
    );
    assert_eq!(
        truth(&invariant("ait-1", Focus::resource(&with_status))),
        Some(true)
    );
    assert_eq!(
        truth(&invariant("ait-1", Focus::resource(&without))),
        Some(false)
    );
    assert_eq!(
        truth(&invariant("ait-1", Focus::resource(&in_error))),
        Some(true)
    );
}

#[test]
fn ele_1_needs_a_value_or_a_child_other_than_id() {
    let patient = json(r#"{"resourceType": "Patient"}"#);
    let name = json(r#"{"family": "Doe"}"#);
    let only_id = json(r#"{"id": "n1"}"#);
    let value = Value::String(String::from("Doe"));
    let extension =
        json(r#"{"extension": [{"url": "http://example.org/x", "valueBoolean": true}]}"#);
    let id = json(r#"{"id": "f1"}"#);
    let element = |value, type_name| invariant("ele-1", Focus::element(value, type_name, &patient));
    assert_eq!(truth(&element(&name, "HumanName")), Some(true));
    assert_eq!(truth(&element(&only_id, "HumanName")), Some(false));
    assert_eq!(truth(&element(&value, "HumanName.family")), Some(true));
    let primitive = |element| {
        invariant(
            "ele-1",
            Focus::primitive(None, Some(element), "HumanName.family", &patient),
        )
    };
    assert_eq!(truth(&primitive(&extension)), Some(true));
    assert_eq!(truth(&primitive(&id)), Some(false));
}

#[test]
fn ext_1_needs_extensions_or_a_value_but_not_both() {
    let patient = json(r#"{"resourceType": "Patient"}"#);
    let valued = json(r#"{"url": "http://example.org/x", "valueString": "a"}"#);
    let both = json(
        r#"{"url": "http://example.org/x", "valueString": "a", "extension": [{"url": "y", "valueBoolean": true}]}"#,
    );
    let neither = json(r#"{"url": "http://example.org/x"}"#);
    let extension = |value| invariant("ext-1", Focus::element(value, "Extension", &patient));
    assert_eq!(truth(&extension(&valued)), Some(true));
    assert_eq!(truth(&extension(&both)), Some(false));
    assert_eq!(truth(&extension(&neither)), Some(false));
}

#[test]
fn bdl_7_refuses_a_repeated_full_url_and_version() {
    let entry = |id: &str| {
        format!(r#"{{"fullUrl": "urn:uuid:{id}", "resource": {{"resourceType": "Patient"}}}}"#)
    };
    let distinct = json(&format!(
        r#"{{"resourceType": "Bundle", "type": "collection", "entry": [{}, {}]}}"#,
        entry("1"),
        entry("2")
    ));
    let repeated = json(&format!(
        r#"{{"resourceType": "Bundle", "type": "collection", "entry": [{}, {}]}}"#,
        entry("1"),
        entry("1")
    ));
    let history = json(&format!(
        r#"{{"resourceType": "Bundle", "type": "history", "entry": [{}, {}]}}"#,
        entry("1"),
        entry("1")
    ));
    assert_eq!(
        truth(&invariant("bdl-7", Focus::resource(&distinct))),
        Some(true)
    );
    assert_eq!(
        truth(&invariant("bdl-7", Focus::resource(&repeated))),
        Some(false)
    );
    assert_eq!(
        truth(&invariant("bdl-7", Focus::resource(&history))),
        Some(true)
    );
}

#[test]
fn per_1_compares_start_and_end_and_is_empty_across_precisions() {
    // N1 §6.2: values of different precision compare as unknown, so the
    // invariant itself answers empty.
    let patient = json(r#"{"resourceType": "Patient"}"#);
    let period =
        |start: &str, end: &str| json(&format!(r#"{{"start": "{start}", "end": "{end}"}}"#));
    let ordered = period("2020-01-01", "2020-02-01");
    let reversed = period("2020-02-01", "2020-01-01");
    let mixed = period("2020-01-01", "2020-01-01T10:00:00Z");
    let evaluate = |value| invariant("per-1", Focus::element(value, "Period", &patient));
    assert_eq!(truth(&evaluate(&ordered)), Some(true));
    assert_eq!(truth(&evaluate(&reversed)), Some(false));
    assert_eq!(truth(&evaluate(&mixed)), None);
}

#[test]
fn txt_1_and_txt_2_read_the_narrative() {
    let patient = json(r#"{"resourceType": "Patient"}"#);
    let good = Value::String(String::from(
        r#"<div xmlns="http://www.w3.org/1999/xhtml">A <b>note</b></div>"#,
    ));
    let script = Value::String(String::from(
        r#"<div xmlns="http://www.w3.org/1999/xhtml"><script>x</script>A</div>"#,
    ));
    let blank = Value::String(String::from(
        r#"<div xmlns="http://www.w3.org/1999/xhtml"> </div>"#,
    ));
    let div = |value| invariant("txt-1", Focus::element(value, "Narrative.div", &patient));
    assert_eq!(truth(&div(&good)), Some(true));
    assert_eq!(truth(&div(&script)), Some(false));
    assert_eq!(truth(&div(&blank)), Some(false));
}
