// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the content of a run into openEHR came from.
//!
//! The openEHR reference model records the origin of transformed content in
//! `FEEDER_AUDIT`, which "defines the semantics of an audit trail which is
//! constructed to describe the origin of data that have been transformed into
//! openEHR form and committed to the system"
//! (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_feeder_audit_class>).
//! A run carries one [`Origin`] on its [`crate::engine::traverse::Defaults`],
//! and the engine writes it into the composition beside the fields it
//! defaulted, so a reader of the stored composition sees both.

use fhir_types::codec::Value;

/// What a source resource that carries no `id` is recorded as.
///
/// An absent identifier is recorded as unknown and never invented. No
/// specification governs the spelling: our own design.
pub const UNKNOWN_SOURCE: &str = "unknown";

/// The system a run's content passed through, and the resource it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    system_id: String,
    originating: Option<String>,
    source: Option<SourceItem>,
    originating_items: Option<Vec<ItemId>>,
    feeder_items: Vec<ItemId>,
    location: Option<Party>,
    provider: Option<Party>,
}

/// A party named in a composition by the engine: a `PARTY_IDENTIFIED` of a
/// name and identifiers (RM Common IM, `PARTY_IDENTIFIED`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Party {
    /// The party's name, when known.
    pub name: Option<String>,
    /// The party's identifiers.
    pub identifiers: Vec<ItemId>,
}

/// One identifier of the item in a system, a `DV_IDENTIFIER` of `FEEDER_AUDIT`.
///
/// RM Common IM, `FEEDER_AUDIT.originating_system_item_ids`: "Identifiers used
/// for the item in the originating system, e.g. filler and placer ids".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemId {
    /// The identifier value.
    pub id: String,
    /// What the identifier is, `DV_IDENTIFIER.type`.
    pub kind: String,
    /// The authority that issued it, `DV_IDENTIFIER.issuer`, when known.
    pub issuer: Option<String>,
}

impl Origin {
    /// Creates the origin of a run through the system `system_id` names.
    ///
    /// `system_id` becomes `FEEDER_AUDIT_DETAILS.system_id`, the "identifier
    /// of the system which handled the information item"
    /// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_feeder_audit_details_class>).
    #[must_use]
    pub fn new(system_id: impl Into<String>) -> Self {
        Self {
            system_id: system_id.into(),
            originating: None,
            source: None,
            originating_items: None,
            feeder_items: Vec::new(),
            location: None,
            provider: None,
        }
    }

    /// Returns this origin with `source` as the resource the content came
    /// from.
    #[must_use]
    pub fn with_source(mut self, source: SourceItem) -> Self {
        self.source = Some(source);
        self
    }

    /// Returns this origin naming `system_id` as the system the content
    /// originated in, apart from the system it passed through.
    ///
    /// `system_id` becomes `originating_system_audit.system_id`, and the system
    /// of [`Origin::new`] the `feeder_system_audit.system_id`
    /// (<https://specifications.openehr.org/releases/RM/Release-1.1.0/common.html#_feeder_audit_class>).
    #[must_use]
    pub fn with_originating_system(mut self, system_id: impl Into<String>) -> Self {
        self.originating = Some(system_id.into());
        self
    }

    /// Returns the system the content originated in, when it is named apart
    /// from the system it passed through.
    #[must_use]
    pub fn originating_system(&self) -> Option<&str> {
        self.originating.as_deref()
    }

    /// Returns this origin naming `items` as the identifiers of the item in the
    /// originating system, in place of the source item's type and id.
    #[must_use]
    pub fn with_originating_items(mut self, items: Vec<ItemId>) -> Self {
        self.originating_items = Some(items);
        self
    }

    /// Returns this origin naming `items` as identifiers of the item in the
    /// feeder system, written before the fields the run defaulted
    /// (`FEEDER_AUDIT.feeder_system_item_ids`).
    #[must_use]
    pub fn with_feeder_items(mut self, items: Vec<ItemId>) -> Self {
        self.feeder_items = items;
        self
    }

    /// Returns this origin naming `location`, the site that handled the item in
    /// the originating system (`originating_system_audit.location`).
    #[must_use]
    pub fn with_location(mut self, location: Party) -> Self {
        self.location = Some(location);
        self
    }

    /// Returns this origin naming `provider`, who created or handled the item
    /// in the originating system (`originating_system_audit.provider`).
    #[must_use]
    pub fn with_provider(mut self, provider: Party) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Returns the site that handled the item in the originating system.
    #[must_use]
    pub const fn location(&self) -> Option<&Party> {
        self.location.as_ref()
    }

    /// Returns who created or handled the item in the originating system.
    #[must_use]
    pub const fn provider(&self) -> Option<&Party> {
        self.provider.as_ref()
    }

    /// Returns the identifiers of the item in the originating system the
    /// caller named, when it named them apart from the source item.
    #[must_use]
    pub fn originating_items(&self) -> Option<&[ItemId]> {
        self.originating_items.as_deref()
    }

    /// Returns the identifiers of the item in the feeder system the caller
    /// named.
    #[must_use]
    pub fn feeder_items(&self) -> &[ItemId] {
        &self.feeder_items
    }

    /// Returns the system the content passed through.
    #[must_use]
    pub fn system_id(&self) -> &str {
        &self.system_id
    }

    /// Returns the resource the content came from, when the caller named one.
    #[must_use]
    pub const fn source(&self) -> Option<&SourceItem> {
        self.source.as_ref()
    }
}

