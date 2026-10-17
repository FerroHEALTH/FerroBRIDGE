// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Evaluating a parsed expression against a document.
//!
//! An [`Environment`] carries the type model, the caller's environment
//! variables and seams, and the clock; a [`Focus`] names the context node and
//! the resources `%resource` and `%rootResource` stand for (FHIR R4
//! §2.9.1.4). Evaluation returns a collection or an [`error::EvalError`],
//! two distinct outcomes: an empty collection is never an error and an error
//! is never an empty collection.

pub mod convert;
pub mod equality;
pub mod error;
mod functions;
mod navigate;
mod operators;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use fhir_types::codec::Value;

use crate::ast::{Expr, Literal};
use crate::model::{Model, named_type, path_type};
use crate::quantity::Quantity;
use crate::seam::{Definitions, Profiles, Resolver, Terminology};
use crate::value::node::{Doc, Node};
use crate::value::{Collection, Item};
use convert::UCUM;
use error::EvalError;

/// What an evaluation runs with: the type model, the environment variables
/// and the seams the caller supplies.
pub struct Environment<'a> {
    model: &'a dyn Model,
    variables: BTreeMap<String, Collection<'a>>,
    resolver: Option<&'a dyn Resolver>,
    terminology: Option<&'a dyn Terminology>,
    profiles: Option<&'a dyn Profiles>,
    definitions: Option<&'a dyn Definitions>,
    now: Option<jiff::Timestamp>,
}

impl fmt::Debug for Environment<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Environment")
            .field("model", &self.model)
            .field("variables", &self.variables.keys().collect::<Vec<_>>())
            .field("resolver", &self.resolver.is_some())
            .field("terminology", &self.terminology.is_some())
            .field("profiles", &self.profiles.is_some())
            .field("definitions", &self.definitions.is_some())
            .field("now", &self.now)
            .finish()
    }
}

impl<'a> Environment<'a> {
    /// An environment over `model` with no variables of its own and no seams.
    #[must_use]
    pub fn new(model: &'a dyn Model) -> Self {
        Self {
            model,
            variables: BTreeMap::new(),
            resolver: None,
            terminology: None,
            profiles: None,
            definitions: None,
            now: None,
        }
    }

    /// Defines the environment variable `%name` (N1 §9), which takes
    /// precedence over a variable of the same name N1 or FHIR R4 define.
    #[must_use]
    pub fn with_variable(mut self, name: impl Into<String>, value: Collection<'a>) -> Self {
        self.variables.insert(name.into(), value);
        self
    }

    /// Supplies the seam `resolve()` calls.
    #[must_use]
    pub fn with_resolver(mut self, resolver: &'a dyn Resolver) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Supplies the seam `memberOf()`, `subsumes()` and `subsumedBy()` call.
    #[must_use]
    pub fn with_terminology(mut self, terminology: &'a dyn Terminology) -> Self {
        self.terminology = Some(terminology);
        self
    }

    /// Supplies the seam `conformsTo()` calls.
    #[must_use]
    pub fn with_profiles(mut self, profiles: &'a dyn Profiles) -> Self {
        self.profiles = Some(profiles);
        self
    }

    /// Supplies the seam `elementDefinition()` and `slice()` call.
    #[must_use]
    pub fn with_definitions(mut self, definitions: &'a dyn Definitions) -> Self {
        self.definitions = Some(definitions);
        self
    }

    /// Fixes the instant `now()`, `today()` and `timeOfDay()` answer; without
    /// it they read the clock once per evaluation.
    #[must_use]
    pub const fn with_now(mut self, now: jiff::Timestamp) -> Self {
        self.now = Some(now);
        self
    }

    /// The type model.
    #[must_use]
    pub fn model(&self) -> &'a dyn Model {
        self.model
    }
}

