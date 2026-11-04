// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping library's model mappings that no published context starts at,
//! each compiled verbatim through a FerroBRIDGE context against the library
//! report template.
//!
//! The template is over three published CKM archetypes, and the suite builds
//! its ADL 2 operational template at test time, so every node a mapping
//! names is the published archetype's own. Each program holds the lens law
//! `PutGet` over a synthetic resource.

use core::error::Error;
use std::sync::Arc;

use ferrobridge_testkit::fixtures::CKM_LIBRARY_ARCHETYPES;
use ferrobridge_testkit::fixtures::FERROBRIDGE_LIBRARY_REPORT_TEMPLATE;
use ferrobridge_testkit::fixtures::FERROBRIDGE_LIBRARY_REPORT_TEMPLATE_ID;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::library::Library;
use fhirconnect::model::load::load_set;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::laws;
use crate::resolve::render;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The contexts this crate authored over the library, relative to this
/// crate's manifest.
const PROJECT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/projects/ferrobridge/library"
);

/// The gender evaluation, mapped onto `Patient.gender`.
const GENDER: &[&str] = &["model/evaluation/org.openehr/gender.v1.yml"];

/// The device cluster, mapped onto `Device`.
const DEVICE: &[&str] = &["model/cluster/org.openehr/device.v1.yml"];

/// Builds the index over the ADL 2 operational template of the library
/// report template, compiled from the vendored CKM archetypes.
fn report_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    for (file, source) in CKM_LIBRARY_ARCHETYPES {
        let archetype = openehr_adl::assemble::parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    let template =
        openehr_adl::assemble::parse_artefact(FERROBRIDGE_LIBRARY_REPORT_TEMPLATE, Dialect::Adl2)
            .map_err(|errors| format!("the template: {errors:?}"))?;
    let opt = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(opt),
        resolved_id: String::from(FERROBRIDGE_LIBRARY_REPORT_TEMPLATE_ID),
    })?)
}

/// Compiles the FerroBRIDGE context `context` over the published `files`
/// against the library report template.
fn program(
    context: &str,
    files: &[&str],
) -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = report_template()?;
    let paths: Vec<String> = files
        .iter()
        .map(|file| format!("{CORPUS}/{file}"))
        .chain([format!("{PROJECT}/{context}.yml")])
        .collect();
    let set = load_set(paths, &Library).map_err(|diagnostics| render(&diagnostics))?;
    let program = compile(
        &set,
        &MappingName::new(context)?,
        &index,
        &SCHEMAS,
        &Library,
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    Ok((program, index))
}

/// Compiles every context of this suite and returns each with the published
/// files it loads, relative to the vendored library.
pub(crate) fn library_chains() -> Result<Vec<crate::support::Chain>, Box<dyn Error>> {
    Ok(vec![
        (
            GENDER,
            program("ferrobridge_library_gender.context", GENDER)?.0,
        ),
        (
            DEVICE,
            program("ferrobridge_library_device.context", DEVICE)?.0,
        ),
    ])
}

#[test]
fn the_template_carries_each_start_archetype_once() -> Result<(), Box<dyn Error>> {
    let index = report_template()?;
    assert_eq!(index.template_id(), FERROBRIDGE_LIBRARY_REPORT_TEMPLATE_ID);
    for archetype in [
        "openEHR-EHR-EVALUATION.gender.v1",
        "openEHR-EHR-CLUSTER.device.v1",
    ] {
        let carried = index
            .nodes()
            .filter(|node| node.node_id() == Some(archetype))
            .count();
        assert_eq!(carried, 1, "{archetype}");
    }
    Ok(())
}

#[test]
fn the_gender_evaluation_compiles_onto_patient() -> Result<(), Box<dyn Error>> {
    let (program, _) = program("ferrobridge_library_gender.context", GENDER)?;
    assert_eq!(program.resource().as_str(), "Patient");
    let models: Vec<&str> = program
        .models()
        .iter()
        .map(|model| model.name().as_str())
        .collect();
    assert_eq!(models, ["EVALUATION.gender.v1"]);
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

#[test]
fn the_device_cluster_compiles_onto_device() -> Result<(), Box<dyn Error>> {
    let (program, _) = program("ferrobridge_library_device.context", DEVICE)?;
    assert_eq!(program.resource().as_str(), "Device");
    let models: Vec<&str> = program
        .models()
        .iter()
        .map(|model| model.name().as_str())
        .collect();
    assert_eq!(models, ["CLUSTER.device.v1"]);
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

#[test]
fn putget_holds_on_a_synthetic_patient_gender() -> Result<(), Box<dyn Error>> {
    let (program, index) = program("ferrobridge_library_gender.context", GENDER)?;
    let patient = serde_json::json!({"resourceType": "Patient", "gender": "female"});
    let declared = laws::putget(&program, &index, &patient, &laws::untouched)?;
    insta::assert_json_snapshot!("library_gender_putget", declared);
    Ok(())
}

/// The library maps `Device.note.text` twice, as `description` onto
/// `items[at0002]` and as `note` onto `items[at0008]`, so the one note is
/// written to both elements and reads back as two; `deviceName.type` is
/// mapped by no method of the file.
#[test]
fn putget_holds_on_a_synthetic_device() -> Result<(), Box<dyn Error>> {
    let (program, index) = program("ferrobridge_library_device.context", DEVICE)?;
    let device = serde_json::json!({
        "resourceType": "Device",
        "deviceName": [{"name": "Synthetic analyser", "type": "user-friendly-name"}],
        "type": {"text": "Synthetic analyser type"},
        "manufacturer": "Synthetic Manufacturer",
        "serialNumber": "SYN-SERIAL-0001",
        "modelNumber": "SYN-MODEL-0001",
        "lotNumber": "SYN-LOT-0001",
        "note": [{"text": "A synthetic device."}]
    });
    let declared = laws::putget(&program, &index, &device, &laws::untouched)?;
    insta::assert_json_snapshot!("library_device_putget", declared);
    Ok(())
}
