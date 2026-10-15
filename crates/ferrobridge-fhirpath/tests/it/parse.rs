// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parser through the public API: the precedence of N1 §6.8, the
//! literal forms of N1 §4.1, and refusals with their positions.

use ferrobridge_fhirpath::ast::{BinaryOp, Expr, Literal};
use ferrobridge_fhirpath::expression::Expression;
use ferrobridge_fhirpath::parse::ParseError;

fn tree(text: &str) -> Expr {
    Expression::parse(text)
        .expect("the expression should parse")
        .tree()
        .clone()
}

#[test]
fn multiplication_binds_tighter_than_addition() {
    // N1 §6.8: #04 *, /, div, mod before #05 +, -, &.
    let Expr::Binary { op, right, .. } = tree("1 + 2 * 3") else {
        panic!("expected a binary expression");
    };
    assert_eq!(op, BinaryOp::Add);
    assert!(matches!(
        *right,
        Expr::Binary {
            op: BinaryOp::Multiply,
            ..
        }
    ));
}

#[test]
fn invocation_binds_tighter_than_unary_minus() {
    // N1 §6.8: `-7.combine(3)` negates the result of the invocation.
    let Expr::Negate(operand) = tree("-7.combine(3)") else {
        panic!("expected a negation");
    };
    assert!(matches!(*operand, Expr::Call { .. }));
}

#[test]
fn is_binds_tighter_than_comparison() {
    // N1 §6.8: #06 is, as before #08 comparison.
    let Expr::Binary { op, right, .. } = tree("1 > 2 is Boolean") else {
        panic!("expected a binary expression");
    };
    assert_eq!(op, BinaryOp::Greater);
    assert!(matches!(*right, Expr::Is { .. }));
}

#[test]
fn implies_binds_loosest() {
    let Expr::Binary { op, .. } = tree("a or b implies c and d") else {
        panic!("expected a binary expression");
    };
    assert_eq!(op, BinaryOp::Implies);
}

#[test]
fn a_quantity_takes_a_ucum_unit_or_a_calendar_keyword() {
    // N1 §4.1.8.
    assert!(matches!(
        tree("4 days"),
        Expr::Literal(Literal::Quantity(..))
    ));
    assert!(matches!(
        tree("10 'mg'"),
        Expr::Literal(Literal::Quantity(..))
    ));
}

#[test]
fn keywords_that_may_be_identifiers_are_identifiers() {
    // N1 §8.5: as, contains, in and is may also be identifiers.
    assert!(Expression::parse("'abc'.contains('b')").is_ok());
    assert!(Expression::parse("text.`div`.exists()").is_ok());
    assert!(Expression::parse("text.div.exists()").is_err());
}

#[test]
fn an_incomplete_expression_is_refused_at_its_end() {
    // N1 §12.1: the grammar is the source of truth.
    assert_eq!(
        Expression::parse("2 + 2 /"),
        Err(ParseError::Syntax { offset: 7 })
    );
}

#[test]
fn a_stray_character_is_refused_where_it_stands() {
    assert_eq!(
        Expression::parse("name # given"),
        Err(ParseError::Lex {
            offset: 5,
            text: String::from("#")
        })
    );
}

#[test]
fn an_impossible_date_is_refused() {
    // N1 §4.1.5: the literal names a calendar date.
    assert!(matches!(
        Expression::parse("@2015-02-30"),
        Err(ParseError::Literal { offset: 0, .. })
    ));
}

#[test]
fn an_integer_past_32_bits_is_refused() {
    // N1 §4.1.3: integers range from -2^31 to 2^31-1.
    assert!(matches!(
        Expression::parse("2147483648"),
        Err(ParseError::Literal { .. })
    ));
    assert!(Expression::parse("2147483647").is_ok());
}

#[test]
fn a_time_takes_no_offset() {
    // N1 §4.1.6: time values have no timezone offset.
    assert!(Expression::parse("@T14:34:28Z").is_err());
}

#[test]
fn an_expression_keeps_its_source() {
    let expression = Expression::parse("name.given").expect("parses");
    assert_eq!(expression.to_string(), "name.given");
    assert_eq!(expression.source(), "name.given");
}
