// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR namespace rules on the resource, its elements and its attributes.

use fhir_types::codec::DecodeErrorKind;
use fhir_types::codec::Value;
use fhir_types::xml::{from_xml, to_xml};

use super::{first, member, namespace_versions};

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
    .expect_err("schemaLocation under an undeclared xsi prefix");
    // NOTE: Namespaces in XML 1.0 §5 (Prefix Declared): an undeclared prefix is malformed XML
    // before R4 xml.html §2.6.1.1 applies, so the refusal names the prefix (#830).
    assert_eq!(
        undeclared.kind,
        DecodeErrorKind::MalformedXml {
            reason: String::from("the prefix xsi has no namespace declaration in scope")
        }
    );
    assert_eq!(undeclared.path, "Resource.schemaLocation");
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
        ] {
            let refused = from_xml(schemas, xml).expect_err(xml);
            assert_eq!(refused.kind, DecodeErrorKind::WrongRoot, "{xml}");
            assert_eq!(refused.path, "Resource", "{xml}");
        }
        // NOTE: Namespaces in XML 1.0 §5 (Prefix Declared): a root under an undeclared prefix
        // is malformed XML before any namespace rule of FHIR applies (#830).
        let unbound =
            r#"<f:Parameters xmlns="http://hl7.org/fhir"><id value="synthetic"/></f:Parameters>"#;
        let refused = from_xml(schemas, unbound).expect_err(unbound);
        assert_eq!(
            refused.kind,
            DecodeErrorKind::MalformedXml {
                reason: String::from("the prefix f has no namespace declaration in scope")
            }
        );
        assert_eq!(refused.path, "Resource");
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

/// An element under a prefix with no declaration in scope, the resource root
/// or a child, makes the document not namespace-well-formed (Namespaces in XML
/// 1.0 §5, Prefix Declared), so it is malformed XML naming the prefix (#830).
#[test]
fn an_unbound_element_prefix_in_the_resource_is_malformed_xml() {
    for schemas in namespace_versions() {
        for (xml, path, prefix) in [
            (
                r#"<f:Parameters><f:parameter><f:name value="n"/></f:parameter></f:Parameters>"#,
                "Resource",
                "f",
            ),
            (
                r#"<Parameters xmlns="http://hl7.org/fhir"><parameter><z:name value="n"/></parameter></Parameters>"#,
                "Resource.parameter.name",
                "z",
            ),
        ] {
            let refused = from_xml(schemas, xml).expect_err(xml);
            let DecodeErrorKind::MalformedXml { reason } = &refused.kind else {
                panic!("{xml}: {:?}", refused.kind);
            };
            assert!(
                reason.contains(&format!("prefix {prefix} ")),
                "{xml}: {reason}"
            );
            assert_eq!(refused.path, path, "{xml}");
        }
    }
}

/// FHIR's attributes are unprefixed; "the only other namespace that occurs in
/// FHIR resources is the XHTML namespace" (R4 xml.html §2.6.1), so a prefixed
/// attribute in another namespace is refused. An attribute under an unbound
/// prefix is refused too, as malformed XML: "The namespace prefix, unless it
/// is `xml` or `xmlns`, MUST have been declared in a namespace declaration
/// attribute" (Namespaces in XML 1.0 §5, Prefix Declared), so the document is
/// not namespace-well-formed before any FHIR rule applies (#830).
#[test]
fn a_prefixed_attribute_outside_the_fhir_namespace_is_refused() {
    for schemas in namespace_versions() {
        let unbound = r#"<Parameters xmlns="http://hl7.org/fhir"><parameter y:id="p1"><name value="n"/></parameter></Parameters>"#;
        let refused = from_xml(schemas, unbound).expect_err(unbound);
        let DecodeErrorKind::MalformedXml { reason } = &refused.kind else {
            panic!("{unbound}: {:?}", refused.kind);
        };
        assert!(reason.contains("prefix y "), "{reason}");
        assert_eq!(refused.path, "Resource.parameter.id");
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
