// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The walk of one resource back through a map's rows.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;

use ferrobridge_term::request::Translation;
use fhir_types::codec::Value;

use crate::map::condition::{self, Operand};
use crate::map::convert;
use crate::map::corpus::{Corpus, Kind, Map, MappedVia};
use crate::map::inverse::grid::Grid;
use crate::map::inverse::line::{
    Gate, Leaf, Line, Slot, choose, deprecated, dot, lines, order, segment_probe, text,
};
use crate::map::inverse::{InverseError, Outcome};
use crate::map::translate::Translator;

/// The state of one inverse.
pub(super) struct Inverse<'c> {
    pub(super) corpus: &'c Corpus,
    pub(super) translator: Option<Translator<'c>>,
    pub(super) outcomes: Vec<Outcome>,
    pub(super) translations: usize,
    /// The path of every FHIR value written back or matched.
    pub(super) consumed: BTreeSet<String>,
}

/// The lengths of an inverse's records at one point, to roll a tentative
/// write back to.
pub(super) struct Mark {
    pub(super) outcomes: usize,
    pub(super) consumed: BTreeSet<String>,
}

impl<'c> Inverse<'c> {
    /// Records the point a tentative write starts at.
    pub(super) fn mark(&self) -> Mark {
        Mark {
            outcomes: self.outcomes.len(),
            consumed: self.consumed.clone(),
        }
    }

    /// Undoes everything a tentative write recorded since `mark`.
    pub(super) fn rollback(&mut self, mark: Mark) {
        self.outcomes.truncate(mark.outcomes);
        self.consumed = mark.consumed;
    }

    /// Writes each leaf back into the repetition of its field its instance
    /// keys, running a row with a condition only where it holds over the
    /// segment as built with the row's own value in place.
    pub(super) async fn fields(
        &mut self,
        leaves: Vec<Leaf<'_, 'c>>,
        id: &str,
    ) -> Result<BTreeMap<usize, Vec<Grid>>, InverseError> {
        let mut fields: BTreeMap<usize, Vec<Grid>> = BTreeMap::new();
        let mut slots: BTreeMap<(usize, String), usize> = BTreeMap::new();
        for (line, leaf, path, instance) in leaves {
            let Some((&position, inner)) = line.source.split_first() else {
                continue;
            };
            if matches!(line.gate, Gate::Computable(_)) && self.consumed.contains(&path) {
                continue;
            }
            let repetitions = fields.entry(position).or_default();
            let index = *slots.entry((position, instance)).or_insert_with(|| {
                repetitions.push(Grid::default());
                repetitions.len().saturating_sub(1)
            });
            let mut grid = repetitions.get(index).cloned().unwrap_or_default();
            let mark = self.mark();
            self.write(line, &leaf, &path, inner, &mut grid).await?;
            if let Gate::Computable(expr) = line.gate {
                let mut tentative = fields.clone();
                if let Some(slot) = tentative
                    .get_mut(&position)
                    .and_then(|grids| grids.get_mut(index))
                {
                    *slot = grid.clone();
                }
                let holds = condition::evaluate(expr, &mut |operand| {
                    segment_probe(&tentative, id, operand)
                });
                if !matches!(holds, Ok(true)) {
                    self.rollback(mark);
                    continue;
                }
            }
            if let Some(slot) = fields
                .get_mut(&position)
                .and_then(|grids| grids.get_mut(index))
            {
                *slot = grid;
            }
        }
        Ok(fields)
    }

