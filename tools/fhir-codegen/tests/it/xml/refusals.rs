// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A refused document names its element, and what the reader admits is read.

use fhir_types::codec::DecodeErrorKind;
use fhir_types::codec::Value;
use fhir_types::xml::from_xml;

use super::{first, member};

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
