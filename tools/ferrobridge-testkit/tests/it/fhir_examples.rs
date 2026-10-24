// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 examples package of each FHIR version through `fhir-types`.
//!
//! Every example resource decodes through the strict JSON codec of its
//! version, re-encodes, and then travels through the XML codec and back. A
//! case passes when both round trips give back the document the package
//! published, compared as the lexical document model, whose objects are
//! ordered maps and whose numbers keep their text, so the comparison is
//! byte equality of the canonical form. The first refusal or difference is
//! the failure reason. No specification governs the instrument: our own
//! design; the codecs answer to <https://hl7.org/fhir/json.html> and
//! <https://hl7.org/fhir/xml.html> of each version.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::Path;

use ferrobridge_testkit::conformance::Case;
use ferrobridge_testkit::conformance::Corpus;
use ferrobridge_testkit::conformance::record;
use ferrobridge_testkit::examples::Package;
use fhir_types::codec::Json;
use fhir_types::codec;
use fhir_types::codec::Value;
use fhir_types::codec::expect_object;
use fhir_types::schema::Schemas;
use fhir_types::xml::from_xml;
use fhir_types::xml::to_xml;

/// Returns why `file` fails its round trips, or `None` when both hold.
fn failure<T: Json>(schemas: &Schemas, file: &Path) -> Option<String> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => return Some(format!("unreadable: {error}")),
    };
    let original: Value = match serde_json::from_str(without_byte_order_mark(&text)) {
        Ok(value) => value,
        Err(error) => return Some(format!("not JSON: {error}")),
    };
    let mut path = codec::Path::root("Resource");
    let decoded = match expect_object(&original, &path).and_then(|o| T::from_json(o, &mut path)) {
        Ok(decoded) => decoded,
        Err(error) => return Some(format!("JSON decode: {error}")),
    };
    let encoded = match decoded.to_json() {
        Ok(object) => Value::Object(object),
        Err(error) => return Some(format!("JSON encode: {error}")),
    };
    if let Some(at) = difference(&original, &encoded, "") {
        return Some(format!("the JSON round trip differs at {at}"));
    }
    let Value::Object(ref object) = encoded else {
        return Some(String::from("the JSON encoder gave back no object"));
    };
    let xml = match to_xml(schemas, object) {
        Ok(xml) => xml,
        Err(error) => return Some(format!("XML encode: {error}")),
    };
    let back = match from_xml(schemas, &xml) {
        Ok(back) => Value::Object(back),
        Err(error) => return Some(format!("XML decode: {error}")),
    };
    difference(&encoded, &back, "").map(|at| format!("the XML round trip differs at {at}"))
}

/// Returns `text` without a leading UTF-8 byte order mark.
///
/// 19 files of `hl7.fhir.r3.examples` 3.0.2 begin with one, which `serde_json`
/// refuses; the fetched files stay verbatim.
fn without_byte_order_mark(text: &str) -> &str {
    // NOTE: https://hl7.org/fhir/STU3/json.html says "The character encoding is always
    // UTF-8" and is silent on a byte order mark: no specification governs this: our own design.
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Returns the first place `after` departs from `before`, by element path.
fn difference(before: &Value, after: &Value, path: &str) -> Option<String> {
    match (before, after) {
        (Value::Object(was), Value::Object(now)) => {
            for (key, value) in was {
                let next = member(path, key);
                match now.get(key) {
                    None => return Some(format!("{next} (dropped)")),
                    Some(other) => {
                        if let Some(found) = difference(value, other, &next) {
                            return Some(found);
                        }
                    }
                }
            }
            now.keys()
                .find(|key| !was.contains_key(*key))
                .map(|key| format!("{} (added)", member(path, key)))
        }
        (Value::Array(was), Value::Array(now)) => {
            for (index, (value, other)) in was.iter().zip(now).enumerate() {
                if let Some(found) = difference(value, other, &format!("{path}[{index}]")) {
                    return Some(found);
                }
            }
            (was.len() != now.len())
                .then(|| format!("{path} ({} items became {})", was.len(), now.len()))
        }
        _ => (before != after).then(|| format!("{path} (value)")),
    }
}

/// Returns the path of the member `key` below `path`.
fn member(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// Whether `package` is on disk, or why its corpus skips.
#[expect(
    clippy::print_stderr,
    reason = "a corpus whose build-time package is absent says it skipped"
)]
fn fetched(package: Package) -> bool {
    let present = package.fetched();
    if !present {
        eprintln!(
            "skipped: {package} is fetched at build time; run scripts/vendor/fhir-packages.sh --build-time"
        );
    }
    present
}

