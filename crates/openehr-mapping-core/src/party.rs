// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The build of a canonical demographic `PARTY` from node values.
//!
//! Simplified Formats defines FLAT for compositions only, and `openehr-sdt`
//! 0.0.87 has no party builder: its composition builder writes
//! `COMPOSITION.category` onto a party root and builds `PARTY.identities` as
//! one object. So a party is built here, through the `openehr-rm` types. Each
//! value is a whole canonical value at a template node; the objects between
//! the root and the node are the ones the operational template constrains
//! there, every list attribute of the `openehr_rm::v1_2::model` attribute table
//! is an array, and the result is decoded strictly as the root's class before
//! it is returned. No composition attribute is written. No specification
//! governs a party build: our own design.

use openehr_its::json::JsonParseError;
use openehr_rm::v1_2::demographic::organisation::Organisation;
use openehr_rm::v1_2::demographic::person::Person;
use openehr_rm::v1_2::demographic::role::Role;
use openehr_rm::v1_2::model::Container;
use openehr_rm::v1_2::paths::PathSegment;
use serde_json::Map;
use serde_json::Value;

use crate::composition::CanonicalDocument;
use crate::composition::NodeValue;
use crate::composition::Place;
use crate::index::WebTemplateIndex;
use crate::index::paths::AqlPath;
use crate::template::PathError;

/// The member that names an object's RM class in canonical JSON.
const TYPE: &str = "_type";

/// The RM release a built party names in `ARCHETYPED.rm_version`.
///
/// RM Common IM Release 1.1.0 §3.2.3 defines `rm_version` as the "Version of
/// the openEHR reference model used to create this object", and a party this
/// module builds holds nothing the development line changed: the one
/// demographic difference, `PARTY.reverse_relationships`, is an attribute it
/// never writes.
const RM_RELEASE: &str = "1.1.0";

/// The demographic class a party is rooted at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PartyClass {
    /// `PERSON`.
    Person,
    /// `ORGANISATION`.
    Organisation,
    /// `ROLE`.
    Role,
}

impl PartyClass {
    /// Returns the class an RM type name names, when it names one of the
    /// three.
    #[must_use]
    pub fn from_rm_type(rm_type: &str) -> Option<Self> {
        match rm_type {
            "PERSON" => Some(Self::Person),
            "ORGANISATION" => Some(Self::Organisation),
            "ROLE" => Some(Self::Role),
            _ => None,
        }
    }

    /// Returns the RM type name of the class.
    #[must_use]
    pub const fn rm_type(self) -> &'static str {
        match self {
            Self::Person => "PERSON",
            Self::Organisation => "ORGANISATION",
            Self::Role => "ROLE",
        }
    }
}

impl core::fmt::Display for PartyClass {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.rm_type())
    }
}

/// Why a party does not build or does not read.
#[derive(Debug, thiserror::Error)]
pub enum PartyError {
    /// The template is rooted at no `PERSON`, `ORGANISATION` or `ROLE`.
    #[error("the template `{template_id}` is rooted at a `{rm_type}`, which is no party")]
    NotAParty {
        /// The template.
        template_id: String,
        /// The RM type of its root.
        rm_type: String,
    },
    /// The index carries no operational template constraints to build from.
    #[error(
        "the template `{template_id}` was indexed without its OPT 1.4, so the objects between \
         its nodes are unknown"
    )]
    NoStructure {
        /// The template.
        template_id: String,
    },
    /// A value is of a form a party build does not take.
    #[error("the value at `{place}` is {reason}, which a party build does not take")]
    UnsupportedValue {
        /// The flat id of the node, or `ctx`.
        place: String,
        /// What the value is.
        reason: &'static str,
    },
    /// A path names an attribute its owner's class does not have.
    #[error("`{path}` names `{attribute}`, which the class `{class}` does not have")]
    UnknownAttribute {
        /// The path being built.
        path: String,
        /// The class of the owner.
        class: String,
        /// The attribute.
        attribute: String,
    },
    /// The operational template constrains no object at a path the build
    /// walks.
    #[error("the template `{template_id}` constrains no object at `{path}`")]
    Unconstrained {
        /// The template.
        template_id: String,
        /// The path.
        path: String,
    },
    /// An instance is written before the instances that precede it.
    #[error("`{path}` writes the instance {position} before the instances that precede it")]
    InstanceGap {
        /// The path.
        path: String,
        /// The 1-based instance.
        position: usize,
    },
    /// A node or an occurrence does not resolve against the template.
    #[error(transparent)]
    Path(#[from] PathError),
    /// The built object does not decode as the root's class.
    #[error("the party built against `{template_id}` is no valid `{}`", class.rm_type())]
    Invalid {
        /// The template.
        template_id: String,
        /// The class it was decoded as.
        class: PartyClass,
        /// What the strict decode refused.
        #[source]
        source: Box<JsonParseError>,
    },
    /// A `ROLE` names no `performer`, which the RM makes mandatory.
    #[error(
        "the ROLE of the template `{template_id}` names no performer, which `ROLE.performer` \
         requires (RM Demographic IM, the ROLE class, 1..1)"
    )]
    RoleWithoutPerformer {
        /// The template.
        template_id: String,
    },
    /// The party was built against another template.
    #[error("the party names the template `{found}`, not `{expected}`")]
    TemplateMismatch {
        /// The template the index carries.
        expected: String,
        /// The template the party names.
        found: String,
    },
}

