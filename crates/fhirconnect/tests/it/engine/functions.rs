// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `mappingCode` registry a run is handed, and a PROGRAMMED mapping run
//! through it in both directions.

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::library::Library;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::functions::Datum;
use fhirconnect::engine::traverse::functions::MappingCodeError;
use fhirconnect::engine::traverse::functions::MappingFunctions;
use fhirconnect::engine::traverse::functions::NoMappingFunctions;
use fhirconnect::engine::traverse::functions::Ports;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::model::ast::ContextMappingFile;
use fhirconnect::model::ast::ModelMappingFile;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::parse::lower_context;
use fhirconnect::model::parse::lower_model;
use fhirconnect::model::semantic::MappingCodeRegistry;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::engine::defaults;
use crate::support::FIXTURES;
use crate::support::render;

/// The vendored medication statement template, relative to this crate.
const MEDIKATIONSEINTRAG: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt/kds-medikationseintrag/KDS_Medikationseintrag.opt"
);

/// The files of the dosage fixture, relative to the fixtures directory.
const DOSAGE_FILES: [&str; 3] = [
    "contexts/ferrobridge_dosage.context.yml",
    "model/ferrobridge_dosage_statement.yml",
    "model/ferrobridge_dosage.yml",
];

/// Builds the index over the medication statement template.
fn medication_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(&std::fs::read_to_string(MEDIKATIONSEINTRAG)?)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Loads the dosage fixture into one set.
fn dosage_set() -> Result<MappingSet, Box<dyn Error>> {
    dosage_set_edited(|_, text| text)
}

/// Loads the dosage fixture into one set, each file's text through `edit`.
fn dosage_set_edited(edit: impl Fn(&str, String) -> String) -> Result<MappingSet, Box<dyn Error>> {
    let mut set = MappingSet::new();
    for name in DOSAGE_FILES {
        let text = edit(name, std::fs::read_to_string(format!("{FIXTURES}/{name}"))?);
        let document = openehr_mapping_core::loader::load_str(name, &text)?;
        if text.contains("\ntype: context\n") {
            let file: ContextMappingFile =
                lower_context(&document).map_err(|diagnostics| render(&diagnostics))?;
            set.insert_context(file)
                .map_err(|error| error.to_string())?;
        } else {
            let file: ModelMappingFile =
                lower_model(&document).map_err(|diagnostics| render(&diagnostics))?;
            set.insert_model(file).map_err(|error| error.to_string())?;
        }
    }
    Ok(set)
}

/// Compiles the dosage fixture against the medication statement template.
fn dosage_program(index: &WebTemplateIndex) -> Result<Arc<Program>, Box<dyn Error>> {
    let context = MappingName::new("ferrobridge_dosage.context")?;
    compile(&dosage_set()?, &context, index, &SCHEMAS, &Library)
        .map_err(|diagnostics| Box::<dyn Error>::from(render(&diagnostics)))
}

/// Returns a synthetic `MedicationStatement` with one dosage whose timing
/// carries `repeat` as its `Timing.repeat`.
fn statement(repeat: &serde_json::Value) -> Value {
    Value::from_serde_json(serde_json::json!({
        "resourceType": "MedicationStatement",
        "medicationCodeableConcept": {"text": "synthetic medication"},
        "dosage": [{"timing": {"repeat": repeat}}]
    }))
}

/// Returns the `Timing.repeat` of the first dosage of a statement.
fn repeat_of(statement: &Value) -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(statement
        .get("dosage")
        .and_then(|dosage| match dosage {
            Value::Array(items) => items.first(),
            _ => None,
        })
        .and_then(|dosage| dosage.get("timing"))
        .and_then(|timing| timing.get("repeat"))
        .ok_or("the statement carries a repeat")?
        .to_serde_json(&mut Path::root("Timing.repeat"))?)
}

#[test]
fn the_default_registry_holds_nothing() {
    let error = NoMappingFunctions
        .to_openehr("diagnosis", &Value::Null)
        .expect_err("the default registry holds no function");
    assert!(
        error.to_string().contains("diagnosis"),
        "the refusal names the function: {error}"
    );
}

