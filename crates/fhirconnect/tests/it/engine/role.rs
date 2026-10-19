// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `PractitionerRole.practitioner` into `ROLE.performer` and back, through
//! the testkit's synthetic `ROLE` template. `ROLE.performer` is a mandatory
//! `PARTY_REF` to "the Version container of Actor playing the role" (RM
//! Demographic IM, the `ROLE` class), and no template node holds it. No
//! specification governs a party context or this correspondence: our own
//! design.

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::demographics::IssuerDemographics;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::party::from_party;
use fhirconnect::engine::traverse::party::to_party;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::party::CanonicalParty;
use openehr_mapping_core::party::PartyError;
use openehr_mapping_core::template::TemplateSource;
use openehr_rm::v1_2::demographic::role::Role;

use crate::resolve::codes;
use crate::resolve::context;
use crate::resolve::render;
use crate::resolve::set_of;

/// The FHIR server base the bridge serves its parties under.
const BASE: &str = "https://bridge.example/fhir";

/// The `versioned_object_uid` of the synthetic performing practitioner.
const PERFORMER: &str = "8f0e7d4c-0000-4000-8000-000000000001";

/// The mappings onto the identity name and the performer.
const MAPPINGS: &str = "  - name: \"name\"\n    with:\n      fhir: \"$resource.code.text\"\n      \
                        openehr: \"$archetype/identities[at0003]/details[at0004]/items[at0005]\"\n  \
                        - name: \"practitioner\"\n    with:\n      fhir: \"$resource.practitioner\"\n      \
                        openehr: \"$archetype/performer\"\n";

/// Builds the index over the testkit's synthetic `ROLE` template.
fn role() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(ferrobridge_testkit::fixtures::ROLE_OPT)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// A model mapping rooted at the synthetic `ROLE` archetype for `resource`.
fn role_model(resource: &str, mappings: &str) -> String {
    format!(
        "grammar: FHIRConnect/v1.0.0\ntype: model\nmetadata:\n  name: ROLE.synthetic.v1\n  version: \
         1.0.0\nspec:\n  system: FHIR\n  version: R4\n  openEhrConfig:\n    archetype: \
         openEHR-DEMOGRAPHIC-ROLE.ferrobridge_role.v1\n  fhirConfig:\n    structureDefinition: \
         http://hl7.org/fhir/StructureDefinition/{resource}\nmappings:\n{mappings}"
    )
}

/// Compiles the party context over the synthetic `ROLE` template.
fn compiled(resource: &str, mappings: &str) -> Result<Arc<Program>, Vec<Diagnostic>> {
    let model = role_model(resource, mappings);
    let context = context(
        "role.context",
        &format!(
            "  profile:\n    url: \"http://hl7.org/fhir/StructureDefinition/{resource}\"\n  \
             template:\n    id: \"ferrobridge.role.v1\"\n  archetypes:\n    - \
             \"ROLE.synthetic.v1\"\n  start: \"ROLE.synthetic.v1\"\n"
        ),
    );
    let fail = |error: String| {
        vec![Diagnostic::error(
            "tests",
            openehr_mapping_core::diagnostic::DiagnosticCode::TemplateBuild,
            error,
        )]
    };
    let set = set_of(&[("model.yml", &model), ("context.yml", &context)])
        .map_err(|error| fail(error.to_string()))?;
    let index = role().map_err(|error| fail(error.to_string()))?;
    let name = MappingName::new("role.context").map_err(|error| fail(error.to_string()))?;
    compile(
        &set,
        &name,
        &index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
}

/// The program over the name and performer mappings.
fn program() -> Result<Arc<Program>, Box<dyn Error>> {
    Ok(compiled("PractitionerRole", MAPPINGS).map_err(|diagnostics| render(&diagnostics))?)
}

/// A synthetic `PractitionerRole` whose practitioner is `reference`.
fn practitioner_role(reference: &str) -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "PractitionerRole",
        "code": [{"text": "Synthetic Role"}],
        "practitioner": {"reference": reference}
    }))
}

/// Runs `document` into a `ROLE` through the bridge's demographics endpoint.
fn into_role(document: &Value) -> Result<Result<CanonicalParty, EngineError>, Box<dyn Error>> {
    let demographics = IssuerDemographics::new(BASE)?;
    Ok(to_party(
        program()?.as_ref(),
        &SCHEMAS,
        &role()?,
        document,
        &Seams::default().with_demographics(&demographics),
        &CallContext::new(),
    )
    .map(fhirconnect::engine::outcome::Outcome::into_value))
}

