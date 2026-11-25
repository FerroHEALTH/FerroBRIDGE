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
use openehr_rm::v1_2::paths::PathSegment;
use openehr_rm::v1_2::paths::RmPath;
use openehr_rm::v1_2::paths::write::SetPathError;
use openehr_rm::v1_2::typing::RmType;
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

/// The RM class of a relationship a party holds.
const RELATIONSHIP: &str = "PARTY_RELATIONSHIP";

/// The `PARTY.relationships` attribute.
const RELATIONSHIPS: &str = "relationships";

/// The `PARTY_RELATIONSHIP.source` attribute.
const SOURCE: &str = "source";

/// One member of a relationship: its path in the party and its value.
type Member = (RmPath, Value);

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
    /// The party does not validate against its template, the RM invariants
    /// or the RM-mandated openEHR terminology
    /// (`openehr_sdt::rm_instance::validate_party`).
    #[error("the party does not validate against `{template_id}`: {}", messages.join("; "))]
    Unvalidated {
        /// The template the party was validated against.
        template_id: String,
        /// One rendered message per violation.
        messages: Vec<String>,
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
    /// A value writes the `source` of a `PARTY_RELATIONSHIP`, which is always
    /// the party that holds it.
    #[error(
        "the value at `{place}` writes PARTY_RELATIONSHIP.source, which is the party that holds \
         the relationship (RM Demographic IM, the PARTY class, invariant Relationships_validity)"
    )]
    RelationshipSource {
        /// The flat id of the relationship node.
        place: String,
    },
    /// A member of a relationship does not write into the built party.
    #[error("`{path}` does not write into the party built against `{template_id}`")]
    RelationshipWrite {
        /// The template.
        template_id: String,
        /// The path of the member.
        path: String,
        /// Why the write was refused.
        #[source]
        source: Box<SetPathError>,
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
    /// The relationships the party holds once its uid names their source.
    unsourced: Vec<Value>,
}

impl CanonicalParty {
    /// Returns the canonical JSON of the party, without the relationships it
    /// awaits the source of ([`Self::awaits_source`]).
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