#[test]
fn a_programmed_mapping_round_trips_the_timing_through_its_ports() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let program = dosage_program(&index)?;
    let repeat = serde_json::json!({
        "duration": 30,
        "durationUnit": "min",
        "frequency": 1,
        "period": 2,
        "periodUnit": "wk"
    });
    let seams = Seams::default().with_functions(&Library);
    let inbound = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &statement(&repeat),
        &seams,
        &defaults(),
        &CallContext::new(),
    )?;
    let written = serde_json::to_string(inbound.value().value())?;
    assert!(written.contains("\"PT30M\""), "{written}");
    assert!(written.contains("\"P2W\""), "{written}");
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        inbound.value(),
        &seams,
        &CallContext::new(),
    )?;
    assert_eq!(repeat_of(outbound.value())?, repeat);
    Ok(())
}

#[test]
fn a_program_compiled_against_the_library_resolves_every_port() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let rendered = dosage_program(&index)?.to_string();
    assert!(
        rendered
            .contains("mappingCode dosageDurationToAdministrationDuration port . [DV_DURATION]"),
        "{rendered}"
    );
    assert!(
        rendered.contains("mappingCode timingNonDaily port items[at0002] [DV_DURATION]"),
        "{rendered}"
    );
    Ok(())
}

#[test]
fn a_function_the_run_does_not_register_refuses_the_unit() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let program = dosage_program(&index)?;
    let error = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &statement(&serde_json::json!({"duration": 30, "durationUnit": "min"})),
        &Seams::default(),
        &defaults(),
        &CallContext::new(),
    )
    .expect_err("the default seams register no function");
    assert!(
        matches!(error, EngineError::MappingCode { ref code, .. } if code == "dosageDurationToAdministrationDuration"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_function_that_refuses_its_input_refuses_the_unit() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let program = dosage_program(&index)?;
    let error = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &statement(&serde_json::json!({"frequency": 3, "period": 1, "periodUnit": "wk"})),
        &Seams::default().with_functions(&Library),
        &defaults(),
        &CallContext::new(),
    )
    .expect_err("three events a week have no place in a non-daily timing");
    assert!(
        matches!(error, EngineError::MappingCode { ref code, .. } if code == "timingNonDaily"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_registry_of_names_alone_compiles_the_function_with_no_port() -> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let context = MappingName::new("ferrobridge_dosage.context")?;
    let codes =
        StaticMappingCodes::new(["dosageDurationToAdministrationDuration", "timingNonDaily"]);
    let program = compile(&dosage_set()?, &context, &index, &SCHEMAS, &codes)
        .map_err(|diagnostics| render(&diagnostics))?;
    let rendered = program.to_string();
    assert!(
        rendered.contains("mappingCode timingNonDaily\n"),
        "{rendered}"
    );
    Ok(())
}

/// The function of the fixture whose mapping is anchored at the non-daily
/// timing cluster.
const STRAY_CODE: &str = "timingNonDaily";

/// A registry that declares `ports` for the non-daily function and writes
/// `class` at `port` whatever it is asked; every other function declares no
/// port and writes nothing.
#[derive(Debug)]
struct Stray {
    ports: &'static [&'static str],
    port: &'static str,
    class: &'static str,
}

impl MappingCodeRegistry for Stray {
    fn contains(&self, _code: &str) -> bool {
        true
    }

    fn ports(&self, code: &str) -> Option<&'static [&'static str]> {
        Some(if code == STRAY_CODE { self.ports } else { &[] })
    }
}

impl MappingFunctions for Stray {
    fn to_fhir(&self, _code: &str, _ports: &Ports) -> Result<Option<Value>, MappingCodeError> {
        Ok(None)
    }

    fn to_openehr(&self, code: &str, _element: &Value) -> Result<Ports, MappingCodeError> {
        if code != STRAY_CODE {
            return Ok(Ports::new());
        }
        Ok(Ports::new().with(
            self.port,
            Datum::new(
                self.class,
                serde_json::json!({"_type": self.class, "value": "P1W"}),
            ),
        ))
    }
}

