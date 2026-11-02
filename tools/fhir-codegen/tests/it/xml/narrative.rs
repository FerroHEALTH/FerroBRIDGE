// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The narrative `div`: its XHTML namespace, its well-formedness and what the
//! writer carries.

use fhir_types::codec::DecodeErrorKind;
use fhir_types::codec::Value;
use fhir_types::schema::Schemas;
use fhir_types::xml::{from_xml, to_xml};

use super::{member, namespace_versions};

/// A synthetic Basic resource with the narrative `div` given.
fn with_narrative(div: &str) -> String {
    format!(
        r#"<Basic xmlns="http://hl7.org/fhir"><text><status value="generated"/>{div}</text><code><text value="synthetic"/></code></Basic>"#
    )
}

/// The narrative is "a root `<div>` element in the xhtml namespace" (R5
/// xml.html §2.1.6.3.3; R4 xml.html §2.6.1: "the only other namespace that
/// occurs in FHIR resources is the XHTML namespace"), every element inside it
/// included.
#[test]
fn the_narrative_div_is_held_to_the_xhtml_namespace() {
    for schemas in namespace_versions() {
        for div in [
            "<div><p>synthetic</p></div>",
            r#"<div xmlns="http://example.org/other"><p>synthetic</p></div>"#,
            r#"<x:div xmlns:x="http://example.org/other"><p>synthetic</p></x:div>"#,
            r#"<h:div xmlns:h="http://hl7.org/fhir"><p>synthetic</p></h:div>"#,
            r#"<h:div xmlns:h="http://www.w3.org/1999/xhtml"><p>synthetic</p></h:div>"#,
            r#"<div xmlns="http://www.w3.org/1999/xhtml"><p xmlns="http://example.org/other">synthetic</p></div>"#,
        ] {
            let xml = with_narrative(div);
            let refused = from_xml(schemas, &xml).expect_err(div);
            assert_eq!(refused.kind, DecodeErrorKind::UnknownElement, "{div}");
            assert_eq!(refused.path, "Resource.text.div", "{div}");
        }
        let div = r#"<div xmlns="http://www.w3.org/1999/xhtml"><p>synthetic <b>text</b></p></div>"#;
        assert_eq!(narrative_of(schemas, &with_narrative(div)), div);
    }
}

/// A narrative the `FerroPIX` fuzzer reached: no whitespace between two
/// attributes, and a name that starts with `%`, both outside
/// `STag ::= '<' Name (S Attribute)* S? '>'` (XML 1.0 §3.1 \[40\]).
const PIX_DIV: &str = r#"<div xmlns="http://www.w3.org/1999/xhtml"><table><tr><td style="width:1px"%title="synthetic">x</td></tr></table></div>"#;

/// A narrative or an element whose attributes are not well-formed XML is
/// refused as malformed on reading, never repaired by inserting whitespace.
#[test]
fn a_start_tag_with_malformed_attributes_is_refused_on_reading() {
    for schemas in namespace_versions() {
        for (xml, path) in [
            (with_narrative(PIX_DIV), "Resource.text.div"),
            (
                with_narrative(r#"<div xmlns="http://www.w3.org/1999/xhtml"><p class="a"id="b">x</p></div>"#),
                "Resource.text.div",
            ),
            (
                r#"<Basic xmlns="http://hl7.org/fhir"><code><text value="synthetic"id="x"/></code></Basic>"#.to_owned(),
                "Resource.code.text",
            ),
        ] {
            let refused = from_xml(schemas, &xml).expect_err(&xml);
            assert!(
                matches!(refused.kind, DecodeErrorKind::MalformedXml { .. }),
                "{xml}: {:?}",
                refused.kind
            );
            assert_eq!(refused.path, path, "{xml}");
        }
    }
}

/// The JSON object of a synthetic Basic resource with the narrative `div`
/// given.
fn basic_with_div(div: &str) -> fhir_types::codec::Object {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Basic",
        "text": {"status": "generated", "div": div},
        "code": {"text": "synthetic"},
    }))
    .as_object()
    .cloned()
    .expect("the synthetic Basic is an object")
}

