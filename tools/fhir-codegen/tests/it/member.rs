// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The member lookup of the element table: a JSON member name resolved to its
//! element, the choice alternative it selects, and the `_` sibling of a
//! primitive (<https://hl7.org/fhir/R4/json.html#2.6.2>).

use fhir_types::schema::{Kind, Schemas, TypeSchema, ValueKind};

/// The emitted table of each version.
fn tables() -> [(&'static str, &'static Schemas); 5] {
    [
        ("stu3", &fhir_types::stu3::schema::SCHEMAS),
        ("r4", &fhir_types::r4::schema::SCHEMAS),
        ("r4b", &fhir_types::r4b::schema::SCHEMAS),
        ("r5", &fhir_types::r5::schema::SCHEMAS),
        ("r6", &fhir_types::r6::schema::SCHEMAS),
    ]
}

fn observation() -> &'static TypeSchema {
    fhir_types::r4::schema::SCHEMAS
        .type_named("Observation")
        .expect("R4 defines Observation")
}

#[test]
fn a_plain_element_is_a_member_by_its_name() {
    let member = observation().member("status").expect("status is a member");
    assert_eq!(member.field.path, "Observation.status");
    assert_eq!(member.kind, Kind::Primitive(ValueKind::Text));
    assert_eq!(member.choice, None);
    assert!(!member.sibling);
}

#[test]
fn a_choice_form_names_its_element_and_the_type_code_it_selects() {
    let member = observation()
        .member("valueDateTime")
        .expect("valueDateTime is a member");
    assert_eq!(member.field.path, "Observation.value[x]");
    assert_eq!(member.choice, Some("dateTime"), "the code keeps its case");
    assert_eq!(member.kind, Kind::Primitive(ValueKind::Text));
    let quantity = observation()
        .member("valueQuantity")
        .expect("valueQuantity is a member");
    assert_eq!(quantity.choice, Some("Quantity"));
    assert_eq!(quantity.kind, Kind::Complex("Quantity"));
}

#[test]
fn the_sibling_of_a_primitive_is_a_member_and_of_a_complex_element_is_not() {
    let sibling = observation()
        .member("_valueString")
        .expect("the sibling of a primitive form is a member");
    assert!(sibling.sibling);
    assert_eq!(sibling.choice, Some("string"));
    assert!(observation().member("_status").is_some_and(|m| m.sibling));
    assert_eq!(
        observation().member("_valueQuantity"),
        None,
        "a complex form has no sibling"
    );
    assert_eq!(observation().member("_code"), None);
}

#[test]
fn a_bare_choice_stem_and_an_unknown_key_are_no_member() {
    assert_eq!(observation().member("value"), None, "the stem alone");
    assert_eq!(observation().member("value[x]"), None);
    assert_eq!(observation().member("valueNonesuch"), None);
    assert_eq!(
        observation().member("valuequantity"),
        None,
        "the case counts"
    );
    assert_eq!(observation().member("nonesuch"), None);
    assert_eq!(observation().member("_"), None);
}

#[test]
fn every_choice_alternative_carries_a_type_code_of_its_element_in_every_version() {
    for (module, schemas) in tables() {
        for schema in schemas.types {
            for field in schema.fields {
                let Kind::Choice(variants) = field.kind else {
                    assert_eq!(field.choices().count(), 0, "{module} {}", field.path);
                    continue;
                };
                let forms: Vec<_> = field.choices().collect();
                assert_eq!(forms.len(), variants.len(), "{module} {}", field.path);
                for (form, (suffix, kind)) in forms.iter().zip(variants) {
                    assert_eq!((form.suffix, form.kind), (*suffix, *kind));
                    assert!(field.types.contains(&form.code), "{module} {}", field.path);
                    let key = format!("{}{suffix}", field.name);
                    let member = schema.member(&key).expect("every form is a member");
                    assert_eq!(member.field, field, "{module} {key}");
                    assert_eq!(member.choice, Some(form.code), "{module} {key}");
                }
            }
        }
    }
}
