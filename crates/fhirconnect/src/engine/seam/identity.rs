// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity seams: what assigns the id of a resource the engine writes
//! and of the reference it points at.

use super::ReferenceError;
use core::fmt;
use sha2::Digest;

/// What assigns the id of a resource a run creates.
pub trait IdentitySink: fmt::Debug + Send + Sync {
    /// Returns the FHIR id the resource `request` describes takes.
    ///
    /// # Errors
    ///
    /// Returns [`ReferenceError`] when no id can be assigned.
    fn identify(&self, request: &IdentityRequest) -> Result<String, ReferenceError>;
}

/// Everything a run knows about one resource it creates.
///
/// The same composition and the same program produce the same request, so a
/// sink that derives the id from it answers the same id on every run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityRequest {
    resource_type: String,
    parent_type: String,
    parent_id: Option<String>,
    composition: Option<String>,
    mapping: String,
    occurrence: Vec<u32>,
    unique: Vec<String>,
}

impl IdentityRequest {
    /// Creates the request for a `resource_type` a mapping creates under a
    /// resource of `parent_type`.
    #[must_use]
    pub fn new(
        resource_type: impl Into<String>,
        parent_type: impl Into<String>,
        mapping: impl Into<String>,
    ) -> Self {
        Self {
            resource_type: resource_type.into(),
            parent_type: parent_type.into(),
            parent_id: None,
            composition: None,
            mapping: mapping.into(),
            occurrence: Vec::new(),
            unique: Vec::new(),
        }
    }

    /// Returns this request with the parent's id, when the run knows it.
    #[must_use]
    pub fn with_parent_id(mut self, id: impl Into<String>) -> Self {
        self.parent_id = Some(id.into());
        self
    }

    /// Returns this request with the `LOCATABLE.uid` of the composition the
    /// run reads.
    #[must_use]
    pub fn with_composition(mut self, uid: impl Into<String>) -> Self {
        self.composition = Some(uid.into());
        self
    }

    /// Returns this request with the openEHR positions of the occurrence the
    /// resource is created for, outermost first.
    #[must_use]
    pub fn with_occurrence(mut self, occurrence: Vec<u32>) -> Self {
        self.occurrence = occurrence;
        self
    }

    /// Returns this request with the `unique` values of a split.
    #[must_use]
    pub fn with_unique(mut self, unique: Vec<String>) -> Self {
        self.unique = unique;
        self
    }

    /// Returns the type of the resource created.
    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// Returns the type of the resource it is created under.
    #[must_use]
    pub fn parent_type(&self) -> &str {
        &self.parent_type
    }

    /// Returns the id of the resource it is created under, when known.
    #[must_use]
    pub fn parent_id(&self) -> Option<&str> {
        self.parent_id.as_deref()
    }

    /// Returns the `LOCATABLE.uid` of the composition read, when it has one.
    #[must_use]
    pub fn composition(&self) -> Option<&str> {
        self.composition.as_deref()
    }

    /// Returns the dotted name of the mapping or the context that creates it.
    #[must_use]
    pub fn mapping(&self) -> &str {
        &self.mapping
    }

    /// Returns the openEHR positions of the occurrence, outermost first.
    #[must_use]
    pub fn occurrence(&self) -> &[u32] {
        &self.occurrence
    }

    /// Returns the `unique` values of a split, in declaration order.
    #[must_use]
    pub fn unique(&self) -> &[String] {
        &self.unique
    }
}

/// What assigns the id of the resource a `$tofhir` run maps a composition to.
///
/// The engine "must implement its own ID management" and an export's resource
/// ids "must remain the same" every time a composition is exported
/// (`docs/specs/fhirconnect/modules/ROOT/pages/engine/id-management.adoc`,
/// §Resource IDs and §Deterministic IDs). A caller with an identity store
/// answers that here; without one the operations keep the id the mapping
/// wrote or derive it from the composition (`crate::operations::run`). The
/// shape of the seam is our own design.
pub trait ResourceIdentity: fmt::Debug + Send + Sync {
    /// Returns the FHIR id the mapped resource `request` describes takes.
    ///
    /// # Errors
    ///
    /// Returns [`ReferenceError`] when no id can be assigned, which refuses
    /// the run.
    fn identify(&self, request: &ResourceRequest) -> Result<String, ReferenceError>;
}

