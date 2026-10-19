// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Party contexts: a context whose operational template is rooted at a
//! `PERSON`, an `ORGANISATION` or a `ROLE` compiles as a party context,
//! recognised from the RM type of the template root, and refuses at load what
//! names a composition.
//!
//! FHIRconnect v1.0.0 admits an `openEHR-DEMOGRAPHIC-*` archetype and defines
//! no mapping rooted at a party, so every rule pinned here is our own design:
//! no specification governs it. The template is the testkit's synthetic
//! `PERSON`, because every `openEHR-DEMOGRAPHIC-*` archetype in the
//! international CKM is a v0 draft.

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::party::PartyClass;
use openehr_mapping_core::template::TemplateSource;

use crate::resolve::codes;
use crate::resolve::context;
use crate::resolve::program_of;
use crate::resolve::render;
use crate::resolve::set_of;

/// The archetype the synthetic party template is rooted at.
const PERSON_ARCHETYPE: &str = "openEHR-DEMOGRAPHIC-PERSON.ferrobridge_person.v1";

/// The mapping onto the name element of the one identity.
pub(crate) const NAME: &str = "  - name: \"name\"\n    with:\n      fhir: \"$resource.name.text\"\n      \
                    openehr: \"$archetype/identities[at0003]/details[at0004]/items[at0005]\"\n";

/// Builds the index over the testkit's synthetic `PERSON` template.
pub(crate) fn person() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(ferrobridge_testkit::fixtures::PERSON_OPT)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Compiles `context` out of `set` against the synthetic `PERSON` template.
pub(crate) fn party_program(
    set: &MappingSet,
    context: &str,
) -> Result<Arc<Program>, Vec<Diagnostic>> {
    let index = person().map_err(|error| {
        vec![Diagnostic::error(
            "tests",
            openehr_mapping_core::diagnostic::DiagnosticCode::TemplateBuild,
            error.to_string(),
        )]
    })?;
    let name = MappingName::new(context).map_err(|error| {
        vec![Diagnostic::error(
            "tests",
            openehr_mapping_core::diagnostic::DiagnosticCode::InvalidMappingName,
            error.to_string(),
        )]
    })?;
    compile(set, &name, &index, &SCHEMAS, &StaticMappingCodes::default())
}

/// A model mapping rooted at the synthetic party archetype, mapping a
/// `Patient`.
pub(crate) fn person_model(name: &str, mappings: &str) -> String {
    format!(
        "grammar: FHIRConnect/v1.0.0\ntype: model\nmetadata:\n  name: {name}\n  version: \
         1.0.0\nspec:\n  system: FHIR\n  version: R4\n  openEhrConfig:\n    archetype: \
         {PERSON_ARCHETYPE}\n  fhirConfig:\n    structureDefinition: \
         http://hl7.org/fhir/StructureDefinition/Patient\nmappings:\n{mappings}"
    )
}

/// The party context over the synthetic `PERSON` template, starting at
/// `PERSON.synthetic.v1` and listing `contexts`.
pub(crate) fn person_context(contexts: &[&str]) -> String {
    let mut listed = String::new();
    if !contexts.is_empty() {
        listed.push_str("  contexts:\n");
        for name in contexts {
            listed.push_str("    - \"");
            listed.push_str(name);
            listed.push_str("\"\n");
        }
    }
    context(
        "person.context",
        &format!(
            "  profile:\n    url: \"http://hl7.org/fhir/StructureDefinition/Patient\"\n  \
             template:\n    id: \"ferrobridge.person.v1\"\n  archetypes:\n    - \
             \"PERSON.synthetic.v1\"\n{listed}  start: \"PERSON.synthetic.v1\"\n"
        ),
    )
}

/// Compiles the party context over a start model carrying `mappings`, and
/// returns the codes of the refusal.
fn refusal_codes(mappings: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let model = person_model("PERSON.synthetic.v1", mappings);
    let context = person_context(&[]);
    let set = set_of(&[("model.yml", &model), ("context.yml", &context)])?;
    match party_program(&set, "person.context") {
        Ok(program) => Err(format!("the party context compiled:\n{program}").into()),
        Err(diagnostics) => Ok(codes(&diagnostics)),
    }
}

