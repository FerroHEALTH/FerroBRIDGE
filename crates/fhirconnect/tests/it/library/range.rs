// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `dosageQuantityToRange` and `rangeToText`, the two functions that carry
//! an R4 `Range`.

use core::error::Error;

use fhir_types::codec::Path;
use fhirconnect::engine::library::Library;
use fhirconnect::engine::traverse::functions::MappingFunctions;
use fhirconnect::engine::traverse::functions::Ports;
use fhirconnect::engine::traverse::functions::SELF_PORT;
use proptest::prelude::*;

use super::element;
use super::held;
use super::holding;
use super::refused_by;
use super::refused_or_cell;

/// The dose range function.
const DOSE_RANGE: &str = "dosageQuantityToRange";

/// The rate text function.
const RANGE_TEXT: &str = "rangeToText";

/// The class of the dose interval.
const INTERVAL: &str = "DV_INTERVAL<DV_QUANTITY>";

/// Returns a UCUM quantity of `value` in `code`, the form the `DV_QUANTITY`
/// cell reads back.
fn ucum(value: i64, code: &str) -> serde_json::Value {
    serde_json::json!({
        "value": value,
        "unit": code,
        "system": "http://unitsofmeasure.org",
        "code": code
    })
}

/// Runs `code` into openEHR and back.
fn both_ways(
    code: &str,
    range: &serde_json::Value,
) -> Result<(Ports, serde_json::Value), Box<dyn Error>> {
    let ports = Library.to_openehr(code, &element(range.clone()))?;
    let back = Library
        .to_fhir(code, &ports)?
        .ok_or("the range reads back")?
        .to_serde_json(&mut Path::root("Range"))?;
    Ok((ports, back))
}

