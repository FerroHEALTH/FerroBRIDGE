// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The engine over the tutorial's logical models (mapping-tutorial.html):
//! copies, the simple form with a default group, conditions and checks,
//! list modes, dependent rules and groups, created instances and the
//! transforms.

use std::error::Error;

use ferrobridge_server::structuremap::engine::{Engine, MapError};
use ferrobridge_server::structuremap::model::Known;
use ferrobridge_server::structuremap::parse::parse;
use fhir_types::codec::Value;
use serde_json::json;

/// A logical model named `name` with the elements `elements`, each a path
/// below the type, its maximum and its type code.
fn model(name: &str, elements: &[(&str, &str, &str)]) -> Value {
    let mut listed = vec![json!({"path": name})];
    listed.extend(elements.iter().map(|(path, max, code)| {
        json!({"path": format!("{name}.{path}"), "max": max, "type": [{"code": code}]})
    }));
    Value::from_serde_json(json!({
        "resourceType": "StructureDefinition",
        "kind": "logical",
        "name": name,
        "snapshot": {"element": listed}
    }))
}

/// The tutorial's models: `TLeft` and `TRight` with an inner type each.
fn structures() -> Result<Known, Box<dyn Error>> {
    let mut known = Known::default();
    for model in [
        model(
            "TLeft",
            &[
                ("a", "1", "string"),
                ("a2", "1", "string"),
                ("a21", "1", "string"),
                ("a23", "*", "string"),
                ("aa", "1", "TLeftInner"),
                ("i", "1", "string"),
                ("m", "1", "integer"),
                ("f1", "1", "string"),
            ],
        ),
        model("TLeftInner", &[("ab", "1", "string")]),
        model(
            "TRight",
            &[
                ("a", "1", "string"),
                ("a2", "1", "string"),
                ("a21", "1", "integer"),
                ("a23", "*", "string"),
                ("aa", "1", "TRightInner"),
                ("j", "1", "string"),
                ("k", "1", "string"),
                ("ptr", "1", "string"),
                ("e", "*", "TRightE"),
            ],
        ),
        model("TRightInner", &[("ab", "1", "string")]),
        model("TRightE", &[("f", "1", "string"), ("g", "1", "string")]),
        model("TRight2", &[("f2", "1", "string")]),
    ] {
        known.logical.add(&model)?;
    }
    Ok(known)
}

/// Runs `groups` as the tutorial map over `source` into a `TRight`.
fn run(
    groups: &str,
    source: serde_json::Value,
) -> Result<(serde_json::Value, Vec<serde_json::Value>), Box<dyn Error>> {
    let map = parse(&format!(
        "map \"http://hl7.org/fhir/StructureMap/tutorial\" = tutorial\n{groups}"
    ))?;
    let engine = Engine::new(vec![map], structures()?);
    let output = engine.transform(
        "http://hl7.org/fhir/StructureMap/tutorial",
        &[Value::from_serde_json(source)],
        "TRight",
    )?;
    let produced = output
        .produced
        .into_iter()
        .map(|value| serde_json::to_value(&value))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((serde_json::to_value(&output.target)?, produced))
}

/// The error of running `groups` over `source`.
fn refused(groups: &str, source: serde_json::Value) -> Result<MapError, Box<dyn Error>> {
    let map = parse(&format!(
        "map \"http://hl7.org/fhir/StructureMap/tutorial\" = tutorial\n{groups}"
    ))?;
    let engine = Engine::new(vec![map], structures()?);
    match engine.transform(
        "http://hl7.org/fhir/StructureMap/tutorial",
        &[Value::from_serde_json(source)],
        "TRight",
    ) {
        Ok(_) => Err("the map ran".into()),
        Err(error) => Ok(error),
    }
}

#[test]
fn a_copy_and_the_simple_form_with_a_default_group() -> Result<(), Box<dyn Error>> {
    let (target, _) = run(
        "group tutorial(source src : TLeft, target tgt : TRight) {\n\
           src.a as a -> tgt.a = a \"rule_a\";\n\
           src.aa -> tgt.aa;\n\
         }\n\
         group ab_content(source src : TLeftInner, target tgt : TRightInner) <<types>> {\n\
           src.ab -> tgt.ab;\n\
         }",
        json!({"a": "x", "aa": {"ab": "y"}}),
    )?;
    assert_eq!(json!({"a": "x", "aa": {"ab": "y"}}), target);
    Ok(())
}

#[test]
fn truncate_where_and_cast() -> Result<(), Box<dyn Error>> {
    let (target, _) = run(
        "group tutorial(source src : TLeft, target tgt : TRight) {\n\
           src.a as a -> tgt.a = truncate(a, 3);\n\
           src.a2 as a where a.length() <= 3 -> tgt.a2 = a;\n\
           src.a21 as a where a.convertsToInteger() -> tgt.a21 = cast(a, 'integer');\n\
           src.i as i where m < 2 -> tgt.j = i;\n\
           src.i as i where m >= 2 -> tgt.k = i;\n\
         }",
        json!({"a": "abcdef", "a2": "toolong", "a21": "42", "i": "v", "m": 1}),
    )?;
    assert_eq!(json!({"a": "abc", "a21": 42, "j": "v"}), target);
    Ok(())
}

