// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR type model an evaluation navigates by.
//!
//! What an element is, whether it repeats, which types a choice admits and
//! where a content reference lands all come from the generated element table
//! of `fhir-types` (`fhir_types::schema::Schemas`); a version plugs in by
//! implementing [`Model`] over its table. [`r4::R4`] is the R4 table and
//! [`stu3::Stu3`] the STU3 one.

pub mod r4;
pub mod stu3;

use std::collections::BTreeMap;
use std::fmt;

use fhir_types::schema::{FieldSchema, Kind, Schemas, TypeSchema, ValueKind};

/// A FHIR version's type model.
pub trait Model: fmt::Debug + Send + Sync {
    /// The version's element table.
    fn schemas(&self) -> &'static Schemas;

    /// The version's primitive types by name, read from the table.
    fn primitives(&self) -> &BTreeMap<&'static str, TypeKind>;

    /// The type `name` specializes, read from the element table.
    ///
    /// `Age` is a `Quantity`, and every resource is a `DomainResource` or a
    /// `Resource`; R4 §2.9.1.1 lets a type name name such a supertype.
    ///
    /// A primitive has no base here, because R4 §2.9.1.5 considers every
    /// primitive an independent type for `ofType()`; [`Model::primitive_base`]
    /// answers it for `is`.
    fn base(&self, name: &str) -> Option<&'static str> {
        self.schemas().base_of(name)
    }

    /// The type the primitive `name` specializes, its `baseDefinition` read
    /// from the table: `code` is a `string`, and `string` an `Element`.
    fn primitive_base(&self, name: &str) -> Option<&'static str> {
        self.schemas().primitive_base_of(name)
    }
}

/// What kind of FHIR type an element holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    /// A primitive, with the scalar its JSON value carries.
    Primitive(ValueKind),
    /// The XHTML narrative.
    Xhtml,
    /// A datatype or a backbone element, with its elements.
    Complex(&'static TypeSchema),
    /// A resource, with its elements.
    Resource(&'static TypeSchema),
}

/// The FHIR type of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FhirType {
    /// The type name: a primitive code, a datatype, a resource, or for a
    /// backbone element the table's name for it.
    pub name: &'static str,
    /// What kind of type it is.
    pub kind: TypeKind,
}

impl FhirType {
    /// The elements of a complex type or a resource.
    #[must_use]
    pub const fn schema(&self) -> Option<&'static TypeSchema> {
        match self.kind {
            TypeKind::Complex(schema) | TypeKind::Resource(schema) => Some(schema),
            TypeKind::Primitive(_) | TypeKind::Xhtml => None,
        }
    }

    /// Whether this is a backbone element, an anonymous type defined inside
    /// another (N1 §10.2.4).
    #[must_use]
    pub fn is_backbone(&self) -> bool {
        matches!(self.kind, TypeKind::Complex(schema) if schema.path != schema.name)
    }

    /// Whether this is a primitive type.
    #[must_use]
    pub const fn is_primitive(&self) -> bool {
        matches!(self.kind, TypeKind::Primitive(_) | TypeKind::Xhtml)
    }
}

/// The type of the resource named `name`.
#[must_use]
pub fn resource_type(model: &dyn Model, name: &str) -> Option<FhirType> {
    let schemas = model.schemas();
    let index = schemas.resources.binary_search(&name).ok()?;
    let name = *schemas.resources.get(index)?;
    let schema = schemas
        .type_named(name)
        .filter(|schema| schema.path == name)?;
    Some(FhirType {
        name,
        kind: TypeKind::Resource(schema),
    })
}

/// The type of the datatype, resource or primitive named `name`; backbone
/// elements have no name of their own and are not found.
#[must_use]
pub fn named_type(model: &dyn Model, name: &str) -> Option<FhirType> {
    if let Some(found) = resource_type(model, name) {
        return Some(found);
    }
    let schemas = model.schemas();
    if let Some(schema) = schemas
        .type_named(name)
        .filter(|schema| schema.path == schema.name)
    {
        let kind = if schemas.specializes(schema.name, "Resource") {
            TypeKind::Resource(schema)
        } else {
            TypeKind::Complex(schema)
        };
        return Some(FhirType {
            name: schema.name,
            kind,
        });
    }
    let (name, kind) = model.primitives().get_key_value(name)?;
    Some(FhirType { name, kind: *kind })
}

