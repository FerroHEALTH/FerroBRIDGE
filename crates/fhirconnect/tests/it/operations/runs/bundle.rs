// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Bundle a run answers: every created resource as an entry, the
//! diagnosis chain both ways, and the one Provenance.

use core::error::Error;

use fhir_types::codec::Json;
use fhir_types::codec::Path;
use fhir_types::r4::resource::Resource;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::seam::Seams;
use fhirconnect::operations::contract::CompositionPayload;
use fhirconnect::operations::contract::ToFhirRequest;
use fhirconnect::operations::contract::ToOpenehrRequest;
use fhirconnect::operations::run;
use fhirconnect::resolve::program::binding::TemplateId;

use crate::operations::COMPOSITION_UID;
use crate::operations::DEVICE;
use crate::operations::NOW;
use crate::operations::SUBJECT_CONTEXT;
use crate::operations::TEMPLATE;
use crate::operations::bundle;
use crate::operations::committed_composition;
use crate::operations::condition;
use crate::operations::program_set;
use crate::operations::settings;

#[test]
fn a_split_answers_every_created_resource_as_an_entry() -> Result<(), Box<dyn Error>> {
    // HierarchyMappings.adoc §split: each anatomical-location cluster creates
    // its own Condition, and the one Provenance covers every one of them.
    let set = program_set(&["ferrobridge_split"])?;
    let mut resource = condition("http://example.org/fhir/StructureDefinition/ferrobridge-split")?;
    if let Some(object) = resource.as_object_mut() {
        object.insert(
            String::from("bodySite"),
            serde_json::json!([{"text": "Left knee"}, {"text": "Right knee"}]),
        );
    }
    let inbound = run::to_openehr(
        &set,
        &SCHEMAS,
        &Seams::default(),
        &settings(),
        &ToOpenehrRequest::new(bundle(vec![resource])?),
    )?;
    let mut built: serde_json::Value = serde_json::from_str(inbound.composition())?;
    if let Some(object) = built.as_object_mut() {
        object.insert(
            String::from("uid"),
            serde_json::json!({ "_type": "OBJECT_VERSION_ID", "value": COMPOSITION_UID }),
        );
    }
    let request = ToFhirRequest::new(CompositionPayload::parse(&built.to_string())?);
    let answer = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    let conditions: Vec<String> = answer
        .bundle()
        .entry
        .iter()
        .filter_map(|entry| match entry.resource {
            Some(Resource::Condition(ref condition)) => condition.id.clone(),
            _ => None,
        })
        .collect();
    assert_eq!(
        conditions.len(),
        2,
        "one Condition per cluster: {conditions:?}"
    );
    let targets = answer
        .bundle()
        .entry
        .iter()
        .find_map(|entry| match entry.resource {
            Some(Resource::Provenance(ref provenance)) => Some(provenance.target.len()),
            _ => None,
        })
        .ok_or("the Bundle carries the Provenance")?;
    assert_eq!(targets, 2, "the Provenance covers both Conditions");
    Ok(())
}

#[test]
fn the_diagnosis_chain_maps_both_ways_through_the_operations() -> Result<(), Box<dyn Error>> {
    // A canonical composition in, a Condition Bundle with a Provenance out;
    // that Bundle back in, the same composition out.
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let committed = committed_composition()?;
    let request = ToFhirRequest::new(CompositionPayload::parse(&committed.to_string())?);
    let answer = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    let carried = answer.bundle();
    assert_eq!(carried.r#type.value.as_deref(), Some("collection"));
    let mapped = carried
        .entry
        .first()
        .and_then(|entry| entry.resource.as_ref())
        .ok_or("the Bundle carries the mapped resource first")?;
    let Resource::Condition(ref condition) = *mapped else {
        return Err(Box::<dyn Error>::from("the mapped resource is a Condition"));
    };
    assert_eq!(
        condition
            .code
            .as_ref()
            .and_then(|code| code.text.as_ref())
            .and_then(|text| text.value.clone()),
        Some(String::from("Synthetic problem one")),
        "the problem name survived the round trip"
    );
    let back = ToOpenehrRequest::new(bundle(vec![
        fhir_types::codec::Value::Object(Json::to_json(mapped)?)
            .to_serde_json(&mut Path::root("Condition"))?,
    ])?)
    .with_template_id(TemplateId::new(TEMPLATE));
    let second = run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &back)?;
    let mut reread: serde_json::Value = serde_json::from_str(second.composition())?;
    // The FEEDER_AUDIT names the resource each pass read (RM 1.1.0
    // common.html §FEEDER_AUDIT), and the second pass reads the Condition
    // `$tofhir` identified, so the audit is asserted on its own and the
    // content is compared without it.
    let audit = reread
        .as_object_mut()
        .and_then(|object| object.remove("feeder_audit"))
        .ok_or("the second pass records its origin")?;
    assert_eq!(
        audit
            .pointer("/originating_system_item_ids/0/id")
            .and_then(serde_json::Value::as_str),
        condition.id.as_deref(),
        "the second pass records the Condition it read: {audit}"
    );
    let first_pass: serde_json::Value = {
        let mut without = committed.clone();
        if let Some(object) = without.as_object_mut() {
            object.remove("uid");
            object.remove("feeder_audit");
        }
        without
    };
    assert_eq!(
        reread, first_pass,
        "the second pass produced another composition"
    );
    Ok(())
}

#[test]
fn every_tofhir_bundle_carries_one_provenance_covering_every_entry() -> Result<(), Box<dyn Error>> {
    // "The engine generates a Provenance resource as a side-effect of every
    // run ... and includes it in the Bundle" (rest-api.adoc, section
    // $tofhir Output).
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let committed = committed_composition()?;
    let request = ToFhirRequest::new(CompositionPayload::parse(&committed.to_string())?);
    let answer = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    let provenances: Vec<&fhir_types::r4::provenance::Provenance> = answer
        .bundle()
        .entry
        .iter()
        .filter_map(|entry| match entry.resource {
            Some(Resource::Provenance(ref carried)) => Some(carried.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(provenances.len(), 1, "a run carries exactly one Provenance");
    let provenance = provenances.first().ok_or("one Provenance")?;
    let mapped: Vec<String> = answer
        .bundle()
        .entry
        .iter()
        .filter_map(|entry| match entry.resource {
            Some(Resource::Condition(ref condition)) => {
                condition.id.as_ref().map(|id| format!("Condition/{id}"))
            }
            _ => None,
        })
        .collect();
    let targets: Vec<String> = provenance
        .target
        .iter()
        .filter_map(|target| target.reference.as_ref())
        .filter_map(|value| value.value.clone())
        .collect();
    assert_eq!(targets, mapped, "the targets cover every mapped entry");
    assert_eq!(
        provenance.recorded.value.as_deref(),
        Some(NOW),
        "recorded is the run time"
    );
    assert_eq!(
        provenance
            .agent
            .first()
            .and_then(|agent| agent.who.reference.as_ref())
            .and_then(|value| value.value.clone()),
        Some(String::from(DEVICE)),
        "a call with no context.who gets the configured device"
    );
    assert_eq!(
        provenance
            .entity
            .first()
            .map(|entity| entity.role.value.clone()),
        Some(Some(String::from("derivation")))
    );
    Ok(())
}
