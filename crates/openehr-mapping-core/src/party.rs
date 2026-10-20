// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The build of a canonical demographic `PARTY` from node values.
//!
//! Simplified Formats defines FLAT for compositions only, and `openehr-sdt`
//! extends its template walk to a party root. Each value is a whole canonical
//! value at a template node, written under the `raw` suffix and built through
//! that walk; a value at the root places its members on the root, and the
//! result is decoded strictly as the root's class before it is returned. No
//! composition attribute is written. No specification governs a party build:
//! our own design.

use openehr_its::json::JsonParseError;
use openehr_rm::v1_2::demographic::organisation::Organisation;
use openehr_rm::v1_2::demographic::person::Person;
use openehr_rm::v1_2::demographic::role::Role;
use openehr_sdt::flat::build::BuildOptions;
use openehr_sdt::flat::error::FlatError;
use serde_json::Map;
use serde_json::Value;

use crate::composition::CanonicalDocument;
use crate::composition::NodeValue;
use crate::composition::Place;
use crate::index::WebTemplateIndex;
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
    /// A value is of a form a party build does not take.
    #[error("the value at `{place}` is {reason}, which a party build does not take")]
    UnsupportedValue {
        /// The flat id of the node, or `ctx`.
        place: String,
        /// What the value is.
        reason: &'static str,
    },
    /// A value at the root names an attribute the root's class does not have.
    #[error("`{path}` names `{attribute}`, which the class `{class}` does not have")]
    UnknownAttribute {
        /// The path being built.
        path: String,
        /// The class of the owner.
        class: String,
        /// The attribute.
        attribute: String,
    },
    /// The `openehr-sdt` builder refused the values.
    #[error("the party values do not build against `{template_id}`")]
    Build {
        /// The template.
        template_id: String,
        /// Why the builder refused them.
        #[source]
        source: Box<FlatError>,
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
    /// JSON). The values below the root are built through the `openehr-sdt`
    /// template walk, which names every object on the way to a node by its
    /// rubric and writes the archetype and template ids with `rm_version`
    /// 1.1.0 on the root; a value at the root places its members on the root.
    ///
    /// # Errors
    ///
    /// Returns [`PartyError::NotAParty`] for a template rooted at no party,
    /// [`PartyError::UnsupportedValue`] for a `ctx/` value, a family below a
    /// node or a value not under `raw`, [`PartyError::Path`] for a node or an
    /// occurrence the template does not have, [`PartyError::Build`] when the
    /// builder refuses the values, [`PartyError::UnknownAttribute`] for a root
    /// value naming an attribute the root's class does not have,
    /// [`PartyError::RoleWithoutPerformer`] for a `ROLE` with no `performer`,
    /// and [`PartyError::Invalid`] when the result does not decode as the
    /// root's class.
    pub fn build_party(&self, values: &[NodeValue]) -> Result<CanonicalParty, PartyError> {
        let kind = self.party_kind()?;
        let mut flat = Map::new();
        let mut on_root = Vec::new();
        for value in values {
            if self.party_value(value)? {
                on_root.push(&value.value);
            } else {
                flat.insert(self.flat_key(value)?.to_string(), value.value.clone());
            }
        }
        let refused = |source| PartyError::Build {
            template_id: self.template_id().to_owned(),
            source: Box::new(source),
        };
        let document = openehr_sdt::flat::sim::flat::parse_flat(&flat).map_err(refused)?;
        let options = BuildOptions::default().with_rm_version(RM_RELEASE);
        let built = openehr_sdt::flat::build::build_root(&document, self.web_template(), &options)
            .map_err(refused)?;
        let Value::Object(mut party) = built else {
            return Err(refused(FlatError::Conversion(String::from(
                "the party root built as no object",
            ))));
        };
        for members in on_root {
            root_members(&mut party, members).ok_or_else(|| PartyError::UnsupportedValue {
                place: self.root().flat_id().as_str().to_owned(),
                reason: "no object",
            })??;
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

    /// Checks that `value` is of a form a party build takes, and answers
    /// whether it is a value at the root.
    fn party_value(&self, value: &NodeValue) -> Result<bool, PartyError> {
        let Place::Node { ref flat_id, .. } = value.place else {
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
        Ok(self.node_by_flat_id(flat_id)?.flat_id() == self.root().flat_id())
    }
}

// TODO(#1000): pass the root's members to the openehr-sdt builder once it
// takes a party root attribute, and build through `build_party`.
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
