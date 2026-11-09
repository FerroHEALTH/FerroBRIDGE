// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The rows the inverse reads, and how an instance finds its label.

use std::collections::BTreeMap;

use fhir_types::codec::Value;
use hl7v2_types::model::Optionality;

use crate::map::condition::{Expr, Operand, Probe};
use crate::map::corpus::{Assignment, Condition, Map, Row};
use crate::map::inverse::grid::Grid;
use crate::map::notation::{Label, Step, Target};

/// When a row runs.
#[derive(Debug, Clone, Copy)]
pub(super) enum Gate<'c> {
    Always,
    Computable(&'c Expr),
}

/// One row the inverse reads: its source positions, its target steps
/// relative to the element the map fills, and the literal it assigns.
#[derive(Debug, Clone)]
pub(super) struct Line<'c> {
    pub(super) row: &'c Row,
    pub(super) source: Vec<usize>,
    pub(super) source_type: Option<String>,
    pub(super) steps: Vec<Step>,
    pub(super) literal: Option<&'c str>,
    pub(super) gate: Gate<'c>,
    pub(super) deprecated: bool,
}

/// Reads the rows of `map` the inverse can run: a path or `$value` target
/// with no reference, no instance but the first, a literal assignment or
/// none, a computable condition or none, and a source the table holds.
/// `table` gives whether each source position is deprecated (marked `B` or
/// `W`) and its type. The lines of a deprecated source are dropped wherever a
/// line of a current one writes the same element back.
pub(super) fn lines<'c>(
    map: &'c Map,
    table: &dyn Fn(usize) -> Option<(bool, Option<&'static str>)>,
) -> Vec<Line<'c>> {
    let mut found = Vec::new();
    for row in &map.rows {
        let steps = match &row.target {
            Ok(Target::Path { instance, steps })
                if instance.is_none() || *instance == Some(Label::Number(1)) =>
            {
                steps.clone()
            }
            Ok(Target::Value { instance })
                if instance.is_none() || *instance == Some(Label::Number(1)) =>
            {
                Vec::new()
            }
            _ => continue,
        };
        if steps.iter().any(|step| step.reference.is_some()) {
            continue;
        }
        let gate = match &row.condition {
            Condition::Always => Gate::Always,
            Condition::Computable(expr) => Gate::Computable(expr),
            _ => continue,
        };
        let literal = match &row.assignment {
            None => None,
            Some(Assignment::Literal(text)) => Some(text.as_str()),
            Some(_) => continue,
        };
        let source = positions(&row.source);
        let Some(&first) = source.first() else {
            continue;
        };
        let Some((deprecated, data_type)) = table(first) else {
            continue;
        };
        let source_type = row
            .source_type
            .clone()
            .or_else(|| data_type.map(String::from));
        found.push(Line {
            row,
            source,
            source_type,
            steps,
            literal,
            gate,
            deprecated,
        });
    }
    let current: Vec<String> = found
        .iter()
        .filter(|line| !line.deprecated)
        .map(element)
        .collect();
    found.retain(|line| !line.deprecated || !current.contains(&element(line)));
    found
}

/// The element a line writes back from: its first step's name, empty for
/// `$value`.
pub(super) fn element(line: &Line<'_>) -> String {
    line.steps
        .first()
        .map(|step| step.name.clone())
        .unwrap_or_default()
}

/// Whether an optionality marks a source deprecated: `B` or `W`.
pub(super) const fn deprecated(optionality: Optionality) -> bool {
    matches!(optionality, Optionality::B | Optionality::W)
}

/// The positions a source code names after its segment or data type:
/// `PID-5.1` and `XPN.1` give `[5, 1]` and `[1]`.
pub(super) fn positions(source: &str) -> Vec<usize> {
    let rest = source
        .split_once('-')
        .map_or_else(
            || source.split_once('.').map(|(_, rest)| rest),
            |(_, rest)| Some(rest),
        )
        .unwrap_or_default();
    rest.split('.')
        .map_while(|part| part.parse::<usize>().ok())
        .collect()
}

/// One value the inverse writes back: the row, the FHIR value, its path, and
/// the instance key that groups the writes of one repetition.
pub(super) type Leaf<'l, 'c> = (&'l Line<'c>, Value, String, String);

/// The lines of one label: the element name, the label, and the lines.
pub(super) type Slot<'l, 'c> = (String, Option<Label>, Vec<&'l Line<'c>>);

