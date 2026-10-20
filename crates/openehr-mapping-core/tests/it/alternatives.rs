// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The value nodes of a polymorphic `ELEMENT`, each reading only a value of
//! its own class.
//!
//! `ELEMENT.value` is declared as the abstract `DATA_VALUE` (openEHR RM
//! Release 1.1.0, data structures, the `ELEMENT` class), so the template's
//! alternatives share the element's `aqlPath` and the `_type` of the one
//! value the element holds picks the node that reads it. The template is the
//! testkit's synthetic note with its text element turned polymorphic.

use core::error::Error;

use ferrobridge_testkit::fixtures::MINIMAL_EVALUATION_OPT;
use ferrobridge_testkit::fixtures::MINIMAL_EVALUATION_TEMPLATE_ID;
use openehr_mapping_core::composition::CanonicalDocument;
use openehr_mapping_core::index::ResolvedNode;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::PathError;
use openehr_mapping_core::template::TemplateSource;
use serde_json::Value;
use serde_json::json;

/// The `aqlPath` every value node of the polymorphic element carries.
const VALUE: &str =
    "/content[openEHR-EHR-EVALUATION.ferrobridge_note.v1]/data[at0001]/items[at0002]/value";

/// The classes the element admits beside its `DV_TEXT`.
const ADDED: [&str; 4] = [
    "DV_DATE_TIME",
    "DV_COUNT",
    "DV_INTERVAL<DV_QUANTITY>",
    "DV_INTERVAL<DV_COUNT>",
];

/// A canonical document of the note template, written by hand.
#[derive(Debug)]
struct Document(Value);

impl CanonicalDocument for Document {
    fn document(&self) -> &Value {
        &self.0
    }

    fn document_template_id(&self) -> &str {
        MINIMAL_EVALUATION_TEMPLATE_ID
    }
}

/// Returns the note template with its text element admitting every class of
/// [`ADDED`] beside `DV_TEXT`.
fn polymorphic_index() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let text = "<rm_type_name>DV_TEXT</rm_type_name>";
    let (head, rest) = MINIMAL_EVALUATION_OPT
        .split_once(text)
        .ok_or("the note admits a text")?;
    let (constraint, tail) = rest
        .split_once("</children>")
        .ok_or("the text constraint closes")?;
    let added: Vec<String> = ADDED
        .iter()
        .map(|class| {
            let escaped = class.replace('<', "&lt;").replace('>', "&gt;");
            format!(
                "<children xsi:type=\"C_COMPLEX_OBJECT\"><rm_type_name>{escaped}</rm_type_name>\
                 <occurrences><lower_included>true</lower_included><upper_included>true</upper_included>\
                 <lower_unbounded>false</lower_unbounded><upper_unbounded>false</upper_unbounded>\
                 <lower>1</lower><upper>1</upper></occurrences><node_id></node_id></children>"
            )
        })
        .collect();
    let added = added.concat();
    let xml = format!("{head}{text}{constraint}</children>{added}{tail}");
    let opt = openehr_its::opt14::from_xml(&xml)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Returns a note whose element holds `value`.
fn holding(value: &Value) -> Document {
    Document(json!({
        "_type": "COMPOSITION",
        "archetype_node_id": "openEHR-EHR-COMPOSITION.ferrobridge_minimal.v1",
        "content": [{
            "_type": "EVALUATION",
            "archetype_node_id": "openEHR-EHR-EVALUATION.ferrobridge_note.v1",
            "data": {
                "_type": "ITEM_TREE",
                "archetype_node_id": "at0001",
                "items": [{
                    "_type": "ELEMENT",
                    "archetype_node_id": "at0002",
                    "value": value,
                }],
            },
        }],
    }))
}

/// Returns the value node of `class`.
fn node<'index>(
    index: &'index WebTemplateIndex,
    class: &str,
) -> Result<&'index ResolvedNode, Box<dyn Error>> {
    Ok(index
        .nodes()
        .find(|node| node.rm_type() == class && node.aql_path().as_str() == VALUE)
        .ok_or_else(|| format!("the template holds the {class} alternative"))?)
}

