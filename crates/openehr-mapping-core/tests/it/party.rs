// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The build of a canonical `PARTY` from a party Web Template.
//!
//! Simplified Formats defines FLAT for compositions only, so these cases
//! record what the `openehr-sdt` party builder makes of the testkit's
//! synthetic `PERSON` template: a `PERSON` that decodes strictly, with its
//! list attributes as arrays and no composition attribute. No specification
//! governs a party build: our own design.

use ferrobridge_testkit::fixtures::PERSON_OPT;
use ferrobridge_testkit::fixtures::PERSON_TEMPLATE_ID;
use openehr_mapping_core::composition::NodeValue;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::template::PathError;
use openehr_mapping_core::template::TemplateSource;
use openehr_rm::v1_2::demographic::person::Person;
use openehr_sdt::flat::build::BuildOptions;

/// The timestamp the builder fills the `ctx/time` default from.
const NOW: &str = "2026-10-19T08:00:00Z";

/// The `aqlPath` of the name element of the one identity.
const NAME: &str = "/identities[at0003]/details[at0004]/items[at0005]/value";

/// Builds the index over the testkit's synthetic `PERSON` template.
#[expect(clippy::expect_used, reason = "test plumbing")]
fn person_index() -> WebTemplateIndex {
    let opt = openehr_its::opt14::from_xml(PERSON_OPT).expect("the fixture parses");
    WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(opt))).expect("the fixture indexes")
}

#[test]
fn a_party_template_indexes_with_its_party_root() {
    let index = person_index();
    assert_eq!(index.template_id(), PERSON_TEMPLATE_ID);
    assert_eq!(index.root().rm_type(), "PERSON");
    assert_eq!(
        index.root().node_id(),
        Some("openEHR-DEMOGRAPHIC-PERSON.ferrobridge_person.v1")
    );
    let identity = index
        .node(&AqlPath::new("/identities[at0003]"))
        .expect("the identity node");
    assert_eq!(identity.rm_type(), "PARTY_IDENTITY");
}

/// What the `openehr-sdt` party builder builds from a FLAT document of the
/// party template: `PARTY.identities` is a list (RM Demographic IM, the
/// `PARTY` class), no `COMPOSITION` attribute reaches the root, and the
/// result decodes strictly as a `PERSON`.
#[test]
fn openehr_sdt_builds_a_canonical_person_from_a_party_web_template() {
    let index = person_index();
    let name = index.node(&AqlPath::new(NAME)).expect("the name node");
    let mut flat = serde_json::Map::new();
    flat.insert(
        format!("{}|value", name.flat_id().as_str()),
        serde_json::Value::from("Synthetic Person"),
    );
    let built = openehr_sdt::flat::convert::party_from_flat(
        &flat,
        index.web_template(),
        &BuildOptions::default().with_rm_version("1.1.0"),
    )
    .expect("the party builder builds the person");
    assert_eq!(built.get("_type"), Some(&serde_json::Value::from("PERSON")));
    assert!(
        built
            .get("identities")
            .is_some_and(serde_json::Value::is_array),
        "the identities are a list: {built}"
    );
    for field in ["category", "language", "territory", "composer", "context"] {
        assert!(built.get(field).is_none(), "the party carries `{field}`");
    }
    assert_eq!(
        built.pointer("/archetype_details/rm_version"),
        Some(&serde_json::Value::from("1.1.0"))
    );
    let person = openehr_its::json::from_canonical_value::<Person>(&built)
        .expect("the built PERSON decodes");
    assert_eq!(person.identities.len(), 1);
}

#[test]
fn the_composition_build_refuses_a_party_template() {
    let index = person_index();
    let name = index.node(&AqlPath::new(NAME)).expect("the name node");
    let values = vec![
        NodeValue::new(
            name,
            serde_json::json!({"_type": "DV_TEXT", "value": "Synthetic Person"}),
        )
        .with_datum("raw"),
    ];
    let refused = index.build_composition(&values, NOW);
    assert!(
        matches!(refused, Err(PathError::InvalidComposition { .. })),
        "a party template built as a composition: {refused:?}"
    );
}
