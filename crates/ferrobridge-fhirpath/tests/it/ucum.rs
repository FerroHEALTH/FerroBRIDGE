// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! UCUM unit expressions and conversion through the public API, and the
//! FHIRPath operations that use them, each case citing its clause.

use std::cmp::Ordering;

use ferrobridge_fhirpath::ucum::{Canonical, UcumError};
use rust_decimal::Decimal;

use super::support::{eval_none, truth};

fn unit(code: &str) -> Canonical {
    Canonical::parse(code).unwrap_or_else(|error| panic!("{code} should parse: {error}"))
}

fn d(text: &str) -> Decimal {
    text.parse().expect("a decimal")
}

fn convert(value: &str, from: &str, to: &str) -> Result<Decimal, UcumError> {
    unit(from).convert(d(value), &unit(to))
}

#[test]
fn a_prefix_scales_its_atom() {
    // UCUM §4: the longest prefix whose remainder is a metric atom.
    assert_eq!(convert("1", "g", "mg"), Ok(d("1000")));
    assert_eq!(convert("1", "kg", "g"), Ok(d("1000")));
    assert_eq!(convert("1", "dam", "m"), Ok(d("10")));
    assert_eq!(convert("1", "ug", "ng"), Ok(d("1000")));
}

#[test]
fn a_prefix_on_a_non_metric_atom_is_no_unit() {
    // UCUM §4.1: only metric units take a prefix.
    assert!(matches!(
        Canonical::parse("k[in_i]"),
        Err(UcumError::UnknownAtom { .. })
    ));
    assert!(matches!(
        Canonical::parse("gram"),
        Err(UcumError::UnknownAtom { .. })
    ));
}

#[test]
fn an_exponent_raises_the_prefix_with_the_atom() {
    // UCUM §9.2: 1 cm3 is 10^-6 m3.
    assert_eq!(convert("1", "cm3", "m3"), Ok(d("0.000001")));
    assert_eq!(convert("1", "m-1", "cm-1"), Ok(d("0.01")));
    assert_eq!(convert("1", "m+2", "dm2"), Ok(d("100")));
}

#[test]
fn the_ten_power_atoms_and_factors_scale() {
    // UCUM §8 and the essence's 10* and 10^ atoms.
    assert_eq!(convert("1", "10*3", "1"), Ok(d("1000")));
    assert_eq!(convert("1", "10^-3", "1"), Ok(d("0.001")));
    assert_eq!(convert("1", "10*3/L", "/mL"), Ok(d("1")));
    assert_eq!(convert("1", "1000", "1"), Ok(d("1000")));
    assert_eq!(convert("1", "%", "1"), Ok(d("0.01")));
}

#[test]
fn operators_evaluate_left_to_right_and_parentheses_group() {
    // UCUM §7.4 and §10: a/b/c is a/(b.c).
    assert_eq!(convert("1", "mmol/L/h", "mmol/(L.h)"), Ok(d("1")));
    assert_eq!(
        convert("1", "/min", "/s"),
        Ok(d("0.0166666666666666666666666667"))
    );
    assert_eq!(convert("1", "kg.m/s2", "N"), Ok(d("1")));
}

#[test]
fn an_annotation_carries_no_meaning() {
    // UCUM §6.2 and §6.4.
    assert_eq!(convert("1", "{tablet}", "1"), Ok(d("1")));
    assert_eq!(convert("1", "mg{total}", "g"), Ok(d("0.001")));
    assert_eq!(convert("3", "{RBC}/uL", "/mL"), Ok(d("3000")));
}

#[test]
fn invalid_expressions_are_refused_with_their_position() {
    // UCUM §3, §5, §6, §7.
    assert_eq!(
        Canonical::parse("m.").map(|_| ()),
        Err(UcumError::Syntax {
            code: String::from("m."),
            position: 2,
            reason: "a unit is missing",
        })
    );
    for code in ["", "m s", "(m", "[in_i", "{a", "kg//m", "m/", "m.(s"] {
        assert!(
            matches!(Canonical::parse(code), Err(UcumError::Syntax { .. })),
            "{code:?}"
        );
    }
    assert!(matches!(
        Canonical::parse("Kg"),
        Err(UcumError::UnknownAtom { .. })
    ));
}

#[test]
fn mass_volume_and_substance_concentrations_convert() {
    assert_eq!(convert("1", "mmol/L", "mol/m3"), Ok(d("1")));
    assert_eq!(convert("5.5", "mmol/L", "umol/mL"), Ok(d("5.5")));
    assert_eq!(convert("1", "g/dL", "mg/mL"), Ok(d("10")));
}

#[test]
fn customary_units_convert_by_their_definitions() {
    // UCUM §34, §35 and §39.
    assert_eq!(convert("1", "[in_i]", "cm"), Ok(d("2.54")));
    assert_eq!(convert("12", "[in_i]", "[ft_i]"), Ok(d("1")));
    assert_eq!(convert("1", "[lb_av]", "g"), Ok(d("453.59237")));
    assert_eq!(convert("3937", "[ft_us]", "m"), Ok(d("1200")));
}

