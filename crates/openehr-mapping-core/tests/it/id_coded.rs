// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An id-coded ADL 2 template: the reference CDR's professional `PERSON`,
//! compiled into an operational template with `openehr-adl`, indexed as an
//! `adl2` template, its nodes addressed by their id-codes, and a party built
//! over them.
//!
//! ADL 2.4 admits two node identifier coding systems, at-codes and id-codes
//! (<https://specifications.openehr.org/releases/AM/latest/ADL2.html>, §4.2.5
//! Node Identifier and Coding Systems), and an archetype path predicate is a
//! shortcut for `[@archetype_node_id = '…']` (BASE Release 1.2.0,
//! Architecture Overview, §Paths and Locators), so a path over an id-coded
//! template names its id-codes.

use std::error::Error;

use ferrobridge_testkit::fixtures::FERROEHR_PROFESSIONAL_PERSON_ADL2;
use ferrobridge_testkit::fixtures::FERROEHR_PROFESSIONAL_PERSON_ID;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::assemble::parse_artefact;
use openehr_adl::opt::create_opt;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::composition::NodeValue;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::party::PartyClass;
use openehr_mapping_core::template::Generation;
use openehr_mapping_core::template::TemplateSource;
use serde_json::Value;
use serde_json::json;

/// The `aqlPath` of the identifier element of the details tree.
const IDENTIFIER: &str = "/details[id2]/items[id3]/value";

/// The `aqlPath` of the family name element of the one identity.
const FAMILY: &str = "/identities[id4]/details[id5]/items[id8]/value";

/// Compiles the professional `PERSON` into an operational template and
/// indexes it as the reference CDR serves it.
fn professional() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let archetype = parse_artefact(FERROEHR_PROFESSIONAL_PERSON_ADL2, Dialect::Adl2)
        .map_err(|errors| format!("{errors:?}"))?;
    let raw = create_opt(&archetype, &ArchetypeRepository::default())?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
        resolved_id: String::from(FERROEHR_PROFESSIONAL_PERSON_ID),
    })?)
}

/// Returns a whole value at the node of `path`, at the first instance of
/// each repeating node on the way.
fn raw(index: &WebTemplateIndex, path: &str, value: Value) -> Result<NodeValue, Box<dyn Error>> {
    let node = index.node(&AqlPath::new(path))?;
    let repeating = index
        .nodes()
        .filter(|candidate| candidate.repeats() && path.starts_with(candidate.aql_path().as_str()))
        .count();
    Ok(NodeValue::new(node, value)
        .with_occurrences(vec![RmPosition::first(); repeating])
        .with_datum("raw"))
}

#[test]
fn an_id_coded_template_indexes_as_an_adl2_template() -> Result<(), Box<dyn Error>> {
    let index = professional()?;
    assert_eq!(Generation::Adl2, index.generation());
    assert_eq!(
        Some("openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1"),
        index.root().node_id(),
        "an archetype root carries its interface id"
    );
    for path in [
        IDENTIFIER,
        FAMILY,
        "/contacts[id9]/addresses[id10]/details[id11]/items[id12]/value",
    ] {
        let node = index.node(&AqlPath::new(path))?;
        assert_eq!(path, node.aql_path().as_str());
    }
    Ok(())
}

#[test]
fn an_at_coded_path_names_no_node_of_an_id_coded_template() -> Result<(), Box<dyn Error>> {
    let index = professional()?;
    assert!(
        index
            .node(&AqlPath::new("/details[at0001]/items[at0002]/value"))
            .is_err(),
        "the template names its nodes with id-codes only"
    );
    Ok(())
}

#[test]
fn a_person_builds_over_the_id_codes() -> Result<(), Box<dyn Error>> {
    let index = professional()?;
    let values = vec![
        raw(
            &index,
            IDENTIFIER,
            json!({"_type": "DV_IDENTIFIER", "issuer": "Synthetic issuer", "assigner": "Synthetic assigner", "id": "SYN-0001", "type": "login"}),
        )?,
        raw(
            &index,
            FAMILY,
            json!({"_type": "DV_TEXT", "value": "Synthetic"}),
        )?,
    ];
    let party = index.build_party(&values)?;
    assert_eq!(PartyClass::Person, party.class());
    let value = party.value();
    assert_eq!(
        json!("id3"),
        value["details"]["items"][0]["archetype_node_id"],
        "{value}"
    );
    assert_eq!(
        json!("SYN-0001"),
        value["details"]["items"][0]["value"]["id"],
        "{value}"
    );
    assert_eq!(
        json!("id4"),
        value["identities"][0]["archetype_node_id"],
        "{value}"
    );
    Ok(())
}
