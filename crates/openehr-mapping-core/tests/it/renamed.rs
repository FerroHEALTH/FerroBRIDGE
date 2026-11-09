// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An instance whose runtime name differs from the name its template path
//! spells is still read and named (#1707).
//!
//! A template's `[atNNNN,'Name']` conjunct spells the name the archetype
//! constrains, while an instance may carry another `LOCATABLE.name` (RM common
//! `master03-archetyped_package.adoc` §"The `LOCATABLE` class"). Where the name
//! is the only one the template spells for that node id under its parent,
//! `openehr-sdt`'s navigation reaches the instance by its node id. The
//! template is the testkit's synthetic note with its entry's path naming it
//! "Note", and the composition names it "Renamed".

use core::error::Error;

use ferrobridge_testkit::fixtures::MINIMAL_EVALUATION_OPT;
use ferrobridge_testkit::fixtures::MINIMAL_EVALUATION_TEMPLATE_ID;
use openehr_mapping_core::composition::CanonicalDocument;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;
use openehr_sdt::flat::webtemplate::model::WebTemplateNode;
use serde_json::Value;
use serde_json::json;

/// The entry's path as the testkit's template spells it.
const ENTRY: &str = "/content[openEHR-EHR-EVALUATION.ferrobridge_note.v1]";

/// The entry's path with the name the archetype constrains.
const NAMED: &str = "/content[openEHR-EHR-EVALUATION.ferrobridge_note.v1,'Note']";

/// The text value below the entry.
const TEXT: &str = "/data[at0001]/items[at0002]/value";

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

/// Spells the entry's name into every path at or below it.
fn name_entry(node: &mut WebTemplateNode) {
    if let Some(below) = node.aql_path.strip_prefix(ENTRY) {
        node.aql_path = format!("{NAMED}{below}");
    }
    node.children.iter_mut().for_each(name_entry);
}

/// The note template, its entry's path naming the entry.
fn index() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(MINIMAL_EVALUATION_OPT)?;
    let built = WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(opt)))?;
    let mut template = built.web_template().clone();
    name_entry(&mut template.tree);
    Ok(WebTemplateIndex::over(template, built.generation())?)
}

/// A note whose entry is named `name` and holds `text`.
fn note(name: &str, text: &str) -> Document {
    Document(json!({
        "_type": "COMPOSITION",
        "archetype_node_id": "openEHR-EHR-COMPOSITION.ferrobridge_minimal.v1",
        "content": [{
            "_type": "EVALUATION",
            "name": { "_type": "DV_TEXT", "value": name },
            "archetype_node_id": "openEHR-EHR-EVALUATION.ferrobridge_note.v1",
            "data": {
                "_type": "ITEM_TREE",
                "archetype_node_id": "at0001",
                "items": [{
                    "_type": "ELEMENT",
                    "archetype_node_id": "at0002",
                    "value": { "_type": "DV_TEXT", "value": text },
                }],
            },
        }],
    }))
}

#[test]
fn a_renamed_instance_is_read_and_named_through_its_node_id() -> Result<(), Box<dyn Error>> {
    let index = index()?;
    let text = format!("{NAMED}{TEXT}");
    let node = index
        .nodes()
        .find(|node| node.aql_path().as_str() == text)
        .ok_or("the template holds the named entry's text")?;
    let document = note("Renamed", "a synthetic note");
    let read = index
        .read(&document, node, &[])?
        .ok_or("the renamed entry's text is read")?;
    assert_eq!(
        Some("a synthetic note"),
        read.get("value").and_then(Value::as_str)
    );
    let path = index
        .entry_path(&document, node, &[])?
        .ok_or("the renamed entry's text is named")?;
    assert_eq!(text, path.to_string());
    Ok(())
}
