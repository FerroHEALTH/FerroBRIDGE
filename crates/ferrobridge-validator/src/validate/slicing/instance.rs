// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A discriminator path followed through the instance: the nodes it selects
//! from one item, each with its type.

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhir_types::schema::FieldSchema;
use fhir_types::schema::Kind;
use fhir_types::schema::TypeSchema;

use crate::issue::IssueType;
use crate::issue::MessageId;
use crate::issue::Severity;
use crate::report::Reason;
use crate::validate::resolver::Resolution;
use crate::validate::slicing::path::Step;
use crate::validate::walk::Form;
use crate::validate::walk::Occurrence;
use crate::validate::walk::Walk;
use crate::validate::walk::members::upper_first;

/// One node a discriminator path selects: its value, the `_` sibling of a
/// primitive, and its type.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Node<'i> {
    /// The value.
    pub(crate) value: Option<&'i Value>,
    /// The `_` sibling of a primitive.
    pub(crate) element: Option<&'i Value>,
    /// The type.
    pub(crate) kind: NodeKind,
}

/// The type of a node.
#[derive(Debug, Clone, Copy)]
pub(crate) enum NodeKind {
    /// A primitive of the type code.
    Primitive(&'static str),
    /// A datatype or backbone element.
    Complex(&'static TypeSchema),
    /// A resource.
    Resource(&'static TypeSchema),
}

impl<'i> Node<'i> {
    /// Returns the node of one occurrence of `field` in the form `form`.
    pub(crate) fn item(
        field: &'static FieldSchema,
        form: &Form,
        occurrence: &Occurrence<'i>,
    ) -> Option<Self> {
        let code = form.choice.or_else(|| field.types.first().copied());
        node(form.kind, code, occurrence.value, occurrence.element)
    }

    /// Returns the type name: a primitive's code, or a datatype's or
    /// resource's name.
    pub(crate) const fn type_name(&self) -> &'static str {
        match self.kind {
            NodeKind::Primitive(code) => code,
            NodeKind::Complex(schema) | NodeKind::Resource(schema) => schema.name,
        }
    }

    /// Returns the object holding the node's members: the value of a
    /// datatype or resource, the `_` sibling of a primitive.
    fn object(&self) -> Option<&'i Object> {
        match self.kind {
            NodeKind::Primitive(_) => self.element.and_then(Value::as_object),
            NodeKind::Complex(_) | NodeKind::Resource(_) => self.value.and_then(Value::as_object),
        }
    }
}

/// The node of a value of `kind`, `None` for a choice or an unknown type.
fn node<'i>(
    kind: Kind,
    code: Option<&'static str>,
    value: Option<&'i Value>,
    element: Option<&'i Value>,
) -> Option<Node<'i>> {
    let kind = match kind {
        Kind::Attribute | Kind::Primitive(_) => NodeKind::Primitive(code?),
        Kind::Xhtml => NodeKind::Primitive("xhtml"),
        Kind::Complex(name) => NodeKind::Complex(SCHEMAS.type_named(name)?),
        Kind::Resource => {
            let name = value?.get("resourceType")?.as_str()?;
            NodeKind::Resource(
                SCHEMAS
                    .is_resource(name)
                    .then(|| SCHEMAS.type_named(name))??,
            )
        }
        Kind::Choice(_) => return None,
    };
    Some(Node {
        value,
        element,
        kind,
    })
}

/// The nodes of the member `key` of `object`, of `kind`, repeating or not.
fn members<'i>(
    object: &'i Object,
    key: &str,
    kind: Kind,
    code: Option<&'static str>,
    many: bool,
) -> Vec<Node<'i>> {
    let value = object.get(key);
    let element = if matches!(kind, Kind::Primitive(_)) {
        object.get(&format!("_{key}"))
    } else {
        None
    };
    if !many {
        if value.is_none() && element.is_none() {
            return Vec::new();
        }
        return node(kind, code, value, element).into_iter().collect();
    }
    let values = value.and_then(Value::as_array).unwrap_or_default();
    let elements = element.and_then(Value::as_array).unwrap_or_default();
    (0..values.len().max(elements.len()))
        .filter_map(|index| {
            let value = values.get(index).filter(|item| !item.is_null());
            let element = elements.get(index).filter(|item| !item.is_null());
            node(kind, code, value, element)
        })
        .collect()
}

