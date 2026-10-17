// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Resolving a `StructureDefinition` snapshot into a typed element list.
//!
//! The snapshot is the fully-resolved element list of a structure
//! (<https://hl7.org/fhir/R4B/structuredefinition.html#snapshot>). Resolution
//! turns each `ElementDefinition` into a [`ResolvedElement`] with typed
//! cardinality, a normalized type list, choice-type detection (a path ending
//! in `[x]`, <https://hl7.org/fhir/R4B/formats.html#choice>), and content
//! references pointed at the element they name.

use crate::fhir::{Binding, ElementDefinition, ElementType, StructureDefinition, StructureKind};

/// The prefix of the `FHIRPath` system types primitives carry as their value type.
const FHIRPATH_SYSTEM_PREFIX: &str = "http://hl7.org/fhirpath/System.";

/// The extension naming the FHIR type behind a `FHIRPath` system type.
const FHIR_TYPE_EXTENSION: &str =
    "http://hl7.org/fhir/StructureDefinition/structuredefinition-fhir-type";

/// The path of the id every element inherits from `Element`.
const ELEMENT_ID: &str = "Element.id";

/// The path of the logical id every resource inherits from `Resource`.
const RESOURCE_ID: &str = "Resource.id";

/// The extension carrying the lexical form of a primitive's value, from R4.
const REGEX_EXTENSION: &str = "http://hl7.org/fhir/StructureDefinition/regex";

/// The extension carrying the lexical form of a primitive's value in STU3.
const STU3_REGEX_EXTENSION: &str =
    "http://hl7.org/fhir/StructureDefinition/structuredefinition-regex";

/// The `_code` extension naming the JSON type of a type without a code.
const JSON_TYPE_EXTENSION: &str =
    "http://hl7.org/fhir/StructureDefinition/structuredefinition-json-type";

/// The FHIR primitives an XML attribute carries as a JSON string.
const STRING_ATTRIBUTE_TYPES: [&str; 4] = ["code", "id", "string", "uri"];

/// A failure while resolving a snapshot.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// The structure states no type, so its root element cannot be named.
    #[error("{url} states no type")]
    NoType {
        /// The structure's canonical URL.
        url: String,
    },
    /// The structure has no snapshot to resolve.
    #[error("{url} has no snapshot")]
    NoSnapshot {
        /// The structure's canonical URL.
        url: String,
    },
    /// An element lacks `min` or `max`.
    #[error("{path} in {url} has no cardinality")]
    MissingCardinality {
        /// The structure's canonical URL.
        url: String,
        /// The element path.
        path: String,
    },
    /// An element's `max` is neither a number nor `*`.
    #[error("{path} in {url} has an invalid max cardinality {max:?}")]
    InvalidMax {
        /// The structure's canonical URL.
        url: String,
        /// The element path.
        path: String,
        /// The offending value.
        max: String,
    },
    /// An element carries neither a type nor a content reference (and is not the root).
    #[error("{path} in {url} has no type and no contentReference")]
    Untyped {
        /// The structure's canonical URL.
        url: String,
        /// The element path.
        path: String,
    },
    /// A type entry carries neither a code nor a `structuredefinition-json-type`.
    #[error("a type of {path} in {url} has neither a code nor a structuredefinition-json-type")]
    UntypedType {
        /// The structure's canonical URL.
        url: String,
        /// The element path.
        path: String,
    },
    /// An XML attribute is typed with a primitive whose JSON form is no string.
    #[error("{path} in {url} is an XML attribute typed {code}, which has no JSON member form")]
    AttributeType {
        /// The structure's canonical URL.
        url: String,
        /// The element path.
        path: String,
        /// The type code.
        code: String,
    },
    /// A content reference names an element the snapshot does not contain.
    #[error("{path} in {url} references {target}, which is not in the snapshot")]
    DanglingReference {
        /// The structure's canonical URL.
        url: String,
        /// The referencing element path.
        path: String,
        /// The referenced path.
        target: String,
    },
}

/// A maximum cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Max {
    /// At most this many.
    Bounded(u32),
    /// Unbounded (`*`).
    Unbounded,
}

impl Max {
    /// Whether more than one value is allowed.
    #[must_use]
    pub fn is_repeating(self) -> bool {
        match self {
            Self::Bounded(n) => n > 1,
            Self::Unbounded => true,
        }
    }
}

/// A normalized type reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    /// The FHIR type name, for example `string`, `Coding`, or `BackboneElement`.
    pub code: String,
    /// The `FHIRPath` system type a primitive's value element carries, if any.
    pub fhirpath_type: Option<String>,
    /// The JSON type a value carries where the package states one in place of
    /// a type code (STU3), or where an XML attribute becomes a JSON member value.
    pub json_type: Option<String>,
    /// Profiles the value must conform to.
    pub profiles: Vec<String>,
    /// Profiles a `Reference` or `canonical` target must conform to.
    pub target_profiles: Vec<String>,
    /// The lexical form the value keeps, as the `regex` extension (R4 and
    /// later) or the `structuredefinition-regex` extension (STU3) spells it
    /// (<https://hl7.org/fhir/R5/datatypes.html#primitive>).
    pub regex: Option<String>,
}

