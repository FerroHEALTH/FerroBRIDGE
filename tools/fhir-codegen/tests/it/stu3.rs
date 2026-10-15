// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shapes `hl7.fhir.r3.core` 3.0.2 states differently from R4, each read
//! once from a synthetic definition, so a failure names one shape, and once
//! from the vendored package, so the shape is the package's own.

use std::fs;

use fhir_codegen::fhir::StructureDefinition;
use fhir_codegen::package::{LoadError, Package, without_byte_order_mark};
use fhir_codegen::snapshot::{ElementShape, ResolveError, ResolvedStructure, TypeRef};

use crate::STU3;

/// A synthetic STU3 resource definition whose root is followed by `elements`.
fn definition(elements: &str) -> StructureDefinition {
    let text = format!(
        r#"{{"resourceType":"StructureDefinition","url":"http://example.org/StructureDefinition/Synthetic","name":"Synthetic","status":"draft","kind":"resource","abstract":false,"type":"Synthetic","derivation":"specialization","snapshot":{{"element":[{{"path":"Synthetic","min":0,"max":"*"}},{elements}]}}}}"#
    );
    serde_json::from_str(&text).expect("the synthetic definition parses")
}

/// The single-typed shape of the element at `path`.
#[expect(
    clippy::panic,
    reason = "the test fails by naming the element whose shape it did not find"
)]
fn typed(structure: &ResolvedStructure, path: &str) -> Vec<TypeRef> {
    match &structure.element(path).expect("the element exists").shape {
        ElementShape::Typed(types) | ElementShape::Choice(types) => types.clone(),
        other => panic!("{path}: expected a typed element, got {other:?}"),
    }
}

#[test]
fn a_byte_order_mark_is_stripped_before_parsing() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(without_byte_order_mark("\u{feff}{}"), "{}");
    assert_eq!(without_byte_order_mark("{}"), "{}");
    assert_eq!(
        without_byte_order_mark("\u{feff}\u{feff}{}"),
        "\u{feff}{}",
        "only the leading mark is a byte order mark"
    );
    let dir = tempfile::tempdir()?;
    let content = dir.path().join("package");
    fs::create_dir_all(&content)?;
    fs::write(
        content.join("package.json"),
        "\u{feff}{\"name\":\"test.pkg\",\"version\":\"0.1.0\"}",
    )?;
    fs::write(
        content.join("ValueSet-a.json"),
        "\u{feff}{\"resourceType\":\"ValueSet\",\"url\":\"http://example.org/vs\",\"status\":\"active\"}",
    )?;
    let package = Package::open(dir.path())?;
    assert!(package.value_sets().contains_key("http://example.org/vs"));
    Ok(())
}

#[test]
fn a_mark_inside_the_text_is_still_a_parse_failure() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let content = dir.path().join("package");
    fs::create_dir_all(&content)?;
    fs::write(
        content.join("package.json"),
        r#"{"name":"test.pkg","version":"0.1.0"}"#,
    )?;
    fs::write(
        content.join("ValueSet-a.json"),
        " \u{feff}{\"resourceType\":\"ValueSet\",\"url\":\"http://example.org/vs\"}",
    )?;
    assert!(matches!(
        Package::open(dir.path()),
        Err(LoadError::Json { .. })
    ));
    Ok(())
}

#[test]
fn the_stu3_package_loads_its_marked_files() {
    let dir = crate::vendor_dir().join("hl7.fhir.r3.core").join("package");
    let marked = fs::read_dir(&dir)
        .expect("the package directory lists")
        .map(|entry| entry.expect("the entry reads").path())
        .filter(|path| {
            fs::read(path)
                .expect("the file reads")
                .starts_with(&[0xEF, 0xBB, 0xBF])
        })
        .count();
    assert_eq!(
        marked, 40,
        "the 3.0.2 files that begin with a byte order mark"
    );
    assert_eq!(STU3.manifest().name, "hl7.fhir.r3.core");
    assert_eq!(STU3.manifest().fhir_versions, vec!["3.0.2".to_owned()]);
    assert!(STU3.manifest().is_stu3());
}

