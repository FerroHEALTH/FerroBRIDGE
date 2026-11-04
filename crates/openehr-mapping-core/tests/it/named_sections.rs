// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Two sibling overlays of one archetype, told apart by name (#1487).
//!
//! The International Patient Summary holds its sections as `SECTION.adhoc.v1`
//! siblings, each with its own name. In data every one of them carries the
//! archetype id as its `archetype_node_id` (RM common
//! `master03-archetyped_package.adoc` §"The `LOCATABLE` class"), so only the
//! runtime name tells them apart (BASE `architecture_overview/master11-paths.adoc`
//! §"Using a Name-based Predicate"). The fixture is that shape in small: the
//! CKM `SECTION.adhoc.v1` twice, as "Notes" holding the synthetic note and
//! "Alerts" holding the synthetic alert, each at most once. A composition
//! holding both is accepted and each entry reads from its own section, and a
//! second "Notes" section is still refused.

use std::error::Error;

use ferrobridge_testkit::fixtures::CKM_MEDICATION_ARCHETYPES;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::assemble::parse_artefact;
use openehr_adl::opt::create_opt;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::index::{ResolvedNode, WebTemplateIndex};
use openehr_mapping_core::template::{PathError, TemplateSource};
use serde_json::{Value, json};

/// The template of two named ad hoc sections.
const TEMPLATE: &str = include_str!("../fixtures/adl2/named_sections.adlt");

/// The template's identifier.
const TEMPLATE_ID: &str = "openEHR-EHR-COMPOSITION.ferrobridge_named_sections.v1.0.0";

/// The synthetic note the "Notes" section holds.
const NOTE: &str = include_str!("../fixtures/adl2/note_evaluation.adls");

/// The synthetic alert the "Alerts" section holds.
const ALERT: &str = include_str!("../fixtures/adl2/alert_evaluation.adls");

/// The CKM ad hoc section both overlays specialise.
const ADHOC: &str = "openEHR-EHR-SECTION.adhoc.v1.adl";

/// Compiles the template over the CKM section and the two synthetic entries.
fn index() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    let (_, adhoc) = CKM_MEDICATION_ARCHETYPES
        .iter()
        .find(|(file, _)| *file == ADHOC)
        .ok_or("the testkit vendors the CKM ad hoc section")?;
    repository.insert(parse_artefact(adhoc, Dialect::Adl14).map_err(|e| format!("{e:?}"))?);
    for source in [NOTE, ALERT] {
        repository.insert(parse_artefact(source, Dialect::Adl2).map_err(|e| format!("{e:?}"))?);
    }
    let template = parse_artefact(TEMPLATE, Dialect::Adl2).map_err(|e| format!("{e:?}"))?;
    let opt = create_opt(&template, &repository).map_err(|e| format!("{e:?}"))?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(opt),
        resolved_id: String::from(TEMPLATE_ID),
    })?)
}

/// The text value node of the entry under the section named `section`.
fn text_node<'a>(index: &'a WebTemplateIndex, section: &str) -> Result<&'a ResolvedNode, String> {
    index
        .nodes()
        .find(|node| {
            let path = node.aql_path().as_str();
            path.contains(&format!("'{section}'")) && path.ends_with("/items[at0002]/value")
        })
        .ok_or_else(|| format!("no text node under the {section} section"))
}

/// A canonical EVALUATION of `archetype` whose one element holds `text`.
fn entry(archetype: &str, text: &str) -> Value {
    json!({
        "_type": "EVALUATION",
        "name": { "_type": "DV_TEXT", "value": "Entry" },
        "archetype_node_id": archetype,
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": { "_type": "ARCHETYPE_ID", "value": archetype },
            "rm_version": "1.1.0"
        },
        "language": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }, "code_string": "en" },
        "encoding": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "IANA_character-sets" }, "code_string": "UTF-8" },
        "subject": { "_type": "PARTY_SELF" },
        "data": {
            "_type": "ITEM_TREE",
            "name": { "_type": "DV_TEXT", "value": "Tree" },
            "archetype_node_id": "at0001",
            "items": [{
                "_type": "ELEMENT",
                "name": { "_type": "DV_TEXT", "value": "Text" },
                "archetype_node_id": "at0002",
                "value": { "_type": "DV_TEXT", "value": text }
            }]
        }
    })
}

/// A canonical ad hoc SECTION named `name` holding `item`.
fn section(name: &str, item: &Value) -> Value {
    json!({
        "_type": "SECTION",
        "name": { "_type": "DV_TEXT", "value": name },
        "archetype_node_id": "openEHR-EHR-SECTION.adhoc.v1",
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": { "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-SECTION.adhoc.v1" },
            "rm_version": "1.1.0"
        },
        "items": [item]
    })
}

/// A canonical composition of the template holding `content`.
fn composition(content: &[Value]) -> Value {
    json!({
        "_type": "COMPOSITION",
        "name": { "_type": "DV_TEXT", "value": "Named sections" },
        "archetype_node_id": "openEHR-EHR-COMPOSITION.ferrobridge_named_sections.v1",
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": { "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.ferrobridge_named_sections.v1" },
            "template_id": { "_type": "TEMPLATE_ID", "value": TEMPLATE_ID },
            "rm_version": "1.1.0"
        },
        "language": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }, "code_string": "en" },
        "territory": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_3166-1" }, "code_string": "NL" },
        "category": {
            "_type": "DV_CODED_TEXT",
            "value": "event",
            "defining_code": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "openehr" }, "code_string": "433" }
        },
        "composer": { "_type": "PARTY_IDENTIFIED", "name": "Synthetic composer" },
        "content": content
    })
}

/// The note in "Notes" and the alert in "Alerts".
fn both() -> Vec<Value> {
    vec![
        section(
            "Notes",
            &entry(
                "openEHR-EHR-EVALUATION.ferrobridge_note.v1",
                "a synthetic note",
            ),
        ),
        section(
            "Alerts",
            &entry(
                "openEHR-EHR-EVALUATION.ferrobridge_alert.v1",
                "a synthetic alert",
            ),
        ),
    ]
}

#[test]
fn a_composition_holding_both_named_sections_is_accepted_and_each_entry_reads_from_its_own()
-> Result<(), Box<dyn Error>> {
    let index = index()?;
    let accepted = index.accept(composition(&both()))?;
    for (name, text) in [
        ("Notes", "a synthetic note"),
        ("Alerts", "a synthetic alert"),
    ] {
        let read = index
            .read(&accepted, text_node(&index, name)?, &[])?
            .ok_or_else(|| format!("the {name} section's entry is present"))?;
        assert_eq!(
            read.get("value").and_then(Value::as_str),
            Some(text),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn a_second_section_of_one_name_is_still_refused() -> Result<(), Box<dyn Error>> {
    let index = index()?;
    let mut content = both();
    content.push(section(
        "Notes",
        &entry("openEHR-EHR-EVALUATION.ferrobridge_note.v1", "another note"),
    ));
    let error = index
        .accept(composition(&content))
        .err()
        .ok_or("a second Notes section exceeds its occurrences")?;
    assert!(
        matches!(&error, PathError::InvalidComposition { messages, .. }
            if messages.iter().any(|m| m.contains("Notes") && m.contains("occurrences"))),
        "{error:?}"
    );
    Ok(())
}
