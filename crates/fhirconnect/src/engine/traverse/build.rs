// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The composition defaults and the build of the values a run produced.

use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_base::v1_3::base_types::identification::terminology_id::TerminologyId;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::NodeValue;
use openehr_mapping_core::index::ResolvedNode;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_mapping_core::template::PathError;
use openehr_rm::v1_2::common::generic::party_identified::PartyIdentifiedData;
use openehr_rm::v1_2::data_types::text::code_phrase::CodePhrase;
use openehr_rm::v1_2::data_types::text::dv_coded_text::DvCodedText;
use openehr_rm::v1_2::support::terminology::openehr_terminology_group_identifiers::OpenehrTerminologyGroupIdentifiersData;

use crate::engine::outcome::Location;
use crate::engine::outcome::Loss;
use crate::engine::outcome::Warning;
use crate::engine::rm;
use crate::engine::rm::RmError;
use crate::engine::rm::RmValue;

use crate::tree::element::Table;

use crate::engine::traverse::Defaults;
use crate::engine::traverse::Held;
use crate::engine::traverse::Routed;
use crate::engine::traverse::Run;
use crate::engine::traverse::error::EngineError;

/// The archetype path of `COMPOSITION.composer`.
const COMPOSER: &str = "/composer";

impl<'a, T: Table + ?Sized> Run<'a, T> {
    /// Fills the composition fields no mapping wrote.
    ///
    /// Returns the openEHR path of every field it filled, in the order it
    /// filled them, which is the order of the
    /// [`Loss::Defaulted`] warnings it
    /// recorded. Which fields take a default is FHIRconnect's
    /// (`engine/defaults-for-fields.adoc`); every default travels as the `ctx/`
    /// key the FLAT builder resolves (Simplified Formats, master06 §Composer,
    /// §time, §setting, §Language and Territory). `ctx/setting` carries the
    /// code and the builder takes the rubric from the openEHR terminology's
    /// [`OpenehrTerminologyGroupIdentifiersData::GROUP_ID_SETTING`] group, so
    /// the project's value is checked against the built composition.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Template`] when the template cannot answer for a
    /// defaulted field.
    pub(super) fn apply_defaults(
        &mut self,
        defaults: &Defaults,
    ) -> Result<Vec<String>, EngineError> {
        let mut filled = Vec::new();
        self.release_void_composer()?;
        self.default_context(&mut filled, COMPOSER, "composer_name", &defaults.composer)?;
        self.default_context(
            &mut filled,
            "/context/start_time",
            "time",
            &defaults.start_time,
        )?;
        if let Some(node) = self.unwritten("/context/setting")? {
            let setting = defaults.setting.clone();
            self.context_keys.push(NodeValue::context(
                "setting",
                serde_json::Value::String(setting.code.clone()),
            ));
            self.expected.push((
                node.flat_id().clone(),
                RmValue::CodedText(Box::new(DvCodedText {
                    value: setting.value,
                    hyperlink: None,
                    formatting: None,
                    mappings: None,
                    language: None,
                    encoding: None,
                    defining_code: CodePhrase {
                        terminology_id: TerminologyId {
                            value: String::from(
                                OpenehrTerminologyGroupIdentifiersData::TERMINOLOGY_ID_OPENEHR,
                            ),
                        },
                        code_string: setting.code,
                        preferred_term: None,
                    },
                })),
            ));
            self.defaulted(&mut filled, "/context/setting");
        }
        if let Some(ref code) = defaults.language {
            self.default_context(&mut filled, "/language", "language", code)?;
        }
        if let Some(ref code) = defaults.territory {
            self.default_context(&mut filled, "/territory", "territory", code)?;
        }
        Ok(filled)
    }

    /// Writes one default as the `ctx/` key `field`, when the template holds
    /// a node at `path` and nothing wrote it already.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Template`] when the template cannot answer for
    /// the path for any reason other than holding no node at it.
    fn default_context(
        &mut self,
        filled: &mut Vec<String>,
        path: &str,
        field: &str,
        value: &str,
    ) -> Result<(), EngineError> {
        if self.unwritten(path)?.is_some() {
            self.context_keys.push(NodeValue::context(
                field,
                serde_json::Value::String(String::from(value)),
            ));
            self.defaulted(filled, path);
        }
        Ok(())
    }

    /// Records that the engine filled the field at `path`.
    fn defaulted(&mut self, filled: &mut Vec<String>, path: &str) {
        self.warnings.push(Warning::new(
            Loss::Defaulted {
                field: String::from(path),
            },
            Location::default().with_openehr(path),
        ));
        filled.push(String::from(path));
    }

