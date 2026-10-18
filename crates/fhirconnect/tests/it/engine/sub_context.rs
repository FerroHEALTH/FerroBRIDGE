// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `slotContext` run in both directions: the entered context maps the
//! referenced resource into the parent composition going in and back out of
//! it, and the resource it pulls in is named as consumed
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/sub-contexts.adoc`).

use core::error::Error;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::RmPosition;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;

use crate::engine::MapIdentities;
use crate::engine::MapReferences;
use crate::engine::defaults;
use crate::engine::reference::condition_with;
use crate::resolve::borrow;
use crate::resolve::context;
use crate::resolve::model;
use crate::resolve::program_of;
use crate::resolve::render;
use crate::resolve::set_of;
use crate::resolve::start_model;
use crate::support::template;

/// The literal reference the synthetic Condition's evidence carries.
const EVIDENCE: &str = "Observation/synthetic-observation-2";

/// Compiles a parent context whose evidence `reference` enters the
/// sub-context `evidence.context` at the problem qualifier slot.
fn program() -> Result<Arc<Program>, Box<dyn Error>> {
    let parent = start_model(
        "mappings:\n  - name: \"problem\"\n    with:\n      fhir: \"$resource.code\"\n      openehr: \
         \"$archetype/data[at0001]/items[at0002]\"\n      type: \"CODEABLECONCEPT\"\n  - name: \"evidence\"\n    with:\n      fhir: \
         \"$resource.evidence.detail\"\n      openehr: \"$reference\"\n    reference:\n      \
         resourceType: \"Observation\"\n      mappings:\n        - name: \"qualifier\"\n          \
         with:\n            fhir: \"$fhirRoot\"\n            openehr: \
         \"$archetype/data[at0001]/items[openEHR-EHR-CLUSTER.problem_qualifier.v2]\"\n          \
         slotContext: \"evidence.context\"\n",
    );
    let cluster = model(
        "CLUSTER.evidence.v2",
        "openEHR-EHR-CLUSTER.problem_qualifier.v2",
        "mappings:\n  - name: \"code\"\n    with:\n      fhir: \"$fhirRoot.code\"\n      openehr: \
         \"$archetype/items[at0063]\"\n      type: \"CODEABLECONCEPT\"\n",
    );
    let parent_context = context(
        "parent.context",
        "  profile:\n    url: \"http://example.org/fhir/StructureDefinition/synthetic\"\n  \
         template:\n    id: \"ferrobridge.diagnose.v1\"\n  archetypes:\n    - \
         \"EVALUATION.synthetic.v1\"\n  contexts:\n    - \"evidence.context\"\n  start: \
         \"EVALUATION.synthetic.v1\"\n",
    );
    let evidence_context = context(
        "evidence.context",
        "  scope: \"sub\"\n  profile:\n    url: \
         \"http://example.org/fhir/StructureDefinition/synthetic-evidence\"\n  template:\n    \
         id: \"ferrobridge.diagnose.v1\"\n  archetypes:\n    - \"CLUSTER.evidence.v2\"\n  start: \
         \"CLUSTER.evidence.v2\"\n",
    );
    let files = [
        ("parent.yml", parent),
        ("cluster.yml", cluster),
        ("parent.context.yml", parent_context),
        ("evidence.context.yml", evidence_context),
    ];
    let set = set_of(&borrow(&files))?;
    Ok(program_of(&set, "parent.context").map_err(|diagnostics| render(&diagnostics))?)
}

/// The synthetic `Observation` the evidence reference points at, coded.
fn evidence() -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "Observation",
        "id": "synthetic-observation-2",
        "status": "final",
        "code": {"coding": [{
            "system": "http://example.org/fhir/CodeSystem/synthetic",
            "code": "s-1",
            "display": "Synthetic finding"
        }]}
    }))
}

/// The code string the qualifier element of `composition` holds.
fn qualifier_code(
    index: &WebTemplateIndex,
    composition: &CanonicalComposition,
) -> Result<Option<String>, Box<dyn Error>> {
    let node = index.node(&AqlPath::new(
        "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[openEHR-EHR-CLUSTER.problem_qualifier.v2]/items[at0063]/value",
    ))?;
    // The qualifier cluster repeats, so its first instance is named.
    let first = [RmPosition::first()];
    Ok(index.read(composition, node, &first)?.and_then(|value| {
        value
            .get("defining_code")?
            .get("code_string")?
            .as_str()
            .map(String::from)
    }))
}

/// Maps the synthetic Condition into openEHR with the evidence resolvable.
fn inbound(
    program: &Program,
    index: &WebTemplateIndex,
) -> Result<fhirconnect::engine::outcome::Outcome<CanonicalComposition>, Box<dyn Error>> {
    let mut held = BTreeMap::new();
    held.insert(String::from(EVIDENCE), evidence());
    let references = MapReferences(held);
    Ok(to_openehr(
        program,
        &SCHEMAS,
        index,
        &condition_with(serde_json::json!({
            "evidence": [{"detail": [{"reference": EVIDENCE}]}]
        }))?,
        &Seams::default().with_references(&references),
        &defaults(),
        &CallContext::new(),
    )?)
}

#[test]
fn a_sub_context_maps_the_referenced_resource_into_the_parent_composition()
-> Result<(), Box<dyn Error>> {
    // sub-contexts.adoc §Rules: "The sub-context maps *into the parent composition*".
    let program = program()?;
    let index = template()?;
    let outcome = inbound(&program, &index)?;
    assert_eq!(
        qualifier_code(&index, outcome.value())?.as_deref(),
        Some("s-1"),
        "the entered context wrote the Observation's code into the parent's cluster"
    );
    Ok(())
}

#[test]
fn a_resource_a_sub_context_pulls_in_is_named_consumed() -> Result<(), Box<dyn Error>> {
    // engine/bundles.adoc §Resources consumed by a sub-context: such a resource "is marked as
    // resolved", so the caller splitting a Bundle does not process it again as a root.
    let program = program()?;
    let index = template()?;
    let outcome = inbound(&program, &index)?;
    assert_eq!(outcome.consumed(), [EVIDENCE]);
    Ok(())
}

#[test]
fn a_sub_context_maps_the_parent_composition_back_into_the_resource() -> Result<(), Box<dyn Error>>
{
    let program = program()?;
    let index = template()?;
    let composition = inbound(&program, &index)?.into_value();
    let mut ids = BTreeMap::new();
    ids.insert(
        String::from("Observation"),
        String::from("created-observation-2"),
    );
    let identities = MapIdentities {
        ids,
        seen: Mutex::default(),
    };
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default().with_identities(&identities),
        &CallContext::new(),
    )?;
    let created = outbound
        .created()
        .first()
        .ok_or("the run created the Observation")?;
    let code = created
        .get("code")
        .and_then(|code| code.get("coding"))
        .and_then(|coding| coding.as_array()?.first())
        .and_then(|coding| coding.get("code"))
        .and_then(Value::as_str);
    assert_eq!(
        code,
        Some("s-1"),
        "the entered context read the cluster into the Observation"
    );
    Ok(())
}
