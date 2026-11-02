// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The call context of a run: the provenance agent, the context patient
//! and the context members a mapping reads.

use core::error::Error;

use fhir_types::r4::resource::Resource;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::operations::contract::CompositionPayload;
use fhirconnect::operations::contract::ToFhirRequest;
use fhirconnect::operations::contract::ToOpenehrRequest;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::run;
use fhirconnect::resolve::program::binding::TemplateId;

use crate::operations::MINIMAL_PROFILE;
use crate::operations::SUBJECT_CONTEXT;
use crate::operations::TEMPLATE;
use crate::operations::bundle;
use crate::operations::committed_composition;
use crate::operations::condition;
use crate::operations::program_set;
use crate::operations::reference;
use crate::operations::settings;

#[test]
fn the_call_context_overrides_the_provenance_agent() -> Result<(), Box<dyn Error>> {
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let committed = committed_composition()?;
    let request = ToFhirRequest::new(CompositionPayload::parse(&committed.to_string())?)
        .with_context(
            CallContext::new()
                .with_who(reference("Practitioner/456"))
                .with_on_behalf_of(reference("Organization/charite")),
        );
    let answer = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    let provenance = answer
        .bundle()
        .entry
        .iter()
        .find_map(|entry| match entry.resource {
            Some(Resource::Provenance(ref carried)) => Some(carried.as_ref()),
            _ => None,
        })
        .ok_or("the Bundle carries a Provenance")?;
    let agent = provenance.agent.first().ok_or("one agent")?;
    assert_eq!(
        agent.who.reference.as_ref().and_then(|v| v.value.clone()),
        Some(String::from("Practitioner/456"))
    );
    assert_eq!(
        agent
            .on_behalf_of
            .as_ref()
            .and_then(|value| value.reference.as_ref())
            .and_then(|value| value.value.clone()),
        Some(String::from("Organization/charite"))
    );
    Ok(())
}

#[test]
fn the_context_patient_takes_precedence_over_the_mapped_subject() -> Result<(), Box<dyn Error>> {
    // "When the engine can resolve the subject itself and `context.patient` is
    // also supplied, the supplied value takes precedence" (rest-api.adoc,
    // section Resolving the patient).
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let committed = committed_composition()?;
    let request = ToFhirRequest::new(CompositionPayload::parse(&committed.to_string())?)
        .with_context(CallContext::new().with_patient(reference("Patient/123")));
    let answer = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    let condition = answer
        .bundle()
        .entry
        .iter()
        .find_map(|entry| match entry.resource {
            Some(Resource::Condition(ref carried)) => Some(carried.as_ref()),
            _ => None,
        })
        .ok_or("the Bundle carries the mapped Condition")?;
    assert_eq!(
        condition
            .subject
            .reference
            .as_ref()
            .and_then(|value| value.value.clone()),
        Some(String::from("Patient/123"))
    );
    Ok(())
}

#[test]
fn a_call_that_omits_the_context_patient_still_succeeds() -> Result<(), Box<dyn Error>> {
    // "An engine MUST NOT require `context.patient`" (rest-api.adoc,
    // section Resolving the patient).
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let committed = committed_composition()?;
    let request = ToFhirRequest::new(CompositionPayload::parse(&committed.to_string())?);
    let answer = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    let condition = answer
        .bundle()
        .entry
        .iter()
        .find_map(|entry| match entry.resource {
            Some(Resource::Condition(ref carried)) => Some(carried.as_ref()),
            _ => None,
        })
        .ok_or("the Bundle carries the mapped Condition")?;
    assert_eq!(
        condition
            .subject
            .reference
            .as_ref()
            .and_then(|value| value.value.clone()),
        Some(String::from("Patient/synthetic-subject-0001")),
        "the subject the chain resolved on its own survived"
    );
    Ok(())
}

#[test]
fn a_context_member_the_caller_supplied_reaches_the_mapping() -> Result<(), Box<dyn Error>> {
    // "`$context` holds values passed in on the REST call ... a context value
    // is referenced from a `manual` `value`" (basics/Variables.adoc).
    let set = program_set(&["ferrobridge_diagnose_context"])?;
    let committed = committed_composition()?;
    let request = ToFhirRequest::new(CompositionPayload::parse(&committed.to_string())?)
        .with_template_id(TemplateId::new(TEMPLATE))
        .with_context(CallContext::new().with_who(reference("Practitioner/456")));
    let answer = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    let condition = answer
        .bundle()
        .entry
        .iter()
        .find_map(|entry| match entry.resource {
            Some(Resource::Condition(ref carried)) => Some(carried.as_ref()),
            _ => None,
        })
        .ok_or("the Bundle carries the mapped Condition")?;
    assert_eq!(
        condition
            .asserter
            .as_ref()
            .and_then(|value| value.reference.as_ref())
            .and_then(|value| value.value.clone()),
        Some(String::from("Practitioner/456"))
    );
    Ok(())
}

#[test]
fn a_context_member_the_caller_did_not_supply_is_a_refusal() -> Result<(), Box<dyn Error>> {
    // A `$context` member is a per-call value, so naming one the call does not
    // carry is a refusal rather than an invented value.
    let set = program_set(&["ferrobridge_diagnose_context"])?;
    let committed = committed_composition()?;
    let request = ToFhirRequest::new(CompositionPayload::parse(&committed.to_string())?)
        .with_template_id(TemplateId::new(TEMPLATE));
    let error = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)
        .err()
        .ok_or("the mapping names a member the call does not carry")?;
    let rendered = error.to_string();
    assert!(
        matches!(error, OperationError::Mapping { .. }),
        "{rendered}"
    );
    assert!(
        error.outcome().issue.first().is_some_and(|issue| issue
            .details
            .as_ref()
            .and_then(|details| details.text.as_ref())
            .and_then(|text| text.value.as_deref())
            .is_some_and(|text| text.contains("$context.who"))),
        "the outcome names the member the call did not carry"
    );
    Ok(())
}

#[test]
fn a_bundle_referencing_two_subjects_is_refused_naming_them() -> Result<(), Box<dyn Error>> {
    let set = program_set(&["ferrobridge_diagnose_minimal"])?;
    let mut first = condition(MINIMAL_PROFILE)?;
    if let Some(object) = first.as_object_mut() {
        object.insert(
            String::from("subject"),
            serde_json::json!({ "reference": "Patient/one" }),
        );
    }
    let mut second = condition(MINIMAL_PROFILE)?;
    if let Some(object) = second.as_object_mut() {
        object.insert(String::from("id"), serde_json::json!("another"));
        object.insert(
            String::from("subject"),
            serde_json::json!({ "reference": "Patient/two" }),
        );
    }
    let request = ToOpenehrRequest::new(bundle(vec![first, second])?);
    let error = run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request)
        .err()
        .ok_or("a mixed-subject bundle cannot be one composition")?;
    let rendered = error.to_string();
    assert!(
        rendered.contains("Patient/one") && rendered.contains("Patient/two"),
        "the refusal names the conflicting subjects: {rendered}"
    );
    assert_eq!(error.issue_type(), "business-rule");
    Ok(())
}