impl TypeRef {
    /// Whether the value is a JSON scalar the generator maps itself, so no
    /// FHIR type of the package backs it.
    #[must_use]
    pub fn is_scalar(&self) -> bool {
        self.fhirpath_type.is_some() || self.json_type.is_some()
    }

    /// The type reference of one `ElementDefinition.type` entry of `element`.
    fn from_element_type(
        url: &str,
        element: &ElementDefinition,
        raw: &ElementType,
    ) -> Result<Self, ResolveError> {
        let regex = raw
            .extension
            .iter()
            .find(|extension| {
                extension.url == REGEX_EXTENSION || extension.url == STU3_REGEX_EXTENSION
            })
            .and_then(|extension| extension.value_string.clone());
        let Some(code) = raw.code.as_deref() else {
            return Self::from_json_type(url, element, raw, regex);
        };
        let system = code.strip_prefix(FHIRPATH_SYSTEM_PREFIX);
        let fhir_type = raw
            .extension
            .iter()
            .find(|extension| extension.url == FHIR_TYPE_EXTENSION)
            .and_then(|extension| extension.value_url.clone());
        let (code, fhirpath_type) = match (system, fhir_type) {
            (Some(system), Some(fhir_type)) => (fhir_type, Some(system.to_owned())),
            // NOTE: no spec names a FHIR type for a bare System.* value type; keep the
            // `FHIRPath` name as the code so the emitter sees exactly what the package says.
            (Some(system), None) => (code.to_owned(), Some(system.to_owned())),
            (None, _) => (code.to_owned(), None),
        };
        let code = if code == "id" && is_element_id(element) {
            // NOTE: R4B element-definitions.html and R5 types-definitions.html type Element.id
            // as string, against the `id` the 4.3.0 and 5.0.0 snapshots state (#672).
            String::from("string")
        } else if code == "string" && is_resource_id(element) {
            // NOTE: R4 resource-definitions.html#Resource.id types Resource.id as id, against
            // the `string` the 4.0.1 package's fhir-type extension states (#798).
            String::from("id")
        } else {
            code
        };
        let json_type = if fhirpath_type.is_none() && is_attribute(element) {
            // NOTE: https://hl7.org/fhir/STU3/json.html: "Composites can have id attributes,
            // which are converted to JSON member values", so an attribute is a plain string.
            if !STRING_ATTRIBUTE_TYPES.contains(&code.as_str()) {
                return Err(ResolveError::AttributeType {
                    url: url.to_owned(),
                    path: element.path.clone(),
                    code,
                });
            }
            Some(String::from("string"))
        } else {
            None
        };
        Ok(Self {
            code,
            fhirpath_type,
            json_type,
            profiles: raw.profile.clone(),
            target_profiles: raw.target_profile.clone(),
            regex,
        })
    }

    /// The type reference of an entry the package types by its `_code`
    /// `structuredefinition-json-type` extension alone.
    ///
    /// A primitive's `value` takes the primitive's own name as its code, the
    /// name R4 states in its `structuredefinition-fhir-type` extension; any
    /// other element takes the JSON type.
    fn from_json_type(
        url: &str,
        element: &ElementDefinition,
        raw: &ElementType,
        regex: Option<String>,
    ) -> Result<Self, ResolveError> {
        let json_type = raw
            .code_element
            .iter()
            .flat_map(|code| &code.extension)
            .find(|extension| extension.url == JSON_TYPE_EXTENSION)
            .and_then(|extension| extension.value_string.clone())
            .ok_or_else(|| ResolveError::UntypedType {
                url: url.to_owned(),
                path: element.path.clone(),
            })?;
        let code = element
            .path
            .strip_suffix(".value")
            .filter(|owner| !owner.contains('.'))
            .map_or_else(|| json_type.clone(), str::to_owned);
        Ok(Self {
            code,
            fhirpath_type: None,
            json_type: Some(json_type),
            profiles: raw.profile.clone(),
            target_profiles: raw.target_profile.clone(),
            regex,
        })
    }

    /// Folds `other`, an entry with the same code, into this one: the union
    /// of the profiles, each kept once in the order first stated.
    fn merge(&mut self, other: Self) {
        for profile in other.profiles {
            if !self.profiles.contains(&profile) {
                self.profiles.push(profile);
            }
        }
        for profile in other.target_profiles {
            if !self.target_profiles.contains(&profile) {
                self.target_profiles.push(profile);
            }
        }
    }
}