/// The context an expression is evaluated on.
#[derive(Clone)]
enum Start<'a> {
    /// A resource.
    Resource(&'a Value),
    /// An element of the named type, with a primitive's `_name` object and
    /// the element path of its definition.
    Element {
        value: Option<&'a Value>,
        element: Option<&'a Value>,
        type_name: String,
        definition: Option<String>,
    },
    /// Items from an earlier evaluation.
    Items(Collection<'a>),
}

/// The context node and the resources `%resource` and `%rootResource` name.
///
/// Its `Debug` form names what kind of context it is and leaves the document
/// out.
#[derive(Clone)]
pub struct Focus<'a> {
    start: Start<'a>,
    resource: Option<&'a Value>,
    root: Option<&'a Value>,
}

impl fmt::Debug for Focus<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = match &self.start {
            Start::Resource(_) => String::from("resource"),
            Start::Element { type_name, .. } => format!("element of {type_name}"),
            Start::Items(items) => format!("{} items", items.len()),
        };
        f.debug_struct("Focus")
            .field("start", &start)
            .field("resource", &self.resource.is_some())
            .field("root", &self.root.is_some())
            .finish()
    }
}

impl<'a> Focus<'a> {
    /// A resource as the context, `%resource` and `%rootResource`.
    #[must_use]
    pub const fn resource(resource: &'a Value) -> Self {
        Self {
            start: Start::Resource(resource),
            resource: Some(resource),
            root: Some(resource),
        }
    }

    /// A contained resource as the context and `%resource`, its container as
    /// `%rootResource` (FHIR R4 §2.9.1.4).
    #[must_use]
    pub const fn contained(resource: &'a Value, container: &'a Value) -> Self {
        Self {
            start: Start::Resource(resource),
            resource: Some(resource),
            root: Some(container),
        }
    }

    /// An element as the context, typed by `type_name` (a type name or an
    /// element path such as `Patient.contact`), inside `resource`.
    ///
    /// A primitive element is its JSON value; [`Focus::primitive`] adds the
    /// `_name` object.
    #[must_use]
    pub fn element(value: &'a Value, type_name: impl Into<String>, resource: &'a Value) -> Self {
        Self {
            start: Start::Element {
                value: Some(value),
                element: None,
                type_name: type_name.into(),
                definition: None,
            },
            resource: Some(resource),
            root: Some(resource),
        }
    }

    /// A primitive element as the context: its JSON value, its `_name`
    /// object, or both, inside `resource`.
    #[must_use]
    pub fn primitive(
        value: Option<&'a Value>,
        element: Option<&'a Value>,
        type_name: impl Into<String>,
        resource: &'a Value,
    ) -> Self {
        Self {
            start: Start::Element {
                value,
                element,
                type_name: type_name.into(),
                definition: None,
            },
            resource: Some(resource),
            root: Some(resource),
        }
    }

    /// Names the element definition an element context is an occurrence
    /// of, by its element path (`RiskAssessment.prediction.probability[x]`);
    /// any other context is left as it was.
    ///
    /// On an occurrence of a choice element, a member another type of the
    /// choice defines navigates to an empty collection where the
    /// occurrence's own type lacks it (N1 §3.2, FHIR R4 §2.9.1.1).
    #[must_use]
    pub fn defined_by(mut self, path: impl Into<String>) -> Self {
        if let Start::Element { definition, .. } = &mut self.start {
            *definition = Some(path.into());
        }
        self
    }

    /// Items from an earlier evaluation as the context.
    #[must_use]
    pub const fn items(items: Collection<'a>, resource: Option<&'a Value>) -> Self {
        Self {
            start: Start::Items(items),
            resource,
            root: resource,
        }
    }

    /// Names the resources `%resource` and `%rootResource` stand for,
    /// replacing what the constructor set (FHIR R4 §2.9.1.4): an element of
    /// a contained resource has that resource as `%resource` and its
    /// container as `%rootResource`, and `None` leaves a variable empty.
    #[must_use]
    pub fn within(mut self, resource: Option<&'a Value>, root: Option<&'a Value>) -> Self {
        self.resource = resource;
        self.root = root;
        self
    }

    /// No context at all: every path is empty.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            start: Start::Items(Vec::new()),
            resource: None,
            root: None,
        }
    }
}