/// The elements whose shape the 6.0.0-ballot5 core model changed from R5 and
/// whose 6.0.0-ballot5 examples carry a value the new model refuses, with the
/// change.
///
/// The examples package writes `null` or `{}` where a value of the R5 type
/// did not carry over into the ballot's type, and keeps an element the ballot
/// removed. R6 ballot5 json.html §2.1.6.4 says "Objects are never empty", and
/// its §2.1.6.4.3 admits `null` only in an array of primitive values aligned
/// with its `_` array, so the decoder refuses each one. A failure on a listed
/// element stays a failure, never a pass, and carries the outcome
/// `ballot-drift <element>` so the corpus counts it; every element listed
/// must still meet such a failure, so the list follows the pins (#380).
const R6_BALLOT_DRIFT: [(&str, &str); 10] = [
    (
        "AllergyIntolerance.category",
        "CodeableConcept 0..*, code 0..* in R5",
    ),
    (
        "ClinicalUseDefinition.subject",
        "CodeableReference 1..*, Reference 0..* in R5",
    ),
    (
        "Condition.bodyStructure",
        "CodeableReference 0..*, new in the ballot",
    ),
    (
        "DiagnosticReport.conclusionCode",
        "CodeableReference 0..*, CodeableConcept 0..* in R5",
    ),
    (
        "OrganizationAffiliation.network",
        "removed in the ballot, Reference 0..* in R5",
    ),
    (
        "Procedure.bodyStructure",
        "CodeableReference 0..*, new in the ballot",
    ),
    (
        "Provenance.authorization",
        "CodeableConcept 0..*, CodeableReference 0..* in R5",
    ),
    (
        "ServiceRequest.asNeededFor",
        "CodeableConcept 0..*, new in the ballot",
    ),
    ("Task.focus", "BackboneElement 0..*, Reference 0..1 in R5"),
    (
        "TerminologyCapabilities.codeSystem.version",
        "BackboneElement 0..*, items written as null",
    ),
];

/// The element a decode failure of `original` lands on, as the resource type
/// holding it and the element names below that resource (`Task.focus`), or
/// `None` for a failure of any other kind.
///
/// The failure names an instance path from `Resource` (`Resource.entry[0].
/// resource.category[0]`); the walk follows it through `original` and starts
/// over at every object carrying a `resourceType`, so a contained or bundled
/// resource names its own type.
fn failed_element(original: &Value, failure: &str) -> Option<String> {
    let rest = failure.strip_prefix("JSON decode: Resource.")?;
    let (path, _) = rest.split_once(": ")?;
    let mut current = original;
    let mut resource = original.get("resourceType")?.as_str()?;
    let mut names: Vec<&str> = Vec::new();
    for segment in path.split('.') {
        let (name, index) = match segment.split_once('[') {
            Some((name, index)) => (name, index.strip_suffix(']')?.parse::<usize>().ok()),
            None => (segment, None),
        };
        names.push(name);
        current = current.get(name)?;
        if let Some(index) = index {
            current = current.as_array()?.get(index)?;
        }
        if let Some(found) = current.get("resourceType").and_then(Value::as_str) {
            resource = found;
            names.clear();
        }
    }
    Some(format!("{resource}.{}", names.join(".")))
}

/// Runs every example of `package` and records the verdicts as `corpus`,
/// counting a failure on an element `drift` lists as `ballot-drift`.
fn measure<T: Json>(
    package: Package,
    corpus: Corpus,
    schemas: &Schemas,
    drift: &[(&str, &str)],
) -> Result<(), Box<dyn Error>> {
    if !fetched(package) {
        return Ok(());
    }
    let files = package.resources()?;
    assert!(
        files.len() > 2000,
        "{package} holds {} example resources",
        files.len()
    );
    let mut cases = Vec::with_capacity(files.len());
    let mut met: BTreeSet<String> = BTreeSet::new();
    for file in &files {
        let id = file
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or("an example file has a UTF-8 name")?;
        cases.push(match failure::<T>(schemas, file) {
            None => Case::pass(id),
            Some(reason) => match drifted(file, &reason, drift)? {
                Some(element) => {
                    met.insert(element.clone());
                    let outcomes = BTreeMap::from([(format!("ballot-drift {element}"), 1)]);
                    Case::fail(id, reason).with_outcomes(outcomes)
                }
                None => Case::fail(id, reason),
            },
        });
    }
    let outcome = record(corpus, &cases)?;
    assert!(
        outcome.regressed.is_empty(),
        "cases the {corpus} pass list records no longer pass: {:?}",
        outcome.regressed
    );
    let stale: Vec<&str> = drift
        .iter()
        .map(|(element, _)| *element)
        .filter(|element| !met.contains(*element))
        .collect();
    assert!(
        stale.is_empty(),
        "{corpus} lists ballot drift no example meets any more: {stale:?}"
    );
    Ok(())
}

