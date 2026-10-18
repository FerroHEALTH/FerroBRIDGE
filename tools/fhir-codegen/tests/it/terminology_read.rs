// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The release-neutral `ValueSet` and `CodeSystem` read: the same resource
//! read from the STU3 and the R4 core package gives the same answer where the
//! releases agree (<https://hl7.org/fhir/STU3/valueset.html>,
//! <https://hl7.org/fhir/R4/valueset.html>,
//! <https://hl7.org/fhir/STU3/codesystem.html>,
//! <https://hl7.org/fhir/R4/codesystem.html>).

use std::fs;

use fhir_codegen::package::without_byte_order_mark;
use fhir_types::codec::{Json, Path, Value, expect_object};
use fhir_types::definition::{
    CodeSystemRead, ConceptRead, ContainsRead, IncludeRead, ValueSetRead,
};

use crate::vendor_dir;

type Stu3ValueSet = fhir_types::stu3::value_set::ValueSet;
type R4ValueSet = fhir_types::r4::value_set::ValueSet;
type Stu3CodeSystem = fhir_types::stu3::code_system::CodeSystem;
type R4CodeSystem = fhir_types::r4::code_system::CodeSystem;

/// The resource in the file `file` of the vendored package `package`,
/// decoded as `T` of the resource type `kind`.
fn resource<T: Json>(package: &str, kind: &str, file: &str) -> T {
    let path = vendor_dir().join(package).join("package").join(file);
    let text = fs::read_to_string(path).expect("the package carries the resource");
    let value: Value =
        serde_json::from_str(without_byte_order_mark(&text)).expect("the resource is JSON");
    let mut at = Path::root(kind);
    let object = expect_object(&value, &at).expect("the resource is an object");
    T::from_json(object, &mut at).expect("the resource decodes")
}

/// Holds that every version implements the four reads.
const fn implements<V: ValueSetRead, S: CodeSystemRead>() {}

#[test]
fn every_version_implements_the_terminology_read() {
    implements::<Stu3ValueSet, Stu3CodeSystem>();
    implements::<R4ValueSet, R4CodeSystem>();
    implements::<fhir_types::r4b::value_set::ValueSet, fhir_types::r4b::code_system::CodeSystem>();
    implements::<fhir_types::r5::value_set::ValueSet, fhir_types::r5::code_system::CodeSystem>();
    implements::<fhir_types::r6::value_set::ValueSet, fhir_types::r6::code_system::CodeSystem>();
}

/// The includes of `value_set`'s composition.
fn includes(value_set: &impl ValueSetRead) -> Vec<IncludeRead<'_>> {
    value_set
        .compose()
        .map(|compose| compose.includes)
        .unwrap_or_default()
}

#[test]
fn an_stu3_uri_import_reads_as_an_r4_canonical_import() {
    // STU3 compose.include.valueSet is a uri, R4's a canonical.
    let stu3: Stu3ValueSet = resource(
        "hl7.fhir.r3.core",
        "ValueSet",
        "ValueSet-yesnodontknow.json",
    );
    let r4: R4ValueSet = resource(
        "hl7.fhir.r4.core",
        "ValueSet",
        "ValueSet-yesnodontknow.json",
    );
    let stu3 = includes(&stu3);
    let r4 = includes(&r4);
    assert_eq!(
        stu3.first().map(|include| include.value_sets.clone()),
        Some(vec!["http://hl7.org/fhir/ValueSet/v2-0136"])
    );
    assert_eq!(
        r4.first().map(|include| include.value_sets.clone()),
        Some(vec!["http://terminology.hl7.org/ValueSet/v2-0136"])
    );
    let listed = |include: Option<&IncludeRead<'_>>| {
        include.map(|include| (include.system.is_some(), include.concepts.len()))
    };
    assert_eq!(listed(stu3.get(1)), Some((true, 1)));
    assert_eq!(listed(stu3.get(1)), listed(r4.get(1)));
}