/// Everything a `$tofhir` run knows about the resource it maps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRequest {
    resource_type: String,
    template: String,
    composition: Option<String>,
    written: Option<String>,
    occurrence: Vec<u32>,
    entry_path: Option<String>,
}

impl ResourceRequest {
    /// Creates the request for a `resource_type` mapped from a composition of
    /// `template`.
    #[must_use]
    pub fn new(resource_type: impl Into<String>, template: impl Into<String>) -> Self {
        Self {
            resource_type: resource_type.into(),
            template: template.into(),
            composition: None,
            written: None,
            occurrence: Vec::new(),
            entry_path: None,
        }
    }

    /// Returns this request with the occurrence of the context's start
    /// archetype the resource maps, the 1-based instance of each repeating
    /// axis down to it.
    #[must_use]
    pub fn with_occurrence(mut self, occurrence: Vec<u32>) -> Self {
        self.occurrence = occurrence;
        self
    }

    /// Returns this request with the path that names the occurrence in the
    /// composition, the entry path of its deterministic id
    /// ([`crate::engine::identity::deterministic_id`]).
    #[must_use]
    pub fn with_entry_path(mut self, path: impl Into<String>) -> Self {
        self.entry_path = Some(path.into());
        self
    }

    /// Returns this request with the `OBJECT_VERSION_ID` of the composition.
    #[must_use]
    pub fn with_composition(mut self, uid: impl Into<String>) -> Self {
        self.composition = Some(uid.into());
        self
    }

    /// Returns this request with the `id` the mapping wrote.
    #[must_use]
    pub fn with_written(mut self, id: impl Into<String>) -> Self {
        self.written = Some(id.into());
        self
    }

    /// Returns the type of the mapped resource.
    #[must_use]
    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    /// Returns the template of the composition the run read.
    #[must_use]
    pub fn template(&self) -> &str {
        &self.template
    }

    /// Returns the `OBJECT_VERSION_ID` of the composition, when it has one.
    #[must_use]
    pub fn composition(&self) -> Option<&str> {
        self.composition.as_deref()
    }

    /// Returns the `id` the mapping wrote, when it wrote one.
    #[must_use]
    pub fn written(&self) -> Option<&str> {
        self.written.as_deref()
    }

    /// Returns the path that names the occurrence in the composition, when
    /// the run named one.
    #[must_use]
    pub fn entry_path(&self) -> Option<&str> {
        self.entry_path.as_deref()
    }

    /// Returns the occurrence of the start archetype the resource maps;
    /// empty when the start archetype is on no repeating axis.
    #[must_use]
    pub fn occurrence(&self) -> &[u32] {
        &self.occurrence
    }
}

/// The sink that derives an id from the request by a digest.
///
/// Every field is framed with its byte length before it is hashed, so two
/// different requests never frame to the same bytes, and the SHA-256 digest
/// renders as 64 lowercase hexadecimal characters, inside the R4 id grammar
/// `[A-Za-z0-9\-\.]{1,64}` (<https://hl7.org/fhir/R4/datatypes.html#id>).
/// No specification governs the derivation: our own design.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DerivedIdentity;

impl IdentitySink for DerivedIdentity {
    fn identify(&self, request: &IdentityRequest) -> Result<String, ReferenceError> {
        let mut hasher = sha2::Sha256::new();
        let mut frame = |field: &[u8]| {
            hasher.update(u64::try_from(field.len()).unwrap_or(u64::MAX).to_be_bytes());
            hasher.update(field);
        };
        frame(request.resource_type.as_bytes());
        frame(request.parent_type.as_bytes());
        frame(request.parent_id.as_deref().unwrap_or_default().as_bytes());
        frame(
            request
                .composition
                .as_deref()
                .unwrap_or_default()
                .as_bytes(),
        );
        frame(request.mapping.as_bytes());
        for position in &request.occurrence {
            frame(&position.to_be_bytes());
        }
        frame(b"unique");
        for value in &request.unique {
            frame(value.as_bytes());
        }
        let mut id = String::with_capacity(64);
        for byte in hasher.finalize().as_slice() {
            for nibble in [byte >> 4, byte & 0x0f] {
                id.extend(char::from_digit(u32::from(nibble), 16));
            }
        }
        Ok(id)
    }
}