/// The element of `drift` the failure `reason` of `file` lands on, `None`
/// when it lands on no listed element.
fn drifted(
    file: &Path,
    reason: &str,
    drift: &[(&str, &str)],
) -> Result<Option<String>, Box<dyn Error>> {
    if drift.is_empty() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(file)?;
    let original: Value = serde_json::from_str(without_byte_order_mark(&text))?;
    Ok(failed_element(&original, reason)
        .filter(|element| drift.iter().any(|(listed, _)| listed == element)))
}

#[test]
fn conformance_the_fhir_stu3_examples_hold_their_pass_list() -> Result<(), Box<dyn Error>> {
    measure::<fhir_types::stu3::resource::Resource>(
        Package::Stu3,
        Corpus::FhirStu3,
        &fhir_types::stu3::schema::SCHEMAS,
        &[],
    )
}

#[test]
fn conformance_the_fhir_r4_examples_hold_their_pass_list() -> Result<(), Box<dyn Error>> {
    measure::<fhir_types::r4::resource::Resource>(
        Package::R4,
        Corpus::FhirR4,
        &fhir_types::r4::schema::SCHEMAS,
        &[],
    )
}

#[test]
fn conformance_the_fhir_r4b_examples_hold_their_pass_list() -> Result<(), Box<dyn Error>> {
    measure::<fhir_types::r4b::resource::Resource>(
        Package::R4b,
        Corpus::FhirR4b,
        &fhir_types::r4b::schema::SCHEMAS,
        &[],
    )
}

#[test]
fn conformance_the_fhir_r5_examples_hold_their_pass_list() -> Result<(), Box<dyn Error>> {
    measure::<fhir_types::r5::resource::Resource>(
        Package::R5,
        Corpus::FhirR5,
        &fhir_types::r5::schema::SCHEMAS,
        &[],
    )
}

#[test]
fn conformance_the_fhir_r6_examples_hold_their_pass_list() -> Result<(), Box<dyn Error>> {
    measure::<fhir_types::r6::resource::Resource>(
        Package::R6,
        Corpus::FhirR6,
        &fhir_types::r6::schema::SCHEMAS,
        &R6_BALLOT_DRIFT,
    )
}

#[test]
fn a_failure_names_the_element_of_the_innermost_resource() -> Result<(), Box<dyn Error>> {
    let bundle: Value = serde_json::from_str(
        r#"{"resourceType":"Bundle","entry":[{"resource":{"resourceType":"AllergyIntolerance","category":[null]}}]}"#,
    )?;
    assert_eq!(
        failed_element(
            &bundle,
            "JSON decode: Resource.entry[0].resource.category[0]: expected an object"
        )
        .as_deref(),
        Some("AllergyIntolerance.category")
    );
    let capabilities: Value = serde_json::from_str(
        r#"{"resourceType":"TerminologyCapabilities","codeSystem":[{},{"version":[null]}]}"#,
    )?;
    assert_eq!(
        failed_element(
            &capabilities,
            "JSON decode: Resource.codeSystem[1].version[0]: expected an object"
        )
        .as_deref(),
        Some("TerminologyCapabilities.codeSystem.version")
    );
    assert_eq!(
        failed_element(&capabilities, "the JSON round trip differs at id (value)"),
        None,
        "only a decode failure names an element"
    );
    Ok(())
}

#[test]
fn a_difference_names_the_first_element_that_moved() -> Result<(), Box<dyn Error>> {
    let before: Value = serde_json::from_str(r#"{"a":[{"b":"x"},{"b":"y"}],"c":1.0}"#)?;
    let changed: Value = serde_json::from_str(r#"{"a":[{"b":"x"},{"b":"z"}],"c":1.0}"#)?;
    let reprinted: Value = serde_json::from_str(r#"{"a":[{"b":"x"},{"b":"y"}],"c":1.00}"#)?;
    let reordered: Value = serde_json::from_str(r#"{"c":1.0,"a":[{"b":"x"},{"b":"y"}]}"#)?;
    let added: Value = serde_json::from_str(r#"{"a":[{"b":"x"},{"b":"y"}],"c":1.0,"d":true}"#)?;
    assert_eq!(
        difference(&before, &changed, "").as_deref(),
        Some("a[1].b (value)")
    );
    assert_eq!(
        difference(&before, &reprinted, "").as_deref(),
        Some("c (value)"),
        "a number keeps its text"
    );
    assert_eq!(
        difference(&before, &reordered, ""),
        None,
        "member order is not content"
    );
    assert_eq!(
        difference(&before, &added, "").as_deref(),
        Some("d (added)")
    );
    Ok(())
}
