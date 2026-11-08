// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The four flags of a `DV_INTERVAL` a tail wrote bound by bound.
//!
//! ITS-JSON marks `lower_unbounded`, `upper_unbounded`, `lower_included` and
//! `upper_included` of `INTERVAL` required, and BASE Release 1.2.0 foundation
//! types §Interval ties them to the bounds: `Lower_unbounded: lower_unbounded
//! implies lower = Void` and `Lower_included_valid: lower_unbounded implies
//! not lower_included`, the same for the upper side. A mapping that writes
//! `range/lower` and `range/upper` as two tails names no flag, so the engine
//! derives each flag the value lacks from the bound it holds once every
//! mapping has written.

use openehr_rm::v1_2::model::Container;
use openehr_rm::v1_2::typing::RmType;
use openehr_rm::v1_2::typing::narrowed;
use openehr_rm::v1_2::typing::step_type;
use serde_json::Value;

/// The class whose flags are derived.
const INTERVAL: &str = "DV_INTERVAL";

/// Fills the flags every `DV_INTERVAL` in `value`, an instance of `ty`, lacks.
///
/// An absent bound is unbounded and not included, a present one bounded and
/// included; a flag the value already holds is kept, since a mapping wrote it.
/// The walk types each member through `openehr_rm::v1_2::typing` from
/// `rm_type`, so an interval is found by its type, whether or not it names a
/// `_type`; a type name that does not parse, and a member the model does not
/// type, are left as they stand.
pub fn fill_flags(value: &mut Value, rm_type: &str) {
    if let Ok(ty) = rm_type.parse::<RmType>() {
        fill(value, &ty);
    }
}

/// Fills the flags of every `DV_INTERVAL` in `value`, an instance of `ty`.
fn fill(value: &mut Value, ty: &RmType) {
    let Value::Object(ref mut members) = *value else {
        return;
    };
    let holder = members
        .get("_type")
        .and_then(Value::as_str)
        .and_then(|tag| narrowed(ty, tag).ok())
        .unwrap_or_else(|| ty.clone());
    if holder.name() == INTERVAL {
        for side in ["lower", "upper"] {
            let bounded = members.get(side).is_some_and(|bound| !bound.is_null());
            members
                .entry(format!("{side}_unbounded"))
                .or_insert(Value::Bool(!bounded));
            members
                .entry(format!("{side}_included"))
                .or_insert(Value::Bool(bounded));
        }
    }
    for (attribute, member) in members.iter_mut() {
        let Ok(step) = step_type(&holder, attribute) else {
            continue;
        };
        match (step.container(), member) {
            (Container::None, member) => fill(member, step.item()),
            (_, &mut Value::Array(ref mut items)) => {
                for item in items {
                    fill(item, step.item());
                }
            }
            (_, _) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Returns the flags of the one other reference range of a quantity
    /// whose range holds `range`.
    fn flags(range: &Value) -> Value {
        let mut quantity = json!({
            "_type": "DV_QUANTITY",
            "magnitude": 1.0,
            "units": "mg/dL",
            "other_reference_ranges": [{"meaning": {"value": "treatment"}, "range": range.clone()}]
        });
        fill_flags(&mut quantity, "DV_QUANTITY");
        let range = &quantity["other_reference_ranges"][0]["range"];
        json!([
            range["lower_unbounded"],
            range["upper_unbounded"],
            range["lower_included"],
            range["upper_included"]
        ])
    }

    fn bound(magnitude: f64) -> Value {
        json!({"magnitude": magnitude, "units": "mg/dL"})
    }

    #[test]
    fn a_range_with_a_lower_bound_alone_is_unbounded_above() {
        assert_eq!(
            flags(&json!({"lower": bound(1.0)})),
            json!([false, true, true, false])
        );
    }

    #[test]
    fn a_range_with_an_upper_bound_alone_is_unbounded_below() {
        assert_eq!(
            flags(&json!({"upper": bound(9.0)})),
            json!([true, false, false, true])
        );
    }

    #[test]
    fn a_range_with_both_bounds_is_bounded_and_included_on_both_sides() {
        assert_eq!(
            flags(&json!({"lower": bound(1.0), "upper": bound(9.0)})),
            json!([false, false, true, true])
        );
    }

    #[test]
    fn a_flag_a_mapping_wrote_is_kept() {
        assert_eq!(
            flags(&json!({"lower": bound(1.0), "upper": bound(9.0), "upper_included": false})),
            json!([false, false, true, false])
        );
    }
}
