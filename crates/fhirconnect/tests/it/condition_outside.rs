// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An `openehrCondition` on a mapping below a repeating event whose target
//! lies outside that event, coming out of openEHR.
//!
//! `basics/Conditions.adoc` (§targetRoot) evaluates the condition at the
//! node its `targetRoot` names, and `basics/Variables.adoc` makes
//! `$archetype` the archetype root, so the target here is the lots of the
//! observation's protocol, which repeat with no event. The synthetic
//! template is `tests/fixtures/opt/observation_protocol.opt`; the mapping is
//! `ferrobridge_condition_outside`, which makes each event its own
//! Observation and maps an event's amount while the protocol names a lot.

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::model::semantic::StaticMappingCodes;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::NodeValue;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;

use crate::laws;
use crate::leave_anchor::protocol_template;
use crate::support::compiled_models;

/// The synthetic observation.
const OBSERVATION: &str = "/content[openEHR-EHR-OBSERVATION.ferrobridge_reading.v1]";

/// The context under test.
const CONTEXT: &str = "ferrobridge_condition_outside";

/// Builds a composition of two events, each with an amount, and a protocol
/// batch naming one lot.
fn two_events(index: &WebTemplateIndex) -> Result<CanonicalComposition, Box<dyn Error>> {
    let event = |instance: u32| RmPosition::new(instance).map(|position| vec![position]);
    let mut values = vec![
        NodeValue::new(
            index.node(&AqlPath::new("/composer"))?,
            "FerroBRIDGE".into(),
        )
        .with_datum("name"),
        NodeValue::new(index.node(&AqlPath::new("/language"))?, "en".into()).with_datum("code"),
        NodeValue::new(index.node(&AqlPath::new("/territory"))?, "NL".into()).with_datum("code"),
        NodeValue::new(
            index.node(&AqlPath::new(format!(
                "{OBSERVATION}/protocol[at0005]/items[at0009]/items[at0010]/value"
            )))?,
            serde_json::json!({"_type": "DV_TEXT", "value": "Synthetic lot A"}),
        )
        .with_occurrences(vec![RmPosition::new(1)?, RmPosition::new(1)?])
        .with_datum("raw"),
    ];
    for (instance, (time, amount)) in [
        (1, ("2026-09-12T06:00:00+02:00", 12)),
        (2, ("2026-09-12T07:00:00+02:00", 13)),
    ] {
        values.push(
            NodeValue::new(
                index.node(&AqlPath::new(format!(
                    "{OBSERVATION}/data[at0001]/events[at0002]/time"
                )))?,
                time.into(),
            )
            .with_occurrences(event(instance)?),
        );
        values.push(
            NodeValue::new(
                index.node(&AqlPath::new(format!(
                    "{OBSERVATION}/data[at0001]/events[at0002]/data[at0003]/items[at0004]/value"
                )))?,
                serde_json::json!({
                    "_type": "DV_QUANTITY",
                    "magnitude": f64::from(amount),
                    "units": "kg",
                    "precision": 0
                }),
            )
            .with_occurrences(event(instance)?)
            .with_datum("raw"),
        );
    }
    Ok(index.build_composition(&values, laws::NOW)?)
}

/// Returns the amount an Observation carries.
fn amount_of(resource: &Value) -> Option<String> {
    let quantity = resource.get("valueQuantity")?;
    quantity.get("value").map(|value| format!("{value:?}"))
}

#[test]
fn a_condition_outside_a_repeating_anchor_holds_for_every_event() -> Result<(), Box<dyn Error>> {
    // Conditions.adoc §targetRoot: the condition reads the lots of the
    // protocol, which no event repeats, so the second event reads the one
    // batch the composition holds and its amount is mapped too.
    let index = protocol_template()?;
    let program = compiled_models(CONTEXT, &[CONTEXT], &index, &StaticMappingCodes::default())?;
    let built = two_events(&index)?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &built,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let second = outbound
        .created()
        .first()
        .ok_or("the second event is an Observation of its own")?;
    assert!(
        amount_of(outbound.value()).is_some(),
        "the first event's amount is mapped: {:?}",
        outbound.value()
    );
    assert!(
        amount_of(second).is_some(),
        "the second event's amount is mapped: {second:?}"
    );
    Ok(())
}