#[test]
fn a_dose_range_is_an_included_interval_of_quantities() -> Result<(), Box<dyn Error>> {
    // DV_INTERVAL.adoc: low and high are lower and upper, included and bounded.
    let range = serde_json::json!({"low": ucum(1, "mg"), "high": ucum(2, "mg")});
    let (ports, back) = both_ways(DOSE_RANGE, &range)?;
    let interval = held(&ports, SELF_PORT, INTERVAL).ok_or("the interval is written")?;
    assert_eq!(
        interval.get("lower_included"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        interval.get("upper_unbounded"),
        Some(&serde_json::json!(false))
    );
    assert_eq!(
        interval.pointer("/upper/units"),
        Some(&serde_json::json!("mg"))
    );
    assert_eq!(back, range);
    Ok(())
}

#[test]
fn a_dose_range_with_one_bound_is_unbounded_on_the_other_side() -> Result<(), Box<dyn Error>> {
    let range = serde_json::json!({"low": ucum(5, "mg")});
    let (ports, back) = both_ways(DOSE_RANGE, &range)?;
    let interval = held(&ports, SELF_PORT, INTERVAL).ok_or("the interval is written")?;
    assert_eq!(
        interval.get("upper_unbounded"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(back, range);
    Ok(())
}

#[test]
fn a_dose_held_as_one_quantity_is_no_range() -> Result<(), Box<dyn Error>> {
    let ports = holding(
        SELF_PORT,
        "DV_QUANTITY",
        serde_json::json!({"_type": "DV_QUANTITY", "magnitude": 1.0, "units": "mg"}),
    );
    assert_eq!(Library.to_fhir(DOSE_RANGE, &ports)?, None);
    Ok(())
}

#[test]
fn a_dose_range_the_interval_cannot_carry_is_refused() {
    for range in [
        serde_json::json!({"id": "r1", "low": ucum(1, "mg")}),
        serde_json::json!({"id": "r1"}),
    ] {
        let error = Library
            .to_openehr(DOSE_RANGE, &element(range.clone()))
            .expect_err("the range has no interval");
        assert!(refused_by(&error, DOSE_RANGE), "{range}: {error:?}");
    }
}

#[test]
fn an_interval_a_range_cannot_carry_is_refused() {
    let quantity = serde_json::json!({"_type": "DV_QUANTITY", "magnitude": 1.0, "units": "mg"});
    for interval in [
        serde_json::json!({"_type": "DV_INTERVAL", "lower": quantity, "lower_unbounded": false,
            "upper_unbounded": true, "lower_included": false, "upper_included": false}),
        serde_json::json!({"_type": "DV_INTERVAL", "lower_unbounded": false,
            "upper_unbounded": true, "lower_included": false, "upper_included": false}),
        serde_json::json!({"_type": "DV_INTERVAL",
            "lower": {"_type": "DV_QUANTITY", "magnitude": 1.0, "units": "mg", "magnitude_status": "<"},
            "lower_unbounded": false, "upper_unbounded": true, "lower_included": true,
            "upper_included": false}),
    ] {
        let error = Library
            .to_fhir(DOSE_RANGE, &holding(SELF_PORT, INTERVAL, interval.clone()))
            .expect_err("the interval is no Range");
        assert!(refused_or_cell(&error, DOSE_RANGE), "{interval}: {error:?}");
    }
}

#[test]
fn a_rate_range_is_written_as_its_text() -> Result<(), Box<dyn Error>> {
    let range = serde_json::json!({"low": ucum(10, "mL/h"), "high": ucum(20, "mL/h")});
    let (ports, back) = both_ways(RANGE_TEXT, &range)?;
    assert_eq!(
        held(&ports, SELF_PORT, "DV_TEXT").and_then(|text| text.get("value")),
        Some(&serde_json::json!("10 mL/h - 20 mL/h"))
    );
    assert_eq!(back, range);
    Ok(())
}

#[test]
fn a_rate_held_as_a_quantity_is_no_range() -> Result<(), Box<dyn Error>> {
    let ports = holding(
        SELF_PORT,
        "DV_QUANTITY",
        serde_json::json!({"_type": "DV_QUANTITY", "magnitude": 1.0, "units": "mL/h"}),
    );
    assert_eq!(Library.to_fhir(RANGE_TEXT, &ports)?, None);
    Ok(())
}

#[test]
fn a_rate_range_the_text_cannot_carry_is_refused() {
    let mut displayed = ucum(1, "mL/h");
    if let Some(object) = displayed.as_object_mut() {
        object.insert(
            String::from("unit"),
            serde_json::json!("millilitre per hour"),
        );
    }
    let mut local = ucum(2, "mL/h");
    if let Some(object) = local.as_object_mut() {
        object.insert(
            String::from("system"),
            serde_json::json!("http://example.org/units"),
        );
    }
    for range in [
        serde_json::json!({"low": ucum(1, "mL/h")}),
        serde_json::json!({"low": displayed, "high": ucum(2, "mL/h")}),
        serde_json::json!({"low": ucum(1, "mL/h"), "high": local}),
        serde_json::json!({"low": ucum(-1, "mL/h"), "high": ucum(2, "mL/h")}),
    ] {
        let error = Library
            .to_openehr(RANGE_TEXT, &element(range.clone()))
            .expect_err("the text cannot carry the range");
        assert!(refused_by(&error, RANGE_TEXT), "{range}: {error:?}");
    }
}

#[test]
fn a_rate_text_that_is_no_written_range_is_refused() {
    for text in ["slowly", "10 mL/h", "10 mL/h - fast", "ten mL/h - 20 mL/h"] {
        let error = Library
            .to_fhir(
                RANGE_TEXT,
                &holding(
                    SELF_PORT,
                    "DV_TEXT",
                    serde_json::json!({"_type": "DV_TEXT", "value": text}),
                ),
            )
            .expect_err("the text is no range");
        assert!(refused_by(&error, RANGE_TEXT), "{text}: {error:?}");
    }
}

#[test]
fn a_function_with_nothing_at_its_port_writes_nothing() -> Result<(), Box<dyn Error>> {
    assert_eq!(Library.to_fhir(DOSE_RANGE, &Ports::new())?, None);
    assert_eq!(Library.to_fhir(RANGE_TEXT, &Ports::new())?, None);
    Ok(())
}

proptest! {
    #[test]
    fn the_rate_text_satisfies_both_lens_laws(low in 0u32..100_000, span in 0u32..100_000) {
        let high = low.saturating_add(span);
        let range = serde_json::json!({
            "low": ucum(i64::from(low), "mL/h"),
            "high": ucum(i64::from(high), "mL/h")
        });
        let ports = Library.to_openehr(RANGE_TEXT, &element(range.clone()))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let back = Library.to_fhir(RANGE_TEXT, &ports)
            .map_err(|error| TestCaseError::fail(error.to_string()))?
            .ok_or_else(|| TestCaseError::fail("the range reads back"))?;
        let again = Library.to_openehr(RANGE_TEXT, &back)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(
            back.to_serde_json(&mut Path::root("Range"))
                .map_err(|error| TestCaseError::fail(error.to_string()))?,
            range
        );
        prop_assert_eq!(again, ports);
    }

    #[test]
    fn the_dose_range_satisfies_both_lens_laws(low in 0u32..100_000, span in 0u32..100_000) {
        let high = low.saturating_add(span);
        let range = serde_json::json!({
            "low": ucum(i64::from(low), "mg"),
            "high": ucum(i64::from(high), "mg")
        });
        let ports = Library.to_openehr(DOSE_RANGE, &element(range.clone()))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let back = Library.to_fhir(DOSE_RANGE, &ports)
            .map_err(|error| TestCaseError::fail(error.to_string()))?
            .ok_or_else(|| TestCaseError::fail("the range reads back"))?;
        let again = Library.to_openehr(DOSE_RANGE, &back)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(
            back.to_serde_json(&mut Path::root("Range"))
                .map_err(|error| TestCaseError::fail(error.to_string()))?,
            range
        );
        prop_assert_eq!(again, ports);
    }
}
