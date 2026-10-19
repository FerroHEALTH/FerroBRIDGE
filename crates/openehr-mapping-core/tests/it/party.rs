// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The build of a canonical `PARTY` from a party Web Template.
//!
//! Simplified Formats defines FLAT for compositions only, so whether the
//! `openehr-sdt` composition builder builds a correct `PARTY` from a party
//! template was unverified. These cases record the answer against the
//! testkit's synthetic `PERSON` template: it does not, so the bridge builds the
//! party through the `openehr-rm` types (#905), and the gap is the upstream
//! request FerroHEALTH/FerroEHR#3810. No specification governs a party build:
//! our own design.

use ferrobridge_testkit::fixtures::PERSON_OPT;
use ferrobridge_testkit::fixtures::PERSON_TEMPLATE_ID;
use openehr_mapping_core::composition::NodeValue;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::template::PathError;
use openehr_mapping_core::template::TemplateSource;
use openehr_rm::v1_2::demographic::person::Person;

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

/// What `openehr-sdt` 0.0.87 builds from a FLAT document of the party
/// template, recorded so a fixed builder fails this case and the build moves
/// to it (#906).
#[test]
fn openehr_sdt_builds_no_canonical_person_from_a_party_web_template() {
    let index = person_index();
    let name = index.node(&AqlPath::new(NAME)).expect("the name node");
    let mut flat = serde_json::Map::new();
    flat.insert(
        format!("{}|value", name.flat_id().as_str()),
        serde_json::Value::from("Synthetic Person"),
    );
    let document = openehr_sdt::flat::sim::flat::parse_flat(&flat).expect("the FLAT parses");
    let built = openehr_sdt::flat::build::build_composition(&document, index.web_template(), NOW)
        .expect("the builder returns an object");
    assert_eq!(built.get("_type"), Some(&serde_json::Value::from("PERSON")));
    // The composition builder writes `COMPOSITION.category` onto the root.
    assert!(
        built.get("category").is_some(),
        "the builder no longer writes a category: {built}"
    );
    // `PARTY.identities` is a list (RM Demographic IM) and is built as one
    // object, because the builder's multiple-attribute set is seeded from
    // `COMPOSITION`, `EHR_STATUS` and `FOLDER` only.
    assert!(
        built
            .get("identities")
            .is_some_and(serde_json::Value::is_object),
        "the builder now builds the identities as a list: {built}"
    );
    let decoded = openehr_its::json::from_canonical_value::<Person>(&built);
    assert!(
        decoded.is_err(),
        "the built PERSON decodes, so the build can move to openehr-sdt (#906)"
    );
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
