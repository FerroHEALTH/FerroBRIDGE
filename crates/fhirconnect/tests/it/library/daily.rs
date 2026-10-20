// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `timingToDaily`, between `Timing.repeat` and the daily timing cluster.

use core::error::Error;

use fhir_types::codec::Path;
use fhirconnect::engine::library::Library;
use fhirconnect::engine::traverse::functions::MappingFunctions;
use fhirconnect::engine::traverse::functions::Ports;
use proptest::prelude::*;

use super::element;
use super::held;
use super::holding;
use super::refused_by;
use super::written;

/// The name of the function.
const DAILY: &str = "timingToDaily";

/// The frequency port.
const FREQUENCY: &str = "items[at0003]";

/// The interval port.
const INTERVAL: &str = "items[at0014]";

/// The time of day port.
const TIME_OF_DAY: &str = "items[at0004]";

/// Runs the function into openEHR and back, returning both sides.
fn both_ways(repeat: &serde_json::Value) -> Result<(Ports, serde_json::Value), Box<dyn Error>> {
    let ports = Library.to_openehr(DAILY, &element(repeat.clone()))?;
    let back = written(Library.to_fhir(DAILY, &ports)?)?;
    Ok((ports, back))
}

/// Returns the frequency quantity JSON of `magnitude` per `units`.
fn frequency(magnitude: u32, units: &str) -> serde_json::Value {
    serde_json::json!({"_type": "DV_QUANTITY", "magnitude": magnitude, "units": units, "precision": 0})
}

#[test]
fn a_frequency_per_one_unit_is_the_frequency_quantity() -> Result<(), Box<dyn Error>> {
    let repeat = serde_json::json!({"frequency": 3, "period": 1, "periodUnit": "d"});
    let (ports, back) = both_ways(&repeat)?;
    let quantity = held(&ports, FREQUENCY, "DV_QUANTITY").ok_or("the frequency is written")?;
    assert_eq!(quantity.get("units"), Some(&serde_json::json!("1/d")));
    assert_eq!(
        quantity
            .get("magnitude")
            .and_then(serde_json::Value::as_f64),
        Some(3.0)
    );
    assert_eq!(back, repeat);
    Ok(())
}

#[test]
fn a_range_of_frequencies_is_an_interval_of_quantities() -> Result<(), Box<dyn Error>> {
    let repeat =
        serde_json::json!({"frequency": 2, "frequencyMax": 4, "period": 1, "periodUnit": "h"});
    let (ports, back) = both_ways(&repeat)?;
    let range = held(&ports, FREQUENCY, "DV_INTERVAL<DV_QUANTITY>")
        .ok_or("the range of frequencies is written")?;
    assert_eq!(
        range
            .pointer("/upper/magnitude")
            .and_then(serde_json::Value::as_f64),
        Some(4.0)
    );
    assert_eq!(back, repeat);
    Ok(())
}

#[test]
fn one_event_per_period_of_more_than_one_unit_is_the_interval() -> Result<(), Box<dyn Error>> {
    let repeat = serde_json::json!({"frequency": 1, "period": 8, "periodUnit": "h"});
    let (ports, back) = both_ways(&repeat)?;
    assert_eq!(
        held(&ports, INTERVAL, "DV_DURATION").and_then(|duration| duration.get("value")),
        Some(&serde_json::json!("PT8H"))
    );
    assert_eq!(back, repeat);
    Ok(())
}

#[test]
fn the_time_of_day_is_the_time() -> Result<(), Box<dyn Error>> {
    let repeat = serde_json::json!({"timeOfDay": ["08:30:00"]});
    let (ports, back) = both_ways(&repeat)?;
    assert_eq!(
        held(&ports, TIME_OF_DAY, "DV_TIME").and_then(|time| time.get("value")),
        Some(&serde_json::json!("08:30:00"))
    );
    assert_eq!(back, repeat);
    Ok(())
}

#[test]
fn a_frequency_with_no_frequency_counts_once_per_period() -> Result<(), Box<dyn Error>> {
    // R4 Timing.repeat.frequency.meaningWhenMissing: "once per period".
    let ports = Library.to_openehr(
        DAILY,
        &element(serde_json::json!({"period": 1, "periodUnit": "d"})),
    )?;
    assert_eq!(
        held(&ports, FREQUENCY, "DV_QUANTITY").and_then(|quantity| quantity.get("units")),
        Some(&serde_json::json!("1/d"))
    );
    Ok(())
}

