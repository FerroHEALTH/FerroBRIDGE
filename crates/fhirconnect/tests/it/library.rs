// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `mappingCode` functions the crate registers, each driven in both
//! directions through the registry the engine is handed.

use core::error::Error;

use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhirconnect::engine::library::Library;
use fhirconnect::engine::traverse::functions::Datum;
use fhirconnect::engine::traverse::functions::MappingCodeError;
use fhirconnect::engine::traverse::functions::MappingFunctions;
use fhirconnect::engine::traverse::functions::Ports;
use fhirconnect::engine::traverse::functions::SELF_PORT;
use fhirconnect::model::semantic::MappingCodeRegistry;
use proptest::prelude::*;

/// The name of the administration duration function.
const DURATION: &str = "dosageDurationToAdministrationDuration";

/// The name of the non-daily timing function.
const NON_DAILY: &str = "timingNonDaily";

/// The interval port of the non-daily timing.
const NON_DAILY_PERIOD: &str = "items[at0002]";

/// Returns a FHIR element from its JSON.
fn element(json: serde_json::Value) -> Value {
    Value::from_serde_json(json)
}

/// Returns the ports holding one `DV_DURATION` at `port`.
fn duration_at(port: &str, iso: &str) -> Ports {
    Ports::new().with(
        port,
        Datum::new(
            "DV_DURATION",
            serde_json::json!({"_type": "DV_DURATION", "value": iso}),
        ),
    )
}

/// Returns the ISO value of the `DV_DURATION` at `port`.
fn iso_at<'ports>(ports: &'ports Ports, port: &str) -> Option<&'ports str> {
    ports
        .get(port)
        .filter(|datum| datum.class() == "DV_DURATION")
        .and_then(|datum| datum.value().get("value"))
        .and_then(serde_json::Value::as_str)
}

/// Returns the FHIR element a function wrote, as JSON.
fn written(produced: Option<Value>) -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(produced
        .ok_or("the function wrote an element")?
        .to_serde_json(&mut Path::root("Timing.repeat"))?)
}

/// Returns whether a refusal is the function's own, naming `code`.
fn refused_by(error: &MappingCodeError, code: &str) -> bool {
    matches!(*error, MappingCodeError::Refused { code: ref named, .. } if named == code)
}

#[test]
fn the_library_registers_each_function_with_its_ports() {
    assert_eq!(
        Library::codes().collect::<Vec<_>>(),
        [DURATION, NON_DAILY],
        "the library holds the functions it implements, in name order"
    );
    assert!(Library.contains(DURATION));
    assert_eq!(Library.ports(DURATION), Some(&[SELF_PORT][..]));
    assert_eq!(Library.ports(NON_DAILY), Some(&[NON_DAILY_PERIOD][..]));
    assert!(!Library.contains("toDaily"));
    assert_eq!(Library.ports("toDaily"), None);
}

#[test]
fn a_function_the_library_does_not_hold_is_refused_by_name() {
    let error = Library
        .to_fhir("toDaily", &Ports::new())
        .expect_err("the library holds no `toDaily`");
    assert_eq!(
        error,
        MappingCodeError::Unknown {
            code: String::from("toDaily")
        }
    );
}

#[test]
fn the_administration_duration_is_written_as_its_iso_duration() -> Result<(), Box<dyn Error>> {
    // DV_DURATION.adoc: `min` is `T00H00M`, so thirty minutes is `PT30M`.
    let ports = Library.to_openehr(
        DURATION,
        &element(serde_json::json!({"duration": 30, "durationUnit": "min"})),
    )?;
    assert_eq!(iso_at(&ports, SELF_PORT), Some("PT30M"));
    Ok(())
}

#[test]
fn the_administration_duration_reads_back_as_its_amount_and_unit() -> Result<(), Box<dyn Error>> {
    let produced = Library.to_fhir(DURATION, &duration_at(SELF_PORT, "P3D"))?;
    assert_eq!(
        written(produced)?,
        serde_json::json!({"duration": 3, "durationUnit": "d"})
    );
    Ok(())
}

