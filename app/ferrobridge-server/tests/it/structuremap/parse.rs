// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR Mapping Language grammar (FHIR R4 `mapping.g4`), over the
//! header and rules the mapping language page and the tutorial write.

use std::error::Error;

use ferrobridge_server::structuremap::parse::parse;
use ferrobridge_server::structuremap::syntax::{
    Dependent, Expression, InputMode, Invocation, Literal, ModelMode, Param, SourceListMode,
    TargetListMode, Transform, TypeMode,
};

/// The tutorial's header (mapping-tutorial.html, step 1), with `rules` in
/// its group.
fn tutorial(rules: &str) -> String {
    format!(
        "map \"http://hl7.org/fhir/StructureMap/tutorial\" = tutorial\n\
         uses \"http://hl7.org/fhir/StructureDefinition/tutorial-left\" as source\n\
         uses \"http://hl7.org/fhir/StructureDefinition/tutorial-right\" as target\n\
         group tutorial(source src : TLeft, target tgt : TRight) {{\n{rules}\n}}\n"
    )
}

#[test]
fn the_header_and_a_group_read_as_the_grammar_writes_them() -> Result<(), Box<dyn Error>> {
    let map = parse(
        "map \"http://hl7.org/fhir/StructureMap/CodeSystem3to4\" = \"R3 to R4 Conversions for CodeSystem\"\n\
         uses \"http://hl7.org/fhir/3.0/StructureDefinition/CodeSystem\" alias CodeSystemR3 as source // documentation\n\
         uses \"http://hl7.org/fhir/StructureDefinition/CodeSystem\" as target\n\
         imports \"http://hl7.org/fhir/StructureMap/*3to4\"\n\
         group CodeSystem(source src : CodeSystemR3, target tgt : CodeSystem) extends DomainResource <<type+>> {\n\
           src.value : code as vs0 -> tgt.value = create(\"code\") as vt0 then code(vs0, vt0) \"valueCode\";\n\
         }",
    )?;
    assert_eq!("http://hl7.org/fhir/StructureMap/CodeSystem3to4", map.url);
    assert_eq!("R3 to R4 Conversions for CodeSystem", map.name);
    assert_eq!(Some("CodeSystemR3"), map.uses[0].alias.as_deref());
    assert_eq!(ModelMode::Source, map.uses[0].mode);
    assert_eq!(ModelMode::Target, map.uses[1].mode);
    assert_eq!(vec![String::from("http://hl7.org/fhir/StructureMap/*3to4")], map.imports);
    let group = &map.groups[0];
    assert_eq!(Some("DomainResource"), group.extends.as_deref());
    assert_eq!(Some(TypeMode::TypePlus), group.type_mode);
    assert_eq!(InputMode::Target, group.parameters[1].mode);
    let rule = &group.rules[0];
    assert_eq!(Some("valueCode"), rule.name.as_deref());
    assert_eq!(Some("code"), rule.sources[0].type_name.as_deref());
    assert_eq!(
        Some(Transform::Invocation(Invocation {
            name: String::from("create"),
            params: vec![Param::Literal(Literal::String(String::from("code")))],
        })),
        rule.targets[0].transform
    );
    assert_eq!(
        Some(Dependent::Invocation(Invocation {
            name: String::from("code"),
            params: vec![
                Param::Variable(String::from("vs0")),
                Param::Variable(String::from("vt0"))
            ],
        })),
        rule.dependent
    );
    Ok(())
}

#[test]
fn the_tutorial_rules_read() -> Result<(), Box<dyn Error>> {
    let map = parse(&tutorial(
        "src.a as a -> tgt.a = a \"rule_a\";\n\
         src.a -> tgt.a;\n\
         src.a2 as a -> tgt.a2 = truncate(a, 20); // just cut it off at 20 characters\n\
         src.a2 as a where a2.length <= 20 -> tgt.a2 = a; // ignore it\n\
         src.a2 as a check a2.length <= 20 -> tgt.a2 = a;\n\
         src.a21 as a where a.isInteger -> tgt.a21 = cast(a, \"integer\");\n\
         src.a23 only_one as a -> tgt.a23 = a;\n\
         src.a23 first as a -> tgt.a23 = a;\n\
         src.aa as s_aa -> tgt.aa as t_aa then { s_aa.ab as ab -> t_aa.ab = ab; };\n\
         src.aa as s_aa -> tgt.aa as t_aa then ab_content(s_aa, t_aa);\n\
         src.d as d -> tgt.d = translate(d, 'uri-of-concept-map', 'code');\n\
         src.i as i where m < 2 -> tgt.j = i;\n\
         src.f as s_f -> tgt.e as t_e first then { s_f -> t_e.f = s_f, t_e.g = 'g2'; };\n\
         src.f1 as s_f1 -> create(\"TRight2\") as rr, tgt.ptr = reference(rr) then { s_f1 -> rr.f2 = s_f1; };",
    ))?;
    let rules = &map.groups[0].rules;
    assert_eq!(14, rules.len());
    assert_eq!(
        Some(Expression(String::from("a2.length <= 20"))),
        rules[3].sources[0].condition
    );
    assert_eq!(
        Some(Expression(String::from("a2.length <= 20"))),
        rules[4].sources[0].check
    );
    assert_eq!(Some(SourceListMode::OnlyOne), rules[6].sources[0].list_mode);
    assert_eq!(Some(TargetListMode::First), rules[12].targets[0].list_mode);
    assert_eq!(None, rules[13].targets[0].context);
    assert_eq!(Some(String::from("rr")), rules[13].targets[0].variable);
    Ok(())
}

#[test]
fn an_embedded_expression_may_be_parenthesized_and_hold_clause_words_in_strings()
-> Result<(), Box<dyn Error>> {
    let map = parse(&tutorial(
        "src.a as a where (a.contains(', where ') and a != 'then') -> tgt.b = (a.upper()), tgt.c = 'x';",
    ))?;
    let rule = &map.groups[0].rules[0];
    assert_eq!(
        Some(Expression(String::from("a.contains(', where ') and a != 'then'"))),
        rule.sources[0].condition
    );
    assert_eq!(
        Some(Transform::Evaluate(Expression(String::from("a.upper()")))),
        rule.targets[0].transform
    );
    assert_eq!(2, rule.targets.len());
    Ok(())
}

#[test]
fn a_text_outside_the_grammar_is_refused_at_its_place() {
    for (text, expected) in [
        ("group g(source s, target t) { s -> t; }", "`map`"),
        ("map \"u\" = n", "at least one group"),
        ("map \"u\" = n group g(source s, target t) { s.a -> t.a }", "`;` at the end of the rule"),
        ("map \"u\" = n group g(source s, target t) { }", "at least one rule"),
        ("map \"u\" = n uses \"x\" as consumer group g(source s, target t) { s -> t; }", "source, queried, target or produced"),
        ("map \"u = n", "a closing quote"),
        ("map \"u\" = n group g(input s) { s -> t; }", "source or target"),
    ] {
        let refused = parse(text);
        assert!(
            refused.as_ref().is_err_and(|error| error.expected == expected),
            "{text}: {refused:?}"
        );
    }
}
