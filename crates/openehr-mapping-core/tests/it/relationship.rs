// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A party's relationship: FerroBRIDGE's related person `PERSON`, its
//! `PARTY_RELATIONSHIP` target written from a value at the relationship node
//! and its source the party itself once its uid is known (RM Demographic IM,
//! the `PARTY` class, invariant `Relationships_validity`).

use std::error::Error;

use ferrobridge_testkit::fixtures::FERROBRIDGE_RELATED_PERSON_ADL2;
use ferrobridge_testkit::fixtures::FERROBRIDGE_RELATED_PERSON_ID;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::assemble::parse_artefact;
use openehr_adl::opt::create_opt;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::composition::NodeValue;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::party::PartyError;
use openehr_mapping_core::template::TemplateSource;
use serde_json::Value;
use serde_json::json;

/// The `aqlPath` of the relationship to the patient.
const RELATIONSHIP: &str = "/relationships[id10]";

/// The `aqlPath` of the coded kind of relationship.
const KIND: &str = "/relationships[id10]/details[id11]/items[id12]/value";

/// The `aqlPath` of the family name.
const FAMILY: &str = "/identities[id5]/details[id6]/items[id9]/value";

/// The uid the CDR gives the related person.
const UID: &str = "a1b2c3d4-0000-4000-8000-0000000000f1";

/// Compiles the related person `PERSON` and indexes it as a CDR serves it.
fn related_person() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let archetype = parse_artefact(FERROBRIDGE_RELATED_PERSON_ADL2, Dialect::Adl2)
        .map_err(|errors| format!("{errors:?}"))?;
    let raw = create_opt(&archetype, &ArchetypeRepository::default())?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
        resolved_id: String::from(FERROBRIDGE_RELATED_PERSON_ID),
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
        .with_occurrences(vec![
            openehr_mapping_core::composition::RmPosition::first();
            repeating
        ])
        .with_datum("raw"))
}

/// Returns the `PARTY_REF` of the patient `PERSON`.
fn patient() -> Value {
    json!({
        "_type": "PARTY_REF",
        "id": {"_type": "HIER_OBJECT_ID", "value": "a1b2c3d4-0000-4000-8000-0000000000e1"},
        "namespace": "demographic",
        "type": "ACTOR"
    })
}

/// Returns the values of a related person whose relationship to the patient
/// carries `relationship`.
fn values(index: &WebTemplateIndex, relationship: Value) -> Result<Vec<NodeValue>, Box<dyn Error>> {
    Ok(vec![
        raw(
            index,
            FAMILY,
            json!({"_type": "DV_TEXT", "value": "Relative"}),
        )?,
        raw(
            index,
            KIND,
            json!({
                "_type": "DV_CODED_TEXT",
                "value": "mother",
                "defining_code": {
                    "_type": "CODE_PHRASE",
                    "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "http://terminology.hl7.org/CodeSystem/v3-RoleCode"},
                    "code_string": "MTH"
                }
            }),
        )?,
        raw(index, RELATIONSHIP, relationship)?,
    ])
}

#[test]
fn a_built_relationship_awaits_the_party_as_its_source() -> Result<(), Box<dyn Error>> {
    let index = related_person()?;
    let party = index.build_party(&values(
        &index,
        json!({"_type": "PARTY_RELATIONSHIP", "target": patient()}),
    )?)?;
    assert!(party.awaits_source());
    assert_eq!(
        None,
        party.value().get("relationships"),
        "{}",
        party.value()
    );
    let sourced = index.source_relationships(&party, UID, "demographic")?;
    assert!(!sourced.awaits_source());
    let relationship = &sourced.value()["relationships"][0];
    assert_eq!(patient(), relationship["target"], "{relationship}");
    assert_eq!(
        json!({
            "_type": "PARTY_REF",
            "id": {"_type": "HIER_OBJECT_ID", "value": UID},
            "namespace": "demographic",
            "type": "PERSON"
        }),
        relationship["source"],
        "{relationship}"
    );
    assert_eq!(
        json!("MTH"),
        relationship["details"]["items"][0]["value"]["defining_code"]["code_string"],
        "{relationship}"
    );
    Ok(())
}

#[test]
fn a_value_writing_the_source_of_a_relationship_is_refused() -> Result<(), Box<dyn Error>> {
    let index = related_person()?;
    let refused = index.build_party(&values(
        &index,
        json!({"_type": "PARTY_RELATIONSHIP", "target": patient(), "source": patient()}),
    )?);
    assert!(
        matches!(refused, Err(PartyError::RelationshipSource { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_party_without_relationships_awaits_no_source() -> Result<(), Box<dyn Error>> {
    let index = related_person()?;
    let party = index.build_party(&[raw(
        &index,
        FAMILY,
        json!({"_type": "DV_TEXT", "value": "Relative"}),
    )?])?;
    assert!(!party.awaits_source());
    assert_eq!(
        party,
        index.source_relationships(&party, UID, "demographic")?
    );
    Ok(())
}
