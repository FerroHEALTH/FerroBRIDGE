// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where each declared loss happened, and that no location carries a value.
//!
//! Every warning names the FHIR element as a FHIRPath in the simple subset
//! `OperationOutcome.issue.expression` admits
//! (<https://hl7.org/fhir/R4/operationoutcome-definitions.html#OperationOutcome.issue.expression>)
//! and the openEHR node as its archetype path. The locations are asserted
//! exactly, per loss and per direction, and the input of each run carries
//! identifying content no location may repeat.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Location;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::SkipReason;
use fhirconnect::engine::outcome::Warning;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::operations::issues;
use fhirconnect::operations::issues::OPENEHR_PATH_SYSTEM;
use openehr_mapping_core::composition::CanonicalComposition;

use crate::support::compiled;
use crate::support::template;

/// The archetype path of the problem-diagnosis data the fixtures write under.
const DATA: &str = "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]";

/// Identifying text the inputs carry, which no location may repeat.
const SITES: [&str; 2] = ["Synthetic site of Jansen", "Synthetic site 0042-77"];

/// Returns the defaults every run into openEHR fills its fields with.
fn defaults() -> Defaults {
    Defaults::at("2026-10-15T08:00:00Z")
        .with_language("en")
        .with_territory("NL")
}

/// Returns the synthetic `Condition` with one `bodySite` per text of [`SITES`].
fn condition_with_sites() -> Result<serde_json::Value, Box<dyn Error>> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    let sites: Vec<serde_json::Value> = SITES
        .iter()
        .map(|text| serde_json::json!({ "text": text }))
        .collect();
    if let Some(object) = parsed.as_object_mut() {
        object.insert(String::from("bodySite"), serde_json::Value::Array(sites));
    }
    Ok(parsed)
}

