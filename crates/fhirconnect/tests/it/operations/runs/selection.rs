// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Which program a run takes and what it refuses: the template pins, the
//! claimed profiles, the composition forms and identity, and the losses.

use core::error::Error;

use fhir_types::r4::resource::Resource;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::identity::deterministic_id;
use fhirconnect::engine::seam::Seams;
use fhirconnect::operations::contract::CompositionPayload;
use fhirconnect::operations::contract::Format;
use fhirconnect::operations::contract::ToFhirRequest;
use fhirconnect::operations::contract::ToOpenehrRequest;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::run;
use fhirconnect::resolve::program::binding::TemplateId;

use crate::operations::COMPOSITION_UID;
use crate::operations::MINIMAL_PROFILE;
use crate::operations::REQUIRED_PROFILE;
use crate::operations::SUBJECT_CONTEXT;
use crate::operations::SUBJECT_PROFILE;
use crate::operations::TEMPLATE;
use crate::operations::bundle;
use crate::operations::committed_composition;
use crate::operations::condition;
use crate::operations::program_set;
use crate::operations::settings;

#[test]
fn a_failed_mapping_answers_an_outcome_and_nothing_else() -> Result<(), Box<dyn Error>> {
    // Fail.adoc: a required child the input does not carry refuses the unit,
    // and FerroBRIDGE answers no partial composition (architecture section
    // 4.7).
    let set = program_set(&["ferrobridge_diagnose_required"])?;
    let mut without_code = condition(REQUIRED_PROFILE)?;
    if let Some(object) = without_code.as_object_mut() {
        object.remove("code");
    }
    let request = ToOpenehrRequest::new(bundle(vec![without_code])?);
    let error = run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request)
        .err()
        .ok_or("the required child is not provided")?;
    assert!(matches!(error, OperationError::Mapping { .. }));
    let outcome = error.outcome();
    assert_eq!(outcome.issue.len(), 1);
    assert_eq!(
        outcome
            .issue
            .first()
            .and_then(|issue| issue.severity.value.clone()),
        Some(String::from("error")),
        "a refusal is an error, never a warning beside a partial result"
    );
    Ok(())
}

#[test]
fn a_disagreeing_template_pin_is_refused() -> Result<(), Box<dyn Error>> {
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let committed = committed_composition()?;
    let request = ToFhirRequest::new(CompositionPayload::parse(&committed.to_string())?)
        .with_template_id(TemplateId::new("another.template.v1"));
    let error = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)
        .err()
        .ok_or("a pin that contradicts the payload is refused")?;
    assert!(matches!(error, OperationError::TemplateDisagreement { .. }));
    assert_eq!(error.issue_type(), "invalid");
    Ok(())
}

#[test]
fn an_agreeing_template_pin_is_accepted() -> Result<(), Box<dyn Error>> {
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let committed = committed_composition()?;
    let request = ToFhirRequest::new(CompositionPayload::parse(&committed.to_string())?)
        .with_template_id(TemplateId::new(TEMPLATE));
    let answer = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    assert_ne!(
        answer.bundle().entry,
        Vec::<fhir_types::r4::bundle::BundleEntry>::new()
    );
    Ok(())
}

#[test]
fn a_flat_composition_maps_in_and_a_flat_composition_maps_out() -> Result<(), Box<dyn Error>> {
    // "An engine MUST support both, and the returned Composition is the same
    // content either way" (rest-api.adoc, section $toopenehr Output).
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let request =
        ToOpenehrRequest::new(bundle(vec![condition(SUBJECT_PROFILE)?])?).with_format(Format::Flat);
    let answer = run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    let flat: serde_json::Value = serde_json::from_str(answer.composition())?;
    let members = flat.as_object().ok_or("a flat document is an object")?;
    assert!(
        members.keys().any(|key| key.contains('/')),
        "a flat document keys its values by template path"
    );
    let inbound = ToFhirRequest::new(CompositionPayload::parse(answer.composition())?)
        .with_template_id(TemplateId::new(TEMPLATE));
    assert!(
        matches!(*inbound.composition(), CompositionPayload::Flat(_)),
        "the flat answer reads back as the flat serialization"
    );
    let error = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &inbound)
        .err()
        .ok_or("a flat payload carries no uid, so the Provenance has no target")?;
    assert!(matches!(error, OperationError::Unidentified { .. }));
    Ok(())
}