/// The JSON `div` is "a single escaped string of XHTML" whose root "needs to
/// be a `<div>` in the xhtml namespace" (R4 json.html §2.6.2.1 and §2.6.2.4),
/// and the XML form writes it as an element of the document, so a string that
/// is no such `div` is refused naming its element path, never written into a
/// malformed document.
#[test]
fn a_narrative_div_that_is_no_xhtml_div_is_refused_on_writing_xml() {
    for schemas in namespace_versions() {
        for div in [
            "synthetic text with no element",
            "<p xmlns=\"http://www.w3.org/1999/xhtml\">synthetic</p>",
            "<div>synthetic</div>",
            r#"<div xmlns="http://example.org/other">synthetic</div>"#,
            r#"<div xmlns="http://www.w3.org/1999/xhtml"><p>synthetic</div>"#,
            r#"<div xmlns="http://www.w3.org/1999/xhtml">synthetic</div><div xmlns="http://www.w3.org/1999/xhtml">two</div>"#,
            r#"<div xmlns="http://www.w3.org/1999/xhtml"><p xmlns="http://example.org/other">synthetic</p></div>"#,
            r#"<div xmlns="http://www.w3.org/1999/xhtml"><x:b>synthetic</x:b></div>"#,
            PIX_DIV,
        ] {
            let refused = to_xml(schemas, &basic_with_div(div)).expect_err(div);
            let text = refused.to_string();
            assert!(text.contains("Basic.text.div"), "{div}: {text}");
        }
        let div = r#"<div xmlns="http://www.w3.org/1999/xhtml"><p>synthetic <b>text</b></p></div>"#;
        let xml = to_xml(schemas, &basic_with_div(div)).expect(div);
        assert_eq!(narrative_of(schemas, &xml), div);
    }
}