/// A demographic party in the canonical JSON of ITS-REST 1.1.0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalParty {
    /// The canonical JSON of the party.
    value: Value,
    /// The template the party was built against.
    template_id: String,
    /// The class of the party.
    class: PartyClass,
}

impl CanonicalParty {
    /// Returns the canonical JSON of the party.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Returns the canonical JSON of the party, consuming it.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.value
    }

    /// Returns the template the party was built against.
    #[must_use]
    pub fn template_id(&self) -> &str {
        &self.template_id
    }

    /// Returns the class of the party.
    #[must_use]
    pub const fn class(&self) -> PartyClass {
        self.class
    }
}

impl CanonicalDocument for CanonicalParty {
    fn document(&self) -> &Value {
        &self.value
    }

    fn document_template_id(&self) -> &str {
        &self.template_id
    }
}

impl WebTemplateIndex {
    /// Builds a canonical party from `values`.
    ///
    /// Every value is a whole canonical value under the `raw` suffix at a
    /// node of the template (Simplified Formats master04 §Raw canonical
    /// JSON). The objects on the way to each node are the ones the
    /// operational template constrains there, named by their rubric, and the
    /// root carries the archetype and template ids with `rm_version` 1.1.0.
    ///
    /// # Errors
    ///
    /// Returns [`PartyError::NotAParty`] for a template rooted at no party,
    /// [`PartyError::NoStructure`] when the index was built without an OPT
    /// 1.4, [`PartyError::UnsupportedValue`] for a `ctx/` value, a family
    /// below a node or a value not under `raw`, [`PartyError::Path`] for a
    /// node or an occurrence the template does not have,
    /// [`PartyError::UnknownAttribute`], [`PartyError::Unconstrained`] and
    /// [`PartyError::InstanceGap`] for a path the build cannot place,
    /// [`PartyError::RoleWithoutPerformer`] for a `ROLE` with no `performer`,
    /// and [`PartyError::Invalid`] when the result does not decode as the
    /// root's class.
    pub fn build_party(&self, values: &[NodeValue]) -> Result<CanonicalParty, PartyError> {
        // TODO(#906): move the build to openehr-sdt once it carries a party builder.
        let kind = self.party_kind()?;
        let root = self
            .structure(&AqlPath::new(String::new()))
            .ok_or_else(|| PartyError::NoStructure {
                template_id: self.template_id().to_owned(),
            })?;
        let mut party = Map::new();
        party.insert(TYPE.to_owned(), Value::from(kind.rm_type()));
        party.insert(
            "name".to_owned(),
            text(root.name.as_deref().unwrap_or(&root.node_id)),
        );
        party.insert(
            "archetype_node_id".to_owned(),
            Value::from(root.node_id.as_str()),
        );
        party.insert(
            "archetype_details".to_owned(),
            serde_json::json!({
                "_type": "ARCHETYPED",
                "archetype_id": {"_type": "ARCHETYPE_ID", "value": root.node_id},
                "template_id": {"_type": "TEMPLATE_ID", "value": self.template_id()},
                "rm_version": RM_RELEASE,
            }),
        );
        for value in values {
            self.place(&mut party, value)?;
        }
        let built = Value::Object(party);
        self.performed(kind, &built)?;
        let value = decode(kind, &built).map_err(|source| PartyError::Invalid {
            template_id: self.template_id().to_owned(),
            class: kind,
            source: Box::new(source),
        })?;
        Ok(CanonicalParty {
            value,
            template_id: self.template_id().to_owned(),
            class: kind,
        })
    }

