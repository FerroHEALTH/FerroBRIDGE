// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The root-set closure: every type the root resources reach.
//!
//! Starting from the root resources, the closure follows every element type
//! and every content reference to the `StructureDefinition` that defines it,
//! transitively, so the emitted module holds the complete set of datatypes
//! and primitives the root set can carry, and nothing else (`codegen.md`:
//! complete within a declared closure). Each type also records the narrowest
//! root set that reaches it, which is the feature the emitted type is gated
//! behind.

use std::collections::{BTreeMap, BTreeSet};

use crate::fhir::{Derivation, StructureDefinition, StructureKind};
use crate::package::Package;
use crate::roots::{RootScope, RootSet};
use crate::snapshot::{ElementShape, ResolveError, ResolvedElement, ResolvedStructure};

/// Type codes that name a structural base rather than a datatype to emit.
///
/// `Element` and `BackboneElement` mark nested structures, which are emitted
/// as part of their parent; `Resource` is the abstract resource type, emitted
/// as the `Resource` enum over the root set.
pub const STRUCTURAL_TYPES: [&str; 3] = ["BackboneElement", "Element", "Resource"];

/// The base type of every element, resolved for the element table alone.
///
/// The FHIR JSON representation carries a primitive's `id` and `extension` in
/// a sibling member named with a leading underscore
/// (<https://hl7.org/fhir/R4/json.html>), and that member's shape is `Element`
/// (<https://hl7.org/fhir/R4/element.html>). No emitted field targets the
/// type, so it stays a structural type for the closure and joins the model as
/// a table-only entry ([`crate::lower::TypeDef::table_only`]).
pub const ELEMENT_TYPE: &str = "Element";

/// A failure while computing the closure.
#[derive(Debug, thiserror::Error)]
pub enum ClosureError {
    /// An element names a type the package does not define.
    #[error("{path} has type {code}, which the package does not define")]
    UnknownType {
        /// The element path.
        path: String,
        /// The type code.
        code: String,
    },
    /// An element names a type whose definition is a profile, not a type.
    #[error("{path} has type {code}, whose definition is a constraint profile, not a type")]
    ProfileAsType {
        /// The element path.
        path: String,
        /// The type code.
        code: String,
    },
    /// A snapshot failed to resolve.
    #[error(transparent)]
    Resolve(#[from] ResolveError),
}

/// The resolved structures of the root set and everything they reference.
#[derive(Debug)]
pub struct TypeClosure {
    structures: BTreeMap<String, ResolvedStructure>,
    element: ResolvedStructure,
    bases: BTreeMap<String, ResolvedStructure>,
    roots: BTreeSet<String>,
    scopes: BTreeMap<String, RootScope>,
}

impl TypeClosure {
    /// Computes the closure of `roots` over `package`.
    ///
    /// Every structure records the narrowest scope of `roots` that reaches it,
    /// so a datatype the terminology set already carries stays terminology
    /// when the wide resource set reaches it too.
    ///
    /// # Errors
    ///
    /// Returns [`ClosureError`] when an element names an undefined type or a
    /// profile, or when a snapshot does not resolve.
    pub fn compute(package: &Package, roots: &RootSet<'_>) -> Result<Self, ClosureError> {
        let structures = walk(package, roots.resources.values().copied())?;
        let terminology: BTreeSet<String> = if roots.holds(RootScope::Resources) {
            walk(package, roots.in_scope(RootScope::Terminology))?
                .into_keys()
                .collect()
        } else {
            structures.keys().cloned().collect()
        };
        let mut scopes: BTreeMap<String, RootScope> = structures
            .keys()
            .map(|name| {
                let scope = if terminology.contains(name) {
                    RootScope::Terminology
                } else {
                    RootScope::Resources
                };
                (name.clone(), scope)
            })
            .collect();
        let element = resolve_named(package, ELEMENT_TYPE, "(the element table)")?;
        let bases = base_chains(package, &structures, &element, &mut scopes)?;
        Ok(Self {
            structures,
            element,
            bases,
            roots: roots
                .resources
                .values()
                .map(|definition| definition.name.clone())
                .collect(),
            scopes,
        })
    }