    /// Returns the node at `path` when the template holds one and no mapping
    /// wrote it.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Template`] when the template cannot answer for
    /// the path for any reason other than holding no node at it.
    fn unwritten(&self, path: &str) -> Result<Option<&'a ResolvedNode>, EngineError> {
        let Some(node) = self.node_at(path)? else {
            return Ok(None);
        };
        let written = self
            .written
            .iter()
            .any(|written| written.flat_id == *node.flat_id());
        Ok((!written).then_some(node))
    }

    /// Forgets a composer the run wrote as a `PARTY_IDENTIFIED` holding no
    /// name, identifier or external reference, so the default composer
    /// applies.
    ///
    /// A FHIR `Reference` carrying only `reference` puts nothing in the
    /// party, since the demographics lookup the `PARTY_IDENTIFIED` cell of
    /// FHIRconnect names is the caller's, and the RM refuses such a party.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Template`] when the template cannot answer for
    /// the composer for any reason other than holding no node at it.
    fn release_void_composer(&mut self) -> Result<(), EngineError> {
        let Some(node) = self.node_at(COMPOSER)? else {
            return Ok(());
        };
        self.written.retain(|written| {
            written.flat_id != *node.flat_id()
                || !matches!(written.value, Held::Value(RmValue::Party(ref party)) if void(party))
        });
        Ok(())
    }

    /// Returns the node at `path`, `None` when the template holds none.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::Template`] when the template cannot answer for
    /// the path for any reason other than holding no node at it.
    fn node_at(&self, path: &str) -> Result<Option<&'a ResolvedNode>, EngineError> {
        match self.index.node(&AqlPath::new(path)) {
            Ok(node) => Ok(Some(node)),
            // NOTE: engine/defaults-for-fields.adoc, the context "is usually
            // populated", so a template with no node at the path takes no default.
            Err(PathError::UnknownPath { .. }) => Ok(None),
            Err(source) => Err(EngineError::Template {
                mapping: String::from(self.program.context().as_str()),
                node: String::from(path),
                source: Box::new(source),
            }),
        }
    }

    /// Returns the values the run produced, as the composition builder wants
    /// them, with the values the builder takes through the `ctx/` keys.
    ///
    /// Every value travels whole as its canonical JSON under `|raw`
    /// (Simplified Formats, master04 §Raw canonical JSON), except at a
    /// composition attribute the builder sets from the context vocabulary
    /// ([`context_keys`]), whose value is returned beside its node so the run
    /// can check the built composition carries it whole.
    pub(super) fn node_values(&self) -> Result<(Vec<NodeValue>, Vec<Routed>), EngineError> {
        let mut values = Vec::new();
        let mut routed = self.expected.clone();
        for written in &self.written {
            let node = self
                .index
                .node_by_flat_id(&written.flat_id)
                .map_err(|source| EngineError::Build {
                    source: Box::new(source),
                })?;
            let refuse = |source: RmError| EngineError::Rm {
                mapping: String::from(node.aql_path().as_str()),
                node: String::from(node.aql_path().as_str()),
                source: Box::new(source),
            };
            let value = match written.value {
                Held::Value(ref value) => value.clone(),
                Held::Partial(ref object) => {
                    let rm_type = object
                        .get("_type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or(node.rm_type());
                    RmValue::from_canonical(
                        rm_type,
                        node.aql_path().as_str(),
                        &serde_json::Value::Object(object.clone()),
                    )
                    .map_err(refuse)?
                }
            };
            if let Some(keys) = context_keys(node, &value) {
                values.extend(keys);
                routed.push((written.flat_id.clone(), value));
                continue;
            }
            values.push(
                NodeValue::new(node, value.to_canonical().map_err(refuse)?)
                    .with_occurrences(written.positions.clone())
                    .with_datum(rm::RAW),
            );
        }
        values.extend(self.families.iter().cloned());
        values.extend(self.context_keys.iter().cloned());
        Ok((values, routed))
    }

    /// Refuses a composition whose context-set attributes do not carry the
    /// value the run wrote there whole.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::ContextAttribute`] when the built value differs
    /// from the written one, and [`EngineError::Build`] or
    /// [`EngineError::Rm`] when the built value cannot be read back.
    pub(super) fn carried_whole(
        &self,
        built: &CanonicalComposition,
        routed: &[Routed],
    ) -> Result<(), EngineError> {
        let refuse_build = |source: PathError| EngineError::Build {
            source: Box::new(source),
        };
        for (flat_id, written) in routed {
            let node = self.index.node_by_flat_id(flat_id).map_err(refuse_build)?;
            let path = node.aql_path().as_str();
            let back = self
                .index
                .read(built, node, &[])
                .map_err(refuse_build)?
                .map(|value| RmValue::from_canonical(node.rm_type(), path, &value))
                .transpose()
                .map_err(|source| EngineError::Rm {
                    mapping: String::from(self.program.context().as_str()),
                    node: String::from(path),
                    source: Box::new(source),
                })?;
            if back.as_ref() != Some(written) {
                return Err(EngineError::ContextAttribute {
                    mapping: String::from(self.program.context().as_str()),
                    node: String::from(path),
                });
            }
        }
        Ok(())
    }
}

