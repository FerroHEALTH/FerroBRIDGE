// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Runs a map: the first group of the map over a source instance into a new
//! target, each rule applied once per combination of its sources' selected
//! occurrences (FHIR R4 §7.7.0.7).
//!
//! Variables hold either a source value or a place in a target tree, so a
//! later statement writes into what an earlier one created. Groups resolve
//! by name across every map the engine holds (§7.7.0.7.3); the simple form
//! `src.a -> tgt.a` copies a primitive and runs the default group of the
//! type pair for a complex value (§7.7.0.8, §7.7.0.9).

mod transform;
pub mod translate;

use std::collections::BTreeMap;
use std::sync::Arc;

use fhir_types::codec::{Object, Value};

use self::translate::{Translate, Upstream};
use super::expression::{ExpressionError, evaluate, holds};
use super::model::{Known, Structures};
use super::syntax::{
    Dependent, Group, InputMode, Rule, Source, SourceListMode, StructureMap, Target,
    TargetListMode, Transform, TypeMode,
};

/// How deep groups and dependent rules may nest before a map is refused as
/// recursive. No specification bounds it: our own design.
const DEPTH: usize = 64;

/// Why a map failed, naming map items and never a value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MapError {
    /// No map has the URL.
    #[error("no map has the URL {0}")]
    NoMap(String),
    /// No group has the name, or more than one.
    #[error("no single group is named {0}")]
    NoGroup(String),
    /// A group is invoked with the wrong number of variables.
    #[error("the group {0} is invoked with the wrong number of variables")]
    Arity(String),
    /// A statement names a variable no scope declares.
    #[error("no variable is named {0}")]
    UnknownVariable(String),
    /// A statement writes into a variable that holds no target.
    #[error("the variable {0} holds no target")]
    NotTarget(String),
    /// A target type has no such element.
    #[error("the type {type_name} has no element {element}")]
    Element {
        /// The type.
        type_name: String,
        /// The element.
        element: String,
    },
    /// A selection breaks its declared cardinality.
    #[error("a source of the rule {0} breaks its cardinality")]
    Cardinality(String),
    /// A `check` condition does not hold.
    #[error("a check of the rule {0} does not hold")]
    Check(String),
    /// An `only_one` source selected more than one occurrence.
    #[error("an only_one source of the rule {0} selected more than one occurrence")]
    OnlyOne(String),
    /// An embedded expression failed.
    #[error(transparent)]
    Expression(#[from] ExpressionError),
    /// A transform is used against its definition.
    #[error("the transform {name} {reason}")]
    Transform {
        /// The transform.
        name: String,
        /// What is wrong.
        reason: &'static str,
    },
    /// No default group maps the type pair.
    #[error("no default group maps {source_type} to {target_type}")]
    NoDefaultGroup {
        /// The source type.
        source_type: String,
        /// The target type.
        target_type: String,
    },
    /// Groups or rules nest deeper than the engine follows.
    #[error("the map nests deeper than {DEPTH} levels")]
    Depth,
    /// The terminology server refused a `translate`, or the call reached no
    /// answer.
    #[error("the translate through {map} failed")]
    Translate {
        /// The concept map URL.
        map: String,
        /// The terminology client's failure.
        #[source]
        source: Upstream,
    },
    /// The terminology server found no translation.
    #[error("the translate through {map} found no translation")]
    NoTranslation {
        /// The concept map URL.
        map: String,
        /// The server's `message`, when it sent one.
        message: Option<String>,
    },
    /// The accepted matches of a `translate` name more than one concept.
    #[error("the translate through {0} found more than one translation")]
    AmbiguousTranslation(String),
    /// The runtime shut down before a blocking run finished.
    #[error("the runtime shut down before the map finished")]
    Interrupted,
}

/// The tree a place lies in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Root {
    /// The target instance.
    Target,
    /// The created instance at this index.
    Produced(usize),
}

/// One step into a JSON tree.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    /// An object member.
    Member(String),
    /// An array item.
    Index(usize),
}

/// A place in a target tree, with the type of what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Place {
    root: Root,
    steps: Vec<Step>,
    type_name: Option<String>,
}

/// What a variable holds.
#[derive(Debug, Clone, PartialEq)]
enum Bound {
    /// A source value, with its type when known.
    Source(Value, Option<String>),
    /// A place in a target tree.
    Target(Place),
}

