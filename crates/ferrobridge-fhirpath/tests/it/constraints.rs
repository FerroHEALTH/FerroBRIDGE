// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The invariants of the vendored FHIR packages
//! (`ElementDefinition.constraint.expression`, FHIR R4 conformance rules
//! <https://hl7.org/fhir/R4/conformance-rules.html#constraints>): every one
//! parses, every R4 and STU3 core one passes the static check against the element it
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
use ferrobridge_fhirpath::model::r4::R4;
use ferrobridge_fhirpath::model::stu3::Stu3;
use ferrobridge_fhirpath::model::{choice_element, named_type, path_types};
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
    // NOTE: RFC 8259 §8.1 lets a parser ignore a leading byte order mark, which four files of
    // hl7.fhir.r3.core 3.0.2 carry (StructureDefinition-Request.json among them).
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let definition: Definition = serde_json::from_str(text).expect("the definition should be JSON");
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
const DEFECTIVE: [(&str, &str, &str, &str); 6] = [
    (
        "hl7.fhir.r3.core",
        "inv-1",
        "Extension",
        "as in R4, the allergyintolerance-substanceExposureRisk extension names its parts as `extension` slices, so `substanceExposureRisk` is no element of Extension",
    ),
    (
        "hl7.fhir.r3.core",
        "sdf-15",
        "StructureDefinition.snapshot",
        "STU3 places sdf-15 on StructureDefinition.snapshot, yet it reads `kind` and `snapshot`, elements of StructureDefinition itself, where R4 places the same expression",
    ),
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
    let mut on_choices = Vec::new();
    let mut through_choices = 0;
    let mut below_primitives = 0;
    let mut skipped = BTreeSet::new();
    let mut refused = Vec::new();
    let mut expected = Vec::new();
    for (key, path, expression) in R4_CORE.iter() {
        let root = path.split('.').next().unwrap_or(path);
        let Some(contexts) = path_types(&R4, path) else {
            skipped.insert(root);
            continue;
        };
        if choice_element(&R4, path).is_some() {
            on_choices.push(key.clone());
        }
        if path
            .rsplit_once('.')
            .is_some_and(|(parent, _)| parent.contains("[x]"))
        {
            through_choices += 1;
        }
        if named_type(&R4, root).is_some_and(|ty| ty.is_primitive()) && path.contains('.') {
            below_primitives += 1;
        }
        let parsed = Expression::parse(expression).expect("the invariant should parse");
        checked += 1;
        match (
            parsed.check_any(&R4, &contexts),
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
    // R4 §2.9.1.1: an invariant on a choice element is checked against every
    // type the choice admits.
    assert_eq!(
        on_choices.len(),
        191,
        "the R4 core package holds 191 invariants on choice elements"
    );
    for key in ["ras-1", "vs-1"] {
        assert!(
            on_choices.iter().any(|found| found == key),
            "{key} sits on a choice element and is checked"
        );
    }
    // R4 §2.9.1.1: a path through a choice element reaches the element in every type the
    // choice admits, as `Observation.value[x].unit` does in the vital-signs profiles.
    assert_eq!(
        through_choices, 14,
        "the R4 core package holds 14 invariants below a choice element"
    );
    // R4 §2.9.1.2: every primitive specializes Element, so `string.extension` is checked.
    assert_eq!(
        below_primitives, 40,
        "the R4 core package holds 40 invariants on the children of primitive types"
    );
    // NOTE: R4 StructureDefinition.kind `logical` marks these five as logical models, no type
    // an instance holds, so the element table carries none of them and their invariants type nothing.
    assert_eq!(
        skipped.into_iter().collect::<Vec<_>>(),
        [
            "Definition",
            "Event",
            "FiveWs",
            "MetadataResource",
            "Request"
        ],
        "only the logical models stay untyped"
    );
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
fn every_stu3_core_constraint_checks_against_its_stu3_element() {
    let package = "hl7.fhir.r3.core";
    let found = constraints(package);
    let (unparsed, _) = unparsed(package, &found);
    assert!(
        unparsed.is_empty(),
        "STU3 invariants that do not parse: {unparsed:#?}"
    );
    let mut checked = 0;
    let mut skipped = BTreeSet::new();
    let mut refused = Vec::new();
    let mut expected = Vec::new();
    for (key, path, expression) in &found {
        let Some(contexts) = path_types(&Stu3, path) else {
            skipped.insert(path.split('.').next().unwrap_or(path));
            continue;
        };
        checked += 1;
        let parsed = Expression::parse(expression).expect("the invariant should parse");
        match (
            parsed.check_any(&Stu3, &contexts),
            defective(package, key, path),
        ) {
            (Err(_), true) => expected.push(key.as_str()),
            (Err(error), false) => refused.push(format!("{key} on {path}: {error}")),
            (Ok(()), true) => refused.push(format!(
                "{key} on {path}: recorded as defective, yet it checks"
            )),
            (Ok(()), false) => {}
        }
    }
    assert!(
        refused.is_empty(),
        "STU3 invariants the check refuses: {refused:#?}"
    );
    assert_eq!(
        expected,
        ["inv-1", "sdf-15"],
        "the recorded defects are refused"
    );
    assert_eq!(
        checked, 1069,
        "the STU3 core invariants with a context type"
    );
    // NOTE: STU3 StructureDefinition.kind `logical` marks Event, MetadataResource and Request,
    // and no element of the STU3 closure is a uuid, so the table carries none of the four.
    assert_eq!(
        skipped.into_iter().collect::<Vec<_>>(),
        ["Event", "MetadataResource", "Request", "uuid"],
        "only the logical models and uuid stay untyped"
    );
}

/// Evaluates an invariant of the STU3 core package by key on `resource`
/// with the STU3 model.
fn stu3_invariant<'a>(key: &str, resource: &'a Value) -> Collection<'a> {
    let found = constraints("hl7.fhir.r3.core");
    let (_, _, expression) = found
        .iter()
        .find(|(candidate, ..)| candidate == key)
        .expect("the invariant should exist");
    Expression::parse(expression)
        .expect("the invariant should parse")
        .evaluate(&Environment::new(&Stu3), Focus::resource(resource))
        .expect("the invariant should evaluate")
}

#[test]
fn obs_6_reads_an_stu3_attachment_value() {
    // STU3 Observation.value[x] admits Attachment, which R4 drops; obs-6 refuses a value
    // beside a dataAbsentReason (https://hl7.org/fhir/STU3/observation.html#invs).
    let observation = json(
        r#"{"resourceType": "Observation", "status": "final", "code": {"text": "x"},
            "valueAttachment": {"contentType": "text/plain"},
            "dataAbsentReason": {"text": "asked"}}"#,
    );
    assert_eq!(truth(&stu3_invariant("obs-6", &observation)), Some(false));
    let typed = Expression::parse("Observation.value.ofType(Attachment).contentType")
        .expect("the expression should parse")
        .evaluate(&Environment::new(&Stu3), Focus::resource(&observation))
        .expect("the path should evaluate");
    assert_eq!(typed.len(), 1);
    let r4 = Expression::parse("Observation.value.ofType(Attachment)")
        .expect("the expression should parse")
        .evaluate(&Environment::new(&R4), Focus::resource(&observation))
        .expect("the path should evaluate");
    assert!(r4.is_empty(), "R4 Observation has no valueAttachment");
}

