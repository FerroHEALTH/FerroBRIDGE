// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node codes the archetype behind an archetype root of an OPT 1.4
//! defines.
//!
//! The fixture is the testkit's minimal operational template, whose
//! `EVALUATION` root defines `at0000` to `at0002` and keeps a node for each.
//! A copy that adds the term `at0003` stands for an archetype node the
//! template excludes: the OPT 1.4 `C_ARCHETYPE_ROOT` carries the archetype's
//! `term_definitions` whole, and the template carries no node for it.

use std::error::Error;

use ferrobridge_testkit::fixtures::MINIMAL_EVALUATION_OPT;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::template::Generation;
use openehr_mapping_core::template::TemplateSource;
use openehr_mapping_core::template::web_template;

/// The `aqlPath` of the fixture's `EVALUATION` root.
const EVALUATION: &str = "/content[openEHR-EHR-EVALUATION.ferrobridge_note.v1]";

/// The last term the fixture's `EVALUATION` root defines, as it spells it.
const LAST_TERM: &str = "<items id=\"description\">The one text element of the synthetic note</items>\n        </term_definitions>";

fn source(xml: &str) -> Result<TemplateSource, Box<dyn Error>> {
    Ok(TemplateSource::opt14(xml)?)
}

fn with_excluded_term() -> Result<String, Box<dyn Error>> {
    if !MINIMAL_EVALUATION_OPT.contains(LAST_TERM) {
        return Err("the fixture no longer spells its last EVALUATION term as expected".into());
    }
    Ok(MINIMAL_EVALUATION_OPT.replacen(
        LAST_TERM,
        &format!(
            "{LAST_TERM}\n        <term_definitions code=\"at0003\">\n          <items id=\"text\">Comment</items>\n          <items id=\"description\">A node the template excludes</items>\n        </term_definitions>"
        ),
        1,
    ))
}

#[test]
fn an_archetype_root_defines_the_codes_of_the_nodes_it_keeps() -> Result<(), Box<dyn Error>> {
    let index = WebTemplateIndex::build(&source(MINIMAL_EVALUATION_OPT)?)?;
    let root = index.node(&AqlPath::new(EVALUATION))?;
    assert_eq!(Some(true), index.archetype_defines(root, "at0002"));
    assert_eq!(Some(false), index.archetype_defines(root, "at0003"));
    Ok(())
}

#[test]
fn an_archetype_root_defines_a_code_the_template_carries_no_node_for()
-> Result<(), Box<dyn Error>> {
    let index = WebTemplateIndex::build(&source(&with_excluded_term()?)?)?;
    let root = index.node(&AqlPath::new(EVALUATION))?;
    assert_eq!(Some(true), index.archetype_defines(root, "at0003"));
    assert!(
        index.nodes().all(|node| node.node_id() != Some("at0003")),
        "the template carries no node for at0003"
    );
    Ok(())
}

#[test]
fn a_node_that_roots_no_archetype_answers_none() -> Result<(), Box<dyn Error>> {
    let index = WebTemplateIndex::build(&source(MINIMAL_EVALUATION_OPT)?)?;
    let leaf = index
        .nodes()
        .find(|node| node.node_id() == Some("at0002"))
        .ok_or("the fixture keeps a node for at0002")?;
    assert_eq!(None, index.archetype_defines(leaf, "at0002"));
    Ok(())
}

#[test]
fn an_index_over_a_built_web_template_has_no_terms_to_read() -> Result<(), Box<dyn Error>> {
    let built = web_template(&source(MINIMAL_EVALUATION_OPT)?)?;
    let index = WebTemplateIndex::over(built, Generation::Adl14)?;
    let root = index.node(&AqlPath::new(EVALUATION))?;
    assert_eq!(None, index.archetype_defines(root, "at0002"));
    Ok(())
}
