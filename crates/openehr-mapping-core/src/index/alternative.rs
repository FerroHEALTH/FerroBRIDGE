// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The value node of a polymorphic `ELEMENT` a stored value belongs to.
//!
//! `ELEMENT.value` is declared as the abstract `DATA_VALUE` (openEHR RM
//! Release 1.1.0, data structures, the `ELEMENT` class), so a template that
//! admits several data types gives the element one value node per type, and
//! every one of them shares the element's `aqlPath`. The element holds one
//! value, and the `_type` its canonical JSON carries (openEHR ITS-JSON) says
//! which node it belongs to. No specification governs the choice among nodes
//! whose classes are related: our own design, the most specific class that
//! the value conforms to wins, and the first in template order on a tie.

use openehr_rm::v1_2::model;
use serde_json::Value;

use crate::index::ResolvedNode;

/// Why the class of a value at a polymorphic `ELEMENT` could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Untyped;

/// Returns the node among `alternatives` that `value` belongs to, `None` when
/// no node admits its class.
///
/// A generic class such as `DV_INTERVAL<DV_QUANTITY>` is named without its
/// parameter in `_type`, so the parameter is read from the `_type` of the
/// interval's `lower` or `upper` limit where one is present.
pub(super) fn chosen<'node>(
    alternatives: &[&'node ResolvedNode],
    value: &Value,
) -> Result<Option<&'node ResolvedNode>, Untyped> {
    let named = value.get("_type").and_then(Value::as_str).ok_or(Untyped)?;
    let parameter = ["lower", "upper"].iter().find_map(|limit| {
        value
            .get(limit)
            .and_then(|bound| bound.get("_type"))
            .and_then(Value::as_str)
    });
    let mut best: Option<(&'node ResolvedNode, usize)> = None;
    for &node in alternatives {
        let (base, declared) = split_generic(node.rm_type());
        let admits = model::is_a(named, base)
            && match (declared, parameter) {
                (Some(declared), Some(parameter)) => model::is_a(parameter, declared),
                _ => true,
            };
        if !admits {
            continue;
        }
        let depth = model::ancestors(base).len();
        if best.is_none_or(|(_, held)| depth > held) {
            best = Some((node, depth));
        }
    }
    Ok(best.map(|(node, _)| node))
}

/// Splits a class name into its base and its one generic parameter.
fn split_generic(rm_type: &str) -> (&str, Option<&str>) {
    match rm_type.split_once('<') {
        Some((base, rest)) => (base, rest.strip_suffix('>')),
        None => (rm_type, None),
    }
}

#[cfg(test)]
mod tests {
    use super::split_generic;

    #[test]
    fn a_generic_class_splits_into_its_base_and_parameter() {
        assert_eq!(
            split_generic("DV_INTERVAL<DV_QUANTITY>"),
            ("DV_INTERVAL", Some("DV_QUANTITY"))
        );
        assert_eq!(split_generic("DV_TEXT"), ("DV_TEXT", None));
    }
}
