// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The elements of an imaging examination result the imaging project
//! carries into no element of its HL7 Europe Imaging Report profiles, each of
//! which a run out of openEHR declares.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use super::{mapping, reference, settings, template};
use eehrxf::mapping::left_out;
use eehrxf::mappings::IMAGING;
use fhirconnect::engine::context::CallContext;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_sdt::flat::example::{DetailLevel, apply_output_uid, example_composition};

/// An imaging report of FerroBRIDGE's template that records one occurrence
/// of every node the template holds, generated from its Web Template
/// (`openehr-sdt`'s example at the `Medium` detail level).
fn every_element() -> Result<String, Box<dyn Error>> {
    let index = WebTemplateIndex::build(&template()?)?;
    let mut example = example_composition(index.web_template(), DetailLevel::Medium)?;
    apply_output_uid(&mut example, index.template_id());
    Ok(serde_json::to_string(&example)?)
}

#[test]
fn the_project_declares_every_element_of_the_result_it_leaves_out() -> Result<(), Box<dyn Error>> {
    let (_, warnings) = mapping()?.to_fhir_with_warnings(
        &every_element()?,
        &settings(),
        CallContext::new().with_patient(reference("Patient/synthetic-patient")),
    )?;
    let named = left_out(&warnings);
    let declared = IMAGING.left_out();
    let comment = IMAGING.note().ok_or("the project's comment")?.value_path();
    assert_eq!(
        named.len(),
        declared.len() + 1,
        "the run leaves out exactly the elements the project declares and its comment: {named:#?}"
    );
    assert!(
        named.contains(&comment),
        "the composition's comment is named: {named:#?}"
    );
    for element in declared {
        let at = format!("[{}]/{}", element.archetype, element.path);
        assert!(
            named
                .iter()
                .any(|path| path.ends_with(&at) || path.contains(&format!("{at}/"))),
            "{at} is declared and the run names it: {named:#?}"
        );
    }
    Ok(())
}