#[test]
fn an_stu3_path_resolves_against_stu3_types() {
    // STU3 Patient.animal (https://hl7.org/fhir/STU3/patient.html) is no element of R4 Patient.
    let patient = json(r#"{"resourceType": "Patient", "animal": {"species": {"text": "dog"}}}"#);
    let path = Expression::parse("Patient.animal.species.text = 'dog'")
        .expect("the expression should parse");
    let answer = path
        .evaluate(&Environment::new(&Stu3), Focus::resource(&patient))
        .expect("STU3 Patient has an animal");
    assert_eq!(truth(&answer), Some(true));
    let patient_type = named_type(&Stu3, "Patient").expect("STU3 Patient");
    assert!(path.check(&Stu3, patient_type).is_ok());
    let r4_type = named_type(&R4, "Patient").expect("R4 Patient");
    assert!(path.check(&R4, r4_type).is_err());
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
fn rng_2_compares_low_and_high_across_ucum_units() {
    // N1 §6.2: quantities of one dimension compare in the more granular unit;
    // FHIR R4 §2.9.1.3: only a UCUM code makes a System.Quantity.
    let patient = json(r#"{"resourceType": "Patient"}"#);
    let range = |low: &str, high: &str| {
        json(&format!(
            r#"{{"low": {{"value": {low}, "system": "http://unitsofmeasure.org", "code": "mg"}},
                "high": {{"value": {high}, "system": "http://unitsofmeasure.org", "code": "g"}}}}"#
        ))
    };
    let ordered = range("500", "1");
    let reversed = range("1500", "1");
    let incommensurable = json(
        r#"{"low": {"value": 1, "system": "http://unitsofmeasure.org", "code": "mg"},
            "high": {"value": 1, "system": "http://unitsofmeasure.org", "code": "mL"}}"#,
    );
    let evaluate = |value| invariant("rng-2", Focus::element(value, "Range", &patient));
    assert_eq!(truth(&evaluate(&ordered)), Some(true));
    assert_eq!(truth(&evaluate(&reversed)), Some(false));
    assert_eq!(truth(&evaluate(&incommensurable)), None);
}

