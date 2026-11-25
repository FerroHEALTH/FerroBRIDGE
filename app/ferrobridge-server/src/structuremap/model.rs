// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The structures a map reads and writes: what an element of a type is,
//! from the FHIR R4 element table or from a logical model's
//! `StructureDefinition`.
//!
//! A logical model is a `StructureDefinition` with `kind = logical`
//! (FHIR R4 structuredefinition.html); its elements are read from the
//! snapshot, or the differential when it has none, and an element that
//! defines children (a `BackboneElement`, an `Element` or no type) is a type
//! of its own named by its element path.

use std::collections::BTreeMap;

use fhir_types::codec::Value;
use fhir_types::schema::Schemas;

/// What one element of a type is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementDef {
    /// The JSON member name the element is written as.
    pub member: String,
    /// Whether it repeats.
    pub many: bool,
    /// The type of a value of it, when it has exactly one.
    pub type_name: Option<String>,
}

/// The structures one map knows.
pub trait Structures: std::fmt::Debug + Send + Sync {
    /// Returns the element `element` of `type_name`, `None` when the type
    /// has no such element or is unknown.
    fn element(&self, type_name: &str, element: &str) -> Option<ElementDef>;

    /// Returns whether `type_name` is a resource type.
    fn is_resource(&self, type_name: &str) -> bool;

    /// Returns whether `type_name` is a primitive type.
    fn is_primitive(&self, type_name: &str) -> bool;
}

/// The FHIR R4 element table.
#[derive(Debug, Clone, Copy)]
pub struct R4;

impl Structures for R4 {
    fn element(&self, type_name: &str, element: &str) -> Option<ElementDef> {
        let schemas: &Schemas = &fhir_types::r4::schema::SCHEMAS;
        let schema = schemas.type_named(type_name)?;
        if let Some(field) = schema.fields.iter().find(|field| field.name == element) {
            let type_name = match field.types {
                [single] => Some(child_type(single, field.path)),
                _ => field.content_reference.map(str::to_owned),
            };
            return Some(ElementDef {
                member: element.to_owned(),
                many: field.many,
                type_name,
            });
        }
        // NOTE: R4 json.html §2.6.2.1; a choice element is written as its stem and its type's name.
        schema.fields.iter().find_map(|field| {
            let suffix = element.strip_prefix(field.name)?;
            let chosen = field.types.iter().find(|ty| upper_first(ty) == suffix)?;
            Some(ElementDef {
                member: element.to_owned(),
                many: field.many,
                type_name: Some((*chosen).to_owned()),
            })
        })
    }

    fn is_resource(&self, type_name: &str) -> bool {
        fhir_types::r4::schema::SCHEMAS.is_resource(type_name)
    }

    fn is_primitive(&self, type_name: &str) -> bool {
        fhir_types::r4::schema::SCHEMAS
            .primitive_base_of(type_name)
            .is_some()
    }
}

/// Returns the type a value of an element with the type code `code` at
/// `path` has: the element path for a backbone element.
fn child_type(code: &str, path: &str) -> String {
    if code == "BackboneElement" || code == "Element" {
        path.to_owned()
    } else {
        code.to_owned()
    }
}

/// Returns `text` with its first letter in upper case.
fn upper_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The FHIR primitive type codes, which a logical model may name by URL.
const PRIMITIVES: [&str; 20] = [
    "base64Binary",
    "boolean",
    "canonical",
    "code",
    "date",
    "dateTime",
    "decimal",
    "id",
    "instant",
    "integer",
    "markdown",
    "oid",
    "positiveInt",
    "string",
    "time",
    "unsignedInt",
    "uri",
    "url",
    "uuid",
    "xhtml",
];

/// One element of a logical model.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LogicalElement {
    many: bool,
    type_name: Option<String>,
}

/// Logical models, by the type name each defines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Logical {
    /// The elements by their path.
    elements: BTreeMap<String, LogicalElement>,
}

/// Why a `StructureDefinition` is no logical model the engine reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LogicalError {
    /// The resource is no `StructureDefinition` of `kind` `logical`.
    #[error("the resource is no logical StructureDefinition")]
    NotLogical,
    /// An element has no path.
    #[error("an element of the logical model has no path")]
    NoPath,
}

impl Logical {
    /// Adds the logical model `definition`.
    ///
    /// # Errors
    /// Returns [`LogicalError`] for a resource that is no logical
    /// `StructureDefinition` or has an element without a path.
    pub fn add(&mut self, definition: &Value) -> Result<(), LogicalError> {
        if definition.get("resourceType").and_then(Value::as_str) != Some("StructureDefinition")
            || definition.get("kind").and_then(Value::as_str) != Some("logical")
        {
            return Err(LogicalError::NotLogical);
        }
        let elements = definition
            .get("snapshot")
            .or_else(|| definition.get("differential"))
            .and_then(|view| view.get("element"))
            .and_then(Value::as_array)
            .ok_or(LogicalError::NotLogical)?;
        for element in elements {
            let path = element
                .get("path")
                .and_then(Value::as_str)
                .ok_or(LogicalError::NoPath)?;
            let many = element
                .get("max")
                .and_then(Value::as_str)
                .is_some_and(|max| max == "*" || max.parse::<u32>().is_ok_and(|max| max > 1));
            let codes: Vec<&str> = element
                .get("type")
                .and_then(Value::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(|ty| ty.get("code").and_then(Value::as_str))
                .map(|code| code.rsplit('/').next().unwrap_or(code))
                .collect();
            let type_name = match codes.as_slice() {
                [] | ["BackboneElement" | "Element"] => Some(path.to_owned()),
                [single] => Some((*single).to_owned()),
                _ => None,
            };
            self.elements
                .insert(path.to_owned(), LogicalElement { many, type_name });
        }
        Ok(())
    }
}

impl Structures for Logical {
    fn element(&self, type_name: &str, element: &str) -> Option<ElementDef> {
        let found = self.elements.get(&format!("{type_name}.{element}"))?;
        Some(ElementDef {
            member: element.to_owned(),
            many: found.many,
            type_name: found.type_name.clone(),
        })
    }

    fn is_resource(&self, _type_name: &str) -> bool {
        false
    }

    fn is_primitive(&self, type_name: &str) -> bool {
        PRIMITIVES.contains(&type_name)
    }
}

/// The logical models a map uses, then the FHIR R4 table.
#[derive(Debug, Clone, Default)]
pub struct Known {
    /// The logical models.
    pub logical: Logical,
}

impl Structures for Known {
    fn element(&self, type_name: &str, element: &str) -> Option<ElementDef> {
        self.logical
            .element(type_name, element)
            .or_else(|| R4.element(type_name, element))
    }

    fn is_resource(&self, type_name: &str) -> bool {
        R4.is_resource(type_name)
    }

    fn is_primitive(&self, type_name: &str) -> bool {
        R4.is_primitive(type_name) || PRIMITIVES.contains(&type_name)
    }
}
