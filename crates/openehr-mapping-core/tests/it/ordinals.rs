// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ordinal options an OPT 1.4 `C_DV_ORDINAL` constrains at a node.
//!
//! The fixture is the testkit's minimal operational template with its one
//! element's `DV_TEXT` value turned into a `DV_ORDINAL` of two synthetic
//! options, each a `value` and a `local` symbol (the openEHR OPT 1.4 XML
//! schema, `C_DV_ORDINAL`; RM Release 1.1.0, data types, `DV_ORDINAL`).

use std::error::Error as StdError;

use ferrobridge_testkit::fixtures::MINIMAL_EVALUATION_OPT;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::ordinals::OrdinalOption;
use openehr_mapping_core::template::Generation;
use openehr_mapping_core::template::TemplateSource;
use openehr_mapping_core::template::web_template;

/// The value constraint of the fixture's one element, as it spells it.
const TEXT_VALUE: &str = "<children xsi:type=\"C_COMPLEX_OBJECT\">
                    <rm_type_name>DV_TEXT</rm_type_name>";

/// The last term the fixture's `EVALUATION` root defines, as it spells it.
const LAST_TERM: &str = "<items id=\"description\">The one text element of the synthetic note</items>\n        </term_definitions>";

/// Returns one `C_DV_ORDINAL` list entry of `value` with the `local` symbol
/// `code`.
fn entry(value: i32, code: &str, rubric: &str) -> String {
    format!(
        "<list><symbol><value>{rubric}</value><defining_code><terminology_id>\
         <value>local</value></terminology_id><code_string>{code}</code_string>\
         </defining_code></symbol><value>{value}</value></list>"
    )
}

/// Returns the fixture with its element's value a `DV_ORDINAL` of two options.
fn ordinal_template() -> Result<String, Box<dyn StdError>> {
    let (head, rest) = MINIMAL_EVALUATION_OPT
        .split_once(TEXT_VALUE)
        .ok_or("the fixture no longer spells its text value as expected")?;
    let (_, tail) = rest
        .split_once("</children>")
        .ok_or("the text value closes")?;
    let ordinal = format!(
        "<children xsi:type=\"C_DV_ORDINAL\"><rm_type_name>DV_ORDINAL</rm_type_name>\
         <occurrences><lower_included>true</lower_included><upper_included>true</upper_included>\
         <lower_unbounded>false</lower_unbounded><upper_unbounded>false</upper_unbounded>\
         <lower>1</lower><upper>1</upper></occurrences><node_id></node_id>{}{}</children>",
        entry(1, "at0003", "Mild"),
        entry(3, "at0004", "Severe"),
    );
    let xml = format!("{head}{ordinal}{tail}");
    if !xml.contains(LAST_TERM) {
        return Err("the fixture no longer spells its last EVALUATION term as expected".into());
    }
    Ok(xml.replacen(
        LAST_TERM,
        &format!(
            "{LAST_TERM}\n        <term_definitions code=\"at0003\">\n          <items id=\"text\">Mild</items>\n          <items id=\"description\">The first synthetic option</items>\n        </term_definitions>\n        <term_definitions code=\"at0004\">\n          <items id=\"text\">Severe</items>\n          <items id=\"description\">The second synthetic option</items>\n        </term_definitions>"
        ),
        1,
    ))
}

#[test]
fn a_dv_ordinal_node_returns_its_options_in_template_order() -> Result<(), Box<dyn StdError>> {
    let index = WebTemplateIndex::build(&TemplateSource::opt14(&ordinal_template()?)?)?;
    let node = index
        .nodes()
        .find(|node| node.rm_type() == "DV_ORDINAL")
        .ok_or("the template holds the ordinal node")?;
    assert_eq!(
        Some(
            &[
                OrdinalOption::new(1, "at0003", "local"),
                OrdinalOption::new(3, "at0004", "local"),
            ][..]
        ),
        index.ordinal_options(node)
    );
    Ok(())
}

#[test]
fn a_node_with_no_ordinal_constraint_returns_none() -> Result<(), Box<dyn StdError>> {
    let index = WebTemplateIndex::build(&TemplateSource::opt14(MINIMAL_EVALUATION_OPT)?)?;
    assert!(
        index
            .nodes()
            .all(|node| index.ordinal_options(node).is_none()),
        "no node of the fixture is an ordinal"
    );
    let ordinal = WebTemplateIndex::build(&TemplateSource::opt14(&ordinal_template()?)?)?;
    assert_eq!(None, ordinal.ordinal_options(ordinal.root()));
    Ok(())
}

#[test]
fn an_index_over_a_built_web_template_has_no_ordinals_to_read() -> Result<(), Box<dyn StdError>> {
    let built = web_template(&TemplateSource::opt14(&ordinal_template()?)?)?;
    let index = WebTemplateIndex::over(built, Generation::Adl14)?;
    let node = index
        .nodes()
        .find(|node| node.rm_type() == "DV_ORDINAL")
        .ok_or("the template holds the ordinal node")?;
    assert_eq!(None, index.ordinal_options(node));
    Ok(())
}