#[test]
fn ras_1_checks_against_a_range_and_reads_percentages() {
    // R4 RiskAssessment ras-1 constrains probability[x] when it is a Range;
    // FHIR R4 §2.9.1.3: only a UCUM code makes a System.Quantity.
    let (_, _, expression) = R4_CORE
        .iter()
        .find(|(key, ..)| key == "ras-1")
        .expect("ras-1 should exist");
    let range = named_type(&R4, "Range").expect("Range is an R4 type");
    assert_eq!(
        Expression::parse(expression)
            .expect("ras-1 should parse")
            .check(&R4, range),
        Ok(())
    );
    let assessment = json(r#"{"resourceType": "RiskAssessment", "status": "final"}"#);
    let percent = json(
        r#"{"low": {"value": 10, "system": "http://unitsofmeasure.org", "code": "%"},
            "high": {"value": 20, "system": "http://unitsofmeasure.org", "code": "%"}}"#,
    );
    let milligrams =
        json(r#"{"low": {"value": 10, "system": "http://unitsofmeasure.org", "code": "mg"}}"#);
    let open = json("{}");
    let evaluate = |value| invariant("ras-1", Focus::element(value, "Range", &assessment));
    assert_eq!(truth(&evaluate(&percent)), Some(true));
    assert_eq!(truth(&evaluate(&milligrams)), Some(false));
    assert_eq!(truth(&evaluate(&open)), Some(true));
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

/// The element `ras-1` constrains, a choice of `decimal` and `Range`.
const PROBABILITY: &str = "RiskAssessment.prediction.probability[x]";

#[test]
fn ras_1_holds_on_a_decimal_probability() {
    // N1 §3.2 and §4.4, FHIR R4 §2.9.1.1: `low` and `high` belong to the
    // Range the choice admits, so on a decimal they are no nodes.
    let assessment = json(r#"{"resourceType": "RiskAssessment", "status": "final"}"#);
    let probability = json("0.25");
    let focus =
        Focus::primitive(Some(&probability), None, "decimal", &assessment).defined_by(PROBABILITY);
    assert_eq!(truth(&invariant("ras-1", focus)), Some(true));
}

#[test]
fn ras_1_reads_the_units_of_a_range_probability() {
    let assessment = json(r#"{"resourceType": "RiskAssessment", "status": "final"}"#);
    let range = |code: &str| {
        json(&format!(
            r#"{{"low": {{"value": 10, "system": "http://unitsofmeasure.org", "code": "{code}"}},
                "high": {{"value": 20, "system": "http://unitsofmeasure.org", "code": "{code}"}}}}"#
        ))
    };
    let percent = range("%");
    let milligrams = range("mg");
    let evaluate = |value| {
        invariant(
            "ras-1",
            Focus::element(value, "Range", &assessment).defined_by(PROBABILITY),
        )
    };
    assert_eq!(truth(&evaluate(&percent)), Some(true));
    assert_eq!(truth(&evaluate(&milligrams)), Some(false));
}

#[test]
fn a_decimal_not_named_as_a_choice_occurrence_has_no_low() {
    // N1 §3: an identifier the context's type cannot resolve is an error.
    let assessment = json(r#"{"resourceType": "RiskAssessment", "status": "final"}"#);
    let probability = json("0.25");
    let focus = Focus::primitive(Some(&probability), None, "decimal", &assessment);
    let result = Expression::parse("low.empty()")
        .expect("the expression should parse")
        .evaluate(&Environment::new(&R4), focus);
    assert!(result.is_err(), "decimal defines no low: {result:?}");
}

#[test]
fn a_member_no_type_of_the_choice_defines_is_refused() {
    let contexts = path_types(&R4, PROBABILITY).expect("the choice should have types");
    let names: Vec<&str> = contexts.iter().map(|ty| ty.name).collect();
    assert_eq!(names, ["decimal", "Range"]);
    let typo = Expression::parse("loww.empty()").expect("the expression should parse");
    assert!(typo.check_any(&R4, &contexts).is_err());
    let range_member = Expression::parse("low.empty()").expect("the expression should parse");
    assert_eq!(range_member.check_any(&R4, &contexts), Ok(()));
}

#[test]
fn a_member_no_type_of_the_choice_defines_fails_the_evaluation() {
    let assessment = json(r#"{"resourceType": "RiskAssessment", "status": "final"}"#);
    let probability = json("0.25");
    let focus =
        Focus::primitive(Some(&probability), None, "decimal", &assessment).defined_by(PROBABILITY);
    let result = Expression::parse("loww.empty()")
        .expect("the expression should parse")
        .evaluate(&Environment::new(&R4), focus);
    assert!(
        result.is_err(),
        "no type of the choice defines loww: {result:?}"
    );
}

#[test]
fn a_path_over_a_choice_ignores_the_type_of_each_value() {
    // N1 §3.2: `Observation.value.unit` reads "all kinds of value".
    let observation = json(
        r#"{"resourceType": "Observation", "status": "final", "code": {"text": "x"},
            "valueString": "high"}"#,
    );
    let unit = Expression::parse("Observation.value.unit")
        .expect("the expression should parse")
        .evaluate(&Environment::new(&R4), Focus::resource(&observation))
        .expect("the path should evaluate");
    assert_eq!(unit, Vec::<ferrobridge_fhirpath::value::Item<'_>>::new());
    let typo = Expression::parse("Observation.value.unitt")
        .expect("the expression should parse")
        .evaluate(&Environment::new(&R4), Focus::resource(&observation));
    assert!(typo.is_err(), "no type of Observation.value defines unitt");
}

/// The names of the types `path` reaches.
fn reached(path: &str) -> Option<Vec<&'static str>> {
    path_types(&R4, path).map(|types| types.iter().map(|ty| ty.name).collect())
}

#[test]
fn a_path_through_a_choice_reaches_the_types_that_define_its_next_element() {
    // R4 §2.9.1.1: `Observation.value[x].unit` is the unit of the Quantity alternative.
    assert_eq!(reached("Observation.value[x].unit"), Some(vec!["string"]));
    assert_eq!(
        reached("Observation.component.value[x].system"),
        Some(vec!["uri"])
    );
    assert_eq!(reached("Observation.value[x].unitt"), None);
}

#[test]
fn a_path_below_a_primitive_reaches_the_elements_of_element() {
    // R4 §2.9.1.2: FHIR.string specializes FHIR.Element, with its id and extension.
    assert_eq!(reached("string.extension"), Some(vec!["Extension"]));
    assert_eq!(reached("code.id"), Some(vec!["string"]));
    assert_eq!(reached("string.value"), None);
}