/// The type a non-choice element holds, `None` for a resource-typed element,
/// whose type each instance names.
#[must_use]
pub fn field_type(model: &dyn Model, field: &'static FieldSchema, kind: Kind) -> Option<FhirType> {
    match kind {
        Kind::Primitive(value) => Some(FhirType {
            name: field.types.first().copied().unwrap_or("string"),
            kind: TypeKind::Primitive(value),
        }),
        Kind::Attribute => Some(FhirType {
            name: field.types.first().copied().unwrap_or("string"),
            kind: TypeKind::Primitive(ValueKind::Text),
        }),
        Kind::Xhtml => Some(FhirType {
            name: "xhtml",
            kind: TypeKind::Xhtml,
        }),
        Kind::Complex(name) => model.schemas().type_named(name).map(|schema| FhirType {
            name,
            kind: TypeKind::Complex(schema),
        }),
        Kind::Resource | Kind::Choice(_) => None,
    }
}

/// The type code a choice suffix names among an element's codes.
#[must_use]
pub fn choice_code(field: &'static FieldSchema, suffix: &str) -> Option<&'static str> {
    field.types.iter().copied().find(|code| {
        let mut chars = code.chars();
        chars.next().is_some_and(|first| {
            suffix.starts_with(first.to_ascii_uppercase())
                && suffix.get(first.len_utf8()..) == Some(chars.as_str())
        })
    })
}

/// Every primitive type the table uses, with the scalar its JSON value
/// carries.
#[must_use]
pub fn primitive_types(schemas: &'static Schemas) -> BTreeMap<&'static str, TypeKind> {
    let mut types = BTreeMap::from([("xhtml", TypeKind::Xhtml)]);
    let scalar = |kind: &Kind| match kind {
        Kind::Primitive(value) => Some(*value),
        Kind::Attribute => Some(ValueKind::Text),
        _ => None,
    };
    for field in schemas.types.iter().flat_map(|schema| schema.fields) {
        if let Some(value) = scalar(&field.kind)
            && let Some(code) = field.types.first()
        {
            types.entry(*code).or_insert(TypeKind::Primitive(value));
        }
        if let Kind::Choice(variants) = field.kind {
            for (suffix, kind) in variants {
                if let Some(value) = scalar(kind)
                    && let Some(code) = choice_code(field, suffix)
                {
                    types.entry(code).or_insert(TypeKind::Primitive(value));
                }
            }
        }
    }
    types
}

/// The type an element path names, for evaluating an expression whose
/// context is that element, as an `ElementDefinition.constraint` is
/// (<https://hl7.org/fhir/R4/conformance-rules.html#constraints>).
///
/// A choice element names no one type; [`path_types`] answers its types.
#[must_use]
pub fn path_type(model: &dyn Model, path: &str) -> Option<FhirType> {
    if !path.contains('.') {
        return named_type(model, path);
    }
    let (_, field) = model.schemas().element(path)?;
    field_type(model, field, field.kind)
}

/// The type one alternative of a choice element holds.
///
/// A primitive alternative is named by its own type code (`dateTime` behind
/// `valueDateTime`), a complex one by its datatype, and a resource-typed one
/// has no type here.
#[must_use]
pub fn variant_type(
    model: &dyn Model,
    field: &'static FieldSchema,
    suffix: &str,
    kind: Kind,
) -> Option<FhirType> {
    let found = field_type(model, field, kind)?;
    Some(match (kind, choice_code(field, suffix)) {
        (Kind::Complex(_), _) | (_, None) => found,
        (_, Some(code)) => FhirType {
            name: code,
            ..found
        },
    })
}

/// Every type a choice element admits (R4 §2.9.1.1), in definition order;
/// `None` when `field` is no choice element or an alternative has no type of
/// its own here.
#[must_use]
pub fn choice_types(model: &dyn Model, field: &'static FieldSchema) -> Option<Vec<FhirType>> {
    let Kind::Choice(variants) = field.kind else {
        return None;
    };
    variants
        .iter()
        .map(|(suffix, kind)| variant_type(model, field, suffix, *kind))
        .collect()
}

/// The types an element path admits, for checking an expression whose
/// context is that element: every type of a choice element (R4 §2.9.1.1),
/// the one type [`path_type`] names otherwise.
///
/// A path that continues through a choice element reaches the types of the
/// next element in every admitted type that defines it
/// (`Observation.value[x].unit` is the `unit` of `Quantity`), and a path
/// below a primitive reaches the elements of `Element`, which every
/// primitive specializes (`string.extension`, R4 §2.9.1.2).
#[must_use]
pub fn path_types(model: &dyn Model, path: &str) -> Option<Vec<FhirType>> {
    if let Some(field) = choice_element(model, path) {
        return choice_types(model, field);
    }
    if let Some(ty) = path_type(model, path) {
        return Some(vec![ty]);
    }
    walk(model, path)
}

/// The types `path` reaches, segment by segment from the type its first
/// segment names, through every type a choice admits and through the
/// elements of `Element` below a primitive; `None` when a segment is no
/// element of any type reached so far, or reaches a resource.
fn walk(model: &dyn Model, path: &str) -> Option<Vec<FhirType>> {
    let schemas = model.schemas();
    let mut segments = path.split('.');
    let mut reach = vec![named_type(model, segments.next()?)?];
    for segment in segments {
        let mut next: Vec<FhirType> = Vec::new();
        for ty in &reach {
            let owner = if ty.is_primitive() {
                "Element"
            } else {
                ty.schema()?.path
            };
            let Some((_, field)) = schemas.element(&format!("{owner}.{segment}")) else {
                continue;
            };
            let types = match field.kind {
                Kind::Choice(_) => choice_types(model, field)?,
                kind => vec![field_type(model, field, kind)?],
            };
            for found in types {
                if !next.contains(&found) {
                    next.push(found);
                }
            }
        }
        if next.is_empty() {
            return None;
        }
        reach = next;
    }
    Some(reach)
}

/// The choice element an element path names, in its base form
/// (`Observation.value[x]`) or an expanded one (`Observation.valueQuantity`).
#[must_use]
pub fn choice_element(model: &dyn Model, path: &str) -> Option<&'static FieldSchema> {
    if !path.contains('.') {
        return None;
    }
    let (_, field) = model.schemas().element(path)?;
    matches!(field.kind, Kind::Choice(_)).then_some(field)
}

/// Whether the type `ty` defines the member `name`; a primitive's members
/// are those of `Element`.
#[must_use]
pub fn defines(model: &dyn Model, ty: &FhirType, name: &str) -> bool {
    ty.schema()
        .or_else(|| model.schemas().type_named("Element"))
        .is_some_and(|schema| schema.fields.iter().any(|field| field.name == name))
}

/// Whether `name` is `ty` or a type `ty` specializes, as far as the model
/// knows the hierarchy.
#[must_use]
pub fn is_a(model: &dyn Model, ty: &FhirType, name: &str) -> bool {
    let mut current = Some(ty.name);
    let mut hops = 0_u8;
    while let Some(candidate) = current {
        if candidate == name {
            return true;
        }
        hops = hops.saturating_add(1);
        if hops > 16 {
            return false;
        }
        current = model.base(candidate);
    }
    false
}

/// Whether `ty` is `name` or a subclass of it for the `is` operator.
///
/// N1 §6.3.1 makes `is` true of a subclass, and a primitive specializes the
/// type its definition names: `code` is a `string`, and every primitive an
/// `Element` (R4 §2.9.1.2). [`is_a`] keeps every primitive independent, as R4 §2.9.1.5 asks of
/// `ofType()`.
#[must_use]
pub fn specializes(model: &dyn Model, ty: &FhirType, name: &str) -> bool {
    let mut current = Some(ty.name);
    let mut hops = 0_u8;
    while let Some(candidate) = current {
        if candidate == name {
            return true;
        }
        hops = hops.saturating_add(1);
        if hops > 16 {
            return false;
        }
        current = if model.primitives().contains_key(candidate) {
            model.primitive_base(candidate)
        } else {
            model.base(candidate)
        };
    }
    false
}
