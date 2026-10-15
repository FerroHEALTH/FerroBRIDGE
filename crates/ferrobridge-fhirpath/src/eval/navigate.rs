// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Path navigation (N1 §3) and type specifiers (N1 §6.3, §10.1).

use std::collections::BTreeSet;

use super::Engine;
use super::error::EvalError;
use crate::ast::TypeSpecifier;
use crate::model::{FhirType, is_a, named_type};
use crate::value::{Collection, Item, SYSTEM_TYPES};

/// A resolved type specifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeRef {
    /// A type of the `System` namespace.
    System(&'static str),
    /// A type of the FHIR model.
    Fhir(FhirType),
}

impl<'a> Engine<'_, 'a> {
    /// The member `name` of every item of `items`.
    ///
    /// An identifier at the start of a path that names the type of the item
    /// (or a supertype) is the item itself (N1 §3, §8.6). A member that no
    /// typed item's type defines is an error, and so is a type name that is
    /// not the item's type, while system values, untyped nodes and an empty
    /// input answer an empty collection.
    pub(crate) fn navigate(
        &self,
        items: &Collection<'a>,
        name: &str,
        at_root: bool,
    ) -> Result<Collection<'a>, EvalError> {
        let model = self.model();
        let mut found = Vec::new();
        let mut defined = false;
        let mut lacking = BTreeSet::new();
        for item in items {
            match item {
                Item::Node(node) => {
                    if at_root
                        && let Some(ty) = node.fhir_type()
                        && !ty.is_primitive()
                        && is_a(model, ty, name)
                    {
                        found.push(item.clone());
                        defined = true;
                        continue;
                    }
                    match node.member(model, name)? {
                        Some(members) => {
                            defined = true;
                            found.extend(members);
                        }
                        None => {
                            lacking.insert(node.fhir_type().map_or("(untyped)", |ty| ty.name));
                        }
                    }
                }
                Item::Type(info) => {
                    defined = true;
                    match name {
                        "namespace" => found.push(Item::String(String::from(info.namespace))),
                        "name" => found.extend(info.name.clone().map(Item::String)),
                        _ => {}
                    }
                }
                // NOTE: N1 §11 lists children(), descendants() and polymorphic members as
                // untypable, and they yield system values; a member of one is empty, not an error.
                _ => defined = true,
            }
        }
        if !defined && !lacking.is_empty() {
            let types = lacking.into_iter().collect::<Vec<_>>().join(" | ");
            if at_root && named_type(model, name).is_some() {
                return Err(EvalError::ContextType {
                    named: String::from(name),
                    context: types,
                });
            }
            return Err(EvalError::UnknownElement {
                type_name: types,
                element: String::from(name),
            });
        }
        Ok(found)
    }

    /// Resolves a type specifier: the FHIR model first, then `System` (N1
    /// §10.1).
    pub(crate) fn resolve_type(&self, specifier: &TypeSpecifier) -> Result<TypeRef, EvalError> {
        let model = self.model();
        let system = || {
            SYSTEM_TYPES
                .iter()
                .find(|name| **name == specifier.name)
                .map(|name| TypeRef::System(name))
        };
        let fhir = || named_type(model, &specifier.name).map(TypeRef::Fhir);
        let resolved = match specifier.namespace.as_deref() {
            None => fhir().or_else(system),
            Some("FHIR") => fhir(),
            Some("System") => system(),
            Some(_) => None,
        };
        resolved.ok_or_else(|| EvalError::UnknownType {
            name: specifier.to_string(),
        })
    }

    /// Whether `item` is of the type `ty` or a subtype (N1 §6.3.1): a FHIR
    /// primitive is not of a system type (FHIR R4 §2.9.1.2).
    pub(crate) fn is_type(&self, item: &Item<'a>, ty: &TypeRef) -> bool {
        match (item, ty) {
            (_, TypeRef::System("Any")) => true,
            (Item::Node(_) | Item::Type(_), TypeRef::System(_)) => false,
            (other, TypeRef::System(name)) => other.system_type() == Some(*name),
            (Item::Node(node), TypeRef::Fhir(fhir)) => node
                .fhir_type()
                .is_some_and(|own| is_a(self.model(), own, fhir.name)),
            (_, TypeRef::Fhir(_)) => false,
        }
    }
}
