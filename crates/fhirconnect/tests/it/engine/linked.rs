// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The linked composition of a `link` whose FHIR side names a resource
//! (`types-of-mappings/concept-type/concept-mappings.adoc`, §Linked mappings).

use core::error::Error;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::PoisonError;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::linked::Linked;
use fhirconnect::engine::linked::LinkedCompositions;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::Outcome;
use fhirconnect::engine::outcome::SkipReason;
use fhirconnect::engine::seam::ReferenceError;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::to_openehr;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;
use openehr_rm::v1_2::paths::EhrUri;

use crate::engine::MapReferences;
use crate::engine::defaults;
use crate::engine::reference::condition_with;
use crate::support::compiled;
use crate::support::template;

/// The reference the Condition's `encounter` carries.
const ENCOUNTER: &str = "Encounter/synthetic-encounter-1";

/// The `ehr:` URI the stub answers for the linked Encounter's composition.
const COMMITTED: &str =
    "ehr:/3a2f1c4e-0000-4000-8000-0000000000ab/compositions/d2b3c1a0-0000-4000-8000-000000000001";

/// A caller that answers one fixed outcome and records what it was asked.
#[derive(Debug)]
struct Answering {
    answer: Result<Linked, &'static str>,
    asked: Mutex<Vec<(String, Option<String>)>>,
}

impl Answering {
    fn new(answer: Result<Linked, &'static str>) -> Self {
        Self {
            answer,
            asked: Mutex::default(),
        }
    }
}

/// The failure [`Answering`] reports when its answer is an error.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct Refused(&'static str);

impl LinkedCompositions for Answering {
    fn composition(&self, reference: &str, resource: &Value) -> Result<Linked, ReferenceError> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((
                String::from(reference),
                resource
                    .get("resourceType")
                    .and_then(Value::as_str)
                    .map(String::from),
            ));
        self.answer
            .clone()
            .map_err(|message| ReferenceError::Fetch {
                reference: String::from(reference),
                source: Box::new(Refused(message)),
            })
    }
}

/// Returns the synthetic Encounter the Condition links.
fn encounter() -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Encounter",
        "id": "synthetic-encounter-1",
        "status": "finished",
        "class": {"system": "http://terminology.hl7.org/CodeSystem/v3-ActCode", "code": "AMB"}
    }))
}

/// What one run of the linking Condition answers.
type Ran = Result<Outcome<CanonicalComposition>, EngineError>;

/// Maps the linking Condition with `references` holding the linked
/// resources and `linked` answering where they stand.
fn run(
    references: &MapReferences,
    linked: &dyn LinkedCompositions,
) -> Result<(Ran, WebTemplateIndex), Box<dyn Error>> {
    let program = compiled("ferrobridge_link")?;
    let index = template()?;
    let ran = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &condition_with(serde_json::json!({"encounter": {"reference": ENCOUNTER}}))?,
        &Seams::default()
            .with_references(references)
            .with_linked(linked),
        &defaults(),
        &CallContext::new(),
    );
    Ok((ran, index))
}

/// Returns the references source holding the synthetic Encounter.
fn holding_encounter() -> MapReferences {
    let mut held = BTreeMap::new();
    held.insert(String::from(ENCOUNTER), encounter());
    MapReferences(held)
}

/// Returns the `LINK.target` values the problem-diagnosis entry carries.
fn link_targets(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<Vec<String>, Box<dyn Error>> {
    let entry = index.node(&AqlPath::new(
        "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]",
    ))?;
    let written = index
        .read(composition, entry, &[])?
        .ok_or("the entry was written")?;
    Ok(written
        .get("links")
        .and_then(serde_json::Value::as_array)
        .map(|links| {
            links
                .iter()
                .filter_map(|link| link.pointer("/target/value")?.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

#[test]
fn a_link_to_a_resource_targets_the_composition_it_was_committed_as() -> Result<(), Box<dyn Error>>
{
    // concept-mappings.adoc §Linked mappings: the linked resource is "saved
    // as a separate composition", which "is linked using the openEHR links";
    // LINK.target is a DV_EHR_URI (RM 1.1.0 data_types.html §DV_EHR_URI).
    let linker = Answering::new(Ok(Linked::At(COMMITTED.parse::<EhrUri>()?)));
    let (ran, index) = run(&holding_encounter(), &linker)?;
    let inbound = ran?;
    assert_eq!(link_targets(&index, inbound.value())?, [COMMITTED]);
    assert_eq!(
        *linker.asked.lock().unwrap_or_else(PoisonError::into_inner),
        [(String::from(ENCOUNTER), Some(String::from("Encounter")))],
        "the caller was handed the resource the reference resolved to"
    );
    Ok(())
}

#[test]
fn a_link_to_a_resource_no_context_maps_is_a_declared_skip() -> Result<(), Box<dyn Error>> {
    // "If a context mapping for this `meta.url` is found, it is executed"
    // (§Linked mappings): with none, no composition exists to link.
    let (ran, index) = run(&holding_encounter(), &Answering::new(Ok(Linked::Unmapped)))?;
    let inbound = ran?;
    assert_eq!(link_targets(&index, inbound.value())?, Vec::<String>::new());
    let reasons: Vec<&SkipReason> = inbound
        .warnings()
        .iter()
        .filter_map(|warning| match *warning.loss() {
            Loss::Skipped { ref reason, .. } => Some(reason),
            _ => None,
        })
        .collect();
    assert_eq!(
        reasons,
        [&SkipReason::UnmappedLink {
            reference: String::from(ENCOUNTER)
        }]
    );
    Ok(())
}

#[test]
fn a_link_to_a_reference_that_resolves_to_nothing_is_a_declared_skip() -> Result<(), Box<dyn Error>>
{
    // engine/references.adoc: a resource that cannot be resolved lets the
    // engine proceed, and the skip is declared.
    let linker = Answering::new(Ok(Linked::Unmapped));
    let (ran, _) = run(&MapReferences::default(), &linker)?;
    let inbound = ran?;
    assert!(
        inbound.warnings().iter().any(|warning| matches!(
            *warning.loss(),
            Loss::Skipped {
                reason: SkipReason::UnresolvedReference { ref reference },
                ..
            } if reference == ENCOUNTER
        )),
        "{:?}",
        inbound.warnings()
    );
    assert!(
        linker
            .asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty(),
        "nothing is committed for a resource that was not resolved"
    );
    Ok(())
}

#[test]
fn a_linked_composition_the_caller_cannot_commit_refuses_the_run() -> Result<(), Box<dyn Error>> {
    // A failed commit is an upstream failure and never a skipped link.
    let (ran, _) = run(
        &holding_encounter(),
        &Answering::new(Err("the CDR refused the linked composition")),
    )?;
    let error = ran.expect_err("the failed commit refuses");
    assert!(matches!(error, EngineError::Reference { .. }), "{error:?}");
    Ok(())
}