// TODO(#241): the three attributes travel as `ctx/` keys until openehr-sdt
// honours `|raw` on the COMPOSITION `language`, `territory` and `composer`
// nodes, which its builder routes through the context (new sibling request S6).
/// Returns the `ctx/` keys a value at a composition attribute the FLAT
/// builder sets from the context vocabulary travels as, `None` for any other
/// node.
///
/// Simplified Formats, master06 §Composer and §Language and Territory:
/// `ctx/language` and `ctx/territory` set the two codes, and the composer
/// travels as [`composer_keys`]. A part the keys do not hold is left out
/// here and refused by [`Run::carried_whole`] once the composition is built.
pub(super) fn context_keys(node: &ResolvedNode, value: &RmValue) -> Option<Vec<NodeValue>> {
    let field = match node.aql_path().as_str() {
        "/language" => "language",
        "/territory" => "territory",
        COMPOSER => {
            return Some(match *value {
                RmValue::Party(ref party) => composer_keys(party),
                _ => Vec::new(),
            });
        }
        _ => return None,
    };
    Some(match *value {
        RmValue::CodePhrase(ref code) => vec![context_key(field, &code.code_string)],
        _ => Vec::new(),
    })
}

/// Returns the `ctx/` keys a composer `PARTY_IDENTIFIED` travels as.
///
/// Simplified Formats, master06 §Composer and §ID Namespace and Scheme:
/// `ctx/composer_name` sets the name, `ctx/composer_id` the
/// `external_ref.id.value`, `ctx/id_namespace` its `namespace` and
/// `ctx/id_scheme` the `id.scheme` of a `GENERIC_ID`. The vocabulary has no
/// key for `identifiers`, so a party carrying them is refused by
/// [`Run::carried_whole`].
fn composer_keys(party: &PartyIdentifiedData) -> Vec<NodeValue> {
    let mut keys: Vec<NodeValue> = party
        .name
        .iter()
        .map(|name| context_key("composer_name", name))
        .collect();
    if let Some(ref reference) = party.external_ref {
        let (id, scheme) = match reference.id {
            ObjectId::GenericId(ref id) => (id.value.as_str(), Some(id.scheme.as_str())),
            ObjectId::HierObjectId(ref id) => (id.value(), None),
            ObjectId::ObjectVersionId(ref id) => (id.value(), None),
            ObjectId::ArchetypeId(ref id) => (id.value.as_str(), None),
            ObjectId::TemplateId(ref id) => (id.value.as_str(), None),
            ObjectId::TerminologyId(ref id) => (id.value.as_str(), None),
        };
        keys.push(context_key("composer_id", id));
        keys.push(context_key("id_namespace", &reference.namespace));
        if let Some(scheme) = scheme {
            keys.push(context_key("id_scheme", scheme));
        }
    }
    keys
}

/// Returns whether `party` holds none of `name`, `identifiers` and
/// `external_ref`.
///
/// The openEHR RM `PARTY_IDENTIFIED` requires "at least one of name,
/// identifier or external_ref", so such a party is no value.
fn void(party: &PartyIdentifiedData) -> bool {
    party.name.is_none() && party.identifiers.is_none() && party.external_ref.is_none()
}

/// Returns the `ctx/` key `field` carrying `text`.
fn context_key(field: &str, text: &str) -> NodeValue {
    NodeValue::context(field, serde_json::Value::String(String::from(text)))
}

#[cfg(test)]
mod tests {
    use openehr_base::v1_3::base_types::identification::generic_id::GenericId;
    use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
    use openehr_base::v1_3::base_types::identification::party_ref::PartyRef;
    use openehr_rm::v1_2::common::generic::party_identified::PartyIdentifiedData;

    use super::composer_keys;
    use super::context_key;
    use super::void;

    fn party(name: Option<&str>, external_ref: Option<PartyRef>) -> PartyIdentifiedData {
        PartyIdentifiedData {
            external_ref,
            name: name.map(String::from),
            identifiers: None,
        }
    }

    fn external_ref() -> PartyRef {
        PartyRef {
            namespace: String::from("demographics"),
            r#type: String::from("PERSON"),
            id: ObjectId::GenericId(GenericId {
                value: String::from("synthetic-practitioner-0001"),
                scheme: String::from("local"),
            }),
        }
    }

    #[test]
    fn a_named_composer_travels_as_its_name() {
        let named = party(Some("Synthetic Recorder"), None);
        assert_eq!(
            composer_keys(&named),
            [context_key("composer_name", "Synthetic Recorder")]
        );
        assert!(!void(&named));
    }

    #[test]
    fn a_nameless_composer_travels_as_its_external_ref() {
        let referenced = party(None, Some(external_ref()));
        assert_eq!(
            composer_keys(&referenced),
            [
                context_key("composer_id", "synthetic-practitioner-0001"),
                context_key("id_namespace", "demographics"),
                context_key("id_scheme", "local"),
            ]
        );
        assert!(!void(&referenced));
    }

    #[test]
    fn an_empty_composer_is_void_and_carries_no_key() {
        let empty = party(None, None);
        assert_eq!(composer_keys(&empty), []);
        assert!(void(&empty));
    }
}
