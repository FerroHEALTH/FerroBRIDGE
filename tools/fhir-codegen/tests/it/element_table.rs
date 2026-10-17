// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The emitted element table against the vendored `StructureDefinition`
//! snapshots it comes from, element by element, and the path lookups over it.
//!
//! Every expectation here is read from the package JSON with `serde_json`, so
//! the table is compared with the definitions
//! (<https://hl7.org/fhir/R4/elementdefinition.html>) rather than with the
//! lowering that produced it.

use std::collections::BTreeMap;
use std::fs;

use fhir_codegen::package::{Package, without_byte_order_mark};
use fhir_types::schema::{Kind, Schemas, TypeSchema, ValueKind};
use serde_json::Value;

use crate::packages;

/// The prefix of the `FHIRPath` system types a primitive's value element carries.
const SYSTEM_PREFIX: &str = "http://hl7.org/fhirpath/System.";

/// The extension naming the FHIR type behind a `FHIRPath` system type.
const FHIR_TYPE_EXTENSION: &str =
    "http://hl7.org/fhir/StructureDefinition/structuredefinition-fhir-type";

/// One element of a snapshot, as the vendored JSON states it.
#[derive(Debug, PartialEq, Eq)]
struct Expected {
    path: String,
    min: u32,
    max: Option<u32>,
    types: Vec<String>,
    content_reference: Option<String>,
    is_summary: bool,
    is_modifier: bool,
}

/// The emitted table of each version, in the order [`packages`] lists them.
fn tables() -> [(&'static str, &'static Schemas); 5] {
    [
        ("stu3", &fhir_types::stu3::schema::SCHEMAS),
        ("r4", &fhir_types::r4::schema::SCHEMAS),
        ("r4b", &fhir_types::r4b::schema::SCHEMAS),
        ("r5", &fhir_types::r5::schema::SCHEMAS),
        ("r6", &fhir_types::r6::schema::SCHEMAS),
    ]
}

/// The snapshot elements of the structure defining `type_name`, read from the
/// vendored package file.
#[expect(
    clippy::panic,
    reason = "the test fails by naming the structure it could not read"
)]
fn snapshot_elements(package: &Package, type_name: &str) -> Vec<Value> {
    let definition = package
        .structure_definition_named(type_name)
        .unwrap_or_else(|| panic!("the package defines a structure named {type_name}"));
    let file = package
        .source_of("StructureDefinition", &definition.url)
        .unwrap_or_else(|| panic!("{type_name} was loaded from a file"));
    let text = fs::read_to_string(file).expect("the structure file reads");
    let raw: Value =
        serde_json::from_str(without_byte_order_mark(&text)).expect("the structure file parses");
    raw.get("snapshot")
        .and_then(|snapshot| snapshot.get("element"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| panic!("{type_name} has a snapshot"))
}

/// The FHIR type code of one `ElementDefinition.type` entry of the element at `path`.
///
/// A `FHIRPath` system type names its FHIR type in the `structuredefinition-fhir-type`
/// extension (<https://hl7.org/fhir/R4/elementdefinition.html>). An STU3
/// primitive's `value` carries no code, only its `_code` JSON type, and takes
/// the primitive's own name, the code R4 states for the same element.
fn type_code(path: &str, entry: &Value) -> String {
    let Some(code) = entry.get("code").and_then(Value::as_str) else {
        assert!(
            entry.get("_code").is_some(),
            "{path}: a type entry without a code carries its _code extensions"
        );
        return path
            .strip_suffix(".value")
            .expect("only a primitive's value is typed without a code")
            .to_owned();
    };
    if !code.starts_with(SYSTEM_PREFIX) {
        return code.to_owned();
    }
    entry
        .get("extension")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|extension| extension.get("url").and_then(Value::as_str) == Some(FHIR_TYPE_EXTENSION))
        .and_then(|extension| extension.get("valueUrl").and_then(Value::as_str))
        .unwrap_or(code)
        .to_owned()
}