    /// Wraps canonical JSON a CDR served as a party of this template, decoded
    /// strictly as the template root's class.
    ///
    /// # Errors
    ///
    /// Returns [`PartyError::NotAParty`] for a template rooted at no party,
    /// [`PartyError::RoleWithoutPerformer`] for a `ROLE` with no `performer`,
    /// and [`PartyError::Invalid`] when the document does not decode as the
    /// root's class.
    pub fn accept_party(&self, value: &Value) -> Result<CanonicalParty, PartyError> {
        let kind = self.party_kind()?;
        self.performed(kind, value)?;
        let value = decode(kind, value).map_err(|source| PartyError::Invalid {
            template_id: self.template_id().to_owned(),
            class: kind,
            source: Box::new(source),
        })?;
        Ok(CanonicalParty {
            value,
            template_id: self.template_id().to_owned(),
            class: kind,
        })
    }

    /// Refuses a `ROLE` that names no `performer`.
    ///
    /// `ROLE.performer` is `1..1`, the "Reference to Version container of
    /// Actor playing the role" (RM Demographic IM, the `ROLE` class), so a
    /// role without one is no valid `ROLE`, built or read.
    fn performed(&self, kind: PartyClass, value: &Value) -> Result<(), PartyError> {
        let named = value
            .get("performer")
            .is_some_and(|performer| !performer.is_null());
        if kind == PartyClass::Role && !named {
            return Err(PartyError::RoleWithoutPerformer {
                template_id: self.template_id().to_owned(),
            });
        }
        Ok(())
    }

    /// Returns the class the template is rooted at.
    fn party_kind(&self) -> Result<PartyClass, PartyError> {
        PartyClass::from_rm_type(self.root().rm_type()).ok_or_else(|| PartyError::NotAParty {
            template_id: self.template_id().to_owned(),
            rm_type: self.root().rm_type().to_owned(),
        })
    }

    /// Places one value into `party`.
    fn place(&self, party: &mut Map<String, Value>, value: &NodeValue) -> Result<(), PartyError> {
        let Place::Node {
            ref flat_id,
            ref occurrences,
        } = value.place
        else {
            return Err(PartyError::UnsupportedValue {
                place: String::from("ctx"),
                reason: "a context value",
            });
        };
        let unsupported = |reason| PartyError::UnsupportedValue {
            place: flat_id.as_str().to_owned(),
            reason,
        };
        if !value.sub_path.is_empty() {
            return Err(unsupported("a family below its node"));
        }
        if !matches!(value.suffixes.as_slice(), [only] if only.name == "raw" && only.index.is_none())
        {
            return Err(unsupported("not a whole value under `raw`"));
        }
        let node = self.node_by_flat_id(flat_id)?;
        let path = self.instance_path(node, occurrences)?;
        if path.segments.is_empty() {
            return root_members(party, &value.value).ok_or_else(|| unsupported("no object"))?;
        }
        let mut current = party;
        let mut spelled = String::new();
        let mut segments = path.segments.iter().peekable();
        while let Some(segment) = segments.next() {
            spelled.push_str(&spell(segment));
            let class = current
                .get(TYPE)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let attribute = openehr_rm::v1_2::model::attribute(&class, &segment.attribute)
                .ok_or_else(|| PartyError::UnknownAttribute {
                    path: spelled.clone(),
                    class: class.clone(),
                    attribute: segment.attribute.clone(),
                })?;
            let list = attribute.container != Container::None;
            let ends = segments.peek().is_none();
            let child = if ends {
                value.value.clone()
            } else {
                self.structural(&spelled)?
            };
            let slot = insert(current, segment, list, ends, child, &spelled)?;
            let Value::Object(next) = slot else {
                return Ok(());
            };
            current = next;
        }
        Ok(())
    }

    /// Returns the empty object the operational template constrains at
    /// `path`, typed and named.
    fn structural(&self, path: &str) -> Result<Value, PartyError> {
        let found = self
            .structure(&AqlPath::new(path.to_owned()))
            .ok_or_else(|| PartyError::Unconstrained {
                template_id: self.template_id().to_owned(),
                path: path.to_owned(),
            })?;
        let mut object = Map::new();
        object.insert(TYPE.to_owned(), Value::from(found.rm_type.as_str()));
        object.insert(
            "name".to_owned(),
            text(found.name.as_deref().unwrap_or(&found.node_id)),
        );
        object.insert(
            "archetype_node_id".to_owned(),
            Value::from(found.node_id.as_str()),
        );
        Ok(Value::Object(object))
    }
}

