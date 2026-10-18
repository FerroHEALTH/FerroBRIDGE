// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The collections whose order is undefined, for the strict ordering mode
//! (`Environment::with_strict_ordering`).
//!
//! N1 §3.1 says the outcome of `children()` and `descendants()` "cannot be
//! assumed to be in any meaningful order", that `first()`, `last()`,
//! `tail()`, `skip()` and `take()` should not be used on collections derived
//! from them, and that an implementation may return an error for an
//! order-dependent operation on a list whose order is undefined. The
//! indexer `[ ]` addresses an item by its ordinal position (N1 §3.1
//! "Indexed"), so it is order-dependent too.

use crate::ast::{BinaryOp, Expr};

/// The functions whose outcome depends on the order of their input (N1 §3.1).
pub(super) const ORDERED_FUNCTIONS: [&str; 5] = ["first", "last", "tail", "skip", "take"];

/// The functions whose output keeps the items of their input, and so its
/// order or the lack of it (N1 §5.2, §5.3, §5.4, §5.9; FHIR R4 §2.9.1.5).
const ORDER_KEEPING: [&str; 15] = [
    "where",
    "select",
    "repeat",
    "ofType",
    "distinct",
    "tail",
    "skip",
    "take",
    "intersect",
    "exclude",
    "union",
    "combine",
    "trace",
    "extension",
    "resolve",
];

/// Whether `expr` evaluates to a collection derived from `children()` or
/// `descendants()`, whose order N1 §3.1 leaves undefined.
///
/// A function that answers one value (`count()`, `first()`, `exists()`)
/// answers a collection whose order is trivially defined; a member, an
/// order-keeping function and the union operator carry the input's lack of
/// order on.
pub(super) fn unordered(expr: &Expr) -> bool {
    match expr {
        Expr::Call { name, .. } if name == "children" || name == "descendants" => true,
        Expr::Call {
            input, name, args, ..
        } if ORDER_KEEPING.contains(&name.as_str()) => {
            input.as_deref().is_some_and(unordered)
                || (matches!(name.as_str(), "union" | "combine") && args.iter().any(unordered))
        }
        Expr::Call { name, args, .. } if name == "iif" => args.iter().skip(1).any(unordered),
        Expr::Member {
            input: Some(input), ..
        } => unordered(input),
        Expr::Invocation { input, variable } => {
            matches!(variable.as_ref(), Expr::This) && unordered(input)
        }
        Expr::As { operand, .. } => unordered(operand),
        Expr::Binary {
            op: BinaryOp::Union,
            left,
            right,
        } => unordered(left) || unordered(right),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::unordered;
    use crate::parse::parse;

    fn of(text: &str) -> bool {
        unordered(&parse(text).expect("parses"))
    }

    #[test]
    fn children_and_descendants_are_unordered() {
        assert!(of("children()"));
        assert!(of("Patient.descendants()"));
    }

    #[test]
    fn order_keeping_steps_carry_the_lack_of_order() {
        assert!(of("children().where(true)"));
        assert!(of("children().name"));
        assert!(of("name | children()"));
        assert!(of("name.union(descendants())"));
        assert!(of("iif(true, children(), name)"));
    }

    #[test]
    fn a_path_and_a_single_value_are_ordered() {
        assert!(!of("Patient.name.given"));
        assert!(!of("children().count()"));
        assert!(!of("children().exists()"));
        assert!(!of("iif(children().exists(), name, name)"));
    }
}