/// Runs the dosage fixture into openEHR through `stray`, compiled and run
/// alike, and returns the refusal.
fn stray_run(stray: &Stray) -> Result<EngineError, Box<dyn Error>> {
    let index = medication_template()?;
    let context = MappingName::new("ferrobridge_dosage.context")?;
    let program = compile(&dosage_set()?, &context, &index, &SCHEMAS, stray)
        .map_err(|diagnostics| render(&diagnostics))?;
    to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &statement(&serde_json::json!({"period": 1, "periodUnit": "wk"})),
        &Seams::default().with_functions(stray),
        &defaults(),
        &CallContext::new(),
    )
    .err()
    .ok_or_else(|| "the stray function is refused".into())
}

/// Returns the refusal a function raised inside an engine error.
fn function_refusal(error: &EngineError) -> Result<&MappingCodeError, Box<dyn Error>> {
    match *error {
        EngineError::MappingCode { ref source, .. } => Ok(source),
        ref other => Err(format!("{other:?}").into()),
    }
}

#[test]
fn a_function_that_writes_a_port_it_does_not_declare_refuses_the_unit() -> Result<(), Box<dyn Error>>
{
    let error = stray_run(&Stray {
        ports: &["items[at0002]"],
        port: "items[at0099]",
        class: "DV_DURATION",
    })?;
    let source = function_refusal(&error)?;
    assert!(
        matches!(*source, MappingCodeError::UndeclaredPort { ref port, .. } if port == "items[at0099]"),
        "{source:?}"
    );
    Ok(())
}

#[test]
fn a_function_that_writes_a_class_the_port_does_not_admit_refuses_the_unit()
-> Result<(), Box<dyn Error>> {
    let error = stray_run(&Stray {
        ports: &["items[at0002]"],
        port: "items[at0002]",
        class: "DV_TEXT",
    })?;
    let source = function_refusal(&error)?;
    assert!(
        matches!(*source, MappingCodeError::UnadmittedClass { ref class, .. } if class == "DV_TEXT"),
        "{source:?}"
    );
    Ok(())
}

/// Compiles the dosage fixture with `stray` and returns the refusal codes.
fn stray_codes(stray: &Stray) -> Result<Vec<String>, Box<dyn Error>> {
    let index = medication_template()?;
    let context = MappingName::new("ferrobridge_dosage.context")?;
    let Err(diagnostics) = compile(&dosage_set()?, &context, &index, &SCHEMAS, stray) else {
        return Err("the stray ports compile".into());
    };
    Ok(diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect())
}

#[test]
fn a_port_onto_a_repeating_node_is_refused_at_load() -> Result<(), Box<dyn Error>> {
    // The non-daily archetype's specific event cluster `at0006` repeats, so a
    // port onto it would hold many values in one call.
    let codes = stray_codes(&Stray {
        ports: &["items[at0006]"],
        port: "items[at0006]",
        class: "CLUSTER",
    })?;
    assert!(
        codes
            .iter()
            .any(|code| code == "fc-repeating-function-port"),
        "{codes:?}"
    );
    Ok(())
}

#[test]
fn a_port_the_template_does_not_hold_is_refused_at_load() -> Result<(), Box<dyn Error>> {
    let codes = stray_codes(&Stray {
        ports: &["items[at9999]"],
        port: "items[at9999]",
        class: "DV_TEXT",
    })?;
    assert!(
        codes.iter().any(|code| code == "fc-unknown-template-node"),
        "{codes:?}"
    );
    Ok(())
}

#[test]
fn a_function_with_ports_under_a_mapping_with_no_openehr_side_is_refused_at_load()
-> Result<(), Box<dyn Error>> {
    let index = medication_template()?;
    let context = MappingName::new("ferrobridge_dosage.context")?;
    let set = dosage_set_edited(|_, text| {
        text.replace(
            "      openehr: \"$archetype/items[openEHR-EHR-CLUSTER.timing_nondaily.v1]\"\n",
            "",
        )
    })?;
    let Err(diagnostics) = compile(&set, &context, &index, &SCHEMAS, &Library) else {
        return Err("a port with no anchor compiles".into());
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "fc-unanchored-function-port"),
        "{}",
        render(&diagnostics)
    );
    Ok(())
}
