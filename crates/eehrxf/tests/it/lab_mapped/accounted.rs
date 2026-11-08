// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The received reference range and order of a laboratory report: each maps
//! into the element `KDS_Laborbericht` provides for it, and an instance the
//! run carries into no structured element is declared in its outcome.

use std::error::Error;

use eehrxf::receive::ReceivedDocument;
use serde_json::Value;

use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::run::Settings;

use super::{
    NOW, committed, mapping, only, received_text, received_text_of, report, result, settings,
};

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

/// The synthetic result with one further range per `(meaning, low, high)`,
/// each typed by its code in the R4 `referencerange-meaning` code system.
fn typed_result(ranges: &[(&str, Option<u32>, Option<u32>)]) -> Result<Value, Box<dyn Error>> {
    let quantity = |value: u32| {
        serde_json::json!({
            "value": value,
            "unit": "mg/dL",
            "system": "http://unitsofmeasure.org",
            "code": "mg/dL"
        })
    };
    let mut typed = result();
    let held = typed["referenceRange"]
        .as_array_mut()
        .ok_or("the synthetic ranges")?;
    for &(meaning, low, high) in ranges {
        let mut range = serde_json::json!({"type": {"coding": [{
            "system": REFERENCE_RANGE_MEANING,
            "code": meaning
        }]}});
        if let Some(low) = low {
            range["low"] = quantity(low);
        }
        if let Some(high) = high {
            range["high"] = quantity(high);
        }
        held.push(range);
    }
    Ok(typed)
}

/// The R4 code system of `Observation.referenceRange.type`.
const REFERENCE_RANGE_MEANING: &str =
    "http://terminology.hl7.org/CodeSystem/referencerange-meaning";

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

// NOTE: openEHR RM data_types §DV_ORDERED `other_reference_ranges` and FHIRconnect DV_QUANTITY.adoc
// §Range: a range that is not the normal range is one of the quantity's other reference ranges.
#[test]
fn a_typed_reference_range_is_another_reference_range_of_the_result() -> Result<(), Box<dyn Error>>
{
    let typed = typed_result(&[("treatment", None, Some(160))])?;
    assert_eq!(not_carried(&typed, &report())?, Vec::<String>::new());
    let document = ReceivedDocument::read(&received_text_of(&typed, &report())?)?;
    let composition = mapping()?.to_openehr(&document, &settings())?;
    let canonical: Value = serde_json::from_str(composition.composition())?;
    let measured = quantity_of(&canonical, 182.0).ok_or("the result's quantity")?;
    assert_eq!(
        measured["normal_range"]["upper"]["magnitude"].as_f64(),
        Some(200.0),
        "the treatment range is no normal range: {measured}"
    );
    let others = measured["other_reference_ranges"]
        .as_array()
        .ok_or("the quantity's other reference ranges")?;
    assert_eq!(others.len(), 1, "{measured}");
    let treatment = &others[0];
    // NOTE: RM REFERENCE_RANGE.meaning is a DV_TEXT, and FHIRconnect DV_TEXT.adoc §CodeableConcept
    // carries each coding as a TERM_MAPPING of it.
    let coded = &treatment["meaning"]["mappings"][0]["target"];
    assert_eq!(coded["code_string"], "treatment", "{treatment}");
    assert_eq!(
        coded["terminology_id"]["value"], REFERENCE_RANGE_MEANING,
        "{treatment}"
    );
    assert_eq!(
        treatment["range"]["upper"]["magnitude"].as_f64(),
        Some(160.0),
        "{treatment}"
    );
    assert_eq!(treatment["range"]["upper"]["units"], "mg/dL", "{treatment}");
    assert!(
        treatment["range"].get("lower").is_none_or(Value::is_null),
        "the treatment range has no lower bound: {treatment}"
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

// NOTE: R4 Observation.referenceRange.type: "Codes to indicate what part of the targeted reference
// population it applies to"; each other reference range is served with its meaning as the type.
#[test]
fn each_other_reference_range_of_a_result_is_served_with_its_type() -> Result<(), Box<dyn Error>> {
    let typed = typed_result(&[
        ("treatment", None, Some(160)),
        ("normal", Some(100), Some(190)),
    ])?;
    let document = ReceivedDocument::read(&received_text_of(&typed, &report())?)?;
    let mapping = mapping()?;
    let composition = mapping.to_openehr(&document, &settings())?;
    let canonical: Value = serde_json::from_str(composition.composition())?;
    let measured = quantity_of(&canonical, 182.0).ok_or("the result's quantity")?;
    assert_eq!(
        measured["other_reference_ranges"].as_array().map(Vec::len),
        Some(2),
        "{measured}"
    );
    let (run, warnings) = mapping.to_fhir_with_warnings(
        &committed(composition.composition())?,
        &Settings::new("Device/ferrobridge-eehrxf-test", NOW),
        CallContext::new(),
    )?;
    let bundle = serde_json::to_value(run.bundle())?;
    let served = only(&bundle, "Observation")?;
    let ranges = served["referenceRange"]
        .as_array()
        .ok_or("the served ranges")?;
    assert_eq!(ranges.len(), 3, "{served}");
    let normal = &ranges[0];
    assert!(normal.get("type").is_none(), "{normal}");
    assert_eq!(normal["low"]["value"], 120, "{normal}");
    assert_eq!(normal["high"]["value"], 200, "{normal}");
    for (range, (meaning, low, high)) in ranges[1..]
        .iter()
        .zip([("treatment", None, 160), ("normal", Some(100), 190)])
    {
        assert_eq!(
            range["type"]["coding"][0],
            serde_json::json!({"system": REFERENCE_RANGE_MEANING, "code": meaning}),
            "{range}"
        );
        assert_eq!(range["high"]["value"], high, "{range}");
        assert_eq!(range["high"]["unit"], "mg/dL", "{range}");
        match low {
            Some(low) => assert_eq!(range["low"]["value"], low, "{range}"),
            None => assert!(range.get("low").is_none(), "{range}"),
        }
    }
    let declared: Vec<String> = warnings
        .iter()
        .filter_map(|warning| warning.location().fhir().map(String::from))
        .filter(|path| path.starts_with("Observation.referenceRange"))
        .collect();
    assert_eq!(declared, Vec::<String>::new(), "{warnings:?}");
    Ok(())
}