/// The leaves with the rows that have no condition first, each group in row
/// order.
pub(super) fn order<'l, 'c>(leaves: Vec<Leaf<'l, 'c>>) -> Vec<Leaf<'l, 'c>> {
    let (mut ordered, computable): (Vec<_>, Vec<_>) = leaves
        .into_iter()
        .partition(|(line, ..)| matches!(line.gate, Gate::Always));
    ordered.extend(computable);
    ordered
}

/// Collects the path of every primitive value under `node`, leaving out the
/// resource's `resourceType`, `id` and `meta`.
pub(super) fn primitives(node: &Value, path: &str, root: bool, found: &mut Vec<String>) {
    match node {
        Value::Object(object) => {
            for (key, member) in object {
                if root && matches!(key.as_str(), "resourceType" | "id" | "meta") {
                    continue;
                }
                let here = format!("{path}{}{key}", dot(path));
                match member {
                    Value::Array(items) => {
                        for (index, item) in items.iter().enumerate() {
                            primitives(item, &format!("{here}[{index}]"), false, found);
                        }
                    }
                    other => primitives(other, &here, false, found),
                }
            }
        }
        Value::Array(_) => {}
        _ => found.push(String::from(path)),
    }
}

/// Reads a condition operand over the fields built so far.
pub(super) fn segment_probe(
    fields: &BTreeMap<usize, Vec<Grid>>,
    id: &str,
    operand: &Operand,
) -> Option<Probe> {
    let Operand::Field { segment, path } = operand else {
        return None;
    };
    if segment != id {
        return None;
    }
    let (position, inner) = path.split_first()?;
    let repetitions = fields.get(position).map(Vec::as_slice).unwrap_or_default();
    let mut probe = repetitions
        .first()
        .map(|grid| grid.probe(inner))
        .unwrap_or_default();
    probe.count = u64::try_from(repetitions.iter().filter(|grid| !grid.is_empty()).count())
        .unwrap_or(u64::MAX);
    Some(probe)
}

/// The separator before a member of `path`.
pub(super) fn dot(path: &str) -> &'static str {
    if path.is_empty() { "" } else { "." }
}

/// The text of a FHIR primitive value, `None` for an object or an array.
pub(super) fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.as_str().to_owned()),
        _ => None,
    }
}

/// The slot of `named` an instance goes to: the first with room whose
/// required matches it holds and whose preferences it holds, else the first
/// with room whose required matches it holds. A label that takes one
/// instance and requires no match takes the instance at its own position:
/// `given[2]` is the second given name.
pub(super) fn choose<'c>(
    slots: &[Slot<'_, 'c>],
    named: &[usize],
    taken: &[usize],
    (position, item): (usize, &Value),
    depth: usize,
    capacity: &dyn Fn(&Line<'c>) -> usize,
) -> Option<usize> {
    let room = |index: usize| {
        slots.get(index).is_some_and(|(_, label, members)| {
            let limit = members.first().map_or(1, |line| capacity(line));
            let required = members
                .iter()
                .any(|line| line.literal.is_some() && matches!(line.gate, Gate::Always));
            let placed = match label {
                Some(Label::Number(number)) if limit == 1 && !required => usize::try_from(*number)
                    .is_ok_and(|number| number == position.saturating_add(1)),
                _ => true,
            };
            placed && taken.get(index).copied().unwrap_or_default() < limit
        })
    };
    let holds = |index: usize, preferred: bool| {
        slots.get(index).is_some_and(|(_, _, members)| {
            members
                .iter()
                .filter_map(|line| line.literal.map(|literal| (line, literal)))
                .filter(|(line, _)| matches!(line.gate, Gate::Always) != preferred)
                .all(|(line, literal)| {
                    read(
                        item,
                        line.steps
                            .get(depth.saturating_add(1)..)
                            .unwrap_or_default(),
                    )
                    .and_then(Value::as_str)
                        == Some(literal)
                })
        })
    };
    named
        .iter()
        .copied()
        .find(|index| room(*index) && holds(*index, false) && holds(*index, true))
        .or_else(|| {
            named
                .iter()
                .copied()
                .find(|index| room(*index) && holds(*index, false))
        })
}

/// The value at `steps` below `node`, through the first instance of each
/// repeating element.
pub(super) fn read<'v>(node: &'v Value, steps: &[Step]) -> Option<&'v Value> {
    let mut current = node;
    for step in steps {
        current = current.get(&step.name)?;
        if let Value::Array(items) = current {
            current = items.first()?;
        }
    }
    Some(current)
}