#[test]
fn a_composition_with_no_identity_is_refused() -> Result<(), Box<dyn Error>> {
    // The chapter puts a Provenance in every $tofhir Bundle and
    // Provenance.target is a reference, so a resource with no identity is a
    // refusal rather than a Bundle with an empty target.
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let mut committed = committed_composition()?;
    if let Some(object) = committed.as_object_mut() {
        object.remove("uid");
    }
    let request = ToFhirRequest::new(CompositionPayload::parse(&committed.to_string())?);
    let error = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)
        .err()
        .ok_or("a composition with no uid gives the Provenance no target")?;
    assert!(matches!(error, OperationError::Unidentified { .. }));
    Ok(())
}

#[test]
fn the_mapped_resource_takes_the_deterministic_id_of_its_entry() -> Result<(), Box<dyn Error>> {
    // NOTE: engine/id-management.adoc §Deterministic IDs, "a hash of the
    // Composition UID and the Entry Path"; the uid enters as its stable object_id.
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
        condition.id.as_deref(),
        Some(
            deterministic_id(
                "d2b3c1a0-0000-4000-8000-000000000001",
                "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]"
            )
            .as_str()
        )
    );
    Ok(())
}

#[test]
fn a_template_the_set_does_not_hold_is_refused() -> Result<(), Box<dyn Error>> {
    let set = program_set(&[SUBJECT_CONTEXT])?;
    let committed = committed_composition()?;
    let mut other = committed;
    if let Some(details) = other
        .get_mut("archetype_details")
        .and_then(serde_json::Value::as_object_mut)
    {
        details.insert(
            String::from("template_id"),
            serde_json::json!({ "value": "not.loaded.v1" }),
        );
    }
    let request = ToFhirRequest::new(CompositionPayload::parse(&other.to_string())?);
    let error = run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)
        .err()
        .ok_or("a template the set does not hold is refused")?;
    assert!(matches!(error, OperationError::UnknownTemplate { .. }));
    assert_eq!(error.issue_type(), "not-found");
    Ok(())
}

#[test]
fn a_bundle_claiming_no_known_profile_is_refused() -> Result<(), Box<dyn Error>> {
    let set = program_set(&["ferrobridge_diagnose_minimal"])?;
    let request = ToOpenehrRequest::new(bundle(vec![condition(
        "http://example.org/fhir/StructureDefinition/unknown",
    )?])?);
    let error = run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request)
        .err()
        .ok_or("no program claims that profile")?;
    assert!(matches!(error, OperationError::Select { .. }));
    assert_eq!(error.issue_type(), "not-found");
    Ok(())
}

#[test]
fn a_template_pin_narrows_two_contexts_over_one_profile() -> Result<(), Box<dyn Error>> {
    // "supplying `templateId` pin-points exactly which context mapping to
    // apply" (rest-api.adoc, section Query parameters).
    let set = program_set(&[
        "ferrobridge_diagnose_minimal",
        "ferrobridge_diagnose_required",
    ])?;
    let request = ToOpenehrRequest::new(bundle(vec![condition(REQUIRED_PROFILE)?])?)
        .with_template_id(TemplateId::new(TEMPLATE));
    let answer = run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    assert!(
        answer.composition().contains("Synthetic problem one"),
        "the pinned context mapped the problem name"
    );
    Ok(())
}

/// Asserts that `outcome` renders `warnings` one issue each, in order, with
/// each FHIR location as the issue's expression.
fn assert_outcome_of(
    outcome: Option<&fhir_types::r4::operation_outcome::OperationOutcome>,
    warnings: &[fhirconnect::engine::outcome::Warning],
) {
    let rendered: Vec<(Option<String>, Vec<String>)> = outcome
        .map(|outcome| outcome.issue.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|issue| {
            (
                issue
                    .details
                    .as_ref()
                    .and_then(|details| details.text.as_ref())
                    .and_then(|text| text.value.clone()),
                issue
                    .expression
                    .iter()
                    .filter_map(|path| path.value.clone())
                    .collect(),
            )
        })
        .collect();
    let expected: Vec<(Option<String>, Vec<String>)> = warnings
        .iter()
        .map(|warning| {
            (
                Some(warning.to_string()),
                warning
                    .location()
                    .fhir()
                    .map(String::from)
                    .into_iter()
                    .collect(),
            )
        })
        .collect();
    assert_eq!(rendered, expected, "one issue per typed loss, in order");
}

