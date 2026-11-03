// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic example composition generated from a template's Web
//! Template, so a mapping can be tried without hand-written input.
//!
//! The generator walks the Web Template and fills each leaf input with a
//! value its constraint admits: the first entry of a coded list, the lower
//! bound of a range, a fixed synthetic text, date or time. The result is
//! built and validated through [`WebTemplateIndex::build_from_flat`], so an
//! example is one the CDR's own validation admits. A template whose every
//! leaf cannot be filled together is tried again with its mandatory leaves
//! alone. Every value is synthetic. No specification governs the generator:
//! our own design.

use openehr_sdt::flat::path::{FlatKey, Segment, Suffix};
use openehr_sdt::flat::webtemplate::model::{
    WebTemplateInput, WebTemplateInputType, WebTemplateNode, WebTemplateRange,
};
use serde_json::{Map, Value};

use crate::composition::CanonicalComposition;
use crate::index::WebTemplateIndex;
use crate::template::PathError;

/// The text every free-text leaf of an example holds.
const TEXT: &str = "Synthetic example text";

/// The instant every date and time leaf of an example names.
const DATE_TIME: &str = "2026-01-01T09:00:00Z";

/// Which leaves an example fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// Every leaf the template holds.
    Every,
    /// The leaves every node on whose way is mandatory.
    Mandatory,
}

impl WebTemplateIndex {
    /// Builds a synthetic example composition of this template.
    ///
    /// `now` is the caller's current timestamp, which the builder uses for
    /// the `ctx/time` default alone; this crate reads no clock.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::NotComposition`] for a template whose root is no
    /// `COMPOSITION`, and the errors of [`Self::build_from_flat`] when no
    /// example of either reach builds.
    pub fn example_composition(&self, now: &str) -> Result<CanonicalComposition, PathError> {
        let tree = &self.web_template().tree;
        if tree.rm_type != "COMPOSITION" {
            return Err(PathError::NotComposition {
                template_id: self.template_id().to_owned(),
            });
        }
        match self.build_from_flat(&self.example_flat(Reach::Every), now) {
            Ok(composition) => Ok(composition),
            Err(every) => self
                .build_from_flat(&self.example_flat(Reach::Mandatory), now)
                .map_err(|_mandatory| every),
        }
    }

    /// Returns the FLAT document of an example of `reach`.
    fn example_flat(&self, reach: Reach) -> Map<String, Value> {
        let template = self.web_template();
        let mut flat = Map::new();
        for (field, value) in [
            ("language", template.default_language.as_str()),
            ("territory", "NL"),
            ("composer_name", "Synthetic Composer"),
        ] {
            let key = FlatKey {
                segments: vec![segment("ctx", None), segment(field, None)],
                suffixes: Vec::new(),
            };
            flat.insert(key.to_string(), Value::String(value.to_owned()));
        }
        let mut path = Vec::new();
        fill(&template.tree, reach, &mut path, &mut flat);
        flat
    }
}

/// Returns a FLAT segment.
fn segment(name: &str, index: Option<u32>) -> Segment {
    Segment {
        name: name.to_owned(),
        index,
    }
}

/// Fills the leaves under `node`, whose FLAT segments up to its parent are
/// `path`, into `flat`.
fn fill(
    node: &WebTemplateNode,
    reach: Reach,
    path: &mut Vec<Segment>,
    flat: &mut Map<String, Value>,
) {
    if node.in_context == Some(true) {
        return;
    }
    if reach == Reach::Mandatory && node.min.unwrap_or(0) < 1 {
        return;
    }
    // NOTE: Simplified Formats, §Instance Indexing: a node that may repeat
    // takes a 0-based index, and the first instance is `:0`.
    let index = (node.max != 1).then_some(0);
    path.push(segment(&node.id, index));
    if node.inputs.is_empty() {
        let mut chosen = std::collections::BTreeMap::new();
        let mut filled = std::collections::BTreeSet::new();
        for child in &node.children {
            // NOTE: no specification governs this: our own design; the
            // alternatives of one polymorphic ELEMENT share its path, and one
            // value is filled.
            if !filled.insert(child.aql_path.as_str()) {
                continue;
            }
            if let Some((attribute, object)) = attribute_of(node, child)
                && !MULTIPLE.contains(&attribute)
                && *chosen.entry(attribute).or_insert(object) != object
            {
                continue;
            }
            fill(child, reach, path, flat);
        }
    } else {
        for (suffix, value) in leaf(node) {
            let key = FlatKey {
                segments: path.clone(),
                suffixes: suffix
                    .map(|name| Suffix { name, index: None })
                    .into_iter()
                    .collect(),
            };
            flat.insert(key.to_string(), value);
        }
    }
    path.pop();
}

