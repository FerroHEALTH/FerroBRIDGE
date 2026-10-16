// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An element or resource of the document under evaluation, typed by the
//! model.
//!
//! FHIRPath sees the logical model, not the JSON form (FHIR R4
//! <https://hl7.org/fhir/R4/fhirpath.html#2.9.1>): a choice element answers
//! to its name without the type suffix, a primitive is one node whether its
//! value, its sibling `_name` object (`id` and `extension`) or both are
//! present, and a contained resource is its own children.

use std::fmt;
use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::schema::{FieldSchema, Kind};

use super::{Collection, Item};
use crate::eval::error::EvalError;
use crate::model::{
    FhirType, Model, TypeKind, choice_element, choice_types, defines, field_type, resource_type,
    variant_type,
};

/// A JSON value of the document, borrowed from the input or owned when a
/// seam returned it.
#[derive(Clone)]
pub(crate) enum Doc<'a> {
    /// Part of the input document.
    Borrowed(&'a Value),
    /// A document a seam returned, such as a resolved reference.
    Owned(Arc<Value>),
}

impl PartialEq for Doc<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl Doc<'_> {
    /// The value.
    pub(crate) fn get(&self) -> &Value {
        match self {
            Self::Borrowed(value) => value,
            Self::Owned(value) => value,
        }
    }

    /// The member `key` of an object, each item when it holds an array.
    fn entries(&self, key: &str) -> Vec<Option<Self>> {
        let each = |member: &Value| match member {
            Value::Array(items) => items.len(),
            _ => 1,
        };
        let Some(count) = self.get().get(key).map(each) else {
            return Vec::new();
        };
        (0..count).map(|index| self.entry(key, index)).collect()
    }

    /// Item `index` of the member `key`, the member itself when it is no
    /// array; `None` for a JSON `null`.
    fn entry(&self, key: &str, index: usize) -> Option<Self> {
        match self {
            Self::Borrowed(value) => pick(value, key, index).map(Self::Borrowed),
            Self::Owned(value) => {
                pick(value, key, index).map(|item| Self::Owned(Arc::new(item.clone())))
            }
        }
    }
}

/// Item `index` of the member `key` of `value`, the member itself when it is
/// no array; `None` for a JSON `null`.
fn pick<'v>(value: &'v Value, key: &str, index: usize) -> Option<&'v Value> {
    let item = match value.get(key)? {
        Value::Array(items) => items.get(index)?,
        other => other,
    };
    (!item.is_null()).then_some(item)
}

/// What a node holds.
#[derive(Clone, PartialEq)]
enum Data<'a> {
    /// A complex element or a resource: a JSON object.
    Complex(Doc<'a>),
    /// A primitive: its JSON value and its `_name` object, each optional.
    Primitive {
        value: Option<Doc<'a>>,
        element: Option<Doc<'a>>,
    },
}

/// An element or resource of the document.
///
/// Its `Debug` form names the type and the definition and leaves the content
/// out, so no clinical value reaches a log through it.
#[derive(Clone, PartialEq)]
pub struct Node<'a> {
    data: Data<'a>,
    ty: Option<FhirType>,
    definition: Option<&'static str>,
}

impl fmt::Debug for Node<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let present = match &self.data {
            Data::Complex(_) => "complex",
            Data::Primitive { value: Some(_), .. } => "primitive with a value",
            Data::Primitive { value: None, .. } => "primitive without a value",
        };
        f.debug_struct("Node")
            .field("type", &self.ty.map(|ty| ty.name))
            .field("definition", &self.definition)
            .field("holds", &present)
            .finish()
    }
}

impl fmt::Display for Node<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.ty {
            Some(ty) => write!(f, "FHIR.{}", ty.name),
            None => f.write_str("FHIR.(untyped)"),
        }
    }
}

impl<'a> Node<'a> {
    /// A resource node, typed by its `resourceType`.
    pub(crate) fn resource(model: &dyn Model, doc: Doc<'a>) -> Self {
        let ty = doc
            .get()
            .get("resourceType")
            .and_then(Value::as_str)
            .and_then(|name| resource_type(model, name));
        Self {
            definition: ty.map(|ty| ty.name),
            data: Data::Complex(doc),
            ty,
        }
    }

    /// A node of a given type over a JSON value: an object for a complex
    /// type, the value (and optionally its `_name` object) for a primitive.
    pub(crate) fn typed(ty: FhirType, value: Doc<'a>, element: Option<Doc<'a>>) -> Self {
        let data = if ty.is_primitive() {
            Data::Primitive {
                value: (!value.get().is_null()).then_some(value),
                element,
            }
        } else {
            Data::Complex(value)
        };
        Self {
            data,
            ty: Some(ty),
            definition: None,
        }
    }

