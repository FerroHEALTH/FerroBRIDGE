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
use std::sync::Mutex;

use fhirconnect::engine::seam::ReferenceError;
use fhirconnect::engine::seam::identity::IdentityRequest;
use fhirconnect::engine::seam::identity::IdentitySink;

use crate::identity::FhirResourceId;
use crate::identity::derive;
use crate::identity::record::CompositionBinding;
use crate::identity::record::CreatedBy;
use crate::identity::store::Store;
use crate::identity::store::StoreError;

/// The identity sink of one outbound run over the entry a parent resource
/// maps.
///
/// A created resource's id is derived from the parent's entry and the
/// engine's [`IdentityRequest`], so the run reads no store: the sink derives
/// each id and keeps the request, and [`CreatedIdentities::persist`] records
/// the ids and their bindings once the run has succeeded. The map wins once
/// written, so a stored id other than the derived one refuses the answer.
pub struct CreatedIdentities<'a> {
    /// The identity map the bindings are recorded in.
    store: &'a dyn Store,
    /// The resource type of the parent.
    parent_type: &'a str,
    /// The logical id of the parent.
    parent: &'a FhirResourceId,
    /// The binding of the parent, read before the run, when the map holds
    /// one.
    binding: Option<CompositionBinding>,
    /// The resources the run identified, in the order it asked.
    asked: Mutex<Vec<(String, String, FhirResourceId)>>,
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
    ///
    /// # Errors
    ///
    /// Returns [`CreatedError::Store`] when the map cannot be read.
    pub async fn new(
        store: &'a dyn Store,
        parent_type: &'a str,
        parent: &'a FhirResourceId,
    ) -> Result<Self, CreatedError> {
        let binding = store.binding_of(parent_type, parent).await?;
        Ok(Self {
            store,
            parent_type,
            parent,
            binding,
            asked: Mutex::new(Vec::new()),
        })
    }

    /// Records every id the run derived and its binding, the first time.
    ///
    /// # Errors
    ///
    /// Returns [`CreatedError::Taken`] when the map holds another id for a
    /// created resource or binds its id to an entry of its own, and
    /// [`CreatedError::Store`] when the map cannot be read or written.
    pub async fn persist(self) -> Result<(), CreatedError> {
        let asked = self
            .asked
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(parent) = self.binding else {
            return Ok(());
        };
        for (created_type, mapping, derived) in asked {
            let map_key = derive::created_map_key(&derived);
            let id = self
                .store
                .record_internal(&created_type, &map_key, &derived)
                .await?;
            let binding = CompositionBinding {
                resource_type: created_type.clone(),
                created: Some(CreatedBy {
                    parent_type: self.parent_type.to_owned(),
                    parent_id: self.parent.as_str().to_owned(),
                    mapping,
                }),
                ..parent.clone()
            };
            let stood = self
                .store
                .record_binding(&created_type, &id, &binding)
                .await?;
            if id != derived || stood.created.is_none() {
                return Err(CreatedError::Taken {
                    resource: format!("{created_type}/{id}"),
                });
            }
        }
        Ok(())
    }
}

impl IdentitySink for CreatedIdentities<'_> {
    fn identify(&self, request: &IdentityRequest) -> Result<String, ReferenceError> {
        let binding = self
            .binding
            .as_ref()
            .ok_or_else(|| ReferenceError::Identity {
                resource_type: request.resource_type().to_owned(),
                source: Box::new(CreatedError::UnboundParent {
                    parent: format!("{}/{}", self.parent_type, self.parent),
                }),
            })?;
        let derived = derive::created_id(binding, request);
        let answer = derived.as_str().to_owned();
        self.asked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((
                request.resource_type().to_owned(),
                request.mapping().to_owned(),
                derived,
            ));
        Ok(answer)
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
