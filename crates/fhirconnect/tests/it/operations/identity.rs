// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The id of the resource `$tofhir` maps, from a caller's identity seam.
//!
//! No specification governs identity across runs: our own design. Without a
//! seam the id derives from the written `id` or the composition's
//! `versioned_object_uid`, which `runs.rs` pins.

use core::error::Error;
use std::sync::Mutex;

use fhir_types::r4::resource::Resource;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::seam::ReferenceError;
use fhirconnect::engine::seam::ResourceIdentity;
use fhirconnect::engine::seam::ResourceRequest;
use fhirconnect::engine::seam::Seams;
use fhirconnect::operations::contract::CompositionPayload;
use fhirconnect::operations::contract::ToFhirRequest;
use fhirconnect::operations::contract::ToFhirResponse;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::run;

use crate::operations::COMPOSITION_UID;
use crate::operations::SUBJECT_CONTEXT;
use crate::operations::TEMPLATE;
use crate::operations::committed_composition;
use crate::operations::program_set;
use crate::operations::settings;

/// An identity store that answers one fixed id, recording every request.
#[derive(Debug)]
struct Store {
    answer: Result<String, String>,
    seen: Mutex<Vec<ResourceRequest>>,
}

impl Store {
    /// Creates a store that answers `answer`, or refuses with its text.
    fn new(answer: Result<&str, &str>) -> Self {
        Self {
            answer: answer.map(String::from).map_err(String::from),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Returns every request the store answered.
    fn seen(&self) -> Vec<ResourceRequest> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl ResourceIdentity for Store {
    fn identify(&self, request: &ResourceRequest) -> Result<String, ReferenceError> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(request.clone());
        self.answer
            .clone()
            .map_err(|reason| ReferenceError::Identity {
                resource_type: String::from(request.resource_type()),
                source: Box::from(reason),
            })
    }
}

/// Runs `$tofhir` over the committed synthetic composition under `seams`.
fn tofhir(seams: &Seams<'_>) -> Result<Result<ToFhirResponse, OperationError>, Box<dyn Error>> {
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let request = ToFhirRequest::new(CompositionPayload::parse(
        &committed_composition()?.to_string(),
    )?);
    Ok(run::to_fhir(&set, &SCHEMAS, seams, &settings(), &request))
}

/// Returns the id of the mapped `Condition` an answer carries.
fn condition_id(answer: &ToFhirResponse) -> Option<String> {
    answer
        .bundle()
        .entry
        .iter()
        .find_map(|entry| match entry.resource {
            Some(Resource::Condition(ref carried)) => carried.id.as_deref().map(String::from),
            _ => None,
        })
}

#[test]
fn a_seam_assigns_the_mapped_resource_its_id() -> Result<(), Box<dyn Error>> {
    let store = Store::new(Ok("stored-condition-1"));
    let answer = tofhir(&Seams::default().with_resource_identity(&store))??;
    assert_eq!(condition_id(&answer).as_deref(), Some("stored-condition-1"));
    let seen = store.seen();
    let [request] = seen.as_slice() else {
        return Err(format!("expected one identity request, got {seen:?}").into());
    };
    assert_eq!(request.resource_type(), "Condition");
    assert_eq!(request.template(), TEMPLATE);
    assert_eq!(request.composition(), Some(COMPOSITION_UID));
    Ok(())
}

#[test]
fn the_provenance_targets_the_id_the_seam_assigned() -> Result<(), Box<dyn Error>> {
    let store = Store::new(Ok("stored-condition-1"));
    let answer = tofhir(&Seams::default().with_resource_identity(&store))??;
    let targets: Vec<String> = answer
        .bundle()
        .entry
        .iter()
        .filter_map(|entry| match entry.resource {
            Some(Resource::Provenance(ref provenance)) => Some(provenance.target.clone()),
            _ => None,
        })
        .flatten()
        .filter_map(|target| target.reference.and_then(|text| text.value))
        .collect();
    assert_eq!(targets, [String::from("Condition/stored-condition-1")]);
    Ok(())
}

#[test]
fn a_seam_that_refuses_refuses_the_run() -> Result<(), Box<dyn Error>> {
    let store = Store::new(Err("the identity store is unavailable"));
    let error = tofhir(&Seams::default().with_resource_identity(&store))?
        .err()
        .ok_or("a refused identity is never a derived one")?;
    assert!(
        matches!(error, OperationError::Identity { ref resource, .. } if resource == "Condition"),
        "{error}"
    );
    assert_eq!(error.issue_type(), "processing");
    Ok(())
}

#[test]
fn a_seam_id_outside_the_id_grammar_is_refused() -> Result<(), Box<dyn Error>> {
    // R4 datatypes.html#id: `[A-Za-z0-9\-\.]{1,64}`.
    let store = Store::new(Ok("not an id"));
    let error = tofhir(&Seams::default().with_resource_identity(&store))?
        .err()
        .ok_or("an id outside the grammar never reaches the Bundle")?;
    assert!(
        matches!(error, OperationError::InvalidId { ref id, .. } if id == "not an id"),
        "{error}"
    );
    Ok(())
}

#[test]
fn without_a_seam_the_id_still_derives_from_the_composition() -> Result<(), Box<dyn Error>> {
    let answer = tofhir(&Seams::default())??;
    assert_eq!(
        condition_id(&answer).as_deref(),
        COMPOSITION_UID.split("::").next()
    );
    Ok(())
}
