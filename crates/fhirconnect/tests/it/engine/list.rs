// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `List` attribute of the reference model below a template node, as an
//! occurrence axis inside the node's value.

use core::error::Error;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;

use crate::support::compiled;
use crate::support::template;
use crate::term_mapping::BROADER_CODE;
use crate::term_mapping::BROADER_SYSTEM;
use crate::term_mapping::annotated_composition;

#[test]
fn each_element_of_a_list_reads_out_as_one_occurrence() -> Result<(), Box<dyn Error>> {
    // RM 1.1.0 data_types §DV_TEXT: `mappings` is a List<TERM_MAPPING>, and FHIRconnect
    // DV_TEXT.adoc pairs each mapping with one `coding`.
    let program = compiled("ferrobridge_list_mappings")?;
    let index = template()?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &annotated_composition(&index, '=')?,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let resource = outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?;
    assert_eq!(
        resource.pointer("/code/coding"),
        Some(&serde_json::json!([
            {"code": BROADER_CODE, "system": BROADER_SYSTEM},
            {"code": "SYN-001", "system": "http://example.org/fhir/CodeSystem/synthetic"},
        ])),
        "each TERM_MAPPING is one coding, in list order: {resource}"
    );
    Ok(())
}

#[test]
fn a_list_with_no_element_reads_out_as_nothing() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_list_mappings")?;
    let index = template()?;
    let plain = crate::term_mapping::composition_without_mappings(&index)?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &plain,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let resource = outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?;
    assert_eq!(resource.pointer("/code"), None, "{resource}");
    Ok(())
}

#[test]
fn a_list_no_child_fills_is_refused_at_load() -> Result<(), Box<dyn Error>> {
    let error = compiled("ferrobridge_tail_unsupported")
        .err()
        .ok_or("a list with no child compiled")?;
    let message = error.to_string();
    assert!(message.contains("fc-uncarried-tail"), "{message}");
    assert!(message.contains("names the list `mappings`"), "{message}");
    Ok(())
}