/// Returns a composition with one anatomical-location cluster per text of
/// [`SITES`], built by the split fixture's run into openEHR.
fn composition_with_clusters() -> Result<CanonicalComposition, Box<dyn Error>> {
    let program = compiled("ferrobridge_split")?;
    let built = to_openehr(
        &program,
        &SCHEMAS,
        &template()?,
        &Value::from_serde_json(condition_with_sites()?),
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?;
    Ok(built.into_value())
}

/// Returns a located warning.
fn located(loss: Loss, fhir: Option<&str>, openehr: Option<&str>) -> Warning {
    let mut location = Location::default();
    if let Some(path) = fhir {
        location = location.with_fhir(path);
    }
    if let Some(path) = openehr {
        location = location.with_openehr(path);
    }
    Warning::new(loss, location)
}

/// Returns every text and number a JSON document carries, except its
/// `resourceType`, which a FHIRPath names as its root.
fn carried(value: &serde_json::Value, found: &mut Vec<String>) {
    match *value {
        serde_json::Value::String(ref text) => found.push(text.clone()),
        serde_json::Value::Number(ref number) => found.push(number.to_string()),
        serde_json::Value::Array(ref items) => {
            for item in items {
                carried(item, found);
            }
        }
        serde_json::Value::Object(ref members) => {
            for (key, member) in members {
                if key != "resourceType" {
                    carried(member, found);
                }
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) => {}
    }
}

/// Asserts that no location, issue expression or issue coding repeats a
/// value of `input`.
fn assert_no_value(input: &serde_json::Value, warnings: &[Warning]) {
    let mut values = Vec::new();
    carried(input, &mut values);
    let mut named: Vec<String> = Vec::new();
    for warning in warnings {
        named.extend(warning.location().fhir().map(String::from));
        named.extend(warning.location().openehr().map(String::from));
        let issue = issues::of_warning(warning);
        named.extend(
            issue
                .expression
                .iter()
                .filter_map(|path| path.value.clone()),
        );
        named.extend(
            issue
                .details
                .iter()
                .flat_map(|details| details.coding.iter())
                .filter_map(|coding| coding.code.as_ref().and_then(|code| code.value.clone())),
        );
    }
    for value in &values {
        for name in &named {
            assert!(
                !name.contains(value.as_str()),
                "the location {name} carries the input value {value}"
            );
        }
    }
}

/// Asserts that each warning's issue carries its FHIRPath as the expression
/// and its openEHR path as an [`OPENEHR_PATH_SYSTEM`] coding, and nothing else.
fn assert_rendered(warnings: &[Warning]) {
    for warning in warnings {
        let issue = issues::of_warning(warning);
        let expressions: Vec<Option<&str>> = issue
            .expression
            .iter()
            .map(|path| path.value.as_deref())
            .collect();
        let expected: Vec<Option<&str>> = warning.location().fhir().map(Some).into_iter().collect();
        assert_eq!(expressions, expected, "the expression of {warning}");
        let codings: Vec<(Option<&str>, Option<&str>)> = issue
            .details
            .as_ref()
            .map(|details| details.coding.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|coding| {
                (
                    coding
                        .system
                        .as_ref()
                        .and_then(|system| system.value.as_deref()),
                    coding.code.as_ref().and_then(|code| code.value.as_deref()),
                )
            })
            .collect();
        let expected: Vec<(Option<&str>, Option<&str>)> = warning
            .location()
            .openehr()
            .map(|path| (Some(OPENEHR_PATH_SYSTEM), Some(path)))
            .into_iter()
            .collect();
        assert_eq!(codings, expected, "the codings of {warning}");
        assert_eq!(
            issue
                .details
                .and_then(|details| details.text)
                .and_then(|text| text.value),
            Some(warning.to_string()),
            "the text stays the loss's own sentence"
        );
    }
}

#[test]
fn every_loss_going_into_openehr_names_both_places() -> Result<(), Box<dyn Error>> {
    let input = condition_with_sites()?;
    let program = compiled("ferrobridge_located")?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &template()?,
        &Value::from_serde_json(input.clone()),
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?;
    let comment = format!("{DATA}/items[at0069]/value");
    let recorded = format!("{DATA}/items[at0077]/value");
    assert_eq!(
        inbound.warnings(),
        [
            located(
                Loss::LastOfMany {
                    path: comment.clone(),
                    dropped: 1,
                },
                Some("Condition.bodySite"),
                Some(&comment),
            ),
            located(
                Loss::Skipped {
                    mapping: String::from("recordedOutboundOnly"),
                    reason: SkipReason::Unidirectional,
                },
                Some("Condition.recordedDate"),
                Some(&recorded),
            ),
            located(
                Loss::Defaulted {
                    field: String::from("/composer"),
                },
                None,
                Some("/composer"),
            ),
            located(
                Loss::Defaulted {
                    field: String::from("/context/start_time"),
                },
                None,
                Some("/context/start_time"),
            ),
            located(
                Loss::Defaulted {
                    field: String::from("/context/setting"),
                },
                None,
                Some("/context/setting"),
            ),
            located(
                Loss::Defaulted {
                    field: String::from("/language"),
                },
                None,
                Some("/language"),
            ),
            located(
                Loss::Defaulted {
                    field: String::from("/territory"),
                },
                None,
                Some("/territory"),
            ),
        ]
    );
    assert_rendered(inbound.warnings());
    assert_no_value(&input, inbound.warnings());
    Ok(())
}

#[test]
fn every_loss_coming_out_to_fhir_names_both_places() -> Result<(), Box<dyn Error>> {
    let composition = composition_with_clusters()?;
    let program = compiled("ferrobridge_located")?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &template()?,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let site =
        format!("{DATA}/items[openEHR-EHR-CLUSTER.anatomical_location.v1]/items[at0001]/value");
    assert_eq!(
        outbound.warnings(),
        [
            located(
                Loss::LastOfMany {
                    path: String::from("CodeableConcept.text"),
                    dropped: 1,
                },
                Some("Condition.code.text"),
                Some(&site),
            ),
            located(
                Loss::Skipped {
                    mapping: String::from("problemInboundOnly"),
                    reason: SkipReason::Unidirectional,
                },
                Some("Condition.code"),
                Some(&format!("{DATA}/items[at0002]/value")),
            ),
        ]
    );
    assert_rendered(outbound.warnings());
    // NOTE: no specification governs this: our own design, the composition holds the
    // template's node ids and names beside its values, so the clinical content checked
    // for is the FHIR input the clusters were built from.
    assert_no_value(&condition_with_sites()?, outbound.warnings());
    Ok(())
}

// A withheld TERM_MAPPING arises coming out to FHIR only: going in, a Coding is
// written back as an `=` mapping, which FHIR carries.
#[test]
fn a_withheld_term_mapping_names_the_element_and_the_mappings() -> Result<(), Box<dyn Error>> {
    let index = template()?;
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &crate::term_mapping::annotated_composition(&index, '?')?,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let mappings = format!("{}/mappings", crate::term_mapping::PROBLEM);
    assert_eq!(
        outbound.warnings(),
        [located(
            Loss::Skipped {
                mapping: String::from("problemDiagnose"),
                reason: SkipReason::TermMappingMatch {
                    match_code: '?',
                    terminology: String::from(crate::term_mapping::BROADER_SYSTEM),
                    code: String::from(crate::term_mapping::BROADER_CODE),
                },
            },
            Some("Condition.code"),
            Some(&mappings),
        )]
    );
    assert_rendered(outbound.warnings());
    assert_no_value(
        &serde_json::json!([
            crate::term_mapping::BROADER_SYSTEM,
            crate::term_mapping::BROADER_CODE,
            "a synthetic problem name",
        ]),
        outbound.warnings(),
    );
    Ok(())
}

/// Renders each warning as its text and its two locations, for a snapshot.
fn rows(warnings: &[Warning]) -> Vec<String> {
    warnings
        .iter()
        .map(|warning| {
            format!(
                "{warning} | {} | {}",
                warning.location().fhir().unwrap_or("-"),
                warning.location().openehr().unwrap_or("-")
            )
        })
        .collect()
}

#[test]
fn the_kds_round_trip_locates_its_one_way_rows_and_its_skips() -> Result<(), Box<dyn Error>> {
    let (program, index) = crate::roundtrip::kds_program()?;
    let input = crate::roundtrip::kds_condition()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(input.clone()),
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    insta::assert_json_snapshot!(
        "kds_located",
        serde_json::json!({
            "first_leg": rows(inbound.warnings()),
            "second_leg": rows(outbound.warnings()),
        })
    );
    assert_rendered(inbound.warnings());
    assert_rendered(outbound.warnings());
    assert_no_value(&input, inbound.warnings());
    assert_no_value(&input, outbound.warnings());
    Ok(())
}