#[test]
fn a_failed_check_and_two_only_one_occurrences_fail_the_map() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        MapError::Check(String::from("checked")),
        refused(
            "group tutorial(source src : TLeft, target tgt : TRight) {\n\
               src.a2 as a check a.length() <= 3 -> tgt.a2 = a \"checked\";\n\
             }",
            json!({"a2": "toolong"}),
        )?
    );
    assert_eq!(
        MapError::OnlyOne(String::from("one")),
        refused(
            "group tutorial(source src : TLeft, target tgt : TRight) {\n\
               src.a23 only_one as a -> tgt.a23 = a \"one\";\n\
             }",
            json!({"a23": ["p", "q"]}),
        )?
    );
    Ok(())
}

#[test]
fn the_list_modes_pick_occurrences() -> Result<(), Box<dyn Error>> {
    let (target, _) = run(
        "group tutorial(source src : TLeft, target tgt : TRight) {\n\
           src.a23 first as a -> tgt.a23 = a;\n\
           src.a23 last as a -> tgt.a23 = a;\n\
           src.a23 not_first as a -> tgt.a = a;\n\
         }",
        json!({"a23": ["p", "q"]}),
    )?;
    assert_eq!(json!({"a23": ["p", "q"], "a": "q"}), target);
    Ok(())
}

#[test]
fn dependent_rules_and_groups_write_inside_what_a_rule_created() -> Result<(), Box<dyn Error>> {
    let (target, _) = run(
        "group tutorial(source src : TLeft, target tgt : TRight) {\n\
           src.aa as s_aa -> tgt.aa as t_aa then { s_aa.ab as ab -> t_aa.ab = ab; };\n\
           src.a23 as s -> tgt.e as t_e then fill(s, t_e);\n\
         }\n\
         group fill(source s, target t) {\n\
           s -> t.f = s, t.g = 'g1';\n\
         }",
        json!({"aa": {"ab": "y"}, "a23": ["p", "q"]}),
    )?;
    assert_eq!(
        json!({"aa": {"ab": "y"}, "e": [{"f": "p", "g": "g1"}, {"f": "q", "g": "g1"}]}),
        target
    );
    Ok(())
}

#[test]
fn a_shared_target_collects_every_application() -> Result<(), Box<dyn Error>> {
    let (target, _) = run(
        "group tutorial(source src : TLeft, target tgt : TRight) {\n\
           src.a23 as s -> tgt.e as t_e share then { s -> t_e.f = s; };\n\
         }",
        json!({"a23": ["p", "q"]}),
    )?;
    assert_eq!(json!({"e": [{"f": "q"}]}), target);
    Ok(())
}

#[test]
fn a_created_instance_is_produced_and_referenced() -> Result<(), Box<dyn Error>> {
    let (target, produced) = run(
        "group tutorial(source src : TLeft, target tgt : TRight) {\n\
           src.f1 as s_f1 -> create(\"TRight2\") as rr then { s_f1 -> rr.f2 = s_f1; };\n\
           src.a as a -> tgt.ptr = append('TRight2/', a);\n\
         }",
        json!({"f1": "v", "a": "1"}),
    )?;
    assert_eq!(json!({"ptr": "TRight2/1"}), target);
    assert_eq!(vec![json!({"f2": "v"})], produced);
    Ok(())
}

#[test]
fn the_datatype_transforms_build_r4_datatypes() -> Result<(), Box<dyn Error>> {
    let map = parse(
        "map \"urn:m\" = m\n\
         group g(source src : TLeft, target tgt : Patient) {\n\
           src.a as a -> tgt.identifier = id('urn:s', a, 'MR');\n\
           src.a2 as v -> tgt.telecom = cp(v);\n\
           src.a21 as v -> tgt.maritalStatus = cc('urn:c', v, 'shown');\n\
           src.i as v -> tgt.active = (v = 'yes');\n\
         }",
    )?;
    let engine = Engine::new(vec![map], structures()?);
    let output = engine.transform(
        "urn:m",
        &[Value::from_serde_json(
            json!({"a": "7", "a2": "someone@example.org", "a21": "M", "i": "yes"}),
        )],
        "Patient",
    )?;
    assert_eq!(
        json!({
            "resourceType": "Patient",
            "identifier": [{"system": "urn:s", "value": "7", "type": {"coding": [
                {"system": "http://terminology.hl7.org/CodeSystem/v2-0203", "code": "MR"}
            ]}}],
            "telecom": [{"system": "email", "value": "someone@example.org"}],
            "maritalStatus": {"coding": [{"system": "urn:c", "code": "M", "display": "shown"}]},
            "active": true
        }),
        serde_json::to_value(&output.target)?
    );
    Ok(())
}

#[test]
fn an_unknown_element_a_missing_group_and_translate_are_refused() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        MapError::Element {
            type_name: String::from("TRight"),
            element: String::from("nothing")
        },
        refused(
            "group tutorial(source src : TLeft, target tgt : TRight) { src.a as a -> tgt.nothing = a; }",
            json!({"a": "x"}),
        )?
    );
    assert_eq!(
        MapError::NoGroup(String::from("absent")),
        refused(
            "group tutorial(source src : TLeft, target tgt : TRight) { src.aa as s -> tgt.aa as t then absent(s, t); }",
            json!({"aa": {}}),
        )?
    );
    assert!(matches!(
        refused(
            "group tutorial(source src : TLeft, target tgt : TRight) { src.a as a -> tgt.a = translate(a, 'urn:cm', 'code'); }",
            json!({"a": "x"}),
        )?,
        MapError::Transform { .. }
    ));
    Ok(())
}