/// The state of one evaluation.
pub(crate) struct Engine<'e, 'a> {
    env: &'e Environment<'a>,
    context: Collection<'a>,
    resource: Collection<'a>,
    root: Collection<'a>,
    now: jiff::Timestamp,
}

/// The variables of the expression being evaluated: the focus (`$this`), the
/// iteration index and the aggregate total.
#[derive(Clone)]
pub(crate) struct Frame<'a> {
    focus: Collection<'a>,
    index: Option<usize>,
    total: Option<Collection<'a>>,
}

impl<'a> Frame<'a> {
    /// The frame of a top-level evaluation over `focus`.
    pub(crate) const fn top(focus: Collection<'a>) -> Self {
        Self {
            focus,
            index: None,
            total: None,
        }
    }
}

/// Evaluates `expr` in `env` on `focus`.
///
/// # Errors
///
/// Returns what the evaluation signals, as [`EvalError`].
pub(crate) fn evaluate<'a>(
    expr: &Expr,
    env: &Environment<'a>,
    focus: Focus<'a>,
) -> Result<Collection<'a>, EvalError> {
    let model = env.model;
    let as_resource = |value: Option<&'a Value>| -> Collection<'a> {
        value
            .map(|value| Item::Node(Node::resource(model, Doc::Borrowed(value))))
            .into_iter()
            .collect()
    };
    let context = match focus.start {
        Start::Resource(value) => as_resource(Some(value)),
        Start::Element {
            value,
            element,
            type_name,
            definition,
        } => {
            let ty = path_type(model, &type_name)
                .or_else(|| named_type(model, &type_name))
                .ok_or(EvalError::UnknownType { name: type_name })?;
            let definition = match definition {
                Some(path) => match model.schemas().element(&path) {
                    Some((_, field)) => Some(field.path),
                    None => return Err(EvalError::UnknownType { name: path }),
                },
                None => None,
            };
            let node = match (value, element) {
                (Some(value), element) => Some(Node::typed(
                    ty,
                    Doc::Borrowed(value),
                    element.map(Doc::Borrowed),
                )),
                (None, Some(element)) => Some(Node::typed(
                    ty,
                    Doc::Owned(Arc::new(Value::Null)),
                    Some(Doc::Borrowed(element)),
                )),
                (None, None) => None,
            };
            node.map(|node| match definition {
                Some(path) => node.with_definition(path),
                None => node,
            })
            .map(Item::Node)
            .into_iter()
            .collect()
        }
        Start::Items(items) => items,
    };
    let engine = Engine {
        env,
        resource: as_resource(focus.resource),
        root: as_resource(focus.root),
        now: env.now.unwrap_or_else(jiff::Timestamp::now),
        context: context.clone(),
    };
    engine.eval(expr, &Frame::top(context))
}

