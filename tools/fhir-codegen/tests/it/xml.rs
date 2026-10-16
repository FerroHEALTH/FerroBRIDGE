// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XML codec: every root-set resource round-trips JSON to XML to JSON,
//! refusals name their element, and a few examples are pinned as golden XML
//! (<https://hl7.org/fhir/R4B/xml.html>).

use std::path::Path;

use fhir_types::codec::Value;
use fhir_types::codec::{DecodeErrorKind, Json};
use fhir_types::schema::Schemas;
use fhir_types::xml::{from_xml, to_xml};
use proptest::prelude::*;

use crate::codec::{decode_file, root_set_files};
use crate::vendor_dir;

/// The canonical JSON of one file of `package`, `None` only for a refusal
/// `codec.rs` lists, and an error naming the path for any other failure.
fn canonical<T: Json>(package: &str, path: &Path) -> Result<Option<Value>, String> {
    let Some(decoded) = decode_file::<T>(package, path)? else {
        return Ok(None);
    };
    let object = decoded
        .to_json()
        .map_err(|e| format!("{}: to JSON: {e}", path.display()))?;
    Ok(Some(Value::Object(object)))
}

/// JSON to XML to JSON, equal to the canonical JSON.
fn round_trip(schemas: &Schemas, canonical: &Value, name: &str) -> Result<(), String> {
    let object = canonical.as_object().ok_or("not an object")?;
    let xml = to_xml(schemas, object).map_err(|e| format!("{name}: to XML: {e}"))?;
    let back = from_xml(schemas, &xml).map_err(|e| format!("{name}: from XML: {e}"))?;
    if Value::Object(back) != *canonical {
        return Err(format!("{name}: the XML round trip differs"));
    }
    Ok(())
}

/// Every root-set file the JSON codec accepts round-trips through XML, except
/// a Bundle that carries a resource outside the root set, which has no schema
/// (the XML codec covers the same closure the typed model does).
fn round_trip_all<T: Json>(package: &str, schemas: &Schemas) {
    let files = root_set_files(package);
    assert!(files.len() > 1000, "{package}: {} files", files.len());
    let mut failures = Vec::new();
    let mut passed = 0;
    let mut unknown_resources = 0;
    for file in &files {
        let name = file
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("");
        let canonical = match canonical::<T>(package, file) {
            Ok(Some(canonical)) => canonical,
            Ok(None) => continue,
            Err(e) => {
                failures.push(e);
                continue;
            }
        };
        match round_trip(schemas, &canonical, name) {
            Ok(()) => passed += 1,
            Err(e) if e.contains("no schema for the resource type") => unknown_resources += 1,
            Err(e) => failures.push(e),
        }
    }
    assert!(
        failures.is_empty(),
        "{package}: {} of {} files failed:\n{}",
        failures.len(),
        files.len(),
        failures
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(passed > 1000, "{package}: {passed} round trips");
    assert!(
        unknown_resources < 20,
        "{package}: {unknown_resources} bundles carry resources outside the root set"
    );
}

#[test]
fn every_stu3_root_set_resource_round_trips_through_xml() {
    round_trip_all::<fhir_types::stu3::resource::Resource>(
        "hl7.fhir.r3.core",
        &fhir_types::stu3::schema::SCHEMAS,
    );
}

#[test]
fn every_r4_root_set_resource_round_trips_through_xml() {
    round_trip_all::<fhir_types::r4::resource::Resource>(
        "hl7.fhir.r4.core",
        &fhir_types::r4::schema::SCHEMAS,
    );
}

#[test]
fn every_r4b_root_set_resource_round_trips_through_xml() {
    round_trip_all::<fhir_types::r4b::resource::Resource>(
        "hl7.fhir.r4b.core",
        &fhir_types::r4b::schema::SCHEMAS,
    );
}

#[test]
fn every_r5_root_set_resource_round_trips_through_xml() {
    round_trip_all::<fhir_types::r5::resource::Resource>(
        "hl7.fhir.r5.core",
        &fhir_types::r5::schema::SCHEMAS,
    );
}

#[test]
fn every_r6_root_set_resource_round_trips_through_xml() {
    round_trip_all::<fhir_types::r6::resource::Resource>(
        "hl7.fhir.r6.core",
        &fhir_types::r6::schema::SCHEMAS,
    );
}

#[test]
fn the_xml_of_examples_is_pinned() {
    let schemas = &fhir_types::r4b::schema::SCHEMAS;
    let first_concept_map = root_set_files("hl7.fhir.r4b.core")
        .into_iter()
        .find(|p| {
            p.file_name()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|n| n.starts_with("ConceptMap-"))
        })
        .and_then(|p| {
            p.file_name()
                .and_then(std::ffi::OsStr::to_str)
                .map(str::to_owned)
        })
        .expect("a ConceptMap example");
    for file in [
        "ValueSet-administrative-gender.json",
        "CodeSystem-administrative-gender.json",
        first_concept_map.as_str(),
    ] {
        let path = vendor_dir()
            .join("hl7.fhir.r4b.core")
            .join("package")
            .join(file);
        let canonical =
            canonical::<fhir_types::r4b::resource::Resource>("hl7.fhir.r4b.core", &path)
                .expect("the file reads")
                .expect("the codec accepts it");
        let xml = to_xml(schemas, canonical.as_object().expect("object")).expect("XML");
        insta::assert_snapshot!(file, xml);
    }
}

