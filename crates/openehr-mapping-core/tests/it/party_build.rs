// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The party build through the `openehr-rm` types: a canonical `PARTY` from
//! node values of the testkit's synthetic `PERSON` template, decoded strictly
//! as `Person`, and every value of a form the build does not take refused.
//! No specification governs a party build: our own design.

use ferrobridge_testkit::fixtures::MINIMAL_EVALUATION_OPT;
use ferrobridge_testkit::fixtures::PERSON_OPT;
use ferrobridge_testkit::fixtures::PERSON_TEMPLATE_ID;
use ferrobridge_testkit::fixtures::ROLE_OPT;
use ferrobridge_testkit::fixtures::ROLE_TEMPLATE_ID;
use openehr_mapping_core::composition::NodeValue;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::party::PartyClass;
use openehr_mapping_core::party::PartyError;
use openehr_mapping_core::template::TemplateSource;
use openehr_rm::v1_2::demographic::person::Person;
use serde_json::Value;
use serde_json::json;

/// The `aqlPath` of the name element of the one identity.
const NAME: &str = "/identities[at0003]/details[at0004]/items[at0005]/value";

/// The `aqlPath` of the identifier element of the details tree.
const IDENTIFIER: &str = "/details[at0001]/items[at0002]/value";

/// Builds the index over an OPT 1.4 fixture.
#[expect(clippy::expect_used, reason = "test plumbing")]
fn index_of(opt: &str) -> WebTemplateIndex {
    let opt = openehr_its::opt14::from_xml(opt).expect("the fixture parses");
    WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(opt))).expect("the fixture indexes")
}

/// Returns a whole value at the node of `path`, at the first instance of
/// each repeating node on the way.
#[expect(clippy::expect_used, reason = "test plumbing")]
fn raw(index: &WebTemplateIndex, path: &str, value: Value) -> NodeValue {
    let node = index.node(&AqlPath::new(path)).expect("the node");
    let repeating = index
        .nodes()
        .filter(|candidate| candidate.repeats() && path.starts_with(candidate.aql_path().as_str()))
        .count();
    NodeValue::new(node, value)
        .with_occurrences(vec![RmPosition::first(); repeating])
        .with_datum("raw")
}

/// The values of one synthetic person: a name and an identifier.
fn person_values(index: &WebTemplateIndex) -> Vec<NodeValue> {
    vec![
        raw(
            index,
            NAME,
            json!({"_type": "DV_TEXT", "value": "Synthetic Person"}),
        ),
        raw(
            index,
            IDENTIFIER,
            json!({
                "_type": "DV_IDENTIFIER",
                "issuer": "Synthetic issuer",
                "assigner": "Synthetic assigner",
                "id": "SYN-0001",
                "type": "MR"
            }),
        ),
    ]
}

#[test]
fn a_person_builds_and_decodes_as_a_person() {
    let index = index_of(PERSON_OPT);
    let party = index
        .build_party(&person_values(&index))
        .expect("the person builds");
    assert_eq!(party.class(), PartyClass::Person);
    assert_eq!(party.template_id(), PERSON_TEMPLATE_ID);
    let value = party.value();
    let person: Person =
        openehr_its::json::from_canonical_value(value).expect("the party decodes as a PERSON");
    assert_eq!(person.identities.len(), 1);
    assert!(
        value.get("identities").is_some_and(Value::is_array),
        "the identities are a list: {value}"
    );
    assert_eq!(
        value.pointer("/identities/0/details/items/0/value/value"),
        Some(&Value::from("Synthetic Person"))
    );
    assert_eq!(
        value.pointer("/identities/0/details/name/value"),
        Some(&Value::from("Identity details"))
    );
    assert_eq!(
        value.pointer("/details/items/0/value/id"),
        Some(&Value::from("SYN-0001"))
    );
    assert_eq!(
        value.pointer("/archetype_details/rm_version"),
        Some(&Value::from("1.1.0"))
    );
    for field in ["category", "language", "territory", "composer", "context"] {
        assert!(value.get(field).is_none(), "the party carries `{field}`");
    }
}

#[test]
fn a_built_person_reads_back_through_the_index() {
    let index = index_of(PERSON_OPT);
    let party = index
        .build_party(&person_values(&index))
        .expect("the person builds");
    let name = index.node(&AqlPath::new(NAME)).expect("the name node");
    let read = index.read(&party, name, &[]).expect("the read resolves");
    assert_eq!(
        read.and_then(|value| value.get("value").cloned()),
        Some(Value::from("Synthetic Person"))
    );
}

#[test]
fn a_person_without_an_identity_is_refused() {
    let index = index_of(PERSON_OPT);
    let identifier = person_values(&index).split_off(1);
    let refused = index.build_party(&identifier);
    assert!(
        matches!(
            refused,
            Err(PartyError::Invalid {
                class: PartyClass::Person,
                ..
            })
        ),
        "a PERSON with no identity built: {refused:?}"
    );
}

#[test]
fn a_context_value_is_refused() {
    let index = index_of(PERSON_OPT);
    let mut values = person_values(&index);
    values.push(NodeValue::context("language", Value::from("en")));
    let refused = index.build_party(&values);
    assert!(
        matches!(refused, Err(PartyError::UnsupportedValue { .. })),
        "a ctx/ value reached a party: {refused:?}"
    );
}