#[test]
fn a_context_over_a_person_template_compiles_as_a_party_context() -> Result<(), Box<dyn Error>> {
    let identifier = "  - name: \"identifier\"\n    with:\n      fhir: \"$resource.identifier\"\n      \
                      openehr: \"$archetype/details[at0001]/items[at0002]\"\n";
    let model = person_model("PERSON.synthetic.v1", &format!("{NAME}{identifier}"));
    let context = person_context(&[]);
    let set = set_of(&[("model.yml", &model), ("context.yml", &context)])?;
    let program =
        party_program(&set, "person.context").map_err(|diagnostics| render(&diagnostics))?;
    assert_eq!(program.party(), Some(PartyClass::Person));
    assert_eq!(program.resource().as_str(), "Patient");
    assert!(
        program.to_string().contains("\n  party PERSON\n"),
        "the rendering names the party class:\n{program}"
    );
    Ok(())
}

#[test]
fn archetype_and_openehr_root_name_the_party_root() -> Result<(), Box<dyn Error>> {
    let nested = "  - name: \"identity\"\n    with:\n      fhir: \"$resource.name\"\n      \
                  openehr: \"$archetype/identities[at0003]\"\n    followedBy:\n      mappings:\n        \
                  - name: \"text\"\n          with:\n            fhir: \"text\"\n            \
                  openehr: \"$openehrRoot/details[at0004]/items[at0005]\"\n";
    let model = person_model("PERSON.synthetic.v1", nested);
    let context = person_context(&[]);
    let set = set_of(&[("model.yml", &model), ("context.yml", &context)])?;
    let program =
        party_program(&set, "person.context").map_err(|diagnostics| render(&diagnostics))?;
    let text = program
        .mapping_named("identity.text")
        .ok_or("the nested mapping is missing")?;
    let target = text
        .openehr()
        .ok_or("the nested mapping has no openEHR side")?;
    assert_eq!(
        target.path().to_string(),
        "/identities[at0003]/details[at0004]/items[at0005]"
    );
    Ok(())
}

#[test]
fn a_composition_template_compiles_as_no_party_context() -> Result<(), Box<dyn Error>> {
    let model = crate::resolve::start_model(
        "mappings:\n  - name: \"problem\"\n    with:\n      fhir: \"$resource.code\"\n      \
         openehr: \"$archetype/data[at0001]/items[at0002]\"\n",
    );
    let context = context("synthetic.context", &crate::resolve::start_context(&[]));
    let set = set_of(&[("model.yml", &model), ("context.yml", &context)])?;
    let program =
        program_of(&set, "synthetic.context").map_err(|diagnostics| render(&diagnostics))?;
    assert_eq!(program.party(), None);
    Ok(())
}

#[test]
fn a_party_context_refuses_the_composition_variable() -> Result<(), Box<dyn Error>> {
    let composer = "  - name: \"composer\"\n    with:\n      fhir: \"$resource.name.text\"\n      \
                    openehr: \"$composition/composer\"\n";
    assert_eq!(
        refusal_codes(&format!("{NAME}{composer}"))?,
        ["fc-party-composition-variable"]
    );
    Ok(())
}

#[test]
fn a_party_context_refuses_a_composition_slot() -> Result<(), Box<dyn Error>> {
    let slot = "  - name: \"composition\"\n    with:\n      fhir: \"$resource\"\n      \
                openehr: \"$composition\"\n    slotArchetype: \"COMPOSITION.synthetic.v1\"\n";
    assert_eq!(
        refusal_codes(&format!("{NAME}{slot}"))?,
        ["fc-party-composition-variable"]
    );
    Ok(())
}