/// Places the members of a value written at the party root onto the root,
/// `None` when the value is no object.
///
/// An attribute of the root that no template node holds (`ROLE.performer`,
/// RM Demographic IM) is written this way; every member must be an attribute
/// of the root's class, and its `_type` names that class and is not copied.
fn root_members(party: &mut Map<String, Value>, value: &Value) -> Option<Result<(), PartyError>> {
    let Value::Object(members) = value else {
        return None;
    };
    let class = party
        .get(TYPE)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    for (attribute, member) in members {
        if attribute == TYPE {
            continue;
        }
        if openehr_rm::v1_2::model::attribute(&class, attribute).is_none() {
            return Some(Err(PartyError::UnknownAttribute {
                path: format!("/{attribute}"),
                class,
                attribute: attribute.clone(),
            }));
        }
        party.insert(attribute.clone(), member.clone());
    }
    Some(Ok(()))
}

/// Puts `child` at `segment` of `owner`, or finds the instance already there,
/// and returns the slot; the value a path ends on (`ends`) replaces the one
/// held there, as a later write to one place does.
fn insert<'a>(
    owner: &'a mut Map<String, Value>,
    segment: &PathSegment,
    list: bool,
    ends: bool,
    child: Value,
    path: &str,
) -> Result<&'a mut Value, PartyError> {
    let slot = owner.entry(segment.attribute.clone());
    if !list {
        return Ok(match slot {
            serde_json::map::Entry::Occupied(mut held) if ends => {
                held.insert(child);
                held.into_mut()
            }
            held => held.or_insert(child),
        });
    }
    let Value::Array(items) = slot.or_insert_with(|| Value::Array(Vec::new())) else {
        return Err(PartyError::InstanceGap {
            path: path.to_owned(),
            position: 1,
        });
    };
    let position = segment.predicate.position.unwrap_or(1);
    let node_id = segment.predicate.archetype_node_id.as_deref();
    let held: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            node_id
                .is_none_or(|id| item.get("archetype_node_id").and_then(Value::as_str) == Some(id))
        })
        .map(|(index, _)| index)
        .collect();
    let at = match held.get(position.saturating_sub(1)) {
        Some(&index) => {
            if let (true, Some(held)) = (ends, items.get_mut(index)) {
                *held = child;
            }
            index
        }
        None if held.len().saturating_add(1) == position => {
            items.push(child);
            items.len().saturating_sub(1)
        }
        None => {
            return Err(PartyError::InstanceGap {
                path: path.to_owned(),
                position,
            });
        }
    };
    items.get_mut(at).ok_or_else(|| PartyError::InstanceGap {
        path: path.to_owned(),
        position,
    })
}

/// Spells one segment as the Web Template builder spells an `aqlPath` step,
/// without its position.
fn spell(segment: &PathSegment) -> String {
    match (
        segment.predicate.archetype_node_id.as_deref(),
        segment.predicate.name_value.as_deref(),
    ) {
        (Some(id), Some(name)) => format!("/{}[{id},'{name}']", segment.attribute),
        (Some(id), None) => format!("/{}[{id}]", segment.attribute),
        (None, _) => format!("/{}", segment.attribute),
    }
}

/// Returns a `DV_TEXT` carrying `value`.
fn text(value: &str) -> Value {
    serde_json::json!({"_type": "DV_TEXT", "value": value})
}

/// Decodes `value` strictly as `class` and encodes it back.
fn decode(class: PartyClass, value: &Value) -> Result<Value, JsonParseError> {
    Ok(match class {
        PartyClass::Person => openehr_its::json::to_canonical_value(
            &openehr_its::json::from_canonical_value::<Person>(value)?,
        ),
        PartyClass::Organisation => {
            openehr_its::json::to_canonical_value(&openehr_its::json::from_canonical_value::<
                Organisation,
            >(value)?)
        }
        PartyClass::Role => openehr_its::json::to_canonical_value(
            &openehr_its::json::from_canonical_value::<Role>(value)?,
        ),
    })
}