    /// Every structure in the closure, keyed by type name, in name order.
    #[must_use]
    pub fn structures(&self) -> &BTreeMap<String, ResolvedStructure> {
        &self.structures
    }

    /// The base `Element` structure, for the table-only entry.
    #[must_use]
    pub fn element(&self) -> &ResolvedStructure {
        &self.element
    }

    /// The abstract structures the closure's `baseDefinition` chains name and
    /// the closure does not otherwise hold, keyed by type name: `Resource`,
    /// `DomainResource` and `BackboneElement`, and from R5 `Base`, `DataType`
    /// and `BackboneType` (<https://hl7.org/fhir/R5/types.html>), for the
    /// table-only entries that let a consumer walk a type's base chain to its
    /// root.
    ///
    /// A primitive's chain is not followed, because the element table carries
    /// no primitive.
    #[must_use]
    pub fn bases(&self) -> &BTreeMap<String, ResolvedStructure> {
        &self.bases
    }

    /// The names of the root resources.
    #[must_use]
    pub fn roots(&self) -> &BTreeSet<String> {
        &self.roots
    }

    /// The narrowest root set that reaches the structure named `name`.
    #[must_use]
    pub fn scope(&self, name: &str) -> Option<RootScope> {
        self.scopes.get(name).copied()
    }

    /// The structures of one kind, in name order.
    pub fn of_kind(&self, kind: StructureKind) -> impl Iterator<Item = &ResolvedStructure> {
        self.structures
            .values()
            .filter(move |structure| structure.kind == kind)
    }
}

/// Resolves `roots` and everything they reference, transitively.
fn walk<'a>(
    package: &Package,
    roots: impl Iterator<Item = &'a StructureDefinition>,
) -> Result<BTreeMap<String, ResolvedStructure>, ClosureError> {
    let mut structures = BTreeMap::new();
    let mut pending: Vec<(String, String)> = roots
        .map(|definition| (definition.name.clone(), String::from("(root)")))
        .collect();
    while let Some((code, referrer)) = pending.pop() {
        if structures.contains_key(&code) || STRUCTURAL_TYPES.contains(&code.as_str()) {
            continue;
        }
        let resolved = resolve_named(package, &code, &referrer)?;
        if resolved.kind != StructureKind::PrimitiveType {
            for element in &resolved.elements {
                referenced(package, element, &mut pending);
            }
        }
        structures.insert(code, resolved);
    }
    Ok(structures)
}