/// Whether `element` is `Element.id` or an element inheriting it.
fn is_element_id(element: &ElementDefinition) -> bool {
    element.path == ELEMENT_ID
        || element
            .base
            .as_ref()
            .is_some_and(|base| base.path == ELEMENT_ID)
}

/// Whether `element` is `Resource.id` or an element inheriting it, the
/// logical id of a resource (<https://hl7.org/fhir/R4/resource.html#id>).
fn is_resource_id(element: &ElementDefinition) -> bool {
    element.path == RESOURCE_ID
        || element
            .base
            .as_ref()
            .is_some_and(|base| base.path == RESOURCE_ID)
}

/// Whether `element` is an XML attribute other than a primitive's value.
fn is_attribute(element: &ElementDefinition) -> bool {
    element.representation.iter().any(|r| r == "xmlAttr")
        && element
            .path
            .rsplit_once('.')
            .is_some_and(|(_, last)| last != "value")
}

/// The type references of `element`, one per type code.
///
/// STU3 states one target profile per type entry, so `Observation.subject`
/// lists `Reference` four times (<https://hl7.org/fhir/STU3/elementdefinition.html>);
/// the entries of one code merge into one carrying every profile, the shape
/// R4 states directly.
fn type_refs(url: &str, element: &ElementDefinition) -> Result<Vec<TypeRef>, ResolveError> {
    let mut types: Vec<TypeRef> = Vec::with_capacity(element.types.len());
    for raw in &element.types {
        let type_ref = TypeRef::from_element_type(url, element, raw)?;
        match types.iter_mut().find(|known| known.code == type_ref.code) {
            Some(known) => known.merge(type_ref),
            None => types.push(type_ref),
        }
    }
    Ok(types)
}
/// What an element holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementShape {
    /// The structure's root element, which has no type of its own.
    Root,
    /// A single-typed element; the list holds one entry per allowed profile set.
    Typed(Vec<TypeRef>),
    /// A choice element (`name[x]`) that takes exactly one of these types.
    Choice(Vec<TypeRef>),
    /// An element whose children are those of another element
    /// (<https://hl7.org/fhir/R4B/elementdefinition-definitions.html#ElementDefinition.contentReference>).
    ContentReference {
        /// The structure the referenced element lives in; `None` means this structure.
        structure: Option<String>,
        /// The referenced element path.
        path: String,
    },
}

/// One resolved snapshot element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedElement {
    /// The element id when the package provides one.
    pub id: Option<String>,
    /// The dotted path from the root type.
    pub path: String,
    /// The minimum cardinality.
    pub min: u32,
    /// The maximum cardinality.
    pub max: Max,
    /// What the element holds.
    pub shape: ElementShape,
    /// The terminology binding for coded elements.
    pub binding: Option<Binding>,
    /// The short description.
    pub short: Option<String>,
    /// The formal definition.
    pub definition: Option<String>,
    /// Whether the element modifies the meaning of its parent.
    pub is_modifier: bool,
    /// Whether the element is part of the summary view.
    pub is_summary: bool,
}

impl ResolvedElement {
    /// The last path segment: the element's own name, with `[x]` kept for choices.
    #[must_use]
    pub fn name(&self) -> &str {
        self.path.rsplit('.').next().unwrap_or(&self.path)
    }

    /// The choice element's name without its `[x]` suffix, or `None` for other shapes.
    #[must_use]
    pub fn choice_stem(&self) -> Option<&str> {
        match self.shape {
            ElementShape::Choice(_) => self.name().strip_suffix("[x]"),
            _ => None,
        }
    }

    /// Whether this element is a direct child of `parent_path`.
    #[must_use]
    pub fn is_child_of(&self, parent_path: &str) -> bool {
        self.path
            .strip_prefix(parent_path)
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(|rest| !rest.contains('.'))
    }
}

/// A structure with its snapshot resolved.
#[derive(Debug, Clone)]
pub struct ResolvedStructure {
    /// The canonical URL.
    pub url: String,
    /// The type name.
    pub name: String,
    /// The kind of structure.
    pub kind: StructureKind,
    /// Whether the type is abstract.
    pub is_abstract: bool,
    /// The canonical URL of the base definition.
    pub base_definition: Option<String>,
    /// The resolved elements, in snapshot order; the first is the root.
    pub elements: Vec<ResolvedElement>,
}

