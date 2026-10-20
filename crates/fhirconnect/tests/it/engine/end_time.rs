// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `EVENT_CONTEXT.end_time`, which the Web Template holds no node for.
//!
//! `$composition` addresses "elements that are not inside the archetype but
//! are part of the context of the surrounding composition"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/basics/Variables.adoc`), and
//! Simplified Formats carries the attribute as the `/_end_time` family of the
//! `context` node
//! (`docs/specs/its-rest/docs/simplified_formats/master05-rm_mapping.adoc`,
//! the `EVENT_CONTEXT` section).

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::index::paths::AqlPath;

use crate::engine::defaults;
use crate::resolve::borrow;
use crate::resolve::context;
use crate::resolve::program_of;
use crate::resolve::render;
use crate::resolve::set_of;
use crate::resolve::start_context;
use crate::resolve::start_model;
use crate::support::template;

/// The synthetic end time the `Condition` records.
const RECORDED: &str = "2026-10-19T09:30:00Z";

/// The problem mapping plus one mapping onto the context's end time.
const BODY: &str = "mappings:\n  - name: \"problem\"\n    with:\n      fhir: \"$resource.code\"\n      \
                    openehr: \"$archetype/data[at0001]/items[at0002]\"\n  - name: \"recorded\"\n    \
                    with:\n      fhir: \"$resource.recordedDate\"\n      openehr: \
                    \"$composition/context/end_time\"\n";

/// Returns the synthetic `Condition` with `recordedDate` set.
fn recorded_condition() -> Result<Value, Box<dyn Error>> {
    let mut parsed: serde_json::Value =
        serde_json::from_str(ferrobridge_testkit::fixtures::R4_CONDITION)?;
    let object = parsed
        .as_object_mut()
        .ok_or("the synthetic Condition is an object")?;
    object.insert(
        String::from("recordedDate"),
        serde_json::Value::String(String::from(RECORDED)),
    );
    Ok(Value::from_serde_json(parsed))
}

#[test]
fn the_context_end_time_round_trips_through_its_flat_family() -> Result<(), Box<dyn Error>> {
    let files = [
        ("model.yml", start_model(BODY)),
        (
            "context.yml",
            context("synthetic.context", &start_context(&[])),
        ),
    ];
    let set = set_of(&borrow(&files))?;
    let program =
        program_of(&set, "synthetic.context").map_err(|diagnostics| render(&diagnostics))?;
    let index = template()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &recorded_condition()?,
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?;
    let node = index.node(&AqlPath::new("/context"))?;
    let written = index
        .read(inbound.value(), node, &[])?
        .ok_or("the run wrote the context")?;
    assert_eq!(
        written
            .pointer("/end_time/value")
            .and_then(serde_json::Value::as_str),
        Some(RECORDED),
        "the end time landed in EVENT_CONTEXT.end_time: {written}"
    );
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    let recorded = outbound
        .value()
        .get("recordedDate")
        .ok_or("the Condition carries the recorded date")?
        .to_serde_json(&mut fhir_types::codec::Path::root("dateTime"))?;
    assert_eq!(recorded, serde_json::json!(RECORDED));
    Ok(())
}