/// The code `Element.id` and every element inheriting it take: `string`, as
/// the published definitions type `Element.id` in every version
/// (<https://hl7.org/fhir/R4B/element-definitions.html#Element.id>,
/// <https://hl7.org/fhir/R5/types-definitions.html#Element.id>), where the
/// 4.3.0 and 5.0.0 snapshots state `id` (#672).
fn element_id_code(element: &Value, code: String) -> String {
    let element_id = element.get("path").and_then(Value::as_str) == Some("Element.id")
        || element.pointer("/base/path").and_then(Value::as_str) == Some("Element.id");
    if element_id && code == "id" {
        String::from("string")
    } else {
        resource_id_code(element, code)
    }
}

/// The code `Resource.id` and every element inheriting it take: `id`, as the
/// published definitions type the logical id in every version
/// (<https://hl7.org/fhir/R4/resource-definitions.html#Resource.id>), where the
/// 4.0.1 snapshot states `string` (#798).
fn resource_id_code(element: &Value, code: String) -> String {
    let resource_id = element.get("path").and_then(Value::as_str) == Some("Resource.id")
        || element.pointer("/base/path").and_then(Value::as_str) == Some("Resource.id");
    if resource_id && code == "string" {
        String::from("id")
    } else {
        code
    }
}

#[test]
fn the_logical_id_of_a_resource_is_an_id_in_every_version() {
    for (module, schemas) in tables() {
        for name in ["Resource", "Patient", "Bundle"] {
            let path = format!("{name}.id");
            let (_, field) = schemas
                .element(&path)
                .unwrap_or_else(|| panic!("{module} {path} resolves"));
            assert_eq!(field.types, ["id"], "{module} {path}");
            assert_eq!(
                field.kind,
                Kind::Primitive(ValueKind::Text),
                "{module} {path}"
            );
        }
    }
}

/// The maximum cardinality as a bound, `None` for `*`.
fn max_of(element: &Value) -> Option<u32> {
    let max = element
        .get("max")
        .and_then(Value::as_str)
        .expect("an element carries a max");
    if max == "*" {
        return None;
    }
    Some(max.parse().expect("a bounded max is a number"))
}