#[test]
fn a_timing_the_daily_cluster_has_no_place_for_is_refused() {
    for repeat in [
        serde_json::json!({"frequency": 1, "period": 1, "periodMax": 2, "periodUnit": "h"}),
        serde_json::json!({"frequency": 2}),
        serde_json::json!({"frequency": 2, "period": 1}),
        serde_json::json!({"frequency": 1, "period": 1, "periodUnit": "wk"}),
        serde_json::json!({"frequency": 2, "period": 8, "periodUnit": "h"}),
        serde_json::json!({"frequency": 1, "frequencyMax": 2, "period": 8, "periodUnit": "h"}),
        serde_json::json!({"timeOfDay": ["08:00:00", "20:00:00"]}),
    ] {
        let error = Library
            .to_openehr(DAILY, &element(repeat.clone()))
            .expect_err("the repeat has no daily timing");
        assert!(refused_by(&error, DAILY), "{repeat}: {error:?}");
    }
}

#[test]
fn a_daily_timing_with_no_timing_repeat_form_is_refused() {
    let both = holding(FREQUENCY, "DV_QUANTITY", frequency(3, "1/d")).with(
        INTERVAL,
        fhirconnect::engine::traverse::functions::Datum::new(
            "DV_DURATION",
            serde_json::json!({"_type": "DV_DURATION", "value": "PT8H"}),
        ),
    );
    for ports in [
        both,
        holding(FREQUENCY, "DV_QUANTITY", frequency(3, "1/wk")),
        holding(
            FREQUENCY,
            "DV_QUANTITY",
            serde_json::json!({"_type": "DV_QUANTITY", "magnitude": 2.5, "units": "1/d", "precision": 1}),
        ),
        holding(
            TIME_OF_DAY,
            "DV_TIME",
            serde_json::json!({"_type": "DV_TIME", "value": "08:00"}),
        ),
        holding(
            INTERVAL,
            "DV_DURATION",
            serde_json::json!({"_type": "DV_DURATION", "value": "P2W"}),
        ),
        holding(
            FREQUENCY,
            "DV_INTERVAL<DV_QUANTITY>",
            serde_json::json!({
                "_type": "DV_INTERVAL",
                "lower": frequency(2, "1/d"),
                "lower_unbounded": false,
                "upper_unbounded": true,
                "lower_included": true,
                "upper_included": false
            }),
        ),
    ] {
        let error = Library
            .to_fhir(DAILY, &ports)
            .expect_err("the cluster has no Timing.repeat form");
        assert!(refused_by(&error, DAILY), "{ports:?}: {error:?}");
    }
}

#[test]
fn an_empty_daily_timing_writes_nothing() -> Result<(), Box<dyn Error>> {
    assert_eq!(Library.to_fhir(DAILY, &Ports::new())?, None);
    Ok(())
}

/// The daily units of time.
const DAILY_UNITS: [&str; 4] = ["s", "min", "h", "d"];

proptest! {
    #[test]
    fn the_daily_timing_satisfies_both_lens_laws(
        events in 1u32..1_000,
        period in 2u32..1_000,
        unit in 0usize..DAILY_UNITS.len(),
        per_unit in proptest::bool::ANY,
    ) {
        let ucum = DAILY_UNITS[unit];
        let repeat = if per_unit {
            serde_json::json!({"frequency": events, "period": 1, "periodUnit": ucum})
        } else {
            serde_json::json!({"frequency": 1, "period": period, "periodUnit": ucum})
        };
        // PutGet: FHIR into openEHR and back reads the element it started from.
        let ports = Library.to_openehr(DAILY, &element(repeat.clone()))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let back = Library.to_fhir(DAILY, &ports)
            .map_err(|error| TestCaseError::fail(error.to_string()))?
            .ok_or_else(|| TestCaseError::fail("the timing reads back"))?;
        prop_assert_eq!(
            back.to_serde_json(&mut Path::root("Timing.repeat"))
                .map_err(|error| TestCaseError::fail(error.to_string()))?,
            repeat
        );
        // GetPut: openEHR out to FHIR and back holds the ports it started from.
        let again = Library.to_openehr(DAILY, &back)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(again, ports);
    }
}