#[test]
fn temperatures_convert_by_their_functions() {
    // UCUM §30.4, §43.4 and §43.5.
    assert_eq!(convert("37", "Cel", "K"), Ok(d("310.15")));
    assert_eq!(convert("310.15", "K", "Cel"), Ok(d("37")));
    assert_eq!(convert("32", "[degF]", "Cel"), Ok(d("0")));
    assert_eq!(convert("212", "[degF]", "Cel"), Ok(d("100")));
    assert_eq!(convert("80", "[degRe]", "Cel"), Ok(d("100")));
    assert_eq!(convert("1", "mCel", "Cel"), Ok(d("0.001")));
    assert_eq!(
        unit("[degF]").size_in(&unit("K")),
        Ok(d("0.5555555555555555555555555556"))
    );
}

#[test]
fn a_special_unit_takes_part_in_no_operation() {
    // UCUM §22.1.
    for code in ["Cel2", "Cel/s", "/Cel", "m.Cel"] {
        assert!(
            matches!(Canonical::parse(code), Err(UcumError::SpecialInTerm { .. })),
            "{code}"
        );
    }
}

#[test]
fn an_unsupported_special_function_refuses_to_convert() {
    let error = convert("7", "[pH]", "mol/L");
    assert_eq!(
        error,
        Err(UcumError::UnsupportedSpecial {
            function: String::from("pH")
        })
    );
    assert!(unit("[pH]").is_commensurable(&unit("mol/L")));
}

#[test]
fn an_arbitrary_unit_converts_to_nothing() {
    // UCUM §24.3 and §25.
    assert_eq!(convert("1", "[iU]", "[IU]"), Err(UcumError::Arbitrary));
    assert_eq!(convert("1", "[iU]/L", "[iU]/mL"), Err(UcumError::Arbitrary));
    assert!(!unit("[iU]").is_commensurable(&unit("[iU]")));
}

#[test]
fn units_of_different_dimensions_do_not_convert() {
    // UCUM §17.
    assert_eq!(convert("1", "mg", "mL"), Err(UcumError::Incommensurable));
    assert_eq!(convert("1", "cm2", "cm"), Err(UcumError::Incommensurable));
}

#[test]
fn the_more_granular_unit_is_the_smaller() {
    assert_eq!(unit("mg").compare_size(&unit("g")), Ok(Ordering::Less));
    assert_eq!(
        unit("[in_i]").compare_size(&unit("cm")),
        Ok(Ordering::Greater)
    );
    assert_eq!(
        unit("N").compare_size(&unit("kg.m/s2")),
        Ok(Ordering::Equal)
    );
}

#[test]
fn the_suite_quantity_cases_hold() {
    // The HL7 FHIRPath suite's testQuantity1 to 4 and 9 (N1 §6.1, §6.6.1).
    for expression in [
        "4.0000 'g' = 4000.0 'mg'",
        "4 'g' ~ 4000 'mg'",
        "4 'g' != 4040 'mg'",
        "4 'g' ~ 4040 'mg'",
        "2.0 'cm' * 2.0 'm' = 0.040 'm2'",
        "37 'Cel' = 310.15 'K'",
        "1 'kg.m/s2' = 1 'N'",
        "1 '[in_i]' > 2.5 'cm'",
    ] {
        assert_eq!(
            truth(&eval_none(expression).expect("evaluates")),
            Some(true),
            "{expression}"
        );
    }
}

#[test]
fn incommensurable_units_compare_as_empty() {
    // N1 §6.1.1 and §6.2: invalid or incommensurable units give empty.
    for expression in [
        "1 'mg' = 1 'mL'",
        "1 'mg' < 1 'mL'",
        "1 'mg' + 1 'mL'",
        "1 'g' = 1 'gram'",
    ] {
        assert_eq!(
            eval_none(expression).expect("evaluates"),
            Vec::new(),
            "{expression}"
        );
    }
    assert_eq!(
        truth(&eval_none("1 'mg' ~ 1 'mL'").expect("evaluates")),
        Some(false)
    );
}

#[test]
fn to_quantity_converts_to_the_unit_asked_for() {
    // N1 §5.5.7.
    let cases = [
        ("1 '[in_i]'.toQuantity('cm') = 2.54 'cm'", Some(true)),
        ("'5 \\'mg\\''.toQuantity('g') = 0.005 'g'", Some(true)),
        ("1 second.toQuantity('ms') = 1000 'ms'", Some(true)),
        ("1 'mg'.convertsToQuantity('g')", Some(true)),
        ("1 'mg'.convertsToQuantity('mL')", Some(false)),
        ("1 day.convertsToQuantity('h')", Some(false)),
    ];
    for (expression, expected) in cases {
        assert_eq!(
            truth(&eval_none(expression).expect("evaluates")),
            expected,
            "{expression}"
        );
    }
    assert_eq!(
        eval_none("1 'mg'.toQuantity('mL')").expect("evaluates"),
        Vec::new()
    );
}