impl<'e, 'a> Engine<'e, 'a> {
    /// The type model.
    pub(crate) fn model(&self) -> &'a dyn Model {
        self.env.model
    }

    /// Evaluates one node of the syntax tree.
    pub(crate) fn eval(&self, expr: &Expr, frame: &Frame<'a>) -> Result<Collection<'a>, EvalError> {
        match expr {
            Expr::Literal(literal) => Ok(vec![literal_item(literal)]),
            Expr::Empty => Ok(Vec::new()),
            Expr::External(name) => self.external(name),
            Expr::This => Ok(frame.focus.clone()),
            Expr::Index => Ok(frame
                .index
                .and_then(|index| i64::try_from(index).ok())
                .map(Item::Integer)
                .into_iter()
                .collect()),
            Expr::Total => Ok(frame.total.clone().unwrap_or_default()),
            Expr::Member { input, name } => {
                let items = self.input(input.as_deref(), frame)?;
                self.navigate(&items, name, input.is_none())
            }
            Expr::Call { input, name, args } => {
                let items = self.input(input.as_deref(), frame)?;
                self.call(name, items, args, frame)
            }
            Expr::Invocation { input, variable } => {
                let items = self.eval(input, frame)?;
                self.eval(variable, &Frame::top(items))
            }
            Expr::Indexer { input, index } => {
                let items = self.eval(input, frame)?;
                let index =
                    convert::integer(self.model(), &self.eval(index, frame)?, "the indexer")?;
                Ok(index
                    .and_then(|index| usize::try_from(index).ok())
                    .and_then(|index| items.into_iter().nth(index))
                    .into_iter()
                    .collect())
            }
            Expr::Negate(operand) => operators::negate(self.model(), &self.eval(operand, frame)?),
            Expr::Plus(operand) => operators::plus(self.model(), &self.eval(operand, frame)?),
            Expr::Binary { op, left, right } => self.binary(*op, left, right, frame),
            Expr::Is {
                operand,
                type_specifier,
            } => {
                let items = self.eval(operand, frame)?;
                let ty = self.resolve_type(type_specifier)?;
                match convert::single(&items, "is")? {
                    Some(item) => Ok(vec![Item::Boolean(self.is_instance(item, &ty))]),
                    None => Ok(Vec::new()),
                }
            }
            Expr::As {
                operand,
                type_specifier,
            } => {
                let items = self.eval(operand, frame)?;
                let ty = self.resolve_type(type_specifier)?;
                match convert::single(&items, "as")? {
                    Some(item) if self.is_type(item, &ty) => Ok(vec![item.clone()]),
                    _ => Ok(Vec::new()),
                }
            }
        }
    }

    /// The input of a member or a call: the expression before the `.`, or
    /// the focus at the start of a path.
    fn input(&self, input: Option<&Expr>, frame: &Frame<'a>) -> Result<Collection<'a>, EvalError> {
        match input {
            Some(input) => self.eval(input, frame),
            None => Ok(frame.focus.clone()),
        }
    }

    /// An environment variable (N1 §9, FHIR R4 §2.9.1.4 and §2.9.1.7).
    fn external(&self, name: &str) -> Result<Collection<'a>, EvalError> {
        if let Some(value) = self.env.variables.get(name) {
            return Ok(value.clone());
        }
        let text = |value: &str| Ok(vec![Item::String(String::from(value))]);
        match name {
            "ucum" => text(UCUM),
            "sct" => text("http://snomed.info/sct"),
            "loinc" => text("http://loinc.org"),
            "context" => Ok(self.context.clone()),
            "resource" => Ok(self.resource.clone()),
            "rootResource" => Ok(self.root.clone()),
            _ => {
                if let Some(id) = name.strip_prefix("vs-") {
                    text(&format!("http://hl7.org/fhir/ValueSet/{id}"))
                } else if let Some(id) = name.strip_prefix("ext-") {
                    text(&format!("http://hl7.org/fhir/StructureDefinition/{id}"))
                } else {
                    Err(EvalError::UnknownVariable {
                        name: String::from(name),
                    })
                }
            }
        }
    }

    /// The `%resource` collection.
    pub(crate) fn resource(&self) -> &Collection<'a> {
        &self.resource
    }

    /// The `%rootResource` collection.
    pub(crate) fn root(&self) -> &Collection<'a> {
        &self.root
    }

    /// The instant of this evaluation.
    pub(crate) const fn now(&self) -> jiff::Timestamp {
        self.now
    }

    /// The environment.
    pub(crate) const fn env(&self) -> &'e Environment<'a> {
        self.env
    }
}

/// The item a literal stands for.
fn literal_item<'a>(literal: &Literal) -> Item<'a> {
    match literal {
        Literal::Boolean(value) => Item::Boolean(*value),
        Literal::String(value) => Item::String(value.clone()),
        Literal::Integer(value) => Item::Integer(*value),
        Literal::Decimal(value) => Item::Decimal(*value),
        Literal::Date(value) => Item::Date(*value),
        Literal::DateTime(value) => Item::DateTime(*value),
        Literal::Time(value) => Item::Time(*value),
        Literal::Quantity(value, unit) => Item::Quantity(Quantity::new(*value, unit.clone())),
    }
}
