// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The release-neutral `ElementDefinition` read: the same element read from
//! an STU3 and an R4 definition gives the same answer where the releases
//! agree (<https://hl7.org/fhir/STU3/elementdefinition.html>,
//! <https://hl7.org/fhir/R4/elementdefinition.html>).

use std::fs;

use fhir_codegen::package::without_byte_order_mark;
use fhir_types::codec::{Json, Path, Value, expect_object};
use fhir_types::definition::{ChoiceValue, ElementDefinitionRead};

use crate::vendor_dir;

/// Decodes `text`, an element definition of either release, as `T`.
fn decode<T: Json>(text: &str) -> T {
    let value: Value = serde_json::from_str(text).expect("the fixture is JSON");
    let mut path = Path::root("ElementDefinition");
    let object = expect_object(&value, &path).expect("the fixture is an object");
    T::from_json(object, &mut path).expect("the element definition decodes")
}

/// The snapshot element at `path` of the core `Observation` definition of the
/// vendored package `package`, decoded as `T`.
fn observation_element<T: Json>(package: &str, path: &str) -> T {
    let file = vendor_dir()
        .join(package)
        .join("package/StructureDefinition-Observation.json");
    let text = fs::read_to_string(file).expect("the package carries Observation");
    let raw: serde_json::Value =
        serde_json::from_str(without_byte_order_mark(&text)).expect("the definition parses");
    let element = raw["snapshot"]["element"]
        .as_array()
        .and_then(|elements| elements.iter().find(|element| element["path"] == path))
        .expect("the snapshot carries the element");
    decode(&element.to_string())
}

/// Holds that `T` implements the read.
const fn implements_the_read<T: ElementDefinitionRead>() {}

#[test]
fn every_version_implements_the_read() {
    implements_the_read::<fhir_types::stu3::element_definition::ElementDefinition>();
    implements_the_read::<fhir_types::r4::element_definition::ElementDefinition>();
    implements_the_read::<fhir_types::r4b::element_definition::ElementDefinition>();
    implements_the_read::<fhir_types::r5::element_definition::ElementDefinition>();
    implements_the_read::<fhir_types::r6::element_definition::ElementDefinition>();
}

type Stu3Element = fhir_types::stu3::element_definition::ElementDefinition;
type R4Element = fhir_types::r4::element_definition::ElementDefinition;

/// Every target profile the types of `element` name, in order.
fn target_profiles(element: &dyn ElementDefinitionRead) -> Vec<String> {
    element
        .types()
        .iter()
        .flat_map(|ty| {
            ty.target_profiles
                .iter()
                .map(|profile| (*profile).to_owned())
        })
        .collect()
}

#[test]
fn a_single_target_profile_per_type_reads_as_the_r4_list() {
    let stu3: Stu3Element = observation_element("hl7.fhir.r3.core", "Observation.subject");
    let r4: R4Element = observation_element("hl7.fhir.r4.core", "Observation.subject");
    assert_eq!(stu3.path(), r4.path());
    assert_eq!((stu3.min(), stu3.max()), (r4.min(), r4.max()));
    assert_eq!(target_profiles(&stu3), target_profiles(&r4));
    assert_eq!(
        stu3.types().len(),
        4,
        "STU3 states one type entry per target"
    );
    assert!(stu3.types().iter().all(|ty| ty.code == Some("Reference")
        && ty.target_profiles.len() == 1
        && ty.profiles.is_empty()));
    assert_eq!(r4.types().len(), 1);
}

#[test]
fn a_value_set_reference_reads_as_the_value_set_url() {
    let stu3: Stu3Element = observation_element("hl7.fhir.r3.core", "Observation.status");
    let r4: R4Element = observation_element("hl7.fhir.r4.core", "Observation.status");
    let (stu3, r4) = (
        stu3.binding().expect("STU3 binds the status"),
        r4.binding().expect("R4 binds the status"),
    );
    assert_eq!(stu3.strength, Some("required"));
    assert_eq!(stu3.strength, r4.strength);
    assert_eq!(
        stu3.value_set,
        Some("http://hl7.org/fhir/ValueSet/observation-status")
    );
    assert_eq!(
        r4.value_set,
        Some("http://hl7.org/fhir/ValueSet/observation-status|4.0.1"),
        "R4 pins the version on the canonical"
    );
}