/// The variables in force, innermost last.
#[derive(Debug, Clone, Default)]
struct Scope(Vec<(String, Bound)>);

impl Scope {
    /// Returns the variable `name`.
    fn get(&self, name: &str) -> Option<&Bound> {
        self.0
            .iter()
            .rev()
            .find(|(bound, _)| bound == name)
            .map(|(_, value)| value)
    }

    /// Returns the scope with `name` bound to `value`.
    fn with(&self, name: &str, value: Bound) -> Self {
        let mut next = self.clone();
        next.0.push((name.to_owned(), value));
        next
    }
}

/// What a map produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    /// The target instance.
    pub target: Value,
    /// The instances `create` made outside the target.
    pub produced: Vec<Value>,
}

/// The maps an engine runs and the structures they read and write.
#[derive(Debug, Clone)]
pub struct Engine {
    maps: Vec<StructureMap>,
    structures: Known,
    translator: Option<Arc<dyn Translate>>,
}

impl Engine {
    /// Returns the engine over `maps`, reading types from `structures`.
    #[must_use]
    pub const fn new(maps: Vec<StructureMap>, structures: Known) -> Self {
        Self {
            maps,
            structures,
            translator: None,
        }
    }

    /// Returns the engine running `translate` through `translator`; without
    /// one, `translate` is refused.
    #[must_use]
    pub fn with_translator(mut self, translator: Arc<dyn Translate>) -> Self {
        self.translator = Some(translator);
        self
    }

    /// Returns the map whose canonical URL is `url`.
    #[must_use]
    pub fn map(&self, url: &str) -> Option<&StructureMap> {
        self.maps.iter().find(|map| map.url == url)
    }

    /// Runs [`Engine::transform`] on a blocking thread, where the translator
    /// may wait on the terminology server.
    ///
    /// # Errors
    /// Returns [`MapError`] as [`Engine::transform`] does, and
    /// [`MapError::Interrupted`] when the runtime shuts down first.
    ///
    /// # Panics
    /// Resumes a panic of the run on the calling task.
    pub async fn transform_blocking(
        self: Arc<Self>,
        url: String,
        sources: Vec<Value>,
        target_type: String,
    ) -> Result<Output, MapError> {
        match tokio::task::spawn_blocking(move || self.transform(&url, &sources, &target_type))
            .await
        {
            Ok(result) => result,
            Err(error) => match error.try_into_panic() {
                Ok(payload) => std::panic::resume_unwind(payload),
                Err(_cancelled) => Err(MapError::Interrupted),
            },
        }
    }

    /// Runs the first group of the map `url` with `sources` bound to its
    /// source parameters in order and a new instance of `target_type` as its
    /// target.
    ///
    /// # Errors
    /// Returns [`MapError`] when the map is unknown or a rule fails.
    pub fn transform(
        &self,
        url: &str,
        sources: &[Value],
        target_type: &str,
    ) -> Result<Output, MapError> {
        let map = self
            .maps
            .iter()
            .find(|map| map.url == url)
            .ok_or_else(|| MapError::NoMap(url.to_owned()))?;
        let group = map
            .groups
            .first()
            .ok_or_else(|| MapError::NoGroup(map.name.clone()))?;
        let mut run = Run {
            engine: self,
            target: self.instance(target_type),
            produced: Vec::new(),
        };
        let mut arguments = Vec::new();
        let mut given = sources.iter();
        for parameter in &group.parameters {
            arguments.push(match parameter.mode {
                InputMode::Source => {
                    let source = given
                        .next()
                        .ok_or_else(|| MapError::Arity(group.name.clone()))?;
                    Bound::Source(source.clone(), parameter.type_name.clone())
                }
                InputMode::Target => Bound::Target(Place {
                    root: Root::Target,
                    steps: Vec::new(),
                    type_name: Some(target_type.to_owned()),
                }),
            });
        }
        if given.next().is_some() {
            return Err(MapError::Arity(group.name.clone()));
        }
        run.group(group, arguments, 0)?;
        Ok(Output {
            target: run.target,
            produced: run.produced,
        })
    }

    /// Returns a new instance of `type_name`: a resource carries its type.
    fn instance(&self, type_name: &str) -> Value {
        let mut object = Object::new();
        if self.structures.is_resource(type_name) {
            object.insert(
                String::from("resourceType"),
                Value::String(type_name.to_owned()),
            );
        }
        Value::Object(object)
    }

