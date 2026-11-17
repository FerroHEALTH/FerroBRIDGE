// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `timingToDaily` against a template whose time of day repeats.
//!
//! The CKM daily timing cluster gives its time of day (`at0004`)
//! `occurrences matches {0..*}`, and R4 `Timing.repeat.timeOfDay` is `0..*`
//! (<https://hl7.org/fhir/R4/datatypes.html#Timing>), so the function
//! declares the port repeating and the engine carries one time per instance
//! both ways (#1972). The template is FerroBRIDGE's medication template over
//! the CKM archetypes; the mapping is `ferrobridge_daily_timing`.

use core::error::Error;

use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::library::Library;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::functions::Datum;
use fhirconnect::engine::traverse::functions::MappingCodeError;
use fhirconnect::engine::traverse::functions::MappingFunctions;
use fhirconnect::engine::traverse::functions::Ports;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::semantic::MappingCodeRegistry;
use fhirconnect::resolve::program::mapping::Method;

use crate::eu_mpd::medication_template;
use crate::laws;
use crate::support::compiled_with;

/// The FLAT key of the time of day of the first dosage, its instance index
/// left open (Simplified Formats, §Flat format).
const TIME_OF_DAY: &str =
    "medication/medication_order/order:0/therapeutic_direction/dosage/daily_timing/specific_time:";

/// Returns a synthetic `MedicationRequest` whose one dosage instruction is
/// taken at `times`.
fn request(times: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "resourceType": "MedicationRequest",
        "status": "active",
        "intent": "order",
        "subject": {"reference": "Patient/synthetic"},
        "medicationCodeableConcept": {"text": "Synthetic medication"},
        "dosageInstruction": [{"timing": {"repeat": {"timeOfDay": times}}}]
    })
}

#[test]
fn the_time_of_day_port_compiles_as_repeating() -> Result<(), Box<dyn Error>> {
    let program = compiled_with(
        "ferrobridge_daily_timing",
        &medication_template()?,
        &Library,
    )?;
    let repeating: Vec<(&str, bool)> = program
        .mappings()
        .iter()
        .filter_map(|mapping| match mapping.method() {
            Method::Programmed { ports, .. } => Some(ports),
            _ => None,
        })
        .flatten()
        .map(|port| (port.name(), port.repeats()))
        .collect();
    assert_eq!(
        repeating,
        [
            ("items[at0003]", false),
            ("items[at0014]", false),
            ("items[at0004]", true)
        ]
    );
    Ok(())
}

#[test]
fn two_times_of_day_go_in_as_two_instances_and_come_back() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let program = compiled_with("ferrobridge_daily_timing", &index, &Library)?;
    let resource = request(&["08:00:00", "20:00:00"]);
    let seams = Seams::default().with_functions(&Library);
    let composition = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(resource.clone()),
        &seams,
        &laws::defaults(),
        &CallContext::new(),
    )?
    .into_value();
    let flat = index.flatten(&composition)?;
    let times: Vec<(&String, &serde_json::Value)> = flat
        .iter()
        .filter(|(key, _)| key.starts_with(TIME_OF_DAY))
        .collect();
    assert_eq!(
        times,
        [
            (
                &format!("{TIME_OF_DAY}0/time_value"),
                &serde_json::json!("08:00:00")
            ),
            (
                &format!("{TIME_OF_DAY}1/time_value"),
                &serde_json::json!("20:00:00")
            )
        ],
        "{flat:?}"
    );
    let out = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &seams,
        &CallContext::new(),
    )?;
    let back = out
        .value()
        .to_serde_json(&mut Path::root("MedicationRequest"))?;
    assert_eq!(
        back.pointer("/dosageInstruction/0/timing/repeat/timeOfDay"),
        Some(&serde_json::json!(["08:00:00", "20:00:00"])),
        "{back}"
    );
    Ok(())
}

/// A registry whose `timingToDaily` declares only the frequency port, which
/// holds one value, and writes two values there.
#[derive(Debug)]
struct TwoFrequencies;

impl MappingCodeRegistry for TwoFrequencies {
    fn contains(&self, code: &str) -> bool {
        code == "timingToDaily"
    }

    fn ports(&self, _code: &str) -> Option<&'static [&'static str]> {
        Some(&["items[at0003]"])
    }
}

impl MappingFunctions for TwoFrequencies {
    fn to_fhir(&self, _code: &str, _ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
        Ok(None)
    }

    fn to_openehr(&self, _code: &str, _element: &Value) -> Result<Ports, MappingCodeError> {
        let frequency = |magnitude: u32| {
            Datum::new(
                "DV_QUANTITY",
                serde_json::json!({"_type": "DV_QUANTITY", "magnitude": magnitude, "units": "1/d", "precision": 0}),
            )
        };
        Ok(Ports::new().with_each("items[at0003]", vec![frequency(1), frequency(2)]))
    }
}

#[test]
fn two_values_at_a_port_that_holds_one_are_refused() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let program = compiled_with("ferrobridge_daily_timing", &index, &TwoFrequencies)?;
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(request(&["08:00:00"])),
        &Seams::default().with_functions(&TwoFrequencies),
        &laws::defaults(),
        &CallContext::new(),
    );
    let Err(EngineError::MappingCode { source, .. }) = refused else {
        return Err(format!("the run was not refused: {refused:?}").into());
    };
    assert_eq!(
        *source,
        MappingCodeError::RepeatedPort {
            code: String::from("timingToDaily"),
            port: String::from("items[at0003]"),
            count: 2
        }
    );
    Ok(())
}