#[test]
fn a_single_target_profile_decodes_as_a_one_element_list() {
    let structure = ResolvedStructure::resolve(&definition(
        r#"{"path":"Synthetic.subject","min":0,"max":"1","type":[{"code":"Reference","profile":"http://example.org/StructureDefinition/P","targetProfile":"http://hl7.org/fhir/StructureDefinition/Patient"}]}"#,
    ))
    .expect("the definition resolves");
    let types = typed(&structure, "Synthetic.subject");
    assert_eq!(types.len(), 1);
    assert_eq!(
        types[0].target_profiles,
        ["http://hl7.org/fhir/StructureDefinition/Patient"]
    );
    assert_eq!(
        types[0].profiles,
        ["http://example.org/StructureDefinition/P"]
    );
}

#[test]
fn type_entries_with_one_code_merge_into_the_union_of_their_profiles() {
    let structure = ResolvedStructure::resolve(&definition(
        r#"{"path":"Synthetic.subject","min":0,"max":"1","type":[{"code":"Reference","targetProfile":"http://hl7.org/fhir/StructureDefinition/Patient"},{"code":"Reference","targetProfile":"http://hl7.org/fhir/StructureDefinition/Group"},{"code":"Reference","targetProfile":"http://hl7.org/fhir/StructureDefinition/Patient"}]},{"path":"Synthetic.value[x]","min":0,"max":"1","type":[{"code":"string"},{"code":"Reference","targetProfile":"http://hl7.org/fhir/StructureDefinition/Patient"},{"code":"Reference","targetProfile":"http://hl7.org/fhir/StructureDefinition/Device"}]}"#,
    ))
    .expect("the definition resolves");
    let subject = typed(&structure, "Synthetic.subject");
    assert_eq!(subject.len(), 1, "one Reference type, never four");
    assert_eq!(
        subject[0].target_profiles,
        [
            "http://hl7.org/fhir/StructureDefinition/Patient",
            "http://hl7.org/fhir/StructureDefinition/Group",
        ]
    );
    let value = typed(&structure, "Synthetic.value[x]");
    assert_eq!(
        value.iter().map(|t| t.code.as_str()).collect::<Vec<_>>(),
        ["string", "Reference"],
        "a choice takes one variant per code"
    );
    assert_eq!(value[1].target_profiles.len(), 2);
}

#[test]
fn the_stu3_observation_subject_is_one_reference_to_four_targets() {
    let observation = ResolvedStructure::resolve(
        STU3.structure_definition_named("Observation")
            .expect("STU3 defines Observation"),
    )
    .expect("Observation resolves");
    let subject = typed(&observation, "Observation.subject");
    assert_eq!(subject.len(), 1);
    assert_eq!(subject[0].code, "Reference");
    assert_eq!(
        subject[0].target_profiles,
        ["Patient", "Group", "Device", "Location"]
            .map(|name| format!("http://hl7.org/fhir/StructureDefinition/{name}"))
    );
}

#[test]
fn a_primitive_value_takes_its_json_type_from_the_code_extension() {
    let text = r#"{"resourceType":"StructureDefinition","url":"http://example.org/StructureDefinition/count","name":"count","status":"draft","kind":"primitive-type","abstract":false,"type":"count","derivation":"specialization","snapshot":{"element":[{"path":"count","min":0,"max":"*"},{"path":"count.value","representation":["xmlAttr"],"min":0,"max":"1","type":[{"extension":[{"url":"http://hl7.org/fhir/StructureDefinition/structuredefinition-regex","valueString":"[0-9]+"}],"_code":{"extension":[{"url":"http://hl7.org/fhir/StructureDefinition/structuredefinition-json-type","valueString":"number"},{"url":"http://hl7.org/fhir/StructureDefinition/structuredefinition-xml-type","valueString":"xsd:int"}]}}]}]}}"#;
    let definition: StructureDefinition = serde_json::from_str(text).expect("parses");
    let structure = ResolvedStructure::resolve(&definition).expect("the primitive resolves");
    let types = typed(&structure, "count.value");
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].code, "count", "the primitive's own name");
    assert_eq!(types[0].json_type.as_deref(), Some("number"));
    assert_eq!(types[0].fhirpath_type, None);
    assert!(types[0].is_scalar());
    assert_eq!(
        types[0].regex.as_deref(),
        Some("[0-9]+"),
        "the STU3 structuredefinition-regex extension"
    );
}

