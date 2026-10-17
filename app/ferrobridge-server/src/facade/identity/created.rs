// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identity of a resource an outbound run creates beside the one it maps.
//!
//! Going out of openEHR, a `hierarchy.split` with `create: resource` and a
//! `reference` mapping both create resources
//! (`types-of-mappings/concept-type/HierarchyMappings.adoc`, §split, and
//! `types-of-mappings/concept-type/Reference.adoc`). A contained resource
//! "does not have an independent existence apart from the resource that
//! contains it" (<https://hl7.org/fhir/R4/references.html#contained>), so the
//! facade gives every created resource a binding of its own and serves it at
//! `[base]/<type>/<id>`. The binding names the entry its parent maps and the
//! parent itself, so a read maps the parent's entry again and answers the
//! created resource of that id. No specification governs the identity: our
//! own design.

use core::fmt;

use fhirconnect::engine::seam::IdentityRequest;
use fhirconnect::engine::seam::IdentitySink;
use fhirconnect::engine::seam::ReferenceError;

use crate::facade::identity::FhirResourceId;
use crate::facade::identity::derive;
use crate::facade::identity::record::CompositionBinding;
use crate::facade::identity::record::CreatedBy;
use crate::facade::identity::store::Store;
use crate::facade::identity::store::StoreError;

/// The identity sink of one outbound run over the entry a parent resource
/// maps.
///
/// The first time a created resource is identified, its id is derived from
/// the parent's entry and the engine's [`IdentityRequest`] and recorded with a
/// binding; from then on the store answers it, because the map wins once
/// written.
pub struct CreatedIdentities<'a> {
    /// The identity map the bindings are read from and recorded in.
    store: &'a dyn Store,
    /// The resource type of the parent.
    parent_type: &'a str,
    /// The logical id of the parent.
    parent: &'a FhirResourceId,
}

impl fmt::Debug for CreatedIdentities<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreatedIdentities")
            .field("parent_type", &self.parent_type)
            .field("parent", &self.parent)
            .finish_non_exhaustive()
    }
}

impl<'a> CreatedIdentities<'a> {
    /// Returns the sink of a run that maps the entry `parent_type/parent` is
    /// bound to.
    #[must_use]
    pub fn new(store: &'a dyn Store, parent_type: &'a str, parent: &'a FhirResourceId) -> Self {
        Self {
            store,
            parent_type,
            parent,
        }
    }

    /// Returns the id of the resource `request` describes, recording it and
    /// its binding the first time.
    fn bind(&self, request: &IdentityRequest) -> Result<FhirResourceId, CreatedError> {
        let parent = self
            .store
            .binding_of(self.parent_type, self.parent)?
            .ok_or_else(|| CreatedError::UnboundParent {
                parent: format!("{}/{}", self.parent_type, self.parent),
            })?;
        let created_type = request.resource_type();
        let key = derive::created_key(&parent, request);
        let map_key = derive::created_map_key(&key);
        let id = match self.store.internal_of(created_type, &map_key)? {
            Some(known) => known,
            None => {
                self.store
                    .record_internal(created_type, &map_key, &derive::created_digest(&key))?
            }
        };
        let binding = CompositionBinding {
            resource_type: created_type.to_owned(),
            created: Some(CreatedBy {
                parent_type: self.parent_type.to_owned(),
                parent_id: self.parent.as_str().to_owned(),
                mapping: request.mapping().to_owned(),
            }),
            ..parent
        };
        let stood = self.store.record_binding(created_type, &id, &binding)?;
        if stood.created.is_none() {
            return Err(CreatedError::Taken {
                resource: format!("{created_type}/{id}"),
            });
        }
        Ok(id)
    }
}

impl IdentitySink for CreatedIdentities<'_> {
    fn identify(&self, request: &IdentityRequest) -> Result<String, ReferenceError> {
        self.bind(request)
            .map(|id| id.as_str().to_owned())
            .map_err(|source| ReferenceError::Identity {
                resource_type: request.resource_type().to_owned(),
                source: Box::new(source),
            })
    }
}

/// Why a created resource could not be given its identity.
#[derive(Debug, thiserror::Error)]
pub enum CreatedError {
    /// The identity map holds no binding for the parent the run maps.
    #[error("the identity map holds no binding for {parent}, which creates the resource")]
    UnboundParent {
        /// The parent, as the wire names it.
        parent: String,
    },
    /// The derived id is already bound to an entry of its own.
    #[error("{resource} is already bound to a composition entry of its own")]
    Taken {
        /// The resource, as the wire names it.
        resource: String,
    },
    /// The identity map could not be read or written.
    #[error(transparent)]
    Store(#[from] StoreError),
}