/// The abstract bases of the closure, following every `baseDefinition` chain
/// of a non-primitive structure, of `element`, and of `BackboneElement` when an
/// element is typed with it, to its root.
///
/// Each base records in `scopes` the narrowest scope of a type that reaches
/// it, so its entry carries the feature of its narrowest specialization.
///
/// # Errors
///
/// Returns [`ClosureError`] when a chain names a structure the package does
/// not define or a profile, or when a snapshot does not resolve.
fn base_chains(
    package: &Package,
    structures: &BTreeMap<String, ResolvedStructure>,
    element: &ResolvedStructure,
    scopes: &mut BTreeMap<String, RootScope>,
) -> Result<BTreeMap<String, ResolvedStructure>, ClosureError> {
    let lookup = |url: &str, referrer: &str| {
        package
            .structure_definitions()
            .get(url)
            .ok_or_else(|| ClosureError::UnknownType {
                path: referrer.to_owned(),
                code: url.to_owned(),
            })
    };
    let mut pending: Vec<(&StructureDefinition, String, RootScope)> = Vec::new();
    let mut backbone: Option<RootScope> = None;
    for structure in structures
        .values()
        .filter(|structure| structure.kind != StructureKind::PrimitiveType)
    {
        let scope = scopes
            .get(&structure.name)
            .copied()
            .unwrap_or(RootScope::Resources);
        if let Some(url) = &structure.base_definition {
            pending.push((lookup(url, &structure.name)?, structure.name.clone(), scope));
        }
        if structure.elements.iter().any(names_backbone_element) {
            backbone = Some(backbone.map_or(scope, |held| held.min(scope)));
        }
    }
    if let Some(url) = &element.base_definition {
        pending.push((
            lookup(url, ELEMENT_TYPE)?,
            ELEMENT_TYPE.to_owned(),
            RootScope::Terminology,
        ));
    }
    if let Some(scope) = backbone {
        let definition = package
            .structure_definition_named(BACKBONE_ELEMENT)
            .ok_or_else(|| ClosureError::UnknownType {
                path: String::from("(an element typed BackboneElement)"),
                code: BACKBONE_ELEMENT.to_owned(),
            })?;
        pending.push((definition, BACKBONE_ELEMENT.to_owned(), scope));
    }
    let mut bases: BTreeMap<String, ResolvedStructure> = BTreeMap::new();
    while let Some((definition, referrer, scope)) = pending.pop() {
        let name = &definition.name;
        if structures.contains_key(name) || name == ELEMENT_TYPE {
            continue;
        }
        if scopes.get(name).is_some_and(|held| *held <= scope) {
            continue;
        }
        scopes.insert(name.clone(), scope);
        let next = if let Some(resolved) = bases.get(name) {
            resolved.base_definition.clone()
        } else {
            if definition.derivation == Some(Derivation::Constraint) {
                return Err(ClosureError::ProfileAsType {
                    path: referrer,
                    code: name.clone(),
                });
            }
            let resolved = ResolvedStructure::resolve(definition)?;
            let next = resolved.base_definition.clone();
            bases.insert(name.clone(), resolved);
            next
        };
        if let Some(url) = next {
            pending.push((lookup(&url, name)?, name.clone(), scope));
        }
    }
    Ok(bases)
}

/// The base of every backbone element (<https://hl7.org/fhir/R4/backboneelement.html>).
const BACKBONE_ELEMENT: &str = "BackboneElement";

/// Whether `element` is typed `BackboneElement`.
fn names_backbone_element(element: &ResolvedElement) -> bool {
    matches!(&element.shape, ElementShape::Typed(types) if types.iter().any(|t| t.code == BACKBONE_ELEMENT))
}

/// The structure `code` names, refused when it is undefined or is a profile.
///
/// `referrer` is the element path that named it, so a failure says where the
/// reference came from.
fn resolve_named(
    package: &Package,
    code: &str,
    referrer: &str,
) -> Result<ResolvedStructure, ClosureError> {
    let definition =
        package
            .structure_definition_named(code)
            .ok_or_else(|| ClosureError::UnknownType {
                path: referrer.to_owned(),
                code: code.to_owned(),
            })?;
    if definition.derivation == Some(Derivation::Constraint) {
        return Err(ClosureError::ProfileAsType {
            path: referrer.to_owned(),
            code: code.to_owned(),
        });
    }
    Ok(ResolvedStructure::resolve(definition)?)
}

/// Queues every type `element` names that the closure has still to resolve.
///
/// A `FHIRPath` type is a primitive the generator maps itself, and a content
/// reference names its target structure by URL.
fn referenced(package: &Package, element: &ResolvedElement, pending: &mut Vec<(String, String)>) {
    match &element.shape {
        ElementShape::Typed(types) | ElementShape::Choice(types) => {
            for type_ref in types {
                if !type_ref.is_scalar() {
                    pending.push((type_ref.code.clone(), element.path.clone()));
                }
            }
        }
        ElementShape::ContentReference {
            structure: Some(url),
            ..
        } => {
            if let Some(target) = package.structure_definitions().get(url) {
                pending.push((target.name.clone(), element.path.clone()));
            }
        }
        ElementShape::ContentReference { .. } | ElementShape::Root => {}
    }
}
