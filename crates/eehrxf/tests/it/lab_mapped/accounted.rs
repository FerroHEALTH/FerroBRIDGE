// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The received reference range and order of a laboratory report: each maps
//! into the element `KDS_Laborbericht` provides for it, and an instance the
//! run carries into no structured element is declared in its outcome.

use std::error::Error;

use eehrxf::receive::ReceivedDocument;
use serde_json::Value;

use super::{mapping, received_text, received_text_of, report, result, settings};

/// The element paths the run's outcome declares not carried into the
/// composition, mapping the report `report` of the result `result`.
fn not_carried(result: &Value, report: &Value) -> Result<Vec<String>, Box<dyn Error>> {
    let document = ReceivedDocument::read(&received_text_of(result, report)?)?;
    let composition = mapping()?.to_openehr(&document, &settings())?;
    let Some(outcome) = composition.outcome() else {
        return Ok(Vec::new());
    };
    let outcome = serde_json::to_value(outcome)?;
    Ok(outcome["issue"]
        .as_array()
        .ok_or("the outcome's issues")?
        .iter()
        .filter(|issue| {
            issue["severity"] == "warning"
                && issue["code"] == "incomplete"
                && issue["details"]["text"]
                    .as_str()
                    .is_some_and(|text| text.contains("no structured element"))
        })
        .filter_map(|issue| issue["expression"][0].as_str().map(String::from))
        .collect())
}

/// Returns the `DV_QUANTITY` of the canonical composition `value` whose
/// magnitude is `magnitude`.
fn quantity_of(value: &Value, magnitude: f64) -> Option<&Value> {
    match value {
        Value::Object(members) => {
            if value["_type"] == "DV_QUANTITY"
                && value["magnitude"]
                    .as_f64()
                    .is_some_and(|held| held.to_bits() == magnitude.to_bits())
            {
                return Some(value);
            }
            members
                .values()
                .find_map(|member| quantity_of(member, magnitude))
        }
        Value::Array(items) => items.iter().find_map(|item| quantity_of(item, magnitude)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

// NOTE: openEHR RM data_types §DV_ORDERED `normal_range`, and R4 Observation.referenceRange.type:
// "If this element is not present then the normal range is assumed".
#[test]
fn a_received_reference_range_is_the_normal_range_of_the_result() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(&received_text()?)?;
    let composition = mapping()?.to_openehr(&document, &settings())?;
    let canonical: Value = serde_json::from_str(composition.composition())?;
    let measured = quantity_of(&canonical, 182.0).ok_or("the result's quantity")?;
    for (bound, magnitude) in [("lower", 120.0), ("upper", 200.0)] {
        assert_eq!(
            measured["normal_range"][bound]["magnitude"].as_f64(),
            Some(magnitude),
            "{bound}: {measured}"
        );
        assert_eq!(
            measured["normal_range"][bound]["units"], "mg/dL",
            "{measured}"
        );
    }
    assert_eq!(
        measured["normal_range"]["lower_included"], true,
        "{measured}"
    );
    Ok(())
}

#[test]
fn a_received_order_identifier_is_the_order_the_template_holds() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(&received_text()?)?;
    let composition = mapping()?.to_openehr(&document, &settings())?;
    let canonical: Value = serde_json::from_str(composition.composition())?;
    let protocol = &canonical["content"][0]["protocol"];
    assert!(
        protocol.to_string().contains("synthetic-order-0001"),
        "the order identifier is in the protocol: {protocol}"
    );
    assert_eq!(not_carried(&result(), &report())?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_typed_reference_range_is_declared_not_carried() -> Result<(), Box<dyn Error>> {
    let mut typed = result();
    let ranges = typed["referenceRange"]
        .as_array_mut()
        .ok_or("the synthetic ranges")?;
    ranges.push(serde_json::json!({
        "type": {"coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/referencerange-meaning",
            "code": "treatment"
        }]},
        "high": {
            "value": 160,
            "unit": "mg/dL",
            "system": "http://unitsofmeasure.org",
            "code": "mg/dL"
        }
    }));
    assert_eq!(
        not_carried(&typed, &report())?,
        ["Observation.referenceRange[1]"]
    );
    let document = ReceivedDocument::read(&received_text_of(&typed, &report())?)?;
    let composition = mapping()?.to_openehr(&document, &settings())?;
    let canonical: Value = serde_json::from_str(composition.composition())?;
    let measured = quantity_of(&canonical, 182.0).ok_or("the result's quantity")?;
    assert_eq!(
        measured["normal_range"]["upper"]["magnitude"].as_f64(),
        Some(200.0),
        "the treatment range is no normal range: {measured}"
    );
    Ok(())
}

#[test]
fn a_reference_range_text_is_declared_not_carried() -> Result<(), Box<dyn Error>> {
    let mut described = result();
    described["referenceRange"][0]["text"] = Value::from("Synthetic range text.");
    assert_eq!(
        not_carried(&described, &report())?,
        ["Observation.referenceRange[0].text"]
    );
    Ok(())
}

#[test]
fn the_order_a_result_answers_is_declared_not_carried() -> Result<(), Box<dyn Error>> {
    let mut ordered = result();
    ordered["basedOn"] = serde_json::json!([{"reference": "ServiceRequest/synthetic-order"}]);
    assert_eq!(
        not_carried(&ordered, &report())?,
        ["Observation.basedOn[0]"]
    );
    Ok(())
}

#[test]
fn an_order_named_by_reference_alone_is_declared_not_carried() -> Result<(), Box<dyn Error>> {
    let mut referenced = report();
    referenced["basedOn"] = serde_json::json!([{"reference": "ServiceRequest/synthetic-order"}]);
    assert_eq!(
        not_carried(&result(), &referenced)?,
        ["DiagnosticReport.basedOn[0].reference"]
    );
    Ok(())
}
