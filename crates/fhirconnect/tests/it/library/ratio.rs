// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `ratio_to_dosage` and `ratio_to_dosage_action`, a rate `Ratio` against the
//! rate and the administration duration.

use core::error::Error;

use fhir_types::codec::Path;
use fhirconnect::engine::library::Library;
use fhirconnect::engine::traverse::functions::Datum;
use fhirconnect::engine::traverse::functions::MappingFunctions;
use fhirconnect::engine::traverse::functions::Ports;
use proptest::prelude::*;

use super::element;
use super::held;
use super::holding;
use super::refused_by;

/// The function over `Dosage.doseAndRate.rate[x]`.
const DOSAGE_RATE: &str = "ratio_to_dosage";

/// The function over `MedicationAdministration.dosage.rate[x]`.
const ACTION_RATE: &str = "ratio_to_dosage_action";

/// The rate port.
const RATE: &str = "items[at0134]";

/// The duration port.
const DURATION: &str = "items[at0102]";

/// Returns a UCUM quantity of `value` in `code`.
fn ucum(value: u64, code: &str) -> serde_json::Value {
    serde_json::json!({
        "value": value,
        "unit": code,
        "system": "http://unitsofmeasure.org",
        "code": code
    })
}

/// Returns the ratio of `amount` `units` over `time` `time_units`.
fn ratio(amount: u64, units: &str, time: u64, time_units: &str) -> serde_json::Value {
    serde_json::json!({"numerator": ucum(amount, units), "denominator": ucum(time, time_units)})
}

/// Returns the ports holding a rate of `magnitude` `units` and the duration
/// `iso`.
fn rate_and_duration(magnitude: f64, units: &str, iso: &str) -> Ports {
    holding(
        RATE,
        "DV_QUANTITY",
        serde_json::json!({"_type": "DV_QUANTITY", "magnitude": magnitude, "units": units, "precision": 0}),
    )
    .with(
        DURATION,
        Datum::new(
            "DV_DURATION",
            serde_json::json!({"_type": "DV_DURATION", "value": iso}),
        ),
    )
}

#[test]
fn an_amount_over_a_time_is_the_rate_and_the_duration() -> Result<(), Box<dyn Error>> {
    // R4 Dosage.doseAndRate.rate[x]: "500ml over 2 hours" is a rateRatio.
    let rate = ratio(500, "mL", 2, "h");
    for code in [DOSAGE_RATE, ACTION_RATE] {
        let ports = Library.to_openehr(code, &element(rate.clone()))?;
        let quantity = held(&ports, RATE, "DV_QUANTITY").ok_or("the rate is written")?;
        assert_eq!(quantity.get("units"), Some(&serde_json::json!("mL/h")));
        assert_eq!(
            quantity
                .get("magnitude")
                .and_then(serde_json::Value::as_f64),
            Some(250.0)
        );
        assert_eq!(
            held(&ports, DURATION, "DV_DURATION").and_then(|duration| duration.get("value")),
            Some(&serde_json::json!("PT2H"))
        );
        let back = Library
            .to_fhir(code, &ports)?
            .ok_or("the ratio reads back")?
            .to_serde_json(&mut Path::root("Ratio"))?;
        assert_eq!(back, rate);
    }
    Ok(())
}

#[test]
fn a_ratio_with_no_exact_rate_or_no_time_is_refused() {
    for rate in [
        ratio(100, "mL", 3, "h"),
        ratio(100, "mL", 2, "mg"),
        ratio(100, "mL", 0, "h"),
        serde_json::json!({"numerator": ucum(100, "mL")}),
        serde_json::json!({"id": "r1", "numerator": ucum(100, "mL"), "denominator": ucum(2, "h")}),
    ] {
        let error = Library
            .to_openehr(DOSAGE_RATE, &element(rate.clone()))
            .expect_err("the ratio has no rate and duration");
        assert!(refused_by(&error, DOSAGE_RATE), "{rate}: {error:?}");
    }
}

#[test]
fn a_rate_and_duration_with_no_ratio_form_are_refused() {
    let text = holding(
        RATE,
        "DV_TEXT",
        serde_json::json!({"_type": "DV_TEXT", "value": "slowly"}),
    )
    .with(
        DURATION,
        Datum::new(
            "DV_DURATION",
            serde_json::json!({"_type": "DV_DURATION", "value": "PT2H"}),
        ),
    );
    for ports in [
        rate_and_duration(250.0, "mL/min", "PT2H"),
        rate_and_duration(250.0, "mL", "PT2H"),
        rate_and_duration(250.0, "mL/h", "PT1H30M"),
        text,
    ] {
        let error = Library
            .to_fhir(DOSAGE_RATE, &ports)
            .expect_err("the rate and duration have no Ratio");
        assert!(refused_by(&error, DOSAGE_RATE), "{ports:?}: {error:?}");
    }
}

#[test]
fn a_rate_with_no_duration_writes_no_ratio() -> Result<(), Box<dyn Error>> {
    let ports = holding(
        RATE,
        "DV_QUANTITY",
        serde_json::json!({"_type": "DV_QUANTITY", "magnitude": 250.0, "units": "mL/h"}),
    );
    assert_eq!(Library.to_fhir(DOSAGE_RATE, &ports)?, None);
    Ok(())
}

proptest! {
    #[test]
    fn the_rate_ratio_satisfies_both_lens_laws(rate in 1u64..10_000, time in 1u64..100) {
        let amount = rate.saturating_mul(time);
        let view = ratio(amount, "mL", time, "h");
        let ports = Library.to_openehr(DOSAGE_RATE, &element(view.clone()))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let back = Library.to_fhir(DOSAGE_RATE, &ports)
            .map_err(|error| TestCaseError::fail(error.to_string()))?
            .ok_or_else(|| TestCaseError::fail("the ratio reads back"))?;
        let again = Library.to_openehr(DOSAGE_RATE, &back)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(
            back.to_serde_json(&mut Path::root("Ratio"))
                .map_err(|error| TestCaseError::fail(error.to_string()))?,
            view
        );
        prop_assert_eq!(again, ports);
    }
}