#[test]
fn xml_refusals_name_the_element() {
    let schemas = &fhir_types::r5::schema::SCHEMAS;
    let refused = |xml: &str| from_xml(schemas, xml).expect_err("refused");
    let unknown =
        refused(r#"<Parameters xmlns="http://hl7.org/fhir"><foo value="1"/></Parameters>"#);
    assert_eq!(unknown.kind, DecodeErrorKind::UnknownElement);
    assert_eq!(unknown.path, "Resource.foo");
    let text = refused(
        r#"<Parameters xmlns="http://hl7.org/fhir"><parameter>text</parameter></Parameters>"#,
    );
    assert_eq!(text.kind, DecodeErrorKind::UnexpectedText);
    assert_eq!(text.path, "Resource.parameter");
    let root = refused(r#"<Coding xmlns="http://hl7.org/fhir"><code value="a"/></Coding>"#);
    assert_eq!(root.kind, DecodeErrorKind::WrongRoot);
    let boolean = refused(
        r#"<Parameters xmlns="http://hl7.org/fhir"><parameter><name value="x"/><valueBoolean value="yes"/></parameter></Parameters>"#,
    );
    assert_eq!(boolean.kind, DecodeErrorKind::BadValue);
    assert_eq!(boolean.path, "Resource.parameter.valueBoolean");
    let attribute = refused(r#"<Parameters xmlns="http://hl7.org/fhir" foo="1"/>"#);
    assert_eq!(attribute.kind, DecodeErrorKind::UnknownProperty);
    let malformed = refused(r#"<Parameters xmlns="http://hl7.org/fhir"><parameter>"#);
    assert!(matches!(
        malformed.kind,
        DecodeErrorKind::MalformedXml { .. }
    ));
    // Element ids, extensions on primitives, and a namespace prefix are read.
    let read = from_xml(
        schemas,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<f:Parameters xmlns:f="http://hl7.org/fhir"><!-- a comment -->
  <f:parameter f:id="p1"><f:name value="n"/><f:valueString id="s1" value="v"><f:extension url="http://example.org/x"><f:valueBoolean value="true"/></f:extension></f:valueString></f:parameter>
</f:Parameters>"#,
    )
    .expect("reads");
    let root = Value::Object(read);
    assert_eq!(member(&root, "resourceType").as_str(), Some("Parameters"));
    let parameter = first(member(&root, "parameter"));
    assert_eq!(member(parameter, "id").as_str(), Some("p1"));
    assert_eq!(member(parameter, "valueString").as_str(), Some("v"));
    let element = member(parameter, "_valueString");
    assert_eq!(member(element, "id").as_str(), Some("s1"));
    assert_eq!(
        member(first(member(element, "extension")), "url").as_str(),
        Some("http://example.org/x")
    );
}

/// Exchanged content "SHALL NOT specify the schema or even contain the schema
/// instance namespace in the resource itself" (R4 xml.html §2.6.1.1, STU3
/// §2.6.1.1, R5 §2.1.6.3.4).
#[test]
fn the_schema_instance_namespace_is_refused_in_a_resource() {
    let declared = r#"<Parameters xmlns="http://hl7.org/fhir" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="http://hl7.org/fhir fhir-single.xsd"><id value="synthetic"/></Parameters>"#;
    for schemas in [
        &fhir_types::stu3::schema::SCHEMAS,
        &fhir_types::r4::schema::SCHEMAS,
        &fhir_types::r5::schema::SCHEMAS,
    ] {
        let refused = from_xml(schemas, declared).expect_err("the root declares xsi");
        assert_eq!(refused.kind, DecodeErrorKind::UnknownProperty);
        assert_eq!(refused.path, "Resource.xmlns:xsi");
    }
    let schemas = &fhir_types::r4::schema::SCHEMAS;
    let nested = from_xml(
        schemas,
        r#"<Parameters xmlns="http://hl7.org/fhir"><parameter xmlns:s="http://www.w3.org/2001/XMLSchema-instance"><name value="n"/></parameter></Parameters>"#,
    )
    .expect_err("a nested element declares the namespace under another prefix");
    assert_eq!(nested.kind, DecodeErrorKind::UnknownProperty);
    assert_eq!(nested.path, "Resource.parameter.xmlns:s");
    let undeclared = from_xml(
        schemas,
        r#"<Parameters xmlns="http://hl7.org/fhir" xsi:schemaLocation="http://hl7.org/fhir fhir-single.xsd"/>"#,
    )
    .expect_err("schemaLocation is no FHIR property");
    assert_eq!(undeclared.kind, DecodeErrorKind::UnknownProperty);
    assert_eq!(undeclared.path, "Resource.schemaLocation");
}

/// The three versions the namespace rules are pinned in.
fn namespace_versions() -> [&'static Schemas; 3] {
    [
        &fhir_types::stu3::schema::SCHEMAS,
        &fhir_types::r4::schema::SCHEMAS,
        &fhir_types::r5::schema::SCHEMAS,
    ]
}

/// "FHIR elements are always in the namespace `http://hl7.org/fhir`" (R4
/// xml.html §2.6.1, STU3 §2.6.1, R5 §2.1.6.3): a root in another namespace,
/// or in none, is no FHIR resource.
#[test]
fn a_root_outside_the_fhir_namespace_is_refused() {
    for schemas in namespace_versions() {
        for xml in [
            r#"<x:Parameters xmlns:x="http://example.org/other"><x:id value="synthetic"/></x:Parameters>"#,
            r#"<Parameters xmlns="http://example.org/other"><id value="synthetic"/></Parameters>"#,
            r#"<Parameters><id value="synthetic"/></Parameters>"#,
            r#"<f:Parameters xmlns="http://hl7.org/fhir"><id value="synthetic"/></f:Parameters>"#,
        ] {
            let refused = from_xml(schemas, xml).expect_err(xml);
            assert_eq!(refused.kind, DecodeErrorKind::WrongRoot, "{xml}");
            assert_eq!(refused.path, "Resource", "{xml}");
        }
    }
}

/// The namespace decides, never the prefix (R4 xml.html §2.6.1).
#[test]
fn a_prefixed_fhir_root_is_read() {
    for schemas in namespace_versions() {
        let read = from_xml(
            schemas,
            r#"<f:Parameters xmlns:f="http://hl7.org/fhir"><f:id value="synthetic"/></f:Parameters>"#,
        )
        .expect("a prefix bound to the FHIR namespace is FHIR");
        let root = Value::Object(read);
        assert_eq!(member(&root, "resourceType").as_str(), Some("Parameters"));
        assert_eq!(member(&root, "id").as_str(), Some("synthetic"));
    }
}

/// A child, a primitive's extension, or a contained resource outside the
/// FHIR namespace is refused at its own element (R4 xml.html §2.6.1).
#[test]
fn a_nested_element_outside_the_fhir_namespace_is_refused() {
    for schemas in namespace_versions() {
        let child = from_xml(
            schemas,
            r#"<Parameters xmlns="http://hl7.org/fhir"><x:parameter xmlns:x="http://example.org/other"><name value="n"/></x:parameter></Parameters>"#,
        )
        .expect_err("a child in a foreign namespace");
        assert_eq!(child.kind, DecodeErrorKind::UnknownElement);
        assert_eq!(child.path, "Resource.parameter");
        let defaulted = from_xml(
            schemas,
            r#"<Parameters xmlns="http://hl7.org/fhir"><parameter xmlns="http://example.org/other"><name value="n"/></parameter></Parameters>"#,
        )
        .expect_err("a child that redeclares the default namespace");
        assert_eq!(defaulted.kind, DecodeErrorKind::UnknownElement);
        assert_eq!(defaulted.path, "Resource.parameter");
        let grandchild = from_xml(
            schemas,
            r#"<Parameters xmlns="http://hl7.org/fhir" xmlns:x="http://example.org/other"><parameter><x:name value="n"/></parameter></Parameters>"#,
        )
        .expect_err("a grandchild under a prefix the root binds elsewhere");
        assert_eq!(grandchild.kind, DecodeErrorKind::UnknownElement);
        assert_eq!(grandchild.path, "Resource.parameter.name");
        let extension = from_xml(
            schemas,
            r#"<Parameters xmlns="http://hl7.org/fhir"><parameter><name value="n"><x:extension xmlns:x="http://example.org/other" url="http://example.org/e"><valueBoolean value="true"/></x:extension></name></parameter></Parameters>"#,
        )
        .expect_err("a primitive's extension in a foreign namespace");
        assert_eq!(extension.kind, DecodeErrorKind::UnknownElement);
        assert_eq!(extension.path, "Resource.parameter.name.extension");
        let contained = from_xml(
            schemas,
            r#"<Parameters xmlns="http://hl7.org/fhir"><parameter><name value="n"/><resource><Parameters xmlns="http://example.org/other"/></resource></parameter></Parameters>"#,
        )
        .expect_err("a nested resource in a foreign namespace");
        assert_eq!(contained.kind, DecodeErrorKind::WrongRoot);
        assert_eq!(contained.path, "Resource.parameter.resource");
    }
}

/// FHIR's attributes are unprefixed; "the only other namespace that occurs in
/// FHIR resources is the XHTML namespace" (R4 xml.html §2.6.1), so a prefixed
/// attribute in another namespace, or under an unbound prefix, is refused.
#[test]
fn a_prefixed_attribute_outside_the_fhir_namespace_is_refused() {
    for schemas in namespace_versions() {
        for (xml, path) in [
            (
                r#"<Parameters xmlns="http://hl7.org/fhir" xmlns:x="http://example.org/other"><parameter x:id="p1"><name value="n"/></parameter></Parameters>"#,
                "Resource.parameter.id",
            ),
            (
                r#"<Parameters xmlns="http://hl7.org/fhir" xmlns:x="http://example.org/other"><parameter><name x:value="n"/></parameter></Parameters>"#,
                "Resource.parameter.name.value",
            ),
            (
                r#"<Parameters xmlns="http://hl7.org/fhir"><parameter><name xml:lang="en" value="n"/></parameter></Parameters>"#,
                "Resource.parameter.name.lang",
            ),
            (
                r#"<Parameters xmlns="http://hl7.org/fhir"><parameter y:id="p1"><name value="n"/></parameter></Parameters>"#,
                "Resource.parameter.id",
            ),
        ] {
            let refused = from_xml(schemas, xml).expect_err(xml);
            assert_eq!(refused.kind, DecodeErrorKind::UnknownProperty, "{xml}");
            assert_eq!(refused.path, path, "{xml}");
        }
        let read = from_xml(
            schemas,
            r#"<Parameters xmlns="http://hl7.org/fhir" xmlns:f="http://hl7.org/fhir"><parameter f:id="p1"><name value="n"/></parameter></Parameters>"#,
        )
        .expect("a prefix bound to the FHIR namespace");
        let parameter = first(member(&Value::Object(read), "parameter")).clone();
        assert_eq!(member(&parameter, "id").as_str(), Some("p1"));
    }
}

/// A synthetic Basic resource with the narrative `div` given.
fn with_narrative(div: &str) -> String {
    format!(
        r#"<Basic xmlns="http://hl7.org/fhir"><text><status value="generated"/>{div}</text><code><text value="synthetic"/></code></Basic>"#
    )
}

/// The narrative is "a root `<div>` element in the xhtml namespace" (R5
/// xml.html §2.1.6.3.3; R4 xml.html §2.6.1: "the only other namespace that
/// occurs in FHIR resources is the XHTML namespace"), and its content is
/// carried verbatim.
#[test]
fn the_narrative_div_is_held_to_the_xhtml_namespace() {
    for schemas in namespace_versions() {
        for div in [
            "<div><p>synthetic</p></div>",
            r#"<div xmlns="http://example.org/other"><p>synthetic</p></div>"#,
            r#"<x:div xmlns:x="http://example.org/other"><p>synthetic</p></x:div>"#,
            r#"<h:div xmlns:h="http://hl7.org/fhir"><p>synthetic</p></h:div>"#,
        ] {
            let xml = with_narrative(div);
            let refused = from_xml(schemas, &xml).expect_err(div);
            assert_eq!(refused.kind, DecodeErrorKind::UnknownElement, "{div}");
            assert_eq!(refused.path, "Resource.text.div", "{div}");
        }
        for div in [
            r#"<div xmlns="http://www.w3.org/1999/xhtml"><p>synthetic <b>text</b></p></div>"#,
            r#"<h:div xmlns:h="http://www.w3.org/1999/xhtml"><h:p>synthetic</h:p></h:div>"#,
        ] {
            let read = from_xml(schemas, &with_narrative(div)).expect(div);
            let text = member(&Value::Object(read), "text").clone();
            assert_eq!(member(&text, "div").as_str(), Some(div));
        }
    }
}

/// Exchanged content SHALL NOT "contain the schema instance namespace in the
/// resource itself" (R4 xml.html §2.6.1.1), the narrative included.
#[test]
fn the_schema_instance_namespace_is_refused_in_the_narrative() {
    for schemas in namespace_versions() {
        for (div, path) in [
            (
                r#"<div xmlns="http://www.w3.org/1999/xhtml" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><p>synthetic</p></div>"#,
                "Resource.text.div.xmlns:xsi",
            ),
            (
                r#"<div xmlns="http://www.w3.org/1999/xhtml"><p xmlns:s="http://www.w3.org/2001/XMLSchema-instance">synthetic</p></div>"#,
                "Resource.text.div.xmlns:s",
            ),
            (
                r#"<div xmlns="http://www.w3.org/1999/xhtml"><p><span xmlns="http://www.w3.org/2001/XMLSchema-instance"/></p></div>"#,
                "Resource.text.div.xmlns",
            ),
        ] {
            let refused = from_xml(schemas, &with_narrative(div)).expect_err(div);
            assert_eq!(refused.kind, DecodeErrorKind::UnknownProperty, "{div}");
            assert_eq!(refused.path, path, "{div}");
        }
    }
}

/// The writer carries the narrative as it was read.
#[test]
fn the_writer_carries_the_narrative_unchanged() {
    let div =
        r#"<div xmlns="http://www.w3.org/1999/xhtml"><p>synthetic &amp; <b>text</b></p></div>"#;
    for schemas in namespace_versions() {
        let read = from_xml(schemas, &with_narrative(div)).expect("reads");
        let xml = to_xml(schemas, &read).expect("writes");
        assert!(xml.contains(div), "{xml}");
        let again = from_xml(schemas, &xml).expect("reads back");
        assert_eq!(again, read);
    }
}

/// The writer declares the FHIR namespace and nothing of XML Schema
/// (R4 xml.html §2.6.1 and §2.6.1.1).
#[test]
fn the_writer_declares_no_schema_instance_namespace() {
    let schemas = &fhir_types::r4::schema::SCHEMAS;
    let read = from_xml(
        schemas,
        r#"<f:Parameters xmlns:f="http://hl7.org/fhir"><f:id value="synthetic"/><f:parameter><f:name value="n"/><f:valueString value="v"/></f:parameter></f:Parameters>"#,
    )
    .expect("reads");
    let xml = to_xml(schemas, &read).expect("writes");
    assert!(
        xml.starts_with(r#"<Parameters xmlns="http://hl7.org/fhir">"#),
        "{xml}"
    );
    assert!(!xml.contains("XMLSchema-instance"), "{xml}");
    assert!(!xml.contains("schemaLocation"), "{xml}");
    assert_eq!(xml.matches("xmlns").count(), 1, "{xml}");
}

fn coding(system: &str, code: &str, display: Option<String>) -> Value {
    let mut object = fhir_types::codec::Object::new();
    object.insert("system".into(), Value::String(system.into()));
    object.insert("code".into(), Value::String(code.into()));
    if let Some(display) = display {
        object.insert("display".into(), Value::String(display));
    }
    Value::Object(object)
}

proptest! {
    /// A `Parameters` of every primitive kind, strings with markup, a nested
    /// coding, and a decimal's lexical form round-trips through XML.
    #[test]
    fn generated_parameters_round_trip_through_xml(
        // A FHIR string holds at least one character: its lexical form is
        // `[ \r\n\t\S]+` in 4.0.1 and `^[\s\S]+$` in 5.0.0
        // (<https://hl7.org/fhir/R5/datatypes.html#primitive>).
        text in "[a-zA-Z0-9 <>&\"'\u{e9}]{1,24}",
        flag in any::<bool>(),
        number in any::<i32>(),
        decimal in prop_oneof![Just("1.50"), Just("0.001"), Just("-3"), Just("100"), Just("2.0"), Just("0.1234567890123456789012345678")],
        code in "[a-z][a-z0-9-]{0,10}",
        display in proptest::option::of("[A-Za-z ]{1,12}"),
    ) {
        let mut parameters = Vec::new();
        let named = |name: &str, key: &str, value: Value| {
            let mut object = fhir_types::codec::Object::new();
            object.insert("name".into(), Value::String(name.into()));
            object.insert(key.into(), value);
            Value::Object(object)
        };
        parameters.push(named("text", "valueString", Value::String(text)));
        parameters.push(named("flag", "valueBoolean", Value::Bool(flag)));
        parameters.push(named("number", "valueInteger", Value::from(number)));
        parameters.push(named(
            "decimal",
            "valueDecimal",
            Value::Number(decimal.parse().expect("a number")),
        ));
        parameters.push(named("coding", "valueCoding", coding("http://example.org/s", &code, display)));
        let mut object = fhir_types::codec::Object::new();
        object.insert("resourceType".into(), Value::String("Parameters".into()));
        object.insert("parameter".into(), Value::Array(parameters));
        let mut path = fhir_types::codec::Path::root("Parameters");
        let typed = fhir_types::r5::parameters::Parameters::from_json(&object, &mut path)
            .expect("the JSON codec accepts the generated resource");
        let canonical = Value::Object(typed.to_json().expect("encodes"));
        round_trip(&fhir_types::r5::schema::SCHEMAS, &canonical, "generated").expect("round trips");
    }
}

/// The member named `key`, which the document under test carries.
fn member<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).expect("the member exists")
}

/// The first item of an array the document under test carries.
fn first(value: &Value) -> &Value {
    value
        .as_array()
        .and_then(<[Value]>::first)
        .expect("a non-empty array")
}