#[test]
fn both_operations_answer_their_typed_losses_beside_the_outcome() -> Result<(), Box<dyn Error>> {
    // rest-api.adoc §Reporting partial results: the gaps "MUST be made
    // explicit through the OperationOutcome"; the typed losses are the same
    // set, in the same order, each located.
    let set = program_set(&["ferrobridge_split"])?;
    let mut resource = condition("http://example.org/fhir/StructureDefinition/ferrobridge-split")?;
    if let Some(object) = resource.as_object_mut() {
        object.insert(
            String::from("bodySite"),
            serde_json::json!([{"text": "Left knee"}, {"text": "Right knee"}]),
        );
    }
    let request = ToOpenehrRequest::new(bundle(vec![resource])?);
    let inbound =
        run::to_openehr_with_warnings(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    let subject = inbound
        .warnings()
        .iter()
        .find(|warning| warning.to_string().contains("subject"))
        .ok_or("the unidirectional subject mapping is a declared skip")?;
    assert_eq!(subject.location().fhir(), Some("Condition.subject"));
    assert_eq!(subject.location().openehr(), None);
    assert_outcome_of(inbound.response().outcome(), inbound.warnings());
    assert_eq!(
        inbound.response(),
        &run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?,
        "the typed losses change nothing of the answer"
    );

    let mut built: serde_json::Value = serde_json::from_str(inbound.response().composition())?;
    if let Some(object) = built.as_object_mut() {
        object.insert(
            String::from("uid"),
            serde_json::json!({ "_type": "OBJECT_VERSION_ID", "value": COMPOSITION_UID }),
        );
    }
    let request = ToFhirRequest::new(CompositionPayload::parse(&built.to_string())?);
    let outbound =
        run::to_fhir_with_warnings(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    let outcome =
        outbound
            .response()
            .bundle()
            .entry
            .iter()
            .find_map(|entry| match entry.resource {
                Some(Resource::OperationOutcome(ref outcome)) => Some(outcome.as_ref()),
                _ => None,
            });
    assert_outcome_of(outcome, outbound.warnings());
    let reported = outcome.is_some();
    let (answer, warnings) = outbound.into_parts();
    assert_eq!(
        answer,
        run::to_fhir(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?,
        "the typed losses change nothing of the answer"
    );
    assert_eq!(!warnings.is_empty(), reported);
    Ok(())
}
#[test]
fn a_resource_claiming_two_profiles_selects_the_one_a_program_maps() -> Result<(), Box<dyn Error>> {
    // R4 resource.html#Meta: `meta.profile` is a list, so a resource claims
    // every profile it names and selection is membership (bundles.adoc).
    let set = program_set(&["ferrobridge_diagnose_minimal"])?;
    let mut resource = condition(MINIMAL_PROFILE)?;
    resource["meta"]["profile"] = serde_json::json!([
        "http://example.org/fhir/StructureDefinition/unknown",
        MINIMAL_PROFILE
    ]);
    let request = ToOpenehrRequest::new(bundle(vec![resource])?);
    let answer = run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request)?;
    assert!(
        answer.composition().contains("Synthetic problem one"),
        "the context of the second profile mapped the Condition"
    );
    Ok(())
}

#[test]
fn a_resource_claiming_two_mapped_profiles_is_refused_naming_both() -> Result<(), Box<dyn Error>> {
    let set = program_set(&[
        "ferrobridge_diagnose_minimal",
        "ferrobridge_diagnose_required",
    ])?;
    let mut resource = condition(MINIMAL_PROFILE)?;
    resource["meta"]["profile"] = serde_json::json!([MINIMAL_PROFILE, REQUIRED_PROFILE]);
    let request = ToOpenehrRequest::new(bundle(vec![resource])?);
    let error = run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request)
        .err()
        .ok_or("two contexts answer the resource")?;
    assert_eq!(error.issue_type(), "multiple-matches");
    let text = Error::source(&error)
        .map(ToString::to_string)
        .ok_or("the refusal carries the selection")?;
    assert!(
        text.contains("ferrobridge_diagnose_minimal.context")
            && text.contains("ferrobridge_diagnose_required.context"),
        "{text}"
    );
    Ok(())
}

#[test]
fn a_bundle_splitting_into_two_compositions_is_refused() -> Result<(), Box<dyn Error>> {
    // ToOpenEhr.fsh: the `composition` output has `max = "1"`.
    let set = program_set(&["ferrobridge_diagnose_minimal"])?;
    let request = ToOpenehrRequest::new(bundle(vec![
        condition(MINIMAL_PROFILE)?,
        condition(MINIMAL_PROFILE)?,
    ])?);
    let error = run::to_openehr(&set, &SCHEMAS, &Seams::default(), &settings(), &request)
        .err()
        .ok_or("one run answers one composition")?;
    assert!(
        matches!(
            error,
            OperationError::SeveralSubjectResources { count: 2, .. }
        ),
        "{error}"
    );
    assert_eq!(error.issue_type(), "business-rule");
    Ok(())
}