/// Returns the classes of the value nodes that read `value`.
fn readers(index: &WebTemplateIndex, value: &Value) -> Result<Vec<String>, Box<dyn Error>> {
    let document = holding(value);
    let mut classes = Vec::new();
    for node in index
        .nodes()
        .filter(|node| node.aql_path().as_str() == VALUE)
    {
        if index.read(&document, node, &[])?.is_some() {
            classes.push(String::from(node.rm_type()));
        }
        assert!(
            index.holds(&document, node, &[])?,
            "every alternative sees that the element holds a value"
        );
    }
    Ok(classes)
}

#[test]
fn the_alternatives_share_the_element_path() -> Result<(), Box<dyn Error>> {
    let index = polymorphic_index()?;
    let text = node(&index, "DV_TEXT")?;
    let classes: Vec<&str> = index
        .alternatives(text)?
        .iter()
        .map(|node| node.rm_type())
        .collect();
    assert_eq!(
        classes,
        [
            "DV_TEXT",
            "DV_DATE_TIME",
            "DV_COUNT",
            "DV_INTERVAL<DV_QUANTITY>",
            "DV_INTERVAL<DV_COUNT>"
        ]
    );
    Ok(())
}

#[test]
fn a_text_reads_at_the_text_node_only() -> Result<(), Box<dyn Error>> {
    let index = polymorphic_index()?;
    let value = json!({"_type": "DV_TEXT", "value": "a synthetic note"});
    assert_eq!(readers(&index, &value)?, ["DV_TEXT"]);
    Ok(())
}

#[test]
fn a_coded_text_reads_at_the_text_node_it_conforms_to() -> Result<(), Box<dyn Error>> {
    // RM data types: DV_CODED_TEXT inherits DV_TEXT.
    let index = polymorphic_index()?;
    let value = json!({
        "_type": "DV_CODED_TEXT",
        "value": "synthetic",
        "defining_code": {
            "_type": "CODE_PHRASE",
            "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "local"},
            "code_string": "at0099"
        }
    });
    assert_eq!(readers(&index, &value)?, ["DV_TEXT"]);
    Ok(())
}

#[test]
fn a_date_time_reads_at_the_date_time_node_only() -> Result<(), Box<dyn Error>> {
    let index = polymorphic_index()?;
    let value = json!({"_type": "DV_DATE_TIME", "value": "2026-10-20T08:00:00Z"});
    assert_eq!(readers(&index, &value)?, ["DV_DATE_TIME"]);
    Ok(())
}

#[test]
fn a_count_reads_at_the_count_node_only() -> Result<(), Box<dyn Error>> {
    let index = polymorphic_index()?;
    let value = json!({"_type": "DV_COUNT", "magnitude": 3});
    assert_eq!(readers(&index, &value)?, ["DV_COUNT"]);
    Ok(())
}

#[test]
fn an_interval_reads_at_the_node_of_its_limit_class() -> Result<(), Box<dyn Error>> {
    let index = polymorphic_index()?;
    let quantities = json!({
        "_type": "DV_INTERVAL",
        "lower": {"_type": "DV_QUANTITY", "magnitude": 1.5, "units": "mg"},
        "upper": {"_type": "DV_QUANTITY", "magnitude": 3.0, "units": "mg"},
        "lower_unbounded": false,
        "upper_unbounded": false
    });
    assert_eq!(readers(&index, &quantities)?, ["DV_INTERVAL<DV_QUANTITY>"]);
    let counts = json!({
        "_type": "DV_INTERVAL",
        "upper": {"_type": "DV_COUNT", "magnitude": 4},
        "lower_unbounded": true,
        "upper_unbounded": false
    });
    assert_eq!(readers(&index, &counts)?, ["DV_INTERVAL<DV_COUNT>"]);
    Ok(())
}

#[test]
fn a_class_no_alternative_admits_reads_nowhere_but_is_held() -> Result<(), Box<dyn Error>> {
    let index = polymorphic_index()?;
    let value = json!({"_type": "DV_BOOLEAN", "value": true});
    assert!(readers(&index, &value)?.is_empty());
    Ok(())
}

#[test]
fn an_untyped_value_at_a_polymorphic_element_is_refused() -> Result<(), Box<dyn Error>> {
    // Simplified Formats master04 §Raw canonical JSON: the value "must include
    // the `_type` property".
    let index = polymorphic_index()?;
    let document = holding(&json!({"value": "no class"}));
    let refused = index.read(&document, node(&index, "DV_TEXT")?, &[]);
    assert!(
        matches!(refused, Err(PathError::UntypedAlternative { .. })),
        "{refused:?}"
    );
    Ok(())
}