/// The direct children of `parent`, in snapshot order, without the elements a
/// max of 0 prohibits (<https://hl7.org/fhir/R4/conformance-rules.html#cardinality>).
fn children_of(elements: &[Value], parent: &str) -> Vec<Expected> {
    let prefix = format!("{parent}.");
    elements
        .iter()
        .filter_map(|element| {
            let path = element
                .get("path")
                .and_then(Value::as_str)
                .expect("an element carries a path");
            let rest = path.strip_prefix(&prefix)?;
            if rest.contains('.') || max_of(element) == Some(0) {
                return None;
            }
            Some(Expected {
                path: path.to_owned(),
                min: element
                    .get("min")
                    .and_then(Value::as_u64)
                    .and_then(|min| u32::try_from(min).ok())
                    .expect("an element carries a min"),
                max: max_of(element),
                types: distinct(
                    element
                        .get("type")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|entry| type_code(path, entry))
                        .map(|code| element_id_code(element, code)),
                ),
                content_reference: element.get("contentReference").and_then(Value::as_str).map(
                    |reference| {
                        reference
                            .split_once('#')
                            .map_or(reference, |(_, path)| path)
                            .to_owned()
                    },
                ),
                is_summary: element
                    .get("isSummary")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                is_modifier: element
                    .get("isModifier")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

/// The codes in their first-stated order, each once: STU3 repeats a code once
/// per target profile (<https://hl7.org/fhir/STU3/elementdefinition.html>),
/// and the element still takes one value of that type.
fn distinct(codes: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for code in codes {
        if !out.contains(&code) {
            out.push(code);
        }
    }
    out
}

/// The JSON name of a choice alternative: the stem with the type name appended,
/// first letter capitalized (<https://hl7.org/fhir/R4/json.html>).
fn choice_suffix(code: &str) -> String {
    let mut chars = code.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The emitted elements of `schema` against the definition's own.
fn assert_type(module: &str, schema: &TypeSchema, expected: &[Expected]) {
    assert_eq!(
        schema.fields.len(),
        expected.len(),
        "{module}: {} holds {} elements, the definition states {}",
        schema.path,
        schema.fields.len(),
        expected.len()
    );
    for (field, expected) in schema.fields.iter().zip(expected) {
        let at = format!("{module}: {}", expected.path);
        assert_eq!(field.path, expected.path, "{at}: the path");
        assert_eq!(
            field.name,
            expected
                .path
                .rsplit('.')
                .next()
                .unwrap()
                .trim_end_matches("[x]"),
            "{at}: the name"
        );
        assert_eq!(field.min, expected.min, "{at}: the min");
        assert_eq!(field.max, expected.max, "{at}: the max");
        assert_eq!(
            field.many,
            expected.max.is_none_or(|max| max > 1),
            "{at}: the repetition"
        );
        assert_eq!(field.types, expected.types, "{at}: the type codes");
        assert_eq!(
            field.content_reference,
            expected.content_reference.as_deref(),
            "{at}: the content reference"
        );
        assert_eq!(
            field.is_summary, expected.is_summary,
            "{at}: the isSummary flag"
        );
        assert_eq!(
            field.is_modifier, expected.is_modifier,
            "{at}: the isModifier flag"
        );
        if expected.path.ends_with("[x]") && expected.content_reference.is_none() {
            let variants = match field.kind {
                Kind::Choice(variants) => variants,
                _ => &[],
            };
            let suffixes: Vec<&str> = variants.iter().map(|(suffix, _)| *suffix).collect();
            let want: Vec<String> = expected
                .types
                .iter()
                .map(|code| choice_suffix(code))
                .collect();
            assert_eq!(suffixes, want, "{at}: the choice suffixes");
        }
    }
}

#[test]
fn every_emitted_element_agrees_with_its_definition() {
    for ((module, _, package), (named, schemas)) in packages().into_iter().zip(tables()) {
        assert_eq!(module, named, "the tables follow the package order");
        let mut snapshots: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for schema in schemas.types {
            let root = schema.path.split('.').next().expect("a path has a root");
            let elements = snapshots
                .entry(root.to_owned())
                .or_insert_with(|| snapshot_elements(package, root));
            assert_type(module, schema, &children_of(elements, schema.path));
        }
    }
}

/// The flags of one `Patient` element in every version, as its package states
/// them: `Patient.active` is in the summary and a modifier, `Patient.name` is in
/// the summary only, and `Patient.photo` is neither
/// (<https://hl7.org/fhir/R4/patient.html>).
#[test]
fn the_summary_and_modifier_flags_are_the_packages_own_in_every_version() {
    for (module, schemas) in tables() {
        let flags = |path: &str| {
            let (_, field) = schemas
                .element(path)
                .unwrap_or_else(|| panic!("{module}: {path} resolves"));
            (field.is_summary, field.is_modifier)
        };
        assert_eq!(
            flags("Patient.active"),
            (true, true),
            "{module}: Patient.active"
        );
        assert_eq!(
            flags("Patient.name"),
            (true, false),
            "{module}: Patient.name"
        );
        assert_eq!(
            flags("Patient.photo"),
            (false, false),
            "{module}: Patient.photo"
        );
    }
}

#[test]
fn a_backbone_type_carries_the_path_that_introduces_it() {
    let schemas = &fhir_types::r4::schema::SCHEMAS;
    let component = schemas
        .type_named("ObservationComponent")
        .expect("R4 emits ObservationComponent");
    assert_eq!(component.path, "Observation.component");
    assert_eq!(
        schemas.type_of("Observation.component"),
        Some(component),
        "the backbone element resolves to its own type"
    );
}

#[test]
fn a_repeating_primitive_element_resolves() {
    let (owner, field) = fhir_types::r4::schema::SCHEMAS
        .element("Patient.name.given")
        .expect("Patient.name.given resolves");
    assert_eq!(owner.name, "HumanName");
    assert_eq!(field.name, "given");
    assert_eq!(field.min, 0);
    assert_eq!(field.max, None);
    assert!(field.many);
    assert_eq!(field.types, ["string"]);
    assert_eq!(field.kind, Kind::Primitive(ValueKind::Text));
}

#[test]
fn a_choice_resolves_on_its_base_path_and_on_every_expanded_form() {
    let schemas = &fhir_types::r4::schema::SCHEMAS;
    let (owner, base) = schemas
        .element("Observation.value[x]")
        .expect("the base path resolves");
    assert_eq!(owner.name, "Observation");
    assert_eq!(base.name, "value");
    assert!(base.types.contains(&"Quantity"));
    let (_, expanded) = schemas
        .element("Observation.valueQuantity")
        .expect("the expanded form resolves");
    assert_eq!(expanded, base, "both forms name one element");
    assert_eq!(
        schemas.type_of("Observation.valueQuantity").map(|t| t.name),
        Some("Quantity")
    );
    assert_eq!(
        schemas.type_of("Observation.valueString"),
        None,
        "a primitive alternative has no complex type"
    );
    assert_eq!(
        schemas.type_of("Observation.value[x]"),
        None,
        "the base form names no single type"
    );
}

#[test]
fn a_resource_typed_element_resolves() {
    let (owner, field) = fhir_types::r4::schema::SCHEMAS
        .element("Bundle.entry.resource")
        .expect("Bundle.entry.resource resolves");
    assert_eq!(owner.name, "BundleEntry");
    assert_eq!(field.kind, Kind::Resource);
    assert_eq!(field.types, ["Resource"]);
}

#[test]
fn a_content_reference_hop_resolves() {
    let (owner, field) = fhir_types::r4::schema::SCHEMAS
        .element("Questionnaire.item.item")
        .expect("the nested item resolves through its content reference");
    assert_eq!(owner.path, "Questionnaire.item");
    assert_eq!(field.content_reference, Some("Questionnaire.item"));
    assert!(
        field.types.is_empty(),
        "a content reference lists no type of its own"
    );
    assert_eq!(
        fhir_types::r4::schema::SCHEMAS
            .element("Questionnaire.item.item.text")
            .map(|(_, field)| field.path),
        Some("Questionnaire.item.text"),
        "the walk continues through the referenced element's children"
    );
}

#[test]
fn a_path_the_table_does_not_know_resolves_to_nothing() {
    let schemas = &fhir_types::r4::schema::SCHEMAS;
    assert_eq!(schemas.element("Patient.nonesuch"), None);
    assert_eq!(schemas.element("Nonesuch.name"), None);
    assert_eq!(
        schemas.element("Patient"),
        None,
        "a type name is no element"
    );
    assert_eq!(
        schemas.element("Patient.name.given.value"),
        None,
        "a path through a primitive names no element"
    );
    assert_eq!(
        schemas.element("ObservationComponent.code"),
        None,
        "a backbone type is reached by its element path, never by its Rust name"
    );
    assert!(
        schemas.element("Observation.component").is_some(),
        "the element path reaches it"
    );
}

/// One emitted element, as the `Element` assertions compare it: the name, the
/// kind, the cardinality bounds and the type codes.
type Member<'a> = (&'a str, Kind, u32, Option<u32>, &'a [&'a str]);

/// The FHIR type each version's table names behind `Element.id`.
///
/// The published definitions type `Element.id` as `string` in every version
/// (<https://hl7.org/fhir/R4B/element-definitions.html#Element.id>), so the
/// table says `string` in 4.3.0 too, where the package's snapshot states
/// `id` (#672).
const ELEMENT_ID_TYPE: [(&str, &str); 5] = [
    ("stu3", "string"),
    ("r4", "string"),
    ("r4b", "string"),
    ("r5", "string"),
    ("r6", "string"),
];

#[test]
fn every_version_carries_the_element_entry_a_primitive_s_sibling_resolves_against() {
    for ((module, schemas), (named, id_type)) in tables().into_iter().zip(ELEMENT_ID_TYPE) {
        assert_eq!(module, named, "the tables follow the package order");
        let element = schemas
            .type_named("Element")
            .unwrap_or_else(|| panic!("{module} emits Element"));
        assert_eq!(element.path, "Element", "{module}: the definition's path");
        let members: Vec<Member<'_>> = element
            .fields
            .iter()
            .map(|field| (field.name, field.kind, field.min, field.max, field.types))
            .collect();
        assert_eq!(
            members,
            [
                ("id", Kind::Attribute, 0, Some(1), &[id_type][..]),
                (
                    "extension",
                    Kind::Complex("Extension"),
                    0,
                    None,
                    &["Extension"][..],
                ),
            ],
            "{module}: the two members Element defines"
        );
    }
}

/// The base a table entry should name, read from the package: the
/// `baseDefinition` of the structure at the entry's root, or for a backbone
/// element the type code of the element that introduces it.
fn expected_base(package: &Package, schema: &TypeSchema) -> Option<String> {
    let root = schema.path.split('.').next().expect("a path has a root");
    if schema.path == root {
        let definition = package
            .structure_definition_named(root)
            .expect("the package defines the entry's structure");
        return definition
            .base_definition
            .as_deref()
            .and_then(|url| url.rsplit('/').next())
            .map(str::to_owned);
    }
    let elements = snapshot_elements(package, root);
    let element = elements
        .iter()
        .find(|element| element["path"].as_str() == Some(schema.path))
        .expect("the backbone's element is in the snapshot");
    let first = element
        .get("type")
        .and_then(|types| types.get(0))
        .expect("a backbone element has a type");
    Some(type_code(schema.path, first))
}

#[test]
fn every_entry_names_the_base_its_definition_states_and_the_table_carries_it() {
    for ((module, _, package), (named, schemas)) in packages().into_iter().zip(tables()) {
        assert_eq!(module, named, "the tables follow the package order");
        for schema in schemas.types {
            assert_eq!(
                schema.base.map(str::to_owned),
                expected_base(package, schema),
                "{module}: the base of {}",
                schema.path
            );
            if let Some(base) = schema.base {
                assert!(
                    schemas.type_named(base).is_some(),
                    "{module}: {} names {base}, which the table carries",
                    schema.path
                );
            }
        }
    }
}

/// The abstract types every version's base chains pass through
/// (<https://hl7.org/fhir/R4/resource.html>, <https://hl7.org/fhir/R4/domainresource.html>,
/// <https://hl7.org/fhir/R4/backboneelement.html>).
const ABSTRACT_TYPES: [&str; 4] = ["BackboneElement", "DomainResource", "Element", "Resource"];

#[test]
fn every_version_carries_the_abstract_types_and_walks_a_type_to_them() {
    for (module, schemas) in tables() {
        for name in ABSTRACT_TYPES {
            assert!(
                schemas
                    .type_named(name)
                    .is_some_and(|schema| schema.path == name),
                "{module} carries {name}"
            );
            assert!(!schemas.is_resource(name), "{module}: {name} is abstract");
        }
        assert_eq!(schemas.base_of("Age"), Some("Quantity"), "{module}: Age");
        assert!(
            schemas.specializes("Age", "Quantity"),
            "{module}: Age is a Quantity"
        );
        assert!(
            schemas.specializes("Age", "Element"),
            "{module}: Age is an Element"
        );
        assert!(
            !schemas.specializes("Quantity", "Age"),
            "{module}: Quantity is no Age"
        );
        assert_eq!(
            schemas.base_of("Patient"),
            Some("DomainResource"),
            "{module}"
        );
        assert!(schemas.specializes("Patient", "DomainResource"), "{module}");
        assert!(schemas.specializes("Patient", "Resource"), "{module}");
        assert!(!schemas.specializes("Bundle", "DomainResource"), "{module}");
        assert!(schemas.specializes("Bundle", "Resource"), "{module}");
        assert!(
            schemas.specializes("PatientContact", "BackboneElement"),
            "{module}: a backbone element"
        );
        assert_eq!(
            schemas.type_named("SimpleQuantity"),
            None,
            "{module}: a profile"
        );
    }
}

#[test]
fn the_root_of_the_hierarchy_is_element_and_resource_until_r5_adds_base() {
    for (module, schemas) in tables() {
        let root = match module {
            "stu3" | "r4" | "r4b" => None,
            _ => Some("Base"),
        };
        assert_eq!(schemas.base_of("Element"), root, "{module}: Element");
        assert_eq!(schemas.base_of("Resource"), root, "{module}: Resource");
    }
}
