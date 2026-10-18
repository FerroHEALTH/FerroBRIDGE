// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The release-neutral `ElementDefinition` read: the same element read from
//! an STU3 and an R4 definition gives the same answer where the releases
//! agree (<https://hl7.org/fhir/STU3/elementdefinition.html>,
//! <https://hl7.org/fhir/R4/elementdefinition.html>).

use std::fs;

use fhir_codegen::package::without_byte_order_mark;
use fhir_types::codec::{Json, Path, Value, expect_object};
use fhir_types::definition::{
    ChoiceValue, ContextRead, ElementDefinitionRead, StructureDefinitionRead,
};

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
    let element = raw
        .pointer("/snapshot/element")
        .and_then(serde_json::Value::as_array)
        .and_then(|elements| {
            elements.iter().find(|element| {
                element.get("path").and_then(serde_json::Value::as_str) == Some(path)
            })
        })
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
    let elements: [&dyn ElementDefinitionRead; 2] = [&stu3, &r4];
    for element in elements {
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
/// The element at `path` of the core definition `name` of the vendored
/// package `package`, decoded as `T`.
fn core_element<T: Json>(package: &str, name: &str, path: &str) -> T {
    let raw = core_definition_json(package, name);
    let element = raw
        .pointer("/snapshot/element")
        .and_then(serde_json::Value::as_array)
        .and_then(|elements| {
            elements
                .iter()
                .find(|element| element.get("id").and_then(serde_json::Value::as_str) == Some(path))
        })
        .expect("the snapshot carries the element");
    decode(&element.to_string())
}

/// The core definition `name` of the vendored package `package`, as JSON.
fn core_definition_json(package: &str, name: &str) -> serde_json::Value {
    let file = vendor_dir()
        .join(package)
        .join(format!("package/StructureDefinition-{name}.json"));
    let text = fs::read_to_string(file).expect("the package carries the definition");
    serde_json::from_str(without_byte_order_mark(&text)).expect("the definition parses")
}

/// The core definition `name` of the vendored package `package`, decoded as
/// `T`.
fn core_definition<T: Json>(package: &str, name: &str) -> T {
    let text = core_definition_json(package, name).to_string();
    let value: Value = serde_json::from_str(&text).expect("the definition is JSON");
    let mut path = Path::root("StructureDefinition");
    let object = expect_object(&value, &path).expect("the definition is an object");
    T::from_json(object, &mut path).expect("the definition decodes")
}

type Stu3Structure = fhir_types::stu3::structure_definition::StructureDefinition;
type R4Structure = fhir_types::r4::structure_definition::StructureDefinition;

/// Holds that `T` implements the structure read.
const fn implements_the_structure_read<T: StructureDefinitionRead>() {}

#[test]
fn every_version_implements_the_structure_read() {
    implements_the_structure_read::<Stu3Structure>();
    implements_the_structure_read::<R4Structure>();
    implements_the_structure_read::<fhir_types::r4b::structure_definition::StructureDefinition>();
    implements_the_structure_read::<fhir_types::r5::structure_definition::StructureDefinition>();
    implements_the_structure_read::<fhir_types::r6::structure_definition::StructureDefinition>();
}

#[test]
fn is_modifier_reads_alike_in_both_releases() {
    let stu3: Stu3Element = observation_element("hl7.fhir.r3.core", "Observation.status");
    let r4: R4Element = observation_element("hl7.fhir.r4.core", "Observation.status");
    assert_eq!(stu3.is_modifier(), Some(true));
    assert_eq!(stu3.is_modifier(), r4.is_modifier());
}

#[test]
fn a_maximum_value_set_reads_as_its_url_from_a_reference_and_a_canonical() {
    // STU3 states elementdefinition-maxValueSet as valueReference, R4 as valueCanonical.
    let stu3: Stu3Element = core_element("hl7.fhir.r3.core", "Account", "Account.language");
    let r4: R4Element = core_element("hl7.fhir.r4.core", "Account", "Account.language");
    let max = |element: &dyn ElementDefinitionRead| {
        element
            .binding()
            .and_then(|binding| binding.max_value_set.map(str::to_owned))
    };
    assert_eq!(
        max(&stu3).as_deref(),
        Some("http://hl7.org/fhir/ValueSet/all-languages")
    );
    assert_eq!(max(&stu3), max(&r4));
}

#[test]
fn a_best_practice_constraint_reads_its_flag() {
    let r4: R4Element = core_element("hl7.fhir.r4.core", "DomainResource", "DomainResource");
    let flag = |key: &str| {
        r4.constraints()
            .into_iter()
            .find(|constraint| constraint.key == Some(key))
            .and_then(|constraint| constraint.best_practice)
    };
    assert_eq!(flag("dom-6"), Some(true));
    assert_eq!(flag("dom-2"), None, "a constraint without the extension");
}

#[test]
fn a_primitive_type_reads_its_regex_where_the_package_states_one() {
    // R4 states the lexical form of string.value in the regex extension of its type; the
    // STU3 package states none.
    let stu3: Stu3Element = core_element("hl7.fhir.r3.core", "string", "string.value");
    let r4: R4Element = core_element("hl7.fhir.r4.core", "string", "string.value");
    let regex = |element: &dyn ElementDefinitionRead| {
        element
            .types()
            .first()
            .and_then(|ty| ty.regex.map(str::to_owned))
    };
    assert_eq!(regex(&r4).as_deref(), Some("[ \\r\\n\\t\\S]+"));
    assert_eq!(regex(&stu3), None);
}

#[test]
fn an_stu3_primitive_reads_its_regex_from_structuredefinition_regex() {
    // STU3 states the lexical form of date.value in the structuredefinition-regex extension
    // (STU3 datatypes.html#primitive), R4 in the regex extension.
    let stu3: Stu3Element = core_element("hl7.fhir.r3.core", "date", "date.value");
    let r4: R4Element = core_element("hl7.fhir.r4.core", "date", "date.value");
    let regex = |element: &dyn ElementDefinitionRead| {
        element
            .types()
            .first()
            .and_then(|ty| ty.regex.map(str::to_owned))
    };
    assert_eq!(
        regex(&stu3).as_deref(),
        Some("-?[0-9]{4}(-(0[1-9]|1[0-2])(-(0[0-9]|[1-2][0-9]|3[0-1]))?)?")
    );
    assert!(regex(&r4).is_some(), "R4 states the form of date.value too");
}

#[test]
fn a_structure_definition_reads_alike_in_both_releases() {
    let stu3: Stu3Structure = core_definition("hl7.fhir.r3.core", "Observation");
    let r4: R4Structure = core_definition("hl7.fhir.r4.core", "Observation");
    assert_eq!(
        stu3.url(),
        Some("http://hl7.org/fhir/StructureDefinition/Observation")
    );
    assert_eq!(stu3.url(), r4.url());
    assert_eq!(stu3.type_name(), Some("Observation"));
    assert_eq!(stu3.type_name(), r4.type_name());
    assert_eq!(stu3.kind(), Some("resource"));
    assert_eq!(stu3.kind(), r4.kind());
    assert_eq!(stu3.is_abstract(), Some(false));
    assert_eq!(stu3.is_abstract(), r4.is_abstract());
    assert_eq!(stu3.derivation(), Some("specialization"));
    assert_eq!(stu3.derivation(), r4.derivation());
    assert_eq!(
        stu3.base_definition(),
        Some("http://hl7.org/fhir/StructureDefinition/DomainResource")
    );
    assert_eq!(stu3.base_definition(), r4.base_definition());
    assert_eq!(
        stu3.version(),
        None,
        "the STU3 core package states no version"
    );
    assert_eq!(r4.version(), Some("4.0.1"));
    let first = |snapshot: Option<&[R4Element]>| {
        snapshot
            .and_then(|elements| elements.first())
            .and_then(|element| element.path().map(str::to_owned))
    };
    assert_eq!(first(r4.snapshot()).as_deref(), Some("Observation"));
    assert_eq!(
        stu3.snapshot()
            .and_then(|elements| elements.first())
            .and_then(ElementDefinitionRead::path),
        Some("Observation")
    );
}

#[test]
fn an_stu3_context_reads_in_the_shape_of_an_r4_context_entry() {
    // STU3 patient-birthTime: contextType resource, context ["Patient.birthDate"]; R4: one
    // entry of type element with that expression. The codes stay each release's own.
    let stu3: Stu3Structure = core_definition("hl7.fhir.r3.core", "patient-birthTime");
    let r4: R4Structure = core_definition("hl7.fhir.r4.core", "patient-birthTime");
    assert_eq!(
        stu3.contexts(),
        [ContextRead {
            kind: Some("resource"),
            expression: Some("Patient.birthDate")
        }]
    );
    assert_eq!(
        r4.contexts(),
        [ContextRead {
            kind: Some("element"),
            expression: Some("Patient.birthDate")
        }]
    );
}

#[test]
fn context_invariants_read_alike_in_both_releases() {
    let stu3: Stu3Structure = core_definition("hl7.fhir.r3.core", "questionnaire-minOccurs");
    let r4: R4Structure = core_definition("hl7.fhir.r4.core", "questionnaire-minOccurs");
    assert_eq!(
        stu3.context_invariants(),
        ["type!='display' and (required=true or %extension.valueInteger=0)"]
    );
    assert_eq!(stu3.context_invariants(), r4.context_invariants());
}