    /// Returns the one group named `name` across every map.
    fn group_named(&self, name: &str) -> Result<&Group, MapError> {
        let mut found = self
            .maps
            .iter()
            .flat_map(|map| &map.groups)
            .filter(|group| group.name == name);
        match (found.next(), found.next()) {
            (Some(group), None) => Ok(group),
            _ => Err(MapError::NoGroup(name.to_owned())),
        }
    }

    /// Returns the default group mapping `source_type` to `target_type`.
    fn default_group(&self, source_type: &str, target_type: &str) -> Option<&Group> {
        self.maps.iter().flat_map(|map| &map.groups).find(|group| {
            group.type_mode.is_some()
                && matches!(group.parameters.as_slice(), [source, target]
                        if source.type_name.as_deref() == Some(source_type)
                            && (target.type_name.as_deref() == Some(target_type)
                                || group.type_mode == Some(TypeMode::TypePlus)))
        })
    }
}

/// One run of a map.
struct Run<'e> {
    engine: &'e Engine,
    target: Value,
    produced: Vec<Value>,
}

/// One selected source occurrence: the variable it binds and its value.
type Selected = (Option<String>, Value, Option<String>);

impl Run<'_> {
    /// Runs `group` with `arguments` bound to its parameters in order.
    fn group(
        &mut self,
        group: &Group,
        arguments: Vec<Bound>,
        depth: usize,
    ) -> Result<(), MapError> {
        if depth > DEPTH {
            return Err(MapError::Depth);
        }
        if arguments.len() != group.parameters.len() {
            return Err(MapError::Arity(group.name.clone()));
        }
        let mut scope = Scope::default();
        for (parameter, argument) in group.parameters.iter().zip(arguments) {
            scope = scope.with(&parameter.name, argument);
        }
        if let Some(extended) = &group.extends {
            let base = self.engine.group_named(extended)?;
            let arguments = base
                .parameters
                .iter()
                .map(|parameter| {
                    scope
                        .get(&parameter.name)
                        .cloned()
                        .ok_or_else(|| MapError::UnknownVariable(parameter.name.clone()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.group(base, arguments, depth + 1)?;
        }
        for rule in &group.rules {
            self.rule(rule, &scope, depth)?;
        }
        Ok(())
    }

    /// Returns the current value of what `bound` holds and its type.
    fn value_of(&self, bound: &Bound) -> (Value, Option<String>) {
        match bound {
            Bound::Source(value, type_name) => (value.clone(), type_name.clone()),
            Bound::Target(place) => (
                self.read(place).cloned().unwrap_or(Value::Null),
                place.type_name.clone(),
            ),
        }
    }

    /// Returns the value at `place`.
    fn read(&self, place: &Place) -> Option<&Value> {
        let mut node = match place.root {
            Root::Target => &self.target,
            Root::Produced(index) => self.produced.get(index)?,
        };
        for step in &place.steps {
            node = match step {
                Step::Member(name) => node.get(name)?,
                Step::Index(index) => node.as_array()?.get(*index)?,
            };
        }
        Some(node)
    }

    /// Returns the variables of `scope` as name and current value.
    fn variables(&self, scope: &Scope) -> Vec<(String, Value)> {
        let mut seen = BTreeMap::new();
        for (name, bound) in scope.0.iter().rev() {
            if !seen.contains_key(name) {
                seen.insert(name.clone(), self.value_of(bound).0);
            }
        }
        seen.into_iter().collect()
    }

    /// Evaluates `text` on `focus` in `scope`.
    fn evaluate(
        &self,
        text: &str,
        focus: Option<&Value>,
        scope: &Scope,
    ) -> Result<Vec<Value>, MapError> {
        let variables = self.variables(scope);
        let borrowed: Vec<(&str, &Value)> = variables
            .iter()
            .map(|(name, value)| (name.as_str(), value))
            .collect();
        Ok(evaluate(text, focus, &borrowed)?)
    }

    /// Applies `rule` in `scope`.
    fn rule(&mut self, rule: &Rule, scope: &Scope, depth: usize) -> Result<(), MapError> {
        if depth > DEPTH {
            return Err(MapError::Depth);
        }
        let name = rule.name.clone().unwrap_or_default();
        let mut selections = Vec::new();
        for source in &rule.sources {
            selections.push(self.select(source, scope, &name)?);
        }
        let combinations = product(&selections);
        let simple = simple_form(rule);
        let mut shared: BTreeMap<usize, Place> = BTreeMap::new();
        let count = combinations.len();
        for (position, combination) in combinations.into_iter().enumerate() {
            let mut inner = scope.clone();
            for (variable, value, type_name) in &combination {
                if let Some(variable) = variable {
                    inner = inner.with(variable, Bound::Source(value.clone(), type_name.clone()));
                }
            }
            if let (true, Some((_, value, type_name))) = (simple, combination.first()) {
                self.simple(rule, &inner, value, type_name.as_deref(), depth)?;
                continue;
            }
            for (index, target) in rule.targets.iter().enumerate() {
                let applies = match target.list_mode {
                    Some(TargetListMode::First) => position == 0,
                    Some(TargetListMode::Last) => position + 1 == count,
                    _ => true,
                };
                if !applies {
                    continue;
                }
                let sharing = matches!(
                    target.list_mode,
                    Some(TargetListMode::Share | TargetListMode::Collate)
                );
                if let (true, Some(place), Some(variable)) =
                    (sharing, shared.get(&index), target.variable.as_ref())
                {
                    inner = inner.with(variable, Bound::Target(place.clone()));
                    continue;
                }
                let bound = self.target(target, &inner)?;
                if let (true, Some(Bound::Target(place))) = (sharing, &bound) {
                    shared.insert(index, place.clone());
                }
                if let (Some(variable), Some(bound)) = (&target.variable, bound) {
                    inner = inner.with(variable, bound);
                }
            }
            match &rule.dependent {
                Some(Dependent::Rules(rules)) => {
                    for nested in rules {
                        self.rule(nested, &inner, depth + 1)?;
                    }
                }
                Some(Dependent::Invocation(invocation)) => {
                    let group = self.engine.group_named(&invocation.name)?;
                    let arguments = invocation
                        .params
                        .iter()
                        .map(|param| match param {
                            super::syntax::Param::Variable(variable) => inner
                                .get(variable)
                                .cloned()
                                .ok_or_else(|| MapError::UnknownVariable(variable.clone())),
                            super::syntax::Param::Literal(literal) => {
                                Ok(Bound::Source(transform::literal(literal), None))
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    self.group(group, arguments, depth + 1)?;
                }
                None => {}
            }
        }
        Ok(())
    }

    /// Selects the occurrences `source` reads in `scope`.
    fn select(
        &self,
        source: &Source,
        scope: &Scope,
        rule: &str,
    ) -> Result<Vec<Selected>, MapError> {
        let bound = scope
            .get(&source.context)
            .ok_or_else(|| MapError::UnknownVariable(source.context.clone()))?;
        let (value, type_name) = self.value_of(bound);
        // NOTE: R4 §7.7.0.7.1; `where` and `check` are evaluated on the context, the variable read.
        let context = value.clone();
        let mut items: Vec<(Value, Option<String>)> = match &source.element {
            None => vec![(value, type_name)],
            Some(element) => {
                let child = type_name
                    .as_deref()
                    .and_then(|ty| self.engine.structures.element(ty, element))
                    .and_then(|definition| definition.type_name);
                match value.get(element) {
                    None | Some(Value::Null) => Vec::new(),
                    Some(Value::Array(values)) => values
                        .iter()
                        .map(|value| (value.clone(), child.clone()))
                        .collect(),
                    Some(value) => vec![(value.clone(), child)],
                }
            }
        };
        if let Some(wanted) = &source.type_name {
            items.retain(|(value, type_name)| of_type(value, type_name.as_deref(), wanted));
        }
        if let Some((min, max)) = source.cardinality {
            let count = u32::try_from(items.len()).unwrap_or(u32::MAX);
            if count < min || max.is_some_and(|max| count > max) {
                return Err(MapError::Cardinality(rule.to_owned()));
            }
        }
        if items.is_empty()
            && let Some(default) = &source.default
        {
            items = self
                .evaluate(&default.0, None, scope)?
                .into_iter()
                .take(1)
                .map(|value| (value, source.type_name.clone()))
                .collect();
        }
        let length = items.len();
        items = match source.list_mode {
            Some(SourceListMode::First) => items.into_iter().take(1).collect(),
            Some(SourceListMode::NotFirst) => items.into_iter().skip(1).collect(),
            Some(SourceListMode::Last) => {
                items.into_iter().skip(length.saturating_sub(1)).collect()
            }
            Some(SourceListMode::NotLast) => {
                items.into_iter().take(length.saturating_sub(1)).collect()
            }
            Some(SourceListMode::OnlyOne) if length > 1 => {
                return Err(MapError::OnlyOne(rule.to_owned()));
            }
            Some(SourceListMode::OnlyOne) | None => items,
        };
        let mut selected = Vec::new();
        for (value, type_name) in items {
            let scoped = match &source.variable {
                Some(variable) => {
                    scope.with(variable, Bound::Source(value.clone(), type_name.clone()))
                }
                None => scope.clone(),
            };
            if let Some(condition) = &source.condition
                && !holds(&self.evaluate(&condition.0, Some(&context), &scoped)?)
            {
                continue;
            }
            if let Some(check) = &source.check
                && !holds(&self.evaluate(&check.0, Some(&context), &scoped)?)
            {
                return Err(MapError::Check(rule.to_owned()));
            }
            selected.push((source.variable.clone(), value, type_name));
        }
        Ok(selected)
    }

    /// Applies one target statement in `scope`; returns what its variable
    /// binds to.
    fn target(&mut self, target: &Target, scope: &Scope) -> Result<Option<Bound>, MapError> {
        let Some(context) = &target.context else {
            let Some(Transform::Invocation(invocation)) = &target.transform else {
                return Ok(None);
            };
            if invocation.name == "create" {
                let type_name = transform::type_parameter(invocation)?;
                self.produced.push(self.engine.instance(&type_name));
                return Ok(Some(Bound::Target(Place {
                    root: Root::Produced(self.produced.len() - 1),
                    steps: Vec::new(),
                    type_name: Some(type_name),
                })));
            }
            let value = transform::invoke(self, invocation, scope)?;
            return Ok(value.map(|value| Bound::Source(value, None)));
        };
        let Some(Bound::Target(place)) = scope.get(context).cloned() else {
            return Err(MapError::NotTarget(context.clone()));
        };
        let Some(element) = &target.element else {
            return Ok(Some(Bound::Target(place)));
        };
        let definition = place
            .type_name
            .as_deref()
            .map(|type_name| {
                self.engine
                    .structures
                    .element(type_name, element)
                    .ok_or_else(|| MapError::Element {
                        type_name: type_name.to_owned(),
                        element: element.clone(),
                    })
            })
            .transpose()?;
        let many = definition
            .as_ref()
            .is_some_and(|definition| definition.many);
        let element_type = definition.and_then(|definition| definition.type_name);
        let value = match &target.transform {
            None => Some(self.created(element_type.as_deref(), element)?),
            Some(Transform::Invocation(invocation)) if invocation.name == "create" => Some(
                self.engine
                    .instance(&transform::type_parameter(invocation)?),
            ),
            Some(transform) => transform::apply(self, transform, scope)?,
        };
        let Some(value) = value else {
            return Ok(None);
        };
        let written = self.write(&place, element, value, many, element_type)?;
        Ok(Some(Bound::Target(written)))
    }

    /// Returns a new value of the complex `type_name` for the auto-created
    /// `element` (§7.7.0.7.2: a primitive or open type is an error).
    fn created(&self, type_name: Option<&str>, element: &str) -> Result<Value, MapError> {
        match type_name {
            Some(type_name) if !self.engine.structures.is_primitive(type_name) => {
                Ok(self.engine.instance(type_name))
            }
            _ => Err(MapError::Transform {
                name: element.to_owned(),
                reason: "auto-creates a primitive or an element of no single type",
            }),
        }
    }

    /// Writes `value` as `element` of the object at `place`: appended when
    /// the element repeats, set otherwise.
    fn write(
        &mut self,
        place: &Place,
        element: &str,
        value: Value,
        many: bool,
        type_name: Option<String>,
    ) -> Result<Place, MapError> {
        let root = match place.root {
            Root::Target => &mut self.target,
            Root::Produced(index) => self
                .produced
                .get_mut(index)
                .ok_or_else(|| MapError::NotTarget(element.to_owned()))?,
        };
        let mut node = root;
        for step in &place.steps {
            node = match (step, node) {
                (Step::Member(name), Value::Object(object)) => object
                    .get_mut(name)
                    .ok_or_else(|| MapError::NotTarget(element.to_owned()))?,
                (Step::Index(index), Value::Array(items)) => items
                    .get_mut(*index)
                    .ok_or_else(|| MapError::NotTarget(element.to_owned()))?,
                _ => return Err(MapError::NotTarget(element.to_owned())),
            };
        }
        let Value::Object(object) = node else {
            return Err(MapError::NotTarget(element.to_owned()));
        };
        let mut steps = place.steps.clone();
        steps.push(Step::Member(element.to_owned()));
        if many {
            let entry = object
                .entry(element.to_owned())
                .or_insert_with(|| Value::Array(Vec::new()));
            let Value::Array(items) = entry else {
                return Err(MapError::NotTarget(element.to_owned()));
            };
            items.push(value);
            steps.push(Step::Index(items.len() - 1));
        } else {
            object.insert(element.to_owned(), value);
        }
        Ok(Place {
            root: place.root,
            steps,
            type_name,
        })
    }

    /// Applies the simple form `src.a -> tgt.a` to one occurrence.
    fn simple(
        &mut self,
        rule: &Rule,
        scope: &Scope,
        value: &Value,
        source_type: Option<&str>,
        depth: usize,
    ) -> Result<(), MapError> {
        let (Some(target), Some(_)) = (rule.targets.first(), rule.sources.first()) else {
            return Ok(());
        };
        let (Some(context), Some(element)) = (&target.context, &target.element) else {
            return Ok(());
        };
        let Some(Bound::Target(place)) = scope.get(context).cloned() else {
            return Err(MapError::NotTarget(context.clone()));
        };
        let definition = place
            .type_name
            .as_deref()
            .and_then(|type_name| self.engine.structures.element(type_name, element));
        let many = definition
            .as_ref()
            .is_some_and(|definition| definition.many);
        let target_type = definition.and_then(|definition| definition.type_name);
        if !matches!(value, Value::Object(_)) {
            self.write(&place, element, value.clone(), many, target_type)?;
            return Ok(());
        }
        let (Some(source_type), Some(target_type)) = (source_type, target_type) else {
            return Err(MapError::NoDefaultGroup {
                source_type: source_type.unwrap_or("(untyped)").to_owned(),
                target_type: String::from("(untyped)"),
            });
        };
        let group = self
            .engine
            .default_group(source_type, &target_type)
            .ok_or_else(|| MapError::NoDefaultGroup {
                source_type: source_type.to_owned(),
                target_type: target_type.clone(),
            })?;
        let created = self.engine.instance(&target_type);
        let written = self.write(&place, element, created, many, Some(target_type))?;
        self.group(
            group,
            vec![
                Bound::Source(value.clone(), Some(source_type.to_owned())),
                Bound::Target(written),
            ],
            depth + 1,
        )
    }
}

/// Returns whether `rule` is the simple form: one source element with no
/// variable, one target element with no transform or variable, and no
/// dependent rules (§7.7.0.8).
fn simple_form(rule: &Rule) -> bool {
    matches!((rule.sources.as_slice(), rule.targets.as_slice()), ([source], [target])
        if source.element.is_some()
            && source.variable.is_none()
            && target.element.is_some()
            && target.transform.is_none()
            && target.variable.is_none())
        && rule.dependent.is_none()
}

/// Returns whether `value` of the known `type_name` is of the type `wanted`.
fn of_type(value: &Value, type_name: Option<&str>, wanted: &str) -> bool {
    match type_name {
        Some(type_name) => type_name == wanted,
        None => match value {
            Value::String(_) => matches!(
                wanted,
                "string" | "code" | "uri" | "id" | "date" | "dateTime"
            ),
            Value::Bool(_) => wanted == "boolean",
            Value::Number(number) => {
                wanted == "decimal" || (number.as_i64().is_some() && wanted == "integer")
            }
            _ => false,
        },
    }
}

/// Returns every combination of one occurrence from each selection.
fn product(selections: &[Vec<Selected>]) -> Vec<Vec<Selected>> {
    selections
        .iter()
        .fold(vec![Vec::new()], |combinations, selection| {
            combinations
                .iter()
                .flat_map(|combination| {
                    selection.iter().map(move |item| {
                        let mut next = combination.clone();
                        next.push(item.clone());
                        next
                    })
                })
                .collect()
        })
}