/// Why a value cannot be one of the message identifiers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MessageIdError {
    /// The text carries no characters.
    #[error("a {kind} cannot be empty")]
    Empty {
        /// Which identifier was being built.
        kind: &'static str,
    },
}

/// The id a sending system gave one message.
///
/// An example is the MSH-10 control id of an HL7 v2 message. The type is
/// distinct from [`MessageType`], so the two cannot be swapped at a call site.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageControlId(String);

impl MessageControlId {
    /// Returns the control id `text` names.
    ///
    /// # Errors
    ///
    /// Returns [`MessageIdError::Empty`] when `text` is empty.
    pub fn new(text: impl Into<String>) -> Result<Self, MessageIdError> {
        let text = text.into();
        // NOTE: no specification governs this: our own design; no vendored
        // source states the MSH-10 length, so only an empty id is refused.
        if text.is_empty() {
            return Err(MessageIdError::Empty {
                kind: "message control id",
            });
        }
        Ok(Self(text))
    }

    /// Returns the control id as the message carried it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The type of one message, such as `ORU^R01`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageType(String);

impl MessageType {
    /// Returns the message type `text` names.
    ///
    /// # Errors
    ///
    /// Returns [`MessageIdError::Empty`] when `text` is empty.
    pub fn new(text: impl Into<String>) -> Result<Self, MessageIdError> {
        let text = text.into();
        if text.is_empty() {
            return Err(MessageIdError::Empty {
                kind: "message type",
            });
        }
        Ok(Self(text))
    }

    /// Returns the message type as the message carried it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The item one run into openEHR read.
///
/// The item is a FHIR resource ([`SourceItem::of`]) or a message a face
/// received ([`SourceItem::message`]). Either way the engine records its type
/// and its id as the one `originating_system_item_ids` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceItem {
    resource_type: String,
    id: Option<String>,
    version_id: Option<String>,
}

impl SourceItem {
    /// Creates a source of `resource_type` with no id and no version.
    #[must_use]
    pub fn new(resource_type: impl Into<String>) -> Self {
        Self {
            resource_type: resource_type.into(),
            id: None,
            version_id: None,
        }
    }

    /// Creates the source of a message, named by its control id.
    ///
    /// The control id becomes the item's id and the message type its type. A
    /// message carries no version, so none is recorded (no specification
    /// governs this allocation: our own design).
    #[must_use]
    pub fn message(control_id: MessageControlId, message_type: MessageType) -> Self {
        Self::new(message_type.0).with_id(control_id.0)
    }

    /// Reads the source item a FHIR resource describes.
    ///
    /// The type is `resourceType`, the id is `id` and the version is
    /// `meta.versionId` (<https://hl7.org/fhir/R4/resource.html>). A document
    /// with no `resourceType` is no resource and answers `None`.
    #[must_use]
    pub fn of(document: &Value) -> Option<Self> {
        let resource_type = document.get("resourceType").and_then(Value::as_str)?;
        let mut source = Self::new(resource_type);
        if let Some(id) = document.get("id").and_then(Value::as_str) {
            source = source.with_id(id);
        }
        if let Some(version) = document
            .get("meta")
            .and_then(|meta| meta.get("versionId"))
            .and_then(Value::as_str)
        {
            source = source.with_version_id(version);
        }
        Some(source)
    }

    /// Returns this source with the resource id `id`.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Returns this source with the resource version `version_id`.
    #[must_use]
    pub fn with_version_id(mut self, version_id: impl Into<String>) -> Self {
        self.version_id = Some(version_id.into());
        self
    }

    /// Returns the item's type: a resource's `resourceType`, or a message's
    /// type.
    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// Returns the item's id, when the item carries one.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Returns the resource version, when the resource carries one.
    #[must_use]
    pub fn version_id(&self) -> Option<&str> {
        self.version_id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::{MessageControlId, MessageIdError, MessageType, SourceItem};
    use fhir_types::codec::Value;

    #[test]
    fn a_source_item_reads_the_type_the_id_and_the_version() {
        let document = Value::from_serde_json(serde_json::json!({
            "resourceType": "Condition",
            "id": "sender-1",
            "meta": {"versionId": "3"}
        }));
        let source = SourceItem::of(&document).expect("a resource names its type");
        assert_eq!(source.resource_type(), "Condition");
        assert_eq!(source.id(), Some("sender-1"));
        assert_eq!(source.version_id(), Some("3"));
    }

    #[test]
    fn a_message_source_names_its_control_id_and_its_type() {
        let source = SourceItem::message(
            MessageControlId::new("MSG-0001").expect("a non-empty control id"),
            MessageType::new("ORU^R01").expect("a non-empty message type"),
        );
        assert_eq!(source.id(), Some("MSG-0001"));
        assert_eq!(source.resource_type(), "ORU^R01");
        assert_eq!(source.version_id(), None);
    }

    #[test]
    fn an_empty_control_id_or_message_type_is_refused() {
        assert_eq!(
            Err(MessageIdError::Empty {
                kind: "message control id"
            }),
            MessageControlId::new("")
        );
        assert_eq!(
            Err(MessageIdError::Empty {
                kind: "message type"
            }),
            MessageType::new("")
        );
    }

    #[test]
    fn a_document_with_no_resource_type_is_no_source() {
        let document = Value::from_serde_json(serde_json::json!({"id": "x"}));
        assert_eq!(SourceItem::of(&document), None);
    }
}
