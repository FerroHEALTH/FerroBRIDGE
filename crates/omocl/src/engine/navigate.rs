// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Instances of a composition, and the walk from one to another.
//!
//! An instance is one node of the canonical JSON together with the path that
//! reaches it from the composition root, every step carrying the 1-based
//! position of the node among the siblings of its archetype node id (openEHR
//! BASE Release 1.2.0 §Paths and Locators, positional predicates). That path
//! is what a record key names, and what a parent step walks back up.

use openehr_sdt::flat::navigate::Navigator;
use serde_json::Value;

use crate::resolve::program::Hop;

/// One step of an instance path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    attribute: String,
    node_id: Option<String>,
    position: usize,
}

/// One node of a composition, with the path that reaches it, as the
/// template-aware navigation of `openehr-sdt` found it.
#[derive(Debug, Clone)]
pub(crate) struct Instance<'a> {
    found: openehr_sdt::flat::navigate::Instance<'a>,
}

impl<'a> Instance<'a> {
    /// Returns the composition root.
    pub(crate) const fn root(composition: &'a Value) -> Self {
        Self {
            found: openehr_sdt::flat::navigate::Instance::root(composition),
        }
    }

    /// Returns the node.
    pub(crate) fn value(&self) -> &'a Value {
        self.found.value()
    }

    /// Returns the instance path, every segment with the 1-based position of
    /// its node among the siblings that carry the same archetype node id.
    pub(crate) fn path(&self) -> String {
        let mut steps = Vec::with_capacity(self.found.steps().len());
        let mut parent = self
            .found
            .ancestor(self.found.steps().len())
            .map(|root| root.value());
        for step in self.found.steps() {
            let node_id = step
                .value()
                .get("archetype_node_id")
                .and_then(Value::as_str);
            let siblings: &[Value] = parent
                .and_then(|held| held.get(&step.segment().attribute))
                .and_then(Value::as_array)
                .map_or(&[], Vec::as_slice);
            let position = step.index().map_or(1, |index| {
                siblings
                    .iter()
                    .take(index)
                    .filter(|sibling| {
                        node_id.is_none()
                            || sibling.get("archetype_node_id").and_then(Value::as_str) == node_id
                    })
                    .count()
                    .saturating_add(1)
            });
            steps.push(Step {
                attribute: step.segment().attribute.clone(),
                node_id: node_id.map(str::to_owned),
                position,
            });
            parent = Some(step.value());
        }
        render(&steps)
    }
}

/// Returns every instance `hop` reaches from `from`, in document order.
///
/// A parent step walks back along `from`'s own path, so `../` always names
/// the ancestor the instance was reached through.
pub(crate) fn follow<'a>(
    navigator: &Navigator,
    from: &Instance<'a>,
    hop: &Hop,
) -> Vec<Instance<'a>> {
    let Some(start) = from.found.ancestor(hop.up()) else {
        return Vec::new();
    };
    navigator
        .descend(&start, hop.down())
        .into_iter()
        .map(|found| Instance { found })
        .collect()
}

/// Renders an instance path.
fn render(steps: &[Step]) -> String {
    let mut rendered = String::new();
    for step in steps {
        rendered.push('/');
        rendered.push_str(&step.attribute);
        rendered.push('[');
        if let Some(ref node_id) = step.node_id {
            rendered.push_str(node_id);
            rendered.push_str(" and ");
        }
        rendered.push_str(&step.position.to_string());
        rendered.push(']');
    }
    rendered
}

/// Returns the instance path of every `ELEMENT` of the composition that
/// carries a value, in document order.
pub(crate) fn valued_elements(composition: &Value) -> Vec<String> {
    let mut found = Vec::new();
    collect(composition, &mut Vec::new(), &mut found);
    found
}

/// Collects the valued elements under one node.
fn collect(node: &Value, steps: &mut Vec<Step>, found: &mut Vec<String>) {
    let Value::Object(object) = node else {
        return;
    };
    let class = object.get("_type").and_then(Value::as_str);
    if class.is_some_and(|class| openehr_rm::v1_2::model::is_a(class, "ELEMENT")) {
        if object.contains_key("value") {
            found.push(render(steps));
        }
        return;
    }
    for (attribute, child) in object {
        let items: Vec<&Value> = match *child {
            Value::Array(ref items) => items.iter().collect(),
            Value::Object(_) => vec![child],
            _ => continue,
        };
        let mut seen: Vec<(&str, usize)> = Vec::new();
        for (index, item) in items.into_iter().enumerate() {
            let node_id = item.get("archetype_node_id").and_then(Value::as_str);
            // A step with no node id selects every sibling, as `identity` does.
            let position = node_id.map_or(index.saturating_add(1), |id| {
                if let Some(entry) = seen.iter_mut().find(|entry| entry.0 == id) {
                    entry.1 = entry.1.saturating_add(1);
                    return entry.1;
                }
                seen.push((id, 1));
                1
            });
            steps.push(Step {
                attribute: attribute.clone(),
                node_id: node_id.map(str::to_owned),
                position,
            });
            collect(item, steps, found);
            steps.pop();
        }
    }
}