    /// Answers whether the party holds relationships that [`Self::value`]
    /// leaves out until [`WebTemplateIndex::source_relationships`] names
    /// their source.
    #[must_use]
    pub const fn awaits_source(&self) -> bool {
        !self.unsourced.is_empty()
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
    /// [`PartyError::Invalid`] when the result does not decode as the root's
    /// class, and [`PartyError::Unvalidated`] when it does not validate against
    /// the template.
    pub fn build_party(&self, values: &[NodeValue]) -> Result<CanonicalParty, PartyError> {
        let kind = self.party_kind()?;
        let root = self.root().flat_id().as_str().to_owned();
        let mut flat = Map::new();
        let mut performed = false;
        let (values, names) = self.split_names(values)?;
        let mut members = Vec::new();
        for value in values {
            if self.party_value(value)? {
                let Value::Object(ref members) = value.value else {
                    return Err(PartyError::UnsupportedValue {
                        place: root,
                        reason: "no object",
                    });
                };
                for (attribute, member) in self.root_members(members)? {
                    performed |= attribute == "performer" && !member.is_null();
                    flat.insert(format!("{root}/{attribute}|raw"), member.clone());
                }
            } else if let Some(written) = self.relationship_members(value)? {
                members.extend(written);
            } else {
                flat.insert(self.flat_key(value)?.to_string(), value.value.clone());
            }
        }
        if kind == PartyClass::Role && !performed {
            return Err(PartyError::RoleWithoutPerformer {
                template_id: self.template_id().to_owned(),
            });
        }
        let refused = |source| PartyError::Build {
            template_id: self.template_id().to_owned(),
            source: Box::new(source),
        };
        let document = openehr_sdt::flat::sim::flat::parse_flat(&flat).map_err(refused)?;
        let options = BuildOptions::default().with_rm_version(RM_RELEASE);
        let mut built =
            openehr_sdt::flat::build::build_root(&document, self.web_template(), &options)
                .map_err(refused)?;
        self.write_names(&mut built, names)?;
        self.write_members(&mut built, members)?;
        let unsourced = take_relationships(&mut built);
        let mut party = self.decoded(kind, &built)?;
        party.unsourced = unsourced;
        Ok(party)
    }

    /// Returns `party` with the relationships it awaits written in, each
    /// sourced at the party `uid` names in `namespace`.
    ///
    /// Every relationship a party holds has the party as its source (RM
    /// Demographic IM, the `PARTY` class, invariant `Relationships_validity`),
    /// and a CDR assigns a new party's uid when it commits it, so a party
    /// built from node values holds its relationships aside until that uid is
    /// known.
    ///
    /// # Errors
    ///
    /// Returns [`PartyError::Invalid`] when the result does not decode as the
    /// party's class and [`PartyError::Unvalidated`] when it does not
    /// validate against the template.
    pub fn source_relationships(
        &self,
        party: &CanonicalParty,
        uid: &str,
        namespace: &str,
    ) -> Result<CanonicalParty, PartyError> {
        if !party.awaits_source() {
            return Ok(party.clone());
        }
        let source = serde_json::json!({
            "_type": "PARTY_REF",
            "id": {"_type": "HIER_OBJECT_ID", "value": uid},
            "namespace": namespace,
            "type": party.class.rm_type(),
        });
        let mut value = party.value.clone();
        let sourced = party.unsourced.iter().cloned().map(|mut relationship| {
            if let Value::Object(ref mut members) = relationship {
                members.insert(String::from(SOURCE), source.clone());
            }
            relationship
        });
        if let Value::Object(ref mut members) = value {
            match members
                .entry(RELATIONSHIPS)
                .or_insert_with(|| Value::Array(Vec::new()))
            {
                Value::Array(held) => held.extend(sourced),
                other => *other = Value::Array(sourced.collect()),
            }
        }
        self.decoded(party.class, &value)
    }

    /// Returns `value` decoded strictly as `kind` and validated against the
    /// template.
    fn decoded(&self, kind: PartyClass, value: &Value) -> Result<CanonicalParty, PartyError> {
        let value = decode(kind, value).map_err(|source| PartyError::Invalid {
            template_id: self.template_id().to_owned(),
            class: kind,
            source: Box::new(source),
        })?;
        self.validated(kind, value)
    }

    /// Returns the members a value at a `PARTY_RELATIONSHIP` node writes into
    /// that relationship, each with its path in the party, or `None` for a
    /// value at any other node.
    ///
    /// An operational template constrains no node for the `PARTY_REF`
    /// `target` of a relationship, so a value at the relationship node
    /// carries it, and it is written into the built party at the
    /// relationship's instance path. No specification governs this: our own
    /// design.
    fn relationship_members(&self, value: &NodeValue) -> Result<Option<Vec<Member>>, PartyError> {
        let Place::Node {
            ref flat_id,
            ref occurrences,
        } = value.place
        else {
            return Ok(None);
        };
        let node = self.node_by_flat_id(flat_id)?;
        if node.rm_type() != RELATIONSHIP {
            return Ok(None);
        }
        let place = || flat_id.as_str().to_owned();
        let Value::Object(ref members) = value.value else {
            return Err(PartyError::UnsupportedValue {
                place: place(),
                reason: "no object",
            });
        };
        let at = self.instance_path(node, occurrences)?;
        members
            .iter()
            .filter(|&(attribute, _)| attribute != TYPE)
            .map(|(attribute, member)| {
                if attribute == SOURCE {
                    return Err(PartyError::RelationshipSource { place: place() });
                }
                if openehr_rm::v1_2::model::attribute(RELATIONSHIP, attribute).is_none() {
                    return Err(PartyError::UnknownAttribute {
                        path: format!("{at}/{attribute}"),
                        class: String::from(RELATIONSHIP),
                        attribute: attribute.clone(),
                    });
                }
                let mut path = at.clone();
                path.segments.push(PathSegment {
                    attribute: attribute.clone(),
                    predicate: openehr_rm::v1_2::paths::Predicate::default(),
                    descendant: false,
                });
                Ok((path, member.clone()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    /// Writes each of `members` into `built`, an instance of the template's
    /// root.
    fn write_members(&self, built: &mut Value, members: Vec<Member>) -> Result<(), PartyError> {
        let root = RmType::simple(self.root().rm_type());
        for (path, member) in members {
            openehr_rm::v1_2::paths::write::set_at_path(built, &root, &path, member).map_err(
                |source| PartyError::RelationshipWrite {
                    template_id: self.template_id().to_owned(),
                    path: path.to_string(),
                    source: Box::new(source),
                },
            )?;
        }
        Ok(())
    }

    /// Wraps canonical JSON a CDR served as a party of this template, decoded
    /// strictly as the template root's class.
    ///
    /// # Errors
    ///
    /// Returns [`PartyError::NotAParty`] for a template rooted at no party,
    /// [`PartyError::RoleWithoutPerformer`] for a `ROLE` with no `performer`,
    /// [`PartyError::Invalid`] when the document does not decode as the root's
    /// class, and [`PartyError::Unvalidated`] when it does not validate against
    /// the template.
    pub fn accept_party(&self, value: &Value) -> Result<CanonicalParty, PartyError> {
        let kind = self.party_kind()?;
        self.performed(kind, value)?;
        self.decoded(kind, value)
    }

    /// Returns `value` as a party of `kind` once it validates against the
    /// template, the RM invariants and the RM-mandated openEHR terminology
    /// (`openehr_sdt::rm_instance::validate_party`).
    fn validated(&self, kind: PartyClass, value: Value) -> Result<CanonicalParty, PartyError> {
        let messages = openehr_sdt::rm_instance::validate_party(&value, self.web_template());
        if !messages.is_empty() {
            return Err(PartyError::Unvalidated {
                template_id: self.template_id().to_owned(),
                messages: messages
                    .iter()
                    .map(|message| format!("{}: {}", message.path, message.message))
                    .collect(),
            });
        }
        Ok(CanonicalParty {
            value,
            template_id: self.template_id().to_owned(),
            class: kind,
            unsourced: Vec::new(),
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

    /// Returns the members of a value written at the party root, each an
    /// attribute of the root's class the builder takes whole under `|raw`
    /// (Simplified Formats master04 §Raw canonical JSON), `ROLE.performer`
    /// first among them (RM Demographic IM); the `_type` names the class and
    /// is not passed on.
    fn root_members<'v>(
        &self,
        members: &'v Map<String, Value>,
    ) -> Result<Vec<(&'v str, &'v Value)>, PartyError> {
        let class = self.root().rm_type();
        members
            .iter()
            .filter(|&(attribute, _)| attribute != TYPE)
            .map(|(attribute, member)| {
                if openehr_rm::v1_2::model::attribute(class, attribute).is_none() {
                    return Err(PartyError::UnknownAttribute {
                        path: format!("/{attribute}"),
                        class: class.to_owned(),
                        attribute: attribute.clone(),
                    });
                }
                Ok((attribute.as_str(), member))
            })
            .collect()
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

/// Takes the relationships out of `built`, which holds none of them once
/// they are taken.
fn take_relationships(built: &mut Value) -> Vec<Value> {
    let Value::Object(ref mut members) = *built else {
        return Vec::new();
    };
    match members.remove(RELATIONSHIPS) {
        Some(Value::Array(relationships)) => relationships,
        Some(other) => vec![other],
        None => Vec::new(),
    }
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