#[test]
fn a_value_set_uri_reads_as_the_value_set_url() {
    let stu3: Stu3Element = decode(
        r#"{"path":"Observation.code","binding":{"strength":"example","valueSetUri":"http://example.org/vs"}}"#,
    );
    assert_eq!(
        stu3.binding().and_then(|binding| binding.value_set),
        Some("http://example.org/vs")
    );
}

#[test]
fn constraints_read_alike_in_both_releases() {
    let stu3: Stu3Element = observation_element("hl7.fhir.r3.core", "Observation");
    let r4: R4Element = observation_element("hl7.fhir.r4.core", "Observation");
    let key = |element: &dyn ElementDefinitionRead, key: &str| {
        element
            .constraints()
            .into_iter()
            .find(|constraint| constraint.key == Some(key))
            .map(|constraint| {
                (
                    constraint.severity.map(str::to_owned),
                    constraint.human.map(str::to_owned),
                )
            })
    };
    assert_eq!(key(&stu3, "obs-6"), key(&r4, "obs-6"));
    assert!(key(&stu3, "obs-6").is_some());
}

/// One synthetic element carrying slicing, a fixed value, a profile and a
/// content reference, written once and decoded by each release; the forms
/// the releases share spell it identically.
const SHARED: &str = r##"{
    "id": "Observation.component:systolic",
    "path": "Observation.component",
    "sliceName": "systolic",
    "min": 1,
    "max": "1",
    "maxLength": 10,
    "contentReference": "#Observation.component",
    "slicing": {"discriminator": [{"type": "pattern", "path": "code"}], "ordered": false, "rules": "open"},
    "fixedCode": "final",
    "constraint": [{"key": "syn-1", "severity": "error", "human": "synthetic", "expression": "true", "xpath": "true()"}]
}"##;

#[test]
fn a_shared_element_reads_the_same_in_both_releases() {
    let stu3: Stu3Element = decode(SHARED);
    let r4: R4Element = decode(SHARED);
    for element in [&stu3 as &dyn ElementDefinitionRead, &r4] {
        assert_eq!(element.element_id(), Some("Observation.component:systolic"));
        assert_eq!(element.slice_name(), Some("systolic"));
        assert_eq!(element.min(), Some(1));
        assert_eq!(element.max(), Some("1"));
        assert_eq!(element.max_length(), Some(10));
        assert_eq!(element.content_reference(), Some("#Observation.component"));
        let slicing = element.slicing().expect("the element is sliced");
        assert_eq!(slicing.rules, Some("open"));
        assert_eq!(slicing.ordered, Some(false));
        assert_eq!(slicing.discriminators.len(), 1);
        assert_eq!(slicing.discriminators[0].kind, Some("pattern"));
        assert_eq!(slicing.discriminators[0].path, Some("code"));
        let fixed = element.fixed().expect("the fixed value encodes");
        assert_eq!(
            fixed,
            Some(ChoiceValue {
                suffix: "Code",
                value: Some(Value::String(String::from("final"))),
                element: None,
            })
        );
        assert_eq!(element.pattern().expect("no pattern encodes"), None);
        let constraint = &element.constraints()[0];
        assert_eq!(
            (constraint.key, constraint.expression),
            (Some("syn-1"), Some("true"))
        );
    }
}

#[test]
fn a_constraint_xpath_reads_as_absent_in_r5_which_drops_it() {
    let r5: fhir_types::r5::element_definition::ElementDefinition = decode(
        r#"{"path":"Observation","constraint":[{"key":"syn-1","severity":"error","human":"synthetic","expression":"true"}]}"#,
    );
    let constraint = &r5.constraints()[0];
    assert_eq!(constraint.xpath, None);
    assert_eq!(constraint.expression, Some("true"));
}