/// The children of `node` named `name`, every type of a choice included.
fn children<'i>(node: &Node<'i>, name: &str) -> Vec<Node<'i>> {
    let Some(object) = node.object() else {
        return Vec::new();
    };
    let schema = match node.kind {
        NodeKind::Complex(schema) | NodeKind::Resource(schema) => schema,
        // NOTE: datatypes.html §2.24.0.1: a primitive's children are its id and extensions.
        NodeKind::Primitive(_) => {
            return match (name, SCHEMAS.type_named("Extension")) {
                ("extension", Some(extension)) => object
                    .get("extension")
                    .and_then(Value::as_array)
                    .unwrap_or_default()
                    .iter()
                    .map(|item| Node {
                        value: Some(item),
                        element: None,
                        kind: NodeKind::Complex(extension),
                    })
                    .collect(),
                _ => Vec::new(),
            };
        }
    };
    let Some(field) = schema.fields.iter().find(|field| field.name == name) else {
        return Vec::new();
    };
    match field.kind {
        Kind::Choice(variants) => variants
            .iter()
            .flat_map(|(suffix, kind)| {
                let code = field
                    .types
                    .iter()
                    .copied()
                    .find(|code| upper_first(code) == *suffix);
                members(
                    object,
                    &format!("{}{suffix}", field.name),
                    *kind,
                    code,
                    field.many,
                )
            })
            .collect(),
        kind => members(
            object,
            field.name,
            kind,
            field.types.first().copied(),
            field.many,
        ),
    }
}

/// Answers whether `node` is of the type `name`.
fn is_type(node: &Node<'_>, name: &str) -> bool {
    node.type_name() == name
        || (matches!(node.kind, NodeKind::Resource(_))
            && (name == "Resource" || name == "DomainResource"))
}

impl<'i> Walk<'_, '_, 'i> {
    /// Follows `steps` from `start`, `Err` with the reason when a reference
    /// on the way does not resolve.
    ///
    /// A reference that resolves to nothing is reported as an issue at
    /// `location` naming `path`, because R4's `resolve()` drops it in silence
    /// (fhirpath.html §2.9.1.5) and a discriminator never may.
    pub(crate) fn follow(
        &mut self,
        start: Node<'i>,
        steps: &[Step],
        path: &str,
        location: &str,
    ) -> Result<Vec<Node<'i>>, Reason> {
        let mut nodes = vec![start];
        for step in steps {
            nodes = match step {
                Step::Member(name) => nodes.iter().flat_map(|node| children(node, name)).collect(),
                Step::Extension(url) => nodes
                    .iter()
                    .flat_map(|node| children(node, "extension"))
                    .filter(|extension| {
                        extension
                            .value
                            .and_then(|value| value.get("url"))
                            .and_then(Value::as_str)
                            == Some(url.as_str())
                    })
                    .collect(),
                Step::OfType(name) => nodes
                    .into_iter()
                    .filter(|node| is_type(node, name))
                    .collect(),
                Step::Resolve => {
                    let mut resolved = Vec::new();
                    for node in &nodes {
                        resolved.push(self.resolve(node, path, location)?);
                    }
                    resolved
                }
            };
        }
        Ok(nodes)
    }

    /// Resolves the reference `node` holds to the resource it points at.
    fn resolve(&mut self, node: &Node<'i>, path: &str, location: &str) -> Result<Node<'i>, Reason> {
        let reference = match node.kind {
            NodeKind::Complex(schema) if schema.name == "Reference" => node
                .value
                .and_then(|value| value.get("reference"))
                .and_then(Value::as_str),
            _ => None,
        };
        // NOTE: references.html §2.3.0.3: a Reference without `reference` (a logical
        // identifier or a display) names no resource the validator can resolve.
        let Some(reference) = reference else {
            return Err(Reason::ReferenceOutOfScope);
        };
        match self.validator().resolver().resolve(reference, self.scope()) {
            Resolution::Found { target, .. } => {
                node_of_resource(target).ok_or(Reason::UnresolvedReference)
            }
            Resolution::OutOfScope => Err(Reason::ReferenceOutOfScope),
            Resolution::Ambiguous => Err(Reason::AmbiguousReference),
            Resolution::Missing => {
                self.out().issue(
                    Severity::Warning,
                    IssueType::NotFound,
                    MessageId::UnresolvedReference,
                    location,
                    format!(
                        "the reference resolves to nothing, so the discriminator {path} is not evaluated"
                    ),
                );
                Err(Reason::UnresolvedReference)
            }
        }
    }
}

/// The node of the resource `value`.
fn node_of_resource(value: &Value) -> Option<Node<'_>> {
    node(Kind::Resource, None, Some(value), None)
}