#[test]
fn every_stu3_primitive_value_is_typed() {
    for definition in STU3.structure_definitions().values() {
        if definition.kind != fhir_codegen::fhir::StructureKind::PrimitiveType {
            continue;
        }
        let structure = ResolvedStructure::resolve(definition).expect("the primitive resolves");
        let path = format!("{}.value", structure.name);
        let types = typed(&structure, &path);
        assert_eq!(types.len(), 1, "{path}");
        assert_eq!(types[0].code, structure.name, "{path}");
        assert!(
            types[0].json_type.is_some(),
            "{path}: the JSON type is read"
        );
    }
    let integer = ResolvedStructure::resolve(
        STU3.structure_definition_named("integer")
            .expect("STU3 defines integer"),
    )
    .expect("integer resolves");
    let types = typed(&integer, "integer.value");
    assert_eq!(types[0].json_type.as_deref(), Some("number"));
    assert_eq!(types[0].regex.as_deref(), Some("-?([0]|([1-9][0-9]*))"));
}

#[test]
fn a_type_with_neither_a_code_nor_a_json_type_is_refused() {
    let refused = ResolvedStructure::resolve(&definition(
        r#"{"path":"Synthetic.value","min":0,"max":"1","type":[{"_code":{"extension":[{"url":"http://hl7.org/fhir/StructureDefinition/structuredefinition-xml-type","valueString":"xsd:string"}]}}]}"#,
    ));
    match refused {
        Err(ResolveError::UntypedType { url, path }) => {
            assert_eq!(url, "http://example.org/StructureDefinition/Synthetic");
            assert_eq!(path, "Synthetic.value");
        }
        other => panic!("expected UntypedType, got {other:?}"),
    }
    let bare = ResolvedStructure::resolve(&definition(
        r#"{"path":"Synthetic.value","min":0,"max":"1","type":[{}]}"#,
    ));
    assert!(matches!(bare, Err(ResolveError::UntypedType { .. })));
}

#[test]
fn an_attribute_typed_with_a_primitive_code_is_a_json_string() {
    let structure = ResolvedStructure::resolve(&definition(
        r#"{"path":"Synthetic.id","representation":["xmlAttr"],"min":0,"max":"1","type":[{"code":"string"}]},{"path":"Synthetic.url","representation":["xmlAttr"],"min":1,"max":"1","type":[{"code":"uri"}]},{"path":"Synthetic.name","min":0,"max":"1","type":[{"code":"string"}]}"#,
    ))
    .expect("the definition resolves");
    for path in ["Synthetic.id", "Synthetic.url"] {
        let types = typed(&structure, path);
        assert_eq!(types[0].json_type.as_deref(), Some("string"), "{path}");
        assert!(types[0].is_scalar(), "{path}");
    }
    let name = typed(&structure, "Synthetic.name");
    assert!(
        !name[0].is_scalar(),
        "an element that is no attribute keeps its primitive type"
    );
}

#[test]
fn an_attribute_of_a_non_string_primitive_is_refused() {
    let refused = ResolvedStructure::resolve(&definition(
        r#"{"path":"Synthetic.flag","representation":["xmlAttr"],"min":0,"max":"1","type":[{"code":"boolean"}]}"#,
    ));
    match refused {
        Err(ResolveError::AttributeType { path, code, .. }) => {
            assert_eq!(path, "Synthetic.flag");
            assert_eq!(code, "boolean");
        }
        other => panic!("expected AttributeType, got {other:?}"),
    }
}