impl ResolvedStructure {
    /// Resolves the snapshot of `definition`.
    ///
    /// # Errors
    ///
    /// Returns a [`ResolveError`] when the structure states no type, the snapshot is missing, an element has
    /// no cardinality or an invalid `max`, a non-root element has neither a
    /// type nor a content reference, or a content reference dangles.
    pub fn resolve(definition: &StructureDefinition) -> Result<Self, ResolveError> {
        let snapshot = definition
            .snapshot
            .as_ref()
            .ok_or_else(|| ResolveError::NoSnapshot {
                url: definition.url.clone(),
            })?;
        let root_path = definition
            .type_name
            .as_deref()
            .ok_or_else(|| ResolveError::NoType {
                url: definition.url.clone(),
            })?;
        let mut elements = Vec::with_capacity(snapshot.element.len());
        for (index, element) in snapshot.element.iter().enumerate() {
            elements.push(resolve_element(
                &definition.url,
                element,
                index == 0,
                root_path,
            )?);
        }
        for element in &elements {
            if let ElementShape::ContentReference {
                structure: None,
                path: target,
            } = &element.shape
                && !elements.iter().any(|candidate| &candidate.path == target)
            {
                return Err(ResolveError::DanglingReference {
                    url: definition.url.clone(),
                    path: element.path.clone(),
                    target: target.clone(),
                });
            }
        }
        Ok(Self {
            url: definition.url.clone(),
            name: definition.name.clone(),
            kind: definition.kind,
            is_abstract: definition.is_abstract,
            base_definition: definition.base_definition.clone(),
            elements,
        })
    }

    /// The element at `path`, if the snapshot has one.
    #[must_use]
    pub fn element(&self, path: &str) -> Option<&ResolvedElement> {
        self.elements.iter().find(|element| element.path == path)
    }

    /// The direct children of the element at `parent_path`, in snapshot order.
    pub fn children_of<'a>(
        &'a self,
        parent_path: &'a str,
    ) -> impl Iterator<Item = &'a ResolvedElement> + 'a {
        self.elements
            .iter()
            .filter(move |element| element.is_child_of(parent_path))
    }
}

fn resolve_element(
    url: &str,
    element: &ElementDefinition,
    is_first: bool,
    root_path: &str,
) -> Result<ResolvedElement, ResolveError> {
    let (Some(min), Some(max)) = (element.min, element.max.as_deref()) else {
        return Err(ResolveError::MissingCardinality {
            url: url.to_owned(),
            path: element.path.clone(),
        });
    };
    let max = parse_max(max).ok_or_else(|| ResolveError::InvalidMax {
        url: url.to_owned(),
        path: element.path.clone(),
        max: max.to_owned(),
    })?;
    let is_root = is_first && element.path == root_path;
    let types = type_refs(url, element)?;
    let shape = match (&element.content_reference, types.is_empty(), is_root) {
        (_, _, true) => ElementShape::Root,
        (Some(reference), _, false) => content_reference(url, reference),
        (None, true, false) => {
            return Err(ResolveError::Untyped {
                url: url.to_owned(),
                path: element.path.clone(),
            });
        }
        (None, false, false) if element.path.ends_with("[x]") => ElementShape::Choice(types),
        (None, false, false) => ElementShape::Typed(types),
    };
    Ok(ResolvedElement {
        id: element.id.clone(),
        path: element.path.clone(),
        min,
        max,
        shape,
        binding: element.binding.clone(),
        short: element.short.clone(),
        definition: element.definition.clone(),
        is_modifier: element.is_modifier.unwrap_or(false),
        is_summary: element.is_summary.unwrap_or(false),
    })
}

/// Splits a `contentReference` into its structure URL and element path.
///
/// A reference is `#path` (this structure) or `url#path` (another structure);
/// a reference to this structure's own URL is local.
fn content_reference(own_url: &str, reference: &str) -> ElementShape {
    let (structure, path) = match reference.split_once('#') {
        Some((structure, path)) => (structure, path),
        None => ("", reference),
    };
    let structure = (!structure.is_empty() && structure != own_url).then(|| structure.to_owned());
    ElementShape::ContentReference {
        structure,
        path: path.to_owned(),
    }
}

fn parse_max(max: &str) -> Option<Max> {
    if max == "*" {
        Some(Max::Unbounded)
    } else {
        max.parse().ok().map(Max::Bounded)
    }
}

#[cfg(test)]
mod tests {
    use super::{Max, parse_max};

    #[test]
    fn max_parses_star_and_numbers() {
        assert_eq!(parse_max("*"), Some(Max::Unbounded));
        assert_eq!(parse_max("1"), Some(Max::Bounded(1)));
        assert_eq!(parse_max("12"), Some(Max::Bounded(12)));
    }

    #[test]
    fn max_rejects_garbage() {
        assert_eq!(parse_max(""), None);
        assert_eq!(parse_max("-1"), None);
        assert_eq!(parse_max("many"), None);
    }

    #[test]
    fn repeating_is_more_than_one() {
        assert!(!Max::Bounded(0).is_repeating());
        assert!(!Max::Bounded(1).is_repeating());
        assert!(Max::Bounded(2).is_repeating());
        assert!(Max::Unbounded.is_repeating());
    }
}