#[test]
fn a_practitioner_reference_becomes_the_roles_performer() -> Result<(), Box<dyn Error>> {
    let party = into_role(&practitioner_role(&format!("Practitioner/{PERFORMER}")))??;
    let role: Role = openehr_its::json::from_canonical_value(party.value())?;
    assert_eq!(role.performer.namespace, "demographic");
    assert_eq!(role.performer.r#type, "ACTOR");
    assert_eq!(
        party.value().pointer("/performer/id"),
        Some(&serde_json::json!({"_type": "HIER_OBJECT_ID", "value": PERFORMER}))
    );
    Ok(())
}

#[test]
fn the_role_maps_back_to_the_same_practitioner() -> Result<(), Box<dyn Error>> {
    let document = practitioner_role(&format!("Practitioner/{PERFORMER}"));
    let party = into_role(&document)??;
    let demographics = IssuerDemographics::new(BASE)?;
    let back = from_party(
        program()?.as_ref(),
        &SCHEMAS,
        &role()?,
        &party,
        &Seams::default().with_demographics(&demographics),
        &CallContext::new(),
    )?
    .into_value();
    assert_eq!(back, document);
    Ok(())
}

#[test]
fn an_absolute_reference_under_the_base_names_the_same_performer() -> Result<(), Box<dyn Error>> {
    let relative = into_role(&practitioner_role(&format!("Practitioner/{PERFORMER}")))??;
    let absolute = into_role(&practitioner_role(&format!(
        "{BASE}/Practitioner/{PERFORMER}/_history/2"
    )))??;
    assert_eq!(
        absolute.value().get("performer"),
        relative.value().get("performer")
    );
    Ok(())
}

#[test]
fn a_practitioner_of_another_server_is_refused() -> Result<(), Box<dyn Error>> {
    let refused = into_role(&practitioner_role(&format!(
        "https://other.example/r4/Practitioner/{PERFORMER}"
    )))?;
    assert!(
        matches!(refused, Err(EngineError::ForeignParty { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_request_local_practitioner_is_refused() -> Result<(), Box<dyn Error>> {
    let refused = into_role(&practitioner_role(
        "urn:uuid:5b0e0c9c-0000-4000-8000-000000000001",
    ))?;
    assert!(
        matches!(refused, Err(EngineError::Demographics { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_reference_of_another_type_is_refused() -> Result<(), Box<dyn Error>> {
    let refused = into_role(&practitioner_role(&format!("Patient/{PERFORMER}")))?;
    assert!(
        matches!(refused, Err(EngineError::Cell { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_display_beside_the_reference_is_refused_rather_than_dropped() -> Result<(), Box<dyn Error>> {
    let refused = into_role(&Value::from_serde_json(serde_json::json!({
        "resourceType": "PractitionerRole",
        "code": [{"text": "Synthetic Role"}],
        "practitioner": {"reference": format!("Practitioner/{PERFORMER}"), "display": "Dr Synthetic"}
    })))?;
    assert!(
        matches!(refused, Err(EngineError::Cell { .. })),
        "{refused:?}"
    );
    Ok(())
}

/// RM Demographic IM, the `ROLE` class: `performer` is `1..1`.
#[test]
fn a_practitioner_role_with_no_practitioner_builds_no_role() -> Result<(), Box<dyn Error>> {
    let refused = into_role(&Value::from_serde_json(serde_json::json!({
        "resourceType": "PractitionerRole",
        "code": [{"text": "Synthetic Role"}]
    })))?;
    let Err(EngineError::PartyBuild { ref source }) = refused else {
        panic!("a ROLE with no performer built: {refused:?}");
    };
    assert!(
        matches!(**source, PartyError::RoleWithoutPerformer { .. }),
        "{source:?}"
    );
    Ok(())
}

#[test]
fn a_reference_admitting_several_types_is_refused_at_load() -> Result<(), Box<dyn Error>> {
    let mappings = "  - name: \"name\"\n    with:\n      fhir: \"$resource.name.text\"\n      \
                    openehr: \"$archetype/identities[at0003]/details[at0004]/items[at0005]\"\n  \
                    - name: \"practitioner\"\n    with:\n      fhir: \"$resource.generalPractitioner\"\n      \
                    openehr: \"$archetype/performer\"\n";
    match compiled("Patient", mappings) {
        Ok(program) => Err(format!("the context compiled:\n{program}").into()),
        Err(diagnostics) => {
            assert_eq!(codes(&diagnostics), ["fc-party-ref-target"]);
            Ok(())
        }
    }
}