#[test]
fn the_stu3_element_id_and_extension_url_are_attributes() {
    for (name, path, code) in [
        ("Element", "Element.id", "string"),
        ("Extension", "Extension.url", "uri"),
    ] {
        let structure = ResolvedStructure::resolve(
            STU3.structure_definition_named(name)
                .expect("STU3 defines the type"),
        )
        .expect("the structure resolves");
        let types = typed(&structure, path);
        assert_eq!(types[0].code, code, "{path}");
        assert_eq!(types[0].json_type.as_deref(), Some("string"), "{path}");
    }
}

#[test]
fn both_stu3_binding_forms_and_the_r4_form_name_the_value_set() {
    let structure = ResolvedStructure::resolve(&definition(
        r#"{"path":"Synthetic.status","min":0,"max":"1","type":[{"code":"code"}],"binding":{"strength":"required","valueSetReference":{"reference":"http://hl7.org/fhir/ValueSet/observation-status"}}},{"path":"Synthetic.mime","min":0,"max":"1","type":[{"code":"code"}],"binding":{"strength":"required","valueSetUri":"http://www.rfc-editor.org/bcp/bcp13.txt"}},{"path":"Synthetic.kind","min":0,"max":"1","type":[{"code":"code"}],"binding":{"strength":"required","valueSet":"http://hl7.org/fhir/ValueSet/kind|4.0.1"}},{"path":"Synthetic.free","min":0,"max":"1","type":[{"code":"code"}],"binding":{"strength":"example"}}"#,
    ))
    .expect("the definition resolves");
    let bound = |path: &str| {
        structure
            .element(path)
            .and_then(|element| element.binding.as_ref())
            .and_then(|binding| binding.bound_value_set().map(str::to_owned))
    };
    assert_eq!(
        bound("Synthetic.status").as_deref(),
        Some("http://hl7.org/fhir/ValueSet/observation-status")
    );
    assert_eq!(
        bound("Synthetic.mime").as_deref(),
        Some("http://www.rfc-editor.org/bcp/bcp13.txt")
    );
    assert_eq!(
        bound("Synthetic.kind").as_deref(),
        Some("http://hl7.org/fhir/ValueSet/kind|4.0.1")
    );
    assert_eq!(bound("Synthetic.free"), None, "an unbound value set");
}

#[test]
fn the_stu3_observation_status_binding_is_read() {
    let observation = ResolvedStructure::resolve(
        STU3.structure_definition_named("Observation")
            .expect("STU3 defines Observation"),
    )
    .expect("Observation resolves");
    let binding = observation
        .element("Observation.status")
        .and_then(|element| element.binding.as_ref())
        .expect("Observation.status is bound");
    assert_eq!(binding.strength, "required");
    assert_eq!(
        binding.bound_value_set(),
        Some("http://hl7.org/fhir/ValueSet/observation-status")
    );
}

#[test]
fn a_structure_without_a_type_is_refused_by_name() {
    // StructureDefinition.type is 1..1 (https://hl7.org/fhir/STU3/structuredefinition.html);
    // 3.0.2 omits it on three logical models.
    for name in ["Definition", "Event", "Request"] {
        let definition = STU3
            .structure_definitions()
            .values()
            .find(|definition| definition.name == name)
            .expect("the logical model is loaded");
        assert_eq!(definition.type_name, None, "{name}");
        match ResolvedStructure::resolve(definition) {
            Err(ResolveError::NoType { url }) => assert_eq!(url, definition.url),
            other => panic!("{name}: expected NoType, got {other:?}"),
        }
    }
}

#[test]
fn every_other_stu3_snapshot_resolves() {
    for definition in STU3.structure_definitions().values() {
        if definition.type_name.is_none() || definition.snapshot.is_none() {
            continue;
        }
        if let Err(error) = ResolvedStructure::resolve(definition) {
            panic!("{}: {error}", definition.url);
        }
    }
}