/// The reference-model attributes that hold several objects; every other
/// attribute holds one, so of two alternatives under it an example fills the
/// first (openEHR RM, `COMPOSITION.content`, `SECTION.items`, `ITEM_TREE.items`,
/// `CLUSTER.items`, `HISTORY.events`, `INSTRUCTION.activities`).
const MULTIPLE: [&str; 4] = ["content", "items", "events", "activities"];

/// Returns the attribute `child` sits under in `parent` and the object it
/// names there, from the first step of its path below its parent's.
fn attribute_of<'a>(
    parent: &WebTemplateNode,
    child: &'a WebTemplateNode,
) -> Option<(&'a str, &'a str)> {
    let below = child.aql_path.strip_prefix(&parent.aql_path)?;
    let step = below.strip_prefix('/')?.split('/').next()?;
    let (attribute, rest) = step.split_once('[')?;
    Some((attribute, rest.trim_end_matches(']')))
}

/// Returns the values of the leaf `node`, each by the suffix it fills.
fn leaf(node: &WebTemplateNode) -> Vec<(Option<String>, Value)> {
    let unit = node
        .inputs
        .iter()
        .find(|input| input.suffix.as_deref() == Some("unit"))
        .and_then(|input| input.list.first());
    let mut values = Vec::new();
    for input in &node.inputs {
        // NOTE: Simplified Formats master04 §Open Value-Sets: `|other` is
        // exclusive with the coded value, which the example fills instead.
        if input.suffix.as_deref() == Some("other") {
            continue;
        }
        let range = match (input.suffix.as_deref(), unit) {
            (Some("magnitude"), Some(unit)) => unit
                .validation
                .as_ref()
                .and_then(|validation| validation.range.as_ref()),
            _ => input
                .validation
                .as_ref()
                .and_then(|validation| validation.range.as_ref()),
        };
        values.push((input.suffix.clone(), value(node, input, range)));
    }
    values
}

/// Returns the value of `input` of the leaf `node` within `range`.
fn value(
    node: &WebTemplateNode,
    input: &WebTemplateInput,
    range: Option<&WebTemplateRange>,
) -> Value {
    if let Some(first) = input.list.first() {
        return Value::String(first.value.clone());
    }
    let suffix = input.suffix.as_deref();
    match input.input_type {
        WebTemplateInputType::Text | WebTemplateInputType::CodedText => {
            Value::String(text(&node.rm_type, suffix).to_owned())
        }
        WebTemplateInputType::Date => Value::String(String::from("2026-01-01")),
        WebTemplateInputType::Time => Value::String(String::from("09:00:00")),
        WebTemplateInputType::Datetime => Value::String(String::from(DATE_TIME)),
        WebTemplateInputType::Duration => Value::String(String::from("PT1H")),
        WebTemplateInputType::Boolean => Value::Bool(true),
        WebTemplateInputType::Integer | WebTemplateInputType::Count => {
            Value::from(number(range, 1.0).round())
        }
        WebTemplateInputType::Decimal | WebTemplateInputType::Quantity => {
            Value::from(number(range, 1.0))
        }
        WebTemplateInputType::Proportion => Value::from(number(range, 1.0)),
    }
}

/// Returns the synthetic text of a leaf of `rm_type` at `suffix`.
fn text(rm_type: &str, suffix: Option<&str>) -> &'static str {
    match (rm_type, suffix) {
        ("DV_URI", _) => "https://example.org/synthetic",
        ("DV_EHR_URI", _) => "ehr://synthetic",
        (_, Some("code")) => "synthetic",
        (_, Some("terminology")) => "local",
        _ => TEXT,
    }
}

/// Returns a number `range` admits, `fallback` when it bounds nothing: its
/// lower bound, one above it when the bound is exclusive, and its upper
/// bound when that is lower.
fn number(range: Option<&WebTemplateRange>, fallback: f64) -> f64 {
    let Some(range) = range else {
        return fallback;
    };
    let lower = range.min.as_ref().and_then(Value::as_f64);
    let upper = range.max.as_ref().and_then(Value::as_f64);
    let candidate = match lower {
        Some(lower) if range.min_op.as_deref() == Some(">") => lower + 1.0,
        Some(lower) => lower,
        None => fallback,
    };
    match upper {
        Some(upper) if candidate > upper => upper,
        _ => candidate,
    }
}
