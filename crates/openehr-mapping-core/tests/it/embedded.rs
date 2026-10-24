// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The template an OPT 1.4 states it embeds at an archetype root.
//!
//! The fixture is the testkit's minimal operational template, whose
//! composition root states its own `template_id`. A copy whose `EVALUATION`
//! root also states one stands for a section template included in a
//! composition template (the openEHR OPT 1.4 XML schema, `C_ARCHETYPE_ROOT`).

use std::error::Error;

use ferrobridge_testkit::fixtures::MINIMAL_EVALUATION_OPT;
use ferrobridge_testkit::fixtures::MINIMAL_EVALUATION_TEMPLATE_ID;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::template::Generation;
use openehr_mapping_core::template::TemplateSource;
use openehr_mapping_core::template::web_template;

/// The `aqlPath` of the fixture's `EVALUATION` root.
const EVALUATION: &str = "/content[openEHR-EHR-EVALUATION.ferrobridge_note.v1]";

/// The archetype id element of the `EVALUATION` root, as the fixture spells it.
const EVALUATION_ID: &str =
    "<value>openEHR-EHR-EVALUATION.ferrobridge_note.v1</value>\n        </archetype_id>";

/// The template the copy states at the `EVALUATION` root.
const NOTE_TEMPLATE: &str = "Synthetic note section";

fn source(xml: &str) -> Result<TemplateSource, Box<dyn Error>> {
    Ok(TemplateSource::opt14(xml)?)
}

fn with_embedded_note() -> Result<String, Box<dyn Error>> {
    if !MINIMAL_EVALUATION_OPT.contains(EVALUATION_ID) {
        return Err("the fixture no longer spells its EVALUATION root as expected".into());
    }
    Ok(MINIMAL_EVALUATION_OPT.replacen(
        EVALUATION_ID,
        &format!("{EVALUATION_ID}\n        <template_id>\n          <value>{NOTE_TEMPLATE}</value>\n        </template_id>"),
        1,
    ))
}

#[test]
fn the_composition_root_states_the_template_itself() -> Result<(), Box<dyn Error>> {
    let index = WebTemplateIndex::build(&source(MINIMAL_EVALUATION_OPT)?)?;
    assert_eq!(
        Some(MINIMAL_EVALUATION_TEMPLATE_ID),
        index.embedded_template(index.root())
    );
    Ok(())
}

#[test]
fn an_archetype_root_that_states_a_template_exposes_it() -> Result<(), Box<dyn Error>> {
    let index = WebTemplateIndex::build(&source(&with_embedded_note()?)?)?;
    let node = index.node(&AqlPath::new(EVALUATION))?;
    assert_eq!(Some(NOTE_TEMPLATE), index.embedded_template(node));
    Ok(())
}

#[test]
fn an_archetype_root_that_states_none_exposes_none() -> Result<(), Box<dyn Error>> {
    let index = WebTemplateIndex::build(&source(MINIMAL_EVALUATION_OPT)?)?;
    let node = index.node(&AqlPath::new(EVALUATION))?;
    assert_eq!(None, index.embedded_template(node));
    Ok(())
}

#[test]
fn an_index_over_a_built_web_template_has_no_template_to_read() -> Result<(), Box<dyn Error>> {
    let built = web_template(&source(&with_embedded_note()?)?)?;
    let index = WebTemplateIndex::over(built, Generation::Adl14)?;
    let node = index.node(&AqlPath::new(EVALUATION))?;
    assert_eq!(None, index.embedded_template(node));
    Ok(())
}