/// The total, the offset, the number of top-level entries, and the codes of
/// the nested entries of the first entry that nests any.
fn expansion(value_set: &impl ValueSetRead) -> (Option<i32>, Option<i32>, usize, Vec<String>) {
    let Some(expansion) = value_set.expansion() else {
        return (None, None, 0, Vec::new());
    };
    let nested = expansion
        .contains
        .iter()
        .find(|entry| !entry.contains().is_empty())
        .map(|entry| {
            assert_eq!(
                entry.is_abstract(),
                Some(true),
                "the entry that nests codes is abstract"
            );
            entry
                .contains()
                .iter()
                .filter_map(|inner| inner.code().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    (
        expansion.total,
        expansion.offset,
        expansion.contains.len(),
        nested,
    )
}

#[test]
fn an_expansion_and_its_nested_entries_read_alike_in_both_releases() {
    let stu3: Stu3ValueSet = resource(
        "hl7.fhir.r3.core",
        "ValueSet",
        "ValueSet-example-expansion.json",
    );
    let r4: R4ValueSet = resource(
        "hl7.fhir.r4.core",
        "ValueSet",
        "ValueSet-example-expansion.json",
    );
    let read = expansion(&stu3);
    assert_eq!((read.0, read.1, read.2), (Some(8), Some(0), 3));
    assert!(!read.3.is_empty(), "the abstract entry nests codes");
    assert_eq!(read, expansion(&r4));
}

/// What the read gives of one code system and one of its concepts.
#[derive(Debug, PartialEq, Eq)]
struct Facts {
    /// The content.
    content: Option<String>,
    /// The declared property codes.
    declared: Vec<String>,
    /// The concept's display.
    display: Option<String>,
    /// The concept's property codes with their JSON values.
    properties: Vec<String>,
    /// The codes nested under the concept.
    children: Vec<String>,
}

/// The facts of `system` and of its concept `code`.
fn concept<S: CodeSystemRead>(system: &S, code: &str) -> Facts {
    let found = system
        .concepts()
        .iter()
        .find(|concept| concept.code() == Some(code))
        .expect("the code system carries the concept");
    let properties = found
        .properties()
        .expect("every property value has a JSON form")
        .into_iter()
        .map(|property| {
            let value = property
                .value
                .and_then(|value| value.value)
                .map(|value| format!("{value:?}"))
                .unwrap_or_default();
            format!("{}={value}", property.code.unwrap_or_default())
        })
        .collect();
    Facts {
        content: system.content().map(str::to_owned),
        declared: system
            .properties()
            .into_iter()
            .filter_map(|property| property.code.map(str::to_owned))
            .collect(),
        display: found.display().map(str::to_owned),
        properties,
        children: found
            .concepts()
            .iter()
            .filter_map(|inner| inner.code().map(str::to_owned))
            .collect(),
    }
}

#[test]
fn a_concept_with_properties_and_children_reads_alike_in_both_releases() {
    let stu3: Stu3CodeSystem = resource(
        "hl7.fhir.r3.core",
        "CodeSystem",
        "CodeSystem-item-type.json",
    );
    let r4: R4CodeSystem = resource(
        "hl7.fhir.r4.core",
        "CodeSystem",
        "CodeSystem-item-type.json",
    );
    let read = concept(&stu3, "question");
    assert_eq!(read.content.as_deref(), Some("complete"));
    assert_eq!(read.declared, ["notSelectable"]);
    assert_eq!(read.properties.len(), 1, "{:?}", read.properties);
    assert!(
        read.properties[0].starts_with("notSelectable="),
        "{:?}",
        read.properties
    );
    assert_eq!(read.children.len(), 14);
    assert_eq!(read, concept(&r4, "question"));
    assert_eq!(stu3.case_sensitive(), Some(true));
    assert_eq!(stu3.hierarchy_meaning(), r4.hierarchy_meaning());
}
