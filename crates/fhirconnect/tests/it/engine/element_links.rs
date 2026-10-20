// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `links` of an `ELEMENT` the Web Template compacts into its value.
//!
//! An `ELEMENT` is a `LOCATABLE` and carries `links` (openEHR RM, the
//! `LOCATABLE` class). FHIRconnect writes `$openehrRoot/links` below an
//! element (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/FHIRReference.adoc`,
//! §Dedicated Node), and Simplified Formats writes the element's `_link:i` on
//! the one FLAT node it shares with its value
//! (`docs/specs/its-rest/docs/simplified_formats/master05-rm_mapping.adoc`,
//! the `ELEMENT` section).

use core::error::Error;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::index::paths::AqlPath;

use crate::engine::defaults;
use crate::engine::reference::condition_with;
use crate::resolve::borrow;
use crate::resolve::codes;
use crate::resolve::context;
use crate::resolve::program_of;
use crate::resolve::refusals;
use crate::resolve::render;
use crate::resolve::set_of;
use crate::resolve::start_context;
use crate::resolve::start_model;
use crate::support::template;

/// A synthetic `ehr:` URI of a composition version the link targets.
const LINKED: &str = "ehr://ferrobridge.example/3a2f1c4e-0000-4000-8000-0000000000ab/compositions/d2b3c1a0-0000-4000-8000-000000000001::ferrobridge.example::1";

/// The `aqlPath` of the diagnosis element of the synthetic template.
const PROBLEM_VALUE: &str =
    "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]/value";

/// The problem mapping with a link below its element.
const BODY: &str = "mappings:\n  - name: \"problem\"\n    with:\n      fhir: \"$resource.code\"\n      \
                    openehr: \"$archetype/data[at0001]/items[at0002]\"\n    followedBy:\n      \
                    mappings:\n        - name: \"encounterLink\"\n          with:\n            fhir: \
                    \"$resource.encounter\"\n            openehr: \"$openehrRoot/links\"\n          \
                    link:\n            meaning: \"the encounter the diagnosis was made in\"\n            \
                    type: \"encounter\"\n";

/// Returns the context every case here compiles.
fn files(body: &str) -> [(&'static str, String); 2] {
    [
        ("model.yml", start_model(body)),
        (
            "context.yml",
            context("synthetic.context", &start_context(&[])),
        ),
    ]
}

#[test]
fn a_link_below_a_compacted_element_is_written_and_read_back() -> Result<(), Box<dyn Error>> {
    let files = files(BODY);
    let set = set_of(&borrow(&files))?;
    let program =
        program_of(&set, "synthetic.context").map_err(|diagnostics| render(&diagnostics))?;
    let index = template()?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_with(serde_json::json!({"encounter": {"reference": LINKED}}))?,
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )?;
    let node = index.node(&AqlPath::new(PROBLEM_VALUE))?;
    let element = index
        .read_element(inbound.value(), node, &[])?
        .ok_or("the run wrote the element")?;
    assert_eq!(
        element
            .pointer("/links/0/target/value")
            .and_then(serde_json::Value::as_str),
        Some(LINKED),
        "the LINK sits on the ELEMENT: {element}"
    );
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    assert_eq!(
        outbound
            .value()
            .get("encounter")
            .and_then(|encounter| encounter.get("reference"))
            .and_then(Value::as_str),
        Some(LINKED),
        "the element's link reads back into the reference"
    );
    Ok(())
}

/// Neither the value nor the `ELEMENT` carries `no_such_attribute`.
#[test]
fn an_attribute_neither_the_value_nor_the_element_carries_is_still_refused()
-> Result<(), Box<dyn Error>> {
    let files = files(&BODY.replace("$openehrRoot/links", "$openehrRoot/no_such_attribute"));
    let diagnostics = refusals(&borrow(&files), "synthetic.context")?;
    assert!(
        codes(&diagnostics).contains(&String::from("fc-unknown-template-node")),
        "{}",
        render(&diagnostics)
    );
    Ok(())
}

/// A path that names the value itself (`.../value/links`) names the links of
/// a data value, which has none.
#[test]
fn links_below_the_value_itself_are_still_refused() -> Result<(), Box<dyn Error>> {
    let files = files(&BODY.replace("$openehrRoot/links", "$openehrRoot/value/links"));
    let diagnostics = refusals(&borrow(&files), "synthetic.context")?;
    assert!(
        codes(&diagnostics).contains(&String::from("fc-unknown-template-node")),
        "{}",
        render(&diagnostics)
    );
    Ok(())
}
