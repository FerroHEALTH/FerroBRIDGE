// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An `ACTION`'s time, a mandatory `DV_DATE_TIME` the Web Template carries no
//! node for, written from a FHIR `Procedure.performedDateTime` and read back.
//!
//! Simplified Formats spells it `time` under the action's node
//! (master05 §ACTION). The template is the published `KDS_Prozedur` the
//! testkit vendors; the context and model are synthetic.

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::parse::lower_context;
use fhirconnect::model::parse::lower_model;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::laws;
use crate::support::render;

/// The synthetic context over the published `KDS_Prozedur`.
const CONTEXT: &str = r#"grammar: FHIRConnect/v1.0.0
type: context
metadata:
  name: ferrobridge_action_time.context
  version: 1.0.0
spec:
  system: FHIR
  version: R4
context:
  profile:
    url: "http://example.org/fhir/StructureDefinition/ferrobridge-action-time"
  template:
    id: "KDS_Prozedur"
  archetypes:
    - "ferrobridge_action_time"
  start: "ferrobridge_action_time"
"#;

/// The synthetic model: the procedure's name, its completed state and when
/// it was performed.
const MODEL: &str = r#"grammar: FHIRConnect/v1.0.0
type: model
metadata:
  name: ferrobridge_action_time
  version: 1.0.0
spec:
  system: FHIR
  version: R4
  openEhrConfig:
    archetype: openEHR-EHR-ACTION.procedure.v1
  fhirConfig:
    structureDefinition: http://hl7.org/fhir/StructureDefinition/Procedure

mappings:
  - name: "procedureName"
    with:
      fhir: "$resource.code"
      openehr: "$archetype/description[at0001]/items[at0002]"
      type: "CODEABLECONCEPT"

  - name: "completed"
    unidirectional: "fhir->openehr"
    with:
      fhir: "$resource"
      openehr: "$archetype/ism_transition/current_state"
      type: NONE
    manual:
      - name: "completed"
        openehr:
          - path: "defining_code/terminology_id/value"
            value: "openehr"
          - path: "defining_code/code_string"
            value: "532"
          - path: "value"
            value: "completed"
        fhirCondition:
          targetRoot: "$resource"
          targetAttribute: "status"
          operator: "one of"
          criteria: "completed"

  - name: "performed"
    with:
      fhir: "$resource.performed.as(DateTime)"
      openehr: "$archetype/time"
"#;

/// When the synthetic procedure was performed.
const PERFORMED: &str = "2026-10-02T10:30:00+02:00";

/// Builds the index over the published `KDS_Prozedur`.
fn template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(ferrobridge_testkit::fixtures::KDS_PROZEDUR_OPT)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Compiles the synthetic context and model against `index`.
fn program(index: &WebTemplateIndex) -> Result<Arc<Program>, Box<dyn Error>> {
    let mut set = MappingSet::new();
    let context = openehr_mapping_core::loader::load_str("context.yml", CONTEXT)?;
    set.insert_context(lower_context(&context).map_err(|diagnostics| render(&diagnostics))?)
        .map_err(|error| error.to_string())?;
    let model = openehr_mapping_core::loader::load_str("model.yml", MODEL)?;
    set.insert_model(lower_model(&model).map_err(|diagnostics| render(&diagnostics))?)
        .map_err(|error| error.to_string())?;
    compile(
        &set,
        &MappingName::new("ferrobridge_action_time.context")?,
        index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| Box::<dyn Error>::from(render(&diagnostics)))
}

/// A synthetic completed `Procedure`.
fn procedure() -> Value {
    let json = serde_json::json!({
        "resourceType": "Procedure",
        "status": "completed",
        "code": {"coding": [{"system": "http://snomed.info/sct", "code": "SYN-PROC-1",
                             "display": "Synthetic procedure"}],
                 "text": "Synthetic procedure"},
        "subject": {"reference": "Patient/synthetic-subject-0001"},
        "performedDateTime": PERFORMED
    });
    Value::from_serde_json(json)
}

#[test]
fn a_procedures_performed_time_is_the_actions_time_both_ways() -> Result<(), Box<dyn Error>> {
    let index = template()?;
    let program = program(&index)?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &procedure(),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    )?;
    let flat = index.flatten(inbound.value())?;
    let times: Vec<(&String, &serde_json::Value)> = flat
        .iter()
        .filter(|(key, _)| {
            key.ends_with("/time") && !key.starts_with("ctx/") && !key.contains("context")
        })
        .collect();
    assert_eq!(
        vec![serde_json::Value::String(String::from(PERFORMED))],
        times
            .iter()
            .map(|(_, value)| (*value).clone())
            .collect::<Vec<_>>(),
        "the action carries the performed time: {times:?}"
    );
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &Seams::default(),
        &CallContext::new(),
    )?;
    let json = outbound
        .value()
        .to_serde_json(&mut Path::root("Procedure"))?;
    assert_eq!(
        PERFORMED, json["performedDateTime"],
        "the action's time reads back as performedDateTime: {json}"
    );
    Ok(())
}

#[test]
fn a_run_into_openehr_names_every_element_it_read_with_the_node_it_wrote()
-> Result<(), Box<dyn Error>> {
    let index = template()?;
    let program = program(&index)?;
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &procedure(),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    )?;
    let reads: Vec<(Option<&str>, Option<&str>)> = inbound
        .reads()
        .iter()
        .map(|read| (read.fhir(), read.openehr()))
        .collect();
    assert_eq!(
        vec![
            (
                Some("Procedure.code"),
                Some(
                    "/content[openEHR-EHR-ACTION.procedure.v1]/description[at0001]/items[at0002]/value"
                )
            ),
            (
                Some("Procedure.performed"),
                Some("/content[openEHR-EHR-ACTION.procedure.v1]/time")
            ),
        ],
        reads,
        "the name and the time are read; the state is written from a condition"
    );
    Ok(())
}