    /// Assigns the instances under `node` to the lines' labels at `depth` and
    /// returns every value a line writes back, recording each element no line
    /// reaches. Each value carries the path of the instance at depth 0 that
    /// holds it, which keys the repetition it is written into.
    #[expect(
        clippy::too_many_arguments,
        reason = "one level of the walk; a struct for it would only rename the parameters"
    )]
    pub(super) fn assign<'l>(
        &mut self,
        lines: &[&'l Line<'c>],
        depth: usize,
        node: &Value,
        path: &str,
        instance: Option<&str>,
        capacity: &dyn Fn(&Line<'c>) -> usize,
        root: bool,
    ) -> Vec<Leaf<'l, 'c>> {
        let mut leaves = Vec::new();
        for line in lines.iter().filter(|line| line.steps.len() == depth) {
            if let Some(literal) = line.literal {
                if node.as_str() == Some(literal) {
                    self.consumed.insert(String::from(path));
                }
            } else {
                leaves.push((
                    *line,
                    node.clone(),
                    String::from(path),
                    String::from(instance.unwrap_or(path)),
                ));
            }
        }
        let mut slots: Vec<Slot<'l, 'c>> = Vec::new();
        for line in lines
            .iter()
            .copied()
            .filter(|line| line.steps.len() > depth)
        {
            let Some(step) = line.steps.get(depth) else {
                continue;
            };
            match slots
                .iter_mut()
                .find(|(name, label, _)| *name == step.name && *label == step.label)
            {
                Some((_, _, members)) => members.push(line),
                None => slots.push((step.name.clone(), step.label.clone(), vec![line])),
            }
        }
        let Value::Object(object) = node else {
            return leaves;
        };
        for (key, member) in object {
            if root && matches!(key.as_str(), "resourceType" | "id" | "meta") {
                continue;
            }
            // NOTE: R4 JSON §Primitive Types: a primitive's extensions sit under `_name`, so a row
            // that steps through `name` into its `extension` reads them there.
            let (name, extended) = match key.strip_prefix('_') {
                Some(primitive) => (primitive, true),
                None => (key.as_str(), false),
            };
            let named: Vec<usize> = slots
                .iter()
                .enumerate()
                .filter(|(_, (slot, ..))| slot == name)
                .map(|(index, _)| index)
                .collect();
            let (items, repeated): (Vec<&Value>, bool) = match member {
                Value::Array(items) => (items.iter().collect(), true),
                other => (vec![other], false),
            };
            let mut taken = vec![0usize; slots.len()];
            for (index, item) in items.into_iter().enumerate() {
                let here = if repeated {
                    format!("{path}{}{key}[{index}]", dot(path))
                } else {
                    format!("{path}{}{key}", dot(path))
                };
                let Some(slot) = choose(&slots, &named, &taken, (index, item), depth, capacity)
                else {
                    continue;
                };
                if let Some(count) = taken.get_mut(slot) {
                    *count = count.saturating_add(1);
                }
                let Some((_, _, members)) = slots.get(slot) else {
                    continue;
                };
                let below: Vec<&Line<'c>>;
                let members = if extended {
                    below = members
                        .iter()
                        .copied()
                        .filter(|line| line.steps.len() > depth.saturating_add(1))
                        .collect();
                    &below
                } else {
                    members
                };
                let held = instance.unwrap_or(&here).to_owned();
                let found = self.assign(
                    members,
                    depth.saturating_add(1),
                    item,
                    &here,
                    Some(&held),
                    capacity,
                    false,
                );
                leaves.extend(found);
            }
        }
        leaves
    }

    /// Writes one value back at `inner`, the positions below the field or
    /// component the line names, into `grid`.
    pub(super) fn write<'a>(
        &'a mut self,
        line: &'a Line<'c>,
        value: &'a Value,
        path: &'a str,
        inner: &'a [usize],
        grid: &'a mut Grid,
    ) -> Pin<Box<dyn Future<Output = Result<(), InverseError>> + 'a>> {
        Box::pin(async move {
            let source_type = line.source_type.clone().unwrap_or_default();
            if let Some(map) = self.datatype_map(line, &source_type) {
                let built = self.datatype(map, &source_type, value, path).await?;
                grid.nest(inner, built);
                return Ok(());
            }
            let Some(text) = text(value) else {
                return Ok(());
            };
            let text = match &line.row.mapped_via {
                Some(MappedVia::Table(id)) => {
                    let Some(map) = self.corpus.get(id).filter(|map| map.kind == Kind::Table)
                    else {
                        return Ok(());
                    };
                    let translated = self.translate(&text, map, path).await?;
                    self.consumed.insert(String::from(path));
                    match translated {
                        Some(code) => code,
                        None => return Ok(()),
                    }
                }
                _ if ["DTM", "DT", "TS", "TM"].contains(&source_type.as_str()) => {
                    match convert::temporal(&source_type, &text) {
                        Ok(converted) => converted,
                        Err(error) => {
                            self.consumed.insert(String::from(path));
                            self.outcomes.push(Outcome::Unconvertible {
                                path: String::from(path),
                                error,
                            });
                            return Ok(());
                        }
                    }
                }
                _ => text,
            };
            grid.set(inner, text);
            self.consumed.insert(String::from(path));
            Ok(())
        })
    }

    /// The data type map a line's value runs back through: the one its
    /// `mappedVia` names, else the one from its source type to its target
    /// type, when the source type has components.
    pub(super) fn datatype_map(&self, line: &Line<'c>, source_type: &str) -> Option<&'c Map> {
        if let Some(MappedVia::Table(id)) = &line.row.mapped_via {
            return self.corpus.get(id).filter(|map| map.kind == Kind::Datatype);
        }
        let complex = hl7v2_types::data_type::find(source_type)
            .is_some_and(|data_type| !data_type.components.is_empty());
        let target_type = line.row.target_type.as_deref()?;
        if !complex {
            return None;
        }
        self.corpus.find("datatype", source_type, target_type).ok()
    }

    /// Runs `map` back over `value`, a FHIR value of its target type, into
    /// the components of `source_type`.
    pub(super) async fn datatype(
        &mut self,
        map: &'c Map,
        source_type: &str,
        value: &Value,
        path: &str,
    ) -> Result<Grid, InverseError> {
        let components = hl7v2_types::data_type::find(source_type)
            .map(|data_type| data_type.components)
            .unwrap_or_default();
        let lines = lines(map, &|position| {
            components.get(position.checked_sub(1)?).map(|component| {
                (
                    deprecated(component.optionality),
                    component.data_type.map(|data| data.code),
                )
            })
        });
        let refs: Vec<&Line<'c>> = lines.iter().collect();
        let leaves = self.assign(&refs, 0, value, path, None, &|_| 1, false);
        let mut grid = Grid::default();
        for (line, leaf, leaf_path, _) in order(leaves) {
            if matches!(line.gate, Gate::Computable(_)) && self.consumed.contains(&leaf_path) {
                continue;
            }
            let mark = self.mark();
            let mut built = grid.clone();
            self.write(line, &leaf, &leaf_path, &line.source, &mut built)
                .await?;
            if let Gate::Computable(expr) = line.gate {
                let holds = condition::evaluate(expr, &mut |operand| match operand {
                    Operand::Component { path, .. } => Some(built.probe(path)),
                    _ => None,
                });
                if !matches!(holds, Ok(true)) {
                    self.rollback(mark);
                    continue;
                }
            }
            grid = built;
        }
        Ok(grid)
    }

    /// Translates `code`, a value of a target system of the guide's table map
    /// `map`, back to its v2 code.
    pub(super) async fn translate(
        &mut self,
        code: &str,
        map: &Map,
        path: &str,
    ) -> Result<Option<String>, InverseError> {
        let Some(translator) = self.translator else {
            self.outcomes.push(Outcome::NoTranslator {
                path: String::from(path),
                concept_map: map.url.clone(),
            });
            return Ok(None);
        };
        for group in map.groups.iter().filter(|group| !group.target.is_empty()) {
            let preferred = self.corpus.maps().find(|other| {
                other.kind == Kind::Table
                    && other
                        .groups
                        .iter()
                        .any(|back| back.source == group.target && back.target == group.source)
            });
            let translation = match preferred {
                Some(back) => Translation::new(group.target.as_str(), code)
                    .with_concept_map(back.url.as_str())
                    .with_target_system(group.source.as_str()),
                None => Translation::new(group.target.as_str(), code)
                    .with_concept_map(map.url.as_str())
                    .with_target_system(group.source.as_str())
                    .with_reverse(true),
            };
            let asked = preferred.map_or(map.url.as_str(), |back| back.url.as_str());
            self.translations = self.translations.saturating_add(1);
            let answer = translator.translate(&translation).await.map_err(|source| {
                InverseError::Terminology {
                    concept_map: String::from(asked),
                    source: Box::new(source),
                }
            })?;
            let mut codes: Vec<String> = Vec::new();
            for found in answer.accepted() {
                if let Some(code) = found
                    .concept
                    .as_ref()
                    .and_then(|concept| concept.code.clone())
                    && !codes.contains(&code)
                {
                    codes.push(code);
                }
            }
            match codes.len() {
                0 => {}
                1 => return Ok(codes.pop()),
                _ => {
                    self.outcomes.push(Outcome::Ambiguous {
                        path: String::from(path),
                        concept_map: String::from(asked),
                        codes,
                    });
                    return Ok(None);
                }
            }
        }
        self.outcomes.push(Outcome::Untranslated {
            path: String::from(path),
            concept_map: map.url.clone(),
        });
        Ok(None)
    }
}