    /// The node as an occurrence of the element defined at `path`, a
    /// path the element table holds.
    pub(crate) const fn with_definition(mut self, path: &'static str) -> Self {
        self.definition = Some(path);
        self
    }

    /// The FHIR type, `None` for a resource whose type the model does not
    /// know.
    #[must_use]
    pub const fn fhir_type(&self) -> Option<&FhirType> {
        self.ty.as_ref()
    }

    /// The element path of the definition this node was reached through, as
    /// `Patient.name.given`; `None` for a node reached otherwise.
    #[must_use]
    pub const fn definition(&self) -> Option<&'static str> {
        self.definition
    }

    /// The JSON value: the object of a complex node or a resource, the
    /// primitive value of a primitive node.
    #[must_use]
    pub fn json(&self) -> Option<&Value> {
        match &self.data {
            Data::Complex(doc) => Some(doc.get()),
            Data::Primitive { value, .. } => value.as_ref().map(Doc::get),
        }
    }

    /// The `_name` object of a primitive: its `id` and `extension`.
    #[must_use]
    pub fn primitive_element(&self) -> Option<&Value> {
        match &self.data {
            Data::Complex(_) => None,
            Data::Primitive { element, .. } => element.as_ref().map(Doc::get),
        }
    }

    /// Whether this is a resource.
    #[must_use]
    pub fn is_resource(&self) -> bool {
        matches!(
            self.ty,
            Some(FhirType {
                kind: TypeKind::Resource(_),
                ..
            })
        ) || (self.ty.is_none()
            && self
                .json()
                .and_then(|json| json.get("resourceType"))
                .is_some())
    }

    /// The object members are read from: the object itself, or a primitive's
    /// `_name` object.
    fn object(&self) -> Option<&Doc<'a>> {
        match &self.data {
            Data::Complex(doc) => Some(doc),
            Data::Primitive { element, .. } => element.as_ref(),
        }
    }

    /// The member `name` (N1 §3): `Some` with its nodes when the node's type
    /// defines it, `None` when it does not.
    ///
    /// A primitive's members are `id` and `extension`, from the table's
    /// `Element`. A node with no type answers every member from its JSON. An
    /// occurrence of a choice element answers `Some` and empty for a member
    /// another type of the choice defines and its own type does not.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::Document`] when the document contradicts the
    /// model, as a string where an object belongs.
    pub fn member(
        &self,
        model: &dyn Model,
        name: &str,
    ) -> Result<Option<Collection<'a>>, EvalError> {
        let Some(ty) = self.ty else {
            return Ok(Some(self.untyped_member(model, name)));
        };
        let schema = match ty.kind {
            TypeKind::Complex(schema) | TypeKind::Resource(schema) => schema,
            TypeKind::Primitive(_) | TypeKind::Xhtml => {
                match model.schemas().type_named("Element") {
                    Some(element) => element,
                    None => return Ok(None),
                }
            }
        };
        let Some(field) = schema.fields.iter().find(|field| field.name == name) else {
            // NOTE: N1 §3.2 lets a path over a polymorphic item ignore its type ("all kinds of
            // value"), and N1 §4.4 makes an absent member no node, so the choice's types bound it.
            let admitted = self
                .definition
                .and_then(|path| choice_element(model, path))
                .and_then(|choice| choice_types(model, choice))
                .is_some_and(|types| types.iter().any(|ty| defines(model, ty, name)));
            return Ok(admitted.then(Vec::new));
        };
        let Some(object) = self.object() else {
            return Ok(Some(Vec::new()));
        };
        read_field(model, object, field).map(Some)
    }

    /// The member `name` of a node with no type, read from its JSON.
    fn untyped_member(&self, model: &dyn Model, name: &str) -> Collection<'a> {
        let Some(object) = self.object() else {
            return Vec::new();
        };
        object
            .entries(name)
            .into_iter()
            .flatten()
            .map(|doc| untyped(model, doc))
            .collect()
    }

    /// Every child node, in definition order, with the member name it holds
    /// (N1 §5.8.1); a primitive's value comes first as a system value (FHIR R4
    /// <https://hl7.org/fhir/R4/fhirpath.html#2.9.1.5>).
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::Document`] when the document contradicts the
    /// model.
    pub fn named_children(&self, model: &dyn Model) -> Result<Vec<(String, Item<'a>)>, EvalError> {
        let mut children = Vec::new();
        let schema = match self.ty.map(|ty| ty.kind) {
            Some(TypeKind::Complex(schema) | TypeKind::Resource(schema)) => schema,
            Some(TypeKind::Primitive(_) | TypeKind::Xhtml) => {
                if let Some(value) = crate::eval::convert::primitive_value(self)? {
                    children.push((String::from("value"), value));
                }
                match model.schemas().type_named("Element") {
                    Some(element) => element,
                    None => return Ok(children),
                }
            }
            None => {
                if let Some(Value::Object(object)) = self.object().map(Doc::get) {
                    for key in object.keys().filter(|key| *key != "resourceType") {
                        for item in self.untyped_member(model, key) {
                            children.push((key.clone(), item));
                        }
                    }
                }
                return Ok(children);
            }
        };
        let Some(object) = self.object() else {
            return Ok(children);
        };
        for field in schema.fields {
            for item in read_field(model, object, field)? {
                children.push((String::from(field.name), item));
            }
        }
        Ok(children)
    }
}

/// The nodes a field holds in `object`.
fn read_field<'a>(
    model: &dyn Model,
    object: &Doc<'a>,
    field: &'static FieldSchema,
) -> Result<Collection<'a>, EvalError> {
    match field.kind {
        Kind::Choice(variants) => {
            let mut items = Vec::new();
            for (suffix, kind) in variants {
                let key = format!("{}{suffix}", field.name);
                let ty = variant_type(model, field, suffix, *kind);
                items.extend(read_key(model, object, &key, field, ty)?);
            }
            Ok(items)
        }
        Kind::Resource => Ok(object
            .entries(field.name)
            .into_iter()
            .flatten()
            .map(|doc| {
                let mut node = Node::resource(model, doc);
                node.definition = Some(field.path);
                Item::Node(node)
            })
            .collect()),
        kind => read_key(
            model,
            object,
            field.name,
            field,
            field_type(model, field, kind),
        ),
    }
}

/// The nodes under `key` (and `_key` for a primitive), typed `ty`.
fn read_key<'a>(
    model: &dyn Model,
    object: &Doc<'a>,
    key: &str,
    field: &'static FieldSchema,
    ty: Option<FhirType>,
) -> Result<Collection<'a>, EvalError> {
    let Some(ty) = ty else {
        return Ok(object
            .entries(key)
            .into_iter()
            .flatten()
            .map(|doc| untyped(model, doc))
            .collect());
    };
    let node = |data| {
        Item::Node(Node {
            data,
            ty: Some(ty),
            definition: Some(field.path),
        })
    };
    if ty.is_primitive() {
        let values = object.entries(key);
        let elements = object.entries(&format!("_{key}"));
        let count = values.len().max(elements.len());
        let mut items = Vec::with_capacity(count);
        for index in 0..count {
            let value = values.get(index).cloned().flatten();
            let element = elements.get(index).cloned().flatten();
            if let Some(value) = &value
                && matches!(value.get(), Value::Object(_) | Value::Array(_))
            {
                return Err(EvalError::Document {
                    element: String::from(field.path),
                    reason: String::from("a primitive holds an object or an array"),
                });
            }
            if value.is_some() || element.is_some() {
                items.push(node(Data::Primitive { value, element }));
            }
        }
        return Ok(items);
    }
    let mut items = Vec::new();
    for doc in object.entries(key).into_iter().flatten() {
        if !matches!(doc.get(), Value::Object(_)) {
            return Err(EvalError::Document {
                element: String::from(field.path),
                reason: format!("the element holds no object for its type {}", ty.name),
            });
        }
        items.push(node(Data::Complex(doc)));
    }
    Ok(items)
}

/// An item for a JSON value no definition types: an object is an untyped
/// node (or a resource), a scalar its system value.
fn untyped<'a>(model: &dyn Model, doc: Doc<'a>) -> Item<'a> {
    match doc.get() {
        Value::Bool(flag) => Item::Boolean(*flag),
        Value::String(text) => Item::String(text.clone()),
        Value::Number(number) => number
            .as_i64()
            .map(Item::Integer)
            .or_else(|| number.as_str().parse().ok().map(Item::Decimal))
            .unwrap_or_else(|| Item::String(String::from(number.as_str()))),
        Value::Object(object) if object.contains_key("resourceType") => {
            Item::Node(Node::resource(model, doc))
        }
        _ => Item::Node(Node {
            data: Data::Complex(doc),
            ty: None,
            definition: None,
        }),
    }
}