/// A narrative inside a contained resource is refused naming the path through
/// the containing resource.
#[test]
fn a_nested_narrative_that_is_no_xhtml_div_names_its_path() {
    let mut outer = basic_with_div(r#"<div xmlns="http://www.w3.org/1999/xhtml">outer</div>"#);
    outer.insert(
        String::from("contained"),
        Value::Array(vec![Value::Object(basic_with_div("<p>inner</p>"))]),
    );
    let refused = to_xml(&fhir_types::r4::schema::SCHEMAS, &outer).expect_err("refused");
    let text = refused.to_string();
    assert!(text.contains("Basic.contained[0].text.div"), "{text}");
}

/// A prefix inside the narrative with no namespace declaration in scope makes
/// the document not namespace-well-formed (Namespaces in XML 1.0 §5, Prefix
/// Declared), so the reader refuses it as malformed XML naming the prefix, on
/// the `div`, on an element inside it, and on an attribute.
#[test]
fn an_unbound_prefix_inside_the_narrative_is_malformed_xml() {
    for schemas in namespace_versions() {
        for (div, path, prefix) in [
            ("<h:div><p>synthetic</p></h:div>", "Resource.text.div", "h"),
            (
                r#"<div xmlns="http://www.w3.org/1999/xhtml"><p>synthetic <x:b>text</x:b></p></div>"#,
                "Resource.text.div",
                "x",
            ),
            (
                r#"<div xmlns="http://www.w3.org/1999/xhtml"><p y:title="t">synthetic</p></div>"#,
                "Resource.text.div.y:title",
                "y",
            ),
        ] {
            let refused = from_xml(schemas, &with_narrative(div)).expect_err(div);
            let DecodeErrorKind::MalformedXml { reason } = &refused.kind else {
                panic!("{div}: {:?}", refused.kind);
            };
            assert!(
                reason.contains(&format!("prefix {prefix} ")),
                "{div}: {reason}"
            );
            assert_eq!(refused.path, path, "{div}");
        }
    }
}

/// The `div` string of the narrative `xml` reads to.
fn narrative_of(schemas: &Schemas, xml: &str) -> String {
    let read = from_xml(schemas, xml).expect(xml);
    let text = member(&Value::Object(read), "text").clone();
    member(&text, "div")
        .as_str()
        .expect("the div is a string")
        .to_owned()
}

/// The JSON `div` is "a single escaped string of XHTML" whose root "needs to
/// be a `<div>` in the xhtml namespace" (R4 json.html §2.6.2.1 and §2.6.2.4,
/// whose example uses the default `xmlns` form), and its content is what a
/// browser takes as `innerHTML` (R4 narrative.html §2.4.0): a prefixed `div`
/// reads to that form, its own declaration or an ancestor's alike.
#[test]
fn a_prefixed_narrative_reads_to_a_self_contained_div() {
    let expected = r#"<div xmlns="http://www.w3.org/1999/xhtml"><p class="c">synthetic <b>text</b><br/></p></div>"#;
    let prefixed = r#"<h:div><h:p class="c">synthetic <h:b>text</h:b><h:br/></h:p></h:div>"#;
    let on_ancestor = format!(
        r#"<Basic xmlns="http://hl7.org/fhir" xmlns:h="http://www.w3.org/1999/xhtml"><text><status value="generated"/>{prefixed}</text><code><text value="synthetic"/></code></Basic>"#
    );
    let on_div = with_narrative(
        r#"<h:div xmlns:h="http://www.w3.org/1999/xhtml"><h:p class="c">synthetic <h:b>text</h:b><h:br/></h:p></h:div>"#,
    );
    for schemas in namespace_versions() {
        for xml in [&on_ancestor, &on_div] {
            let div = narrative_of(schemas, xml);
            assert_eq!(div, expected, "{xml}");
            // The string reads back alone, under a root that binds no prefix.
            assert_eq!(narrative_of(schemas, &with_narrative(&div)), div);
        }
    }
}

/// An attribute value is written in double quotes, so a `"` it holds is
/// escaped (XML 1.0 §2.3 `AttValue`, §4.6): a single-quoted value with a
/// double quote reads to a well-formed string that reads back equal.
#[test]
fn a_single_quoted_narrative_attribute_is_escaped_for_double_quotes() {
    let source = r#"<div xmlns="http://www.w3.org/1999/xhtml" title='a "b"'><p title='c "d" &amp; e'>synthetic</p></div>"#;
    let expected = r#"<div xmlns="http://www.w3.org/1999/xhtml" title="a &quot;b&quot;"><p title="c &quot;d&quot; &amp; e">synthetic</p></div>"#;
    for schemas in namespace_versions() {
        let div = narrative_of(schemas, &with_narrative(source));
        assert_eq!(div, expected);
        assert_eq!(narrative_of(schemas, &with_narrative(&div)), div);
    }
}

/// A prefixed attribute in the narrative has no namespace FHIR admits (R4
/// xml.html §2.6.1); `xml:lang` is in the XML namespace and is kept.
#[test]
fn a_prefixed_narrative_attribute_outside_the_xml_namespace_is_refused() {
    for schemas in namespace_versions() {
        let refused = from_xml(
            schemas,
            &with_narrative(
                r#"<div xmlns="http://www.w3.org/1999/xhtml"><p xmlns:x="http://example.org/other" x:a="1">synthetic</p></div>"#,
            ),
        )
        .expect_err("refused");
        assert_eq!(refused.kind, DecodeErrorKind::UnknownProperty);
        assert_eq!(refused.path, "Resource.text.div.x:a");
        let div =
            r#"<div xmlns="http://www.w3.org/1999/xhtml" xml:lang="nl"><p>synthetic</p></div>"#;
        assert_eq!(narrative_of(schemas, &with_narrative(div)), div);
    }
}

/// The writer carries the self-contained `div` an ancestor-bound prefix read
/// to, and that XML reads back to the same resource.
#[test]
fn the_writer_round_trips_a_narrative_bound_on_an_ancestor() {
    let xml = r#"<Basic xmlns="http://hl7.org/fhir" xmlns:h="http://www.w3.org/1999/xhtml"><text><status value="generated"/><h:div title='a "b"'><h:p>synthetic</h:p></h:div></text><code><text value="synthetic"/></code></Basic>"#;
    let div = r#"<div xmlns="http://www.w3.org/1999/xhtml" title="a &quot;b&quot;"><p>synthetic</p></div>"#;
    for schemas in namespace_versions() {
        let read = from_xml(schemas, xml).expect("reads");
        let written = to_xml(schemas, &read).expect("writes");
        assert!(written.contains(div), "{written}");
        let again = from_xml(schemas, &written).expect("reads back");
        assert_eq!(again, read);
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