#[test]
fn a_value_not_under_raw_is_refused() {
    let index = index_of(PERSON_OPT);
    let name = index.node(&AqlPath::new(NAME)).expect("the name node");
    let values = vec![
        NodeValue::new(name, Value::from("Synthetic Person"))
            .with_occurrences(vec![RmPosition::first()])
            .with_datum("value"),
    ];
    let refused = index.build_party(&values);
    assert!(
        matches!(refused, Err(PartyError::UnsupportedValue { .. })),
        "a suffixed value reached a party: {refused:?}"
    );
}

#[test]
fn a_value_the_class_refuses_is_refused() {
    let index = index_of(PERSON_OPT);
    let mut values = person_values(&index);
    values.truncate(1);
    values.push(raw(
        &index,
        IDENTIFIER,
        json!({"_type": "DV_IDENTIFIER", "id": "SYN-0002", "unknown": true}),
    ));
    let refused = index.build_party(&values);
    assert!(
        matches!(refused, Err(PartyError::Invalid { .. })),
        "a value the class refuses built: {refused:?}"
    );
}

#[test]
fn a_composition_template_builds_no_party() {
    let index = index_of(MINIMAL_EVALUATION_OPT);
    let refused = index.build_party(&[]);
    assert!(
        matches!(refused, Err(PartyError::NotAParty { ref rm_type, .. }) if rm_type == "COMPOSITION"),
        "a composition template built a party: {refused:?}"
    );
}

#[test]
fn a_later_value_at_one_place_replaces_the_earlier() {
    let index = index_of(PERSON_OPT);
    let mut values = person_values(&index);
    values.push(raw(
        &index,
        NAME,
        json!({"_type": "DV_TEXT", "value": "Synthetic Other"}),
    ));
    let party = index.build_party(&values).expect("the person builds");
    assert_eq!(
        party
            .value()
            .pointer("/identities/0/details/items/0/value/value"),
        Some(&Value::from("Synthetic Other"))
    );
}

/// The values of one synthetic role: a name, and the performer at the root.
fn role_values(index: &WebTemplateIndex, performer: Option<Value>) -> Vec<NodeValue> {
    let mut values = vec![raw(
        index,
        NAME,
        json!({"_type": "DV_TEXT", "value": "Synthetic Role"}),
    )];
    if let Some(performer) = performer {
        values.push(
            NodeValue::new(
                index.root(),
                json!({"_type": "ROLE", "performer": performer}),
            )
            .with_datum("raw"),
        );
    }
    values
}

/// A synthetic `PARTY_REF` to a party of the demographic store.
fn performer() -> Value {
    json!({
        "_type": "PARTY_REF",
        "namespace": "demographic",
        "type": "ACTOR",
        "id": {"_type": "HIER_OBJECT_ID", "value": "8f0e7d4c-0000-4000-8000-000000000001"}
    })
}

#[test]
fn a_value_at_the_role_root_places_the_performer_on_the_root() {
    let index = index_of(ROLE_OPT);
    let built = index
        .build_party(&role_values(&index, Some(performer())))
        .expect("a ROLE with a performer builds");
    assert_eq!(built.class(), PartyClass::Role);
    assert_eq!(built.value().get("performer"), Some(&performer()));
    assert_eq!(
        built
            .value()
            .pointer("/archetype_details/template_id/value"),
        Some(&Value::from(ROLE_TEMPLATE_ID))
    );
}

/// RM Demographic IM, the `ROLE` class: `performer` is `1..1`.
#[test]
fn a_role_without_a_performer_is_refused_naming_the_rm_attribute() {
    let index = index_of(ROLE_OPT);
    let refused = index.build_party(&role_values(&index, None));
    assert!(
        matches!(refused, Err(PartyError::RoleWithoutPerformer { .. })),
        "a ROLE without a performer built: {refused:?}"
    );
    if let Err(error) = refused {
        assert!(error.to_string().contains("ROLE.performer"), "{error}");
    }
}

/// RM Demographic IM, the `ROLE` class: a role a CDR serves with no
/// `performer` is no valid `ROLE`.
#[test]
fn a_served_role_without_a_performer_is_refused() {
    let index = index_of(ROLE_OPT);
    let mut served = index
        .build_party(&role_values(&index, Some(performer())))
        .expect("a ROLE with a performer builds")
        .into_value();
    if let Some(object) = served.as_object_mut() {
        object.remove("performer");
    }
    let refused = index.accept_party(&served);
    assert!(
        matches!(refused, Err(PartyError::RoleWithoutPerformer { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_root_value_naming_no_attribute_of_the_class_is_refused() {
    let index = index_of(ROLE_OPT);
    let mut values = role_values(&index, Some(performer()));
    values.push(
        NodeValue::new(index.root(), json!({"_type": "ROLE", "carer": performer()}))
            .with_datum("raw"),
    );
    let refused = index.build_party(&values);
    assert!(
        matches!(refused, Err(PartyError::UnknownAttribute { .. })),
        "{refused:?}"
    );
}