#[test]
fn a_party_context_refuses_a_defaulted_composition_field() -> Result<(), Box<dyn Error>> {
    for field in [
        "composer",
        "context/start_time",
        "language",
        "territory",
        "category",
    ] {
        let mapping = format!(
            "  - name: \"field\"\n    with:\n      fhir: \"$resource.name.text\"\n      \
             openehr: \"$archetype/{field}\"\n"
        );
        assert_eq!(
            refusal_codes(&format!("{NAME}{mapping}"))?,
            ["fc-party-composition-field"],
            "the field `{field}`"
        );
    }
    Ok(())
}

#[test]
fn a_party_context_refuses_a_participations_function() -> Result<(), Box<dyn Error>> {
    let participation = "  - name: \"practitioner\"\n    with:\n      fhir: \
                         \"$resource.generalPractitioner\"\n      openehr: \"$archetype\"\n    \
                         participationsFunction: \"generalPractitioner\"\n";
    assert_eq!(
        refusal_codes(&format!("{NAME}{participation}"))?,
        ["fc-party-participation"]
    );
    Ok(())
}

#[test]
fn a_party_context_refuses_a_link() -> Result<(), Box<dyn Error>> {
    let link = "  - name: \"organization\"\n    with:\n      fhir: \
                \"$resource.managingOrganization\"\n      openehr: \"$archetype/links\"\n    link:\n      \
                meaning: \"the managing organization\"\n      type: \"organization\"\n";
    assert_eq!(refusal_codes(&format!("{NAME}{link}"))?, ["fc-party-link"]);
    Ok(())
}

#[test]
fn a_party_context_refuses_entering_a_context_that_slots_a_composition()
-> Result<(), Box<dyn Error>> {
    let entering = "  - name: \"entered\"\n    with:\n      fhir: \"$fhirRoot\"\n      openehr: \
                    \"$archetype/details[at0001]\"\n    slotContext: \"sub.context\"\n";
    let parent = person_model("PERSON.synthetic.v1", &format!("{NAME}{entering}"));
    let slotting = person_model(
        "PERSON.slotting.v1",
        &format!(
            "{NAME}  - name: \"composition\"\n    with:\n      fhir: \"$resource\"\n      \
             openehr: \"$composition\"\n    slotArchetype: \"COMPOSITION.synthetic.v1\"\n"
        ),
    );
    let sub = context(
        "sub.context",
        "  profile:\n    url: \"http://hl7.org/fhir/StructureDefinition/Patient\"\n  template:\n    \
         id: \"ferrobridge.person.v1\"\n  archetypes:\n    - \"PERSON.slotting.v1\"\n  start: \
         \"PERSON.slotting.v1\"\n",
    );
    let context = person_context(&["sub.context"]);
    let set = set_of(&[
        ("parent.yml", &parent),
        ("slotting.yml", &slotting),
        ("sub.yml", &sub),
        ("context.yml", &context),
    ])?;
    let diagnostics = match party_program(&set, "person.context") {
        Ok(program) => return Err(format!("the party context compiled:\n{program}").into()),
        Err(diagnostics) => diagnostics,
    };
    assert_eq!(codes(&diagnostics), ["fc-party-sub-context-composition"]);
    Ok(())
}

#[test]
fn a_party_program_never_reaches_the_composition_builder() -> Result<(), Box<dyn Error>> {
    let model = person_model("PERSON.synthetic.v1", NAME);
    let context = person_context(&[]);
    let set = set_of(&[("model.yml", &model), ("context.yml", &context)])?;
    let program =
        party_program(&set, "person.context").map_err(|diagnostics| render(&diagnostics))?;
    let patient = Value::from_serde_json(serde_json::json!({
        "resourceType": "Patient",
        "name": [{"text": "Synthetic Person"}]
    }));
    let refused = to_openehr(
        &program,
        &SCHEMAS,
        &person()?,
        &patient,
        &Seams::default(),
        &Defaults::at("2026-10-19T08:00:00Z"),
        &CallContext::new(),
    );
    assert!(
        matches!(
            refused,
            Err(EngineError::PartyProgram {
                party: PartyClass::Person,
                ..
            })
        ),
        "a party program reached the composition builder: {refused:?}"
    );
    Ok(())
}