#[test]
fn a_timing_with_no_duration_writes_no_administration_duration() -> Result<(), Box<dyn Error>> {
    let ports = Library.to_openehr(DURATION, &element(serde_json::json!({"count": 2})))?;
    assert!(ports.is_empty());
    assert_eq!(Library.to_fhir(DURATION, &Ports::new())?, None);
    Ok(())
}

#[test]
fn an_administration_duration_with_no_place_in_openehr_is_refused() {
    for repeat in [
        serde_json::json!({"duration": 30, "durationMax": 40, "durationUnit": "min"}),
        serde_json::json!({"duration": 30}),
        serde_json::json!({"duration": 30, "durationUnit": "ms"}),
        serde_json::json!({"duration": 1.5, "durationUnit": "h"}),
        serde_json::json!({"duration": -1, "durationUnit": "h"}),
        serde_json::json!({"duration": 30, "durationUnit": "min",
            "_duration": {"extension": [{"url": "http://example.org/x", "valueString": "x"}]}}),
    ] {
        let error = Library
            .to_openehr(DURATION, &element(repeat.clone()))
            .expect_err("the repeat has no administration duration");
        assert!(refused_by(&error, DURATION), "{repeat}: {error:?}");
    }
}

#[test]
fn an_element_that_is_no_timing_repeat_is_refused_as_a_decode() {
    let error = Library
        .to_openehr(DURATION, &element(serde_json::json!({"unknown": 1})))
        .expect_err("`unknown` is no member of Timing.repeat");
    assert!(
        matches!(
            error,
            MappingCodeError::FhirDecode {
                element: "Timing.repeat",
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn an_administration_duration_of_two_components_reads_in_the_smallest() -> Result<(), Box<dyn Error>>
{
    // As the DV_DURATION cell reads it: one hour and thirty minutes is ninety minutes.
    let produced = Library.to_fhir(DURATION, &duration_at(SELF_PORT, "PT1H30M"))?;
    assert_eq!(
        written(produced)?,
        serde_json::json!({"duration": 90, "durationUnit": "min"})
    );
    Ok(())
}

#[test]
fn an_administration_duration_with_no_single_unit_is_refused() {
    let error = Library
        .to_fhir(DURATION, &duration_at(SELF_PORT, "P1M2D"))
        .expect_err("months and days share no unit");
    assert!(
        matches!(error, MappingCodeError::Cell { ref code, .. } if code == DURATION),
        "{error:?}"
    );
}

#[test]
fn a_port_value_that_is_no_duration_is_refused_as_a_decode() {
    let ports = Ports::new().with(
        SELF_PORT,
        Datum::new("DV_DURATION", serde_json::json!({"_type": "DV_DURATION"})),
    );
    let error = Library
        .to_fhir(DURATION, &ports)
        .expect_err("a DV_DURATION carries a value");
    assert!(
        matches!(error, MappingCodeError::RmDecode { ref port, .. } if port.is_empty()),
        "{error:?}"
    );
}

#[test]
fn a_non_daily_timing_is_written_as_its_interval() -> Result<(), Box<dyn Error>> {
    for repeat in [
        serde_json::json!({"frequency": 1, "period": 2, "periodUnit": "wk"}),
        serde_json::json!({"period": 2, "periodUnit": "wk"}),
    ] {
        let ports = Library.to_openehr(NON_DAILY, &element(repeat.clone()))?;
        assert_eq!(iso_at(&ports, NON_DAILY_PERIOD), Some("P2W"), "{repeat}");
    }
    Ok(())
}

#[test]
fn a_non_daily_interval_reads_back_as_once_per_period() -> Result<(), Box<dyn Error>> {
    // R4 Timing.repeat.frequency: with no frequency "the event occurs once per
    // period, but systems SHOULD always be specific about this".
    for (iso, period, unit) in [("P2W", 2, "wk"), ("P3M", 3, "mo"), ("P1Y", 1, "a")] {
        let produced = Library.to_fhir(NON_DAILY, &duration_at(NON_DAILY_PERIOD, iso))?;
        assert_eq!(
            written(produced)?,
            serde_json::json!({"frequency": 1, "period": period, "periodUnit": unit})
        );
    }
    Ok(())
}

#[test]
fn a_timing_that_is_not_once_per_period_or_is_daily_is_refused() {
    for repeat in [
        serde_json::json!({"frequency": 3, "period": 1, "periodUnit": "wk"}),
        serde_json::json!({"frequency": 1, "frequencyMax": 2, "period": 1, "periodUnit": "wk"}),
        serde_json::json!({"period": 1, "periodMax": 2, "periodUnit": "wk"}),
        serde_json::json!({"period": 1}),
        serde_json::json!({"period": 8, "periodUnit": "h"}),
        serde_json::json!({"period": 1, "periodUnit": "d"}),
    ] {
        let error = Library
            .to_openehr(NON_DAILY, &element(repeat.clone()))
            .expect_err("the repeat has no non-daily interval");
        assert!(refused_by(&error, NON_DAILY), "{repeat}: {error:?}");
    }
}

#[test]
fn a_daily_interval_is_refused_going_out() {
    let error = Library
        .to_fhir(NON_DAILY, &duration_at(NON_DAILY_PERIOD, "PT8H"))
        .expect_err("eight hours is a daily interval");
    assert!(refused_by(&error, NON_DAILY), "{error:?}");
}

/// The UCUM units of time with the ISO designator each renders.
const UNITS: [(&str, &str); 7] = [
    ("s", "PT{}S"),
    ("min", "PT{}M"),
    ("h", "PT{}H"),
    ("d", "P{}D"),
    ("wk", "P{}W"),
    ("mo", "P{}M"),
    ("a", "P{}Y"),
];

proptest! {
    #[test]
    fn the_administration_duration_satisfies_both_lens_laws(
        amount in 0u32..100_000,
        unit in 0usize..UNITS.len(),
    ) {
        let (ucum, pattern) = UNITS[unit];
        let repeat = serde_json::json!({"duration": amount, "durationUnit": ucum});
        // PutGet: FHIR into openEHR and back reads the element it started from.
        let ports = Library.to_openehr(DURATION, &element(repeat.clone()))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let back = Library.to_fhir(DURATION, &ports)
            .map_err(|error| TestCaseError::fail(error.to_string()))?
            .ok_or_else(|| TestCaseError::fail("the duration reads back"))?
            .to_serde_json(&mut Path::root("Timing.repeat"))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(back, repeat);
        // GetPut: openEHR out to FHIR and back holds the duration it started from.
        let iso = pattern.replace("{}", &amount.to_string());
        let out = Library.to_fhir(DURATION, &duration_at(SELF_PORT, &iso))
            .map_err(|error| TestCaseError::fail(error.to_string()))?
            .ok_or_else(|| TestCaseError::fail("the duration reads out"))?;
        let again = Library.to_openehr(DURATION, &out)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(iso_at(&again, SELF_PORT), Some(iso.as_str()));
    }

    #[test]
    fn the_non_daily_interval_satisfies_both_lens_laws(
        amount in 1u32..1_000,
        unit in 4usize..UNITS.len(),
    ) {
        let (ucum, pattern) = UNITS[unit];
        let repeat = serde_json::json!({"frequency": 1, "period": amount, "periodUnit": ucum});
        let ports = Library.to_openehr(NON_DAILY, &element(repeat.clone()))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let back = Library.to_fhir(NON_DAILY, &ports)
            .map_err(|error| TestCaseError::fail(error.to_string()))?
            .ok_or_else(|| TestCaseError::fail("the interval reads back"))?
            .to_serde_json(&mut Path::root("Timing.repeat"))
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(back, repeat);
        let iso = pattern.replace("{}", &amount.to_string());
        let out = Library.to_fhir(NON_DAILY, &duration_at(NON_DAILY_PERIOD, &iso))
            .map_err(|error| TestCaseError::fail(error.to_string()))?
            .ok_or_else(|| TestCaseError::fail("the interval reads out"))?;
        let again = Library.to_openehr(NON_DAILY, &out)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(iso_at(&again, NON_DAILY_PERIOD), Some(iso.as_str()));
    }
}
