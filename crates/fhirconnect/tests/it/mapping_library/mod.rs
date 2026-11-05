// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping library's model mappings that no published context starts at,
//! each compiled verbatim through a FerroBRIDGE context against a library
//! template built from the CKM archetypes.
//!
//! Each template is over published CKM archetypes, and the suite builds its
//! ADL 2 operational template at test time, so every node a mapping names is
//! the published archetype's own. The lineage of each template root, the
//! archetypes its source specialises, is carried into the index, because
//! `create_opt` writes an OPT that states no parent.

mod clusters;
mod compositions;
mod ips;
mod medication;

use core::error::Error;
use std::sync::Arc;

use ferrobridge_testkit::fixtures::CKM_HIGHMED_ARCHETYPES;
use ferrobridge_testkit::fixtures::CKM_LIBRARY_ARCHETYPES;
use ferrobridge_testkit::fixtures::CKM_MEDICATION_ARCHETYPES;
use ferrobridge_testkit::fixtures::FERROBRIDGE_LIBRARY_TEMPLATES;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::library::Library;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::load::load_set;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::parse::Dialect;
use openehr_am::v2_4::aom2::archetype::archetype::Archetype;
use openehr_am::v2_4::aom2::archetype::authored_archetype::AuthoredArchetype;
use openehr_mapping_core::diagnostic::Diagnostic;
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

/// The contexts and stand-ins this crate authored over the library,
/// relative to this crate's manifest.
const PROJECT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/projects/ferrobridge/library"
);

/// The library report template.
const REPORT: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_report.v1.0.0";

/// The gender evaluation, mapped onto `Patient.gender`.
const GENDER: &[&str] = &["model/evaluation/org.openehr/gender.v1.yml"];

/// The device cluster, mapped onto `Device`.
const DEVICE: &[&str] = &["model/cluster/org.openehr/device.v1.yml"];

/// The deepest lineage a template root may state before the walk refuses.
const LINEAGE_LIMIT: usize = 16;

/// Returns the archetype `archetype` specialises, when it states one.
fn parent_of(archetype: &Archetype) -> Option<&str> {
    match *archetype {
        Archetype::AuthoredArchetype(ref authored) => match **authored {
            AuthoredArchetype::OperationalTemplate(ref opt) => opt.parent_archetype_id.as_deref(),
            AuthoredArchetype::Template(ref template) => template.parent_archetype_id.as_deref(),
            AuthoredArchetype::AuthoredArchetype(ref data) => data.parent_archetype_id.as_deref(),
        },
        Archetype::TemplateOverlay(ref overlay) => overlay.parent_archetype_id.as_deref(),
    }
}

/// Returns the archetypes `template` specialises, the nearest first, each
/// parent read from the archetype the one before it names.
fn lineage(
    template: &Archetype,
    repository: &ArchetypeRepository,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut lineage = Vec::new();
    let mut next = parent_of(template).map(str::to_owned);
    while let Some(parent) = next {
        if lineage.len() == LINEAGE_LIMIT {
            return Err(format!("the lineage of the template passes {LINEAGE_LIMIT}").into());
        }
        next = repository
            .resolve(&parent, None)
            .and_then(parent_of)
            .map(str::to_owned);
        lineage.push(parent);
    }
    Ok(lineage)
}

/// Builds the repository of every vendored CKM archetype the library
/// templates read.
fn repository() -> Result<ArchetypeRepository, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    for (file, source) in CKM_LIBRARY_ARCHETYPES
        .iter()
        .chain(&CKM_HIGHMED_ARCHETYPES)
        .chain(&CKM_MEDICATION_ARCHETYPES)
    {
        let archetype = openehr_adl::assemble::parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    Ok(repository)
}

/// Builds the index over the ADL 2 operational template of the library
/// template `id`, compiled from the vendored CKM archetypes, with the
/// lineage its source states.
fn template(id: &str) -> Result<WebTemplateIndex, Box<dyn Error>> {
    let (_, source) = FERROBRIDGE_LIBRARY_TEMPLATES
        .iter()
        .find(|(held, _)| *held == id)
        .ok_or_else(|| format!("no library template is `{id}`"))?;
    let repository = repository()?;
    let template = openehr_adl::assemble::parse_artefact(source, Dialect::Adl2)
        .map_err(|errors| format!("{id}: {errors:?}"))?;
    let opt = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("{id}: the operational template: {errors:?}"))?;
    let index = WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(opt),
        resolved_id: id.to_owned(),
    })?;
    Ok(index.with_root_lineage(lineage(&template, &repository)?))
}

/// Loads the published `files` beside the FerroBRIDGE `fixtures`.
fn set(files: &[&str], fixtures: &[&str]) -> Result<MappingSet, Box<dyn Error>> {
    let paths: Vec<String> = files
        .iter()
        .map(|file| format!("{CORPUS}/{file}"))
        .chain(fixtures.iter().map(|file| format!("{PROJECT}/{file}")))
        .collect();
    Ok(load_set(paths, &Library).map_err(|diagnostics| render(&diagnostics))?)
}

/// A compiled program with the index it was compiled against, or the
/// refusals of its compilation.
type Compiled = Result<(Arc<Program>, WebTemplateIndex), Vec<Diagnostic>>;

/// Compiles the FerroBRIDGE context `context` over the published `files`
/// and the `fixtures` against the library template `id`.
fn compiled(
    id: &str,
    context: &str,
    files: &[&str],
    fixtures: &[&str],
) -> Result<Compiled, Box<dyn Error>> {
    let index = template(id)?;
    let mut loaded = fixtures.to_vec();
    let file = format!("{context}.yml");
    loaded.push(file.as_str());
    let set = set(files, &loaded)?;
    Ok(compile(
        &set,
        &MappingName::new(context)?,
        &index,
        &SCHEMAS,
        &Library,
    )
    .map(|program| (program, index)))
}

/// Compiles as [`compiled`] does and refuses a refused compilation.
fn program(
    id: &str,
    context: &str,
    files: &[&str],
    fixtures: &[&str],
) -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    compiled(id, context, files, fixtures)?.map_err(|diagnostics| render(&diagnostics).into())
}

/// Returns the names of the model mappings `program` compiled, in order.
fn models(program: &Program) -> Vec<&str> {
    program
        .models()
        .iter()
        .map(|model| model.name().as_str())
        .collect()
}

/// Compiles every context of this suite that compiles and returns each with
/// the published files it loads, relative to the vendored library.
pub(crate) fn library_chains() -> Result<Vec<crate::support::Chain>, Box<dyn Error>> {
    let mut chains = vec![
        (
            GENDER,
            program(REPORT, "ferrobridge_library_gender.context", GENDER, &[])?.0,
        ),
        (
            DEVICE,
            program(REPORT, "ferrobridge_library_device.context", DEVICE, &[])?.0,
        ),
    ];
    chains.extend(clusters::chains()?);
    chains.extend(compositions::chains()?);
    chains.push(ips::chain()?);
    Ok(chains)
}

#[test]
fn the_template_carries_each_start_archetype_once() -> Result<(), Box<dyn Error>> {
    let index = template(REPORT)?;
    assert_eq!(index.template_id(), REPORT);
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
    let (program, _) = program(REPORT, "ferrobridge_library_gender.context", GENDER, &[])?;
    assert_eq!(program.resource().as_str(), "Patient");
    assert_eq!(models(&program), ["EVALUATION.gender.v1"]);
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

#[test]
fn the_device_cluster_compiles_onto_device() -> Result<(), Box<dyn Error>> {
    let (program, _) = program(REPORT, "ferrobridge_library_device.context", DEVICE, &[])?;
    assert_eq!(program.resource().as_str(), "Device");
    assert_eq!(models(&program), ["CLUSTER.device.v1"]);
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

#[test]
fn putget_holds_on_a_synthetic_patient_gender() -> Result<(), Box<dyn Error>> {
    let (program, index) = program(REPORT, "ferrobridge_library_gender.context", GENDER, &[])?;
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
    let (program, index) = program(REPORT, "ferrobridge_library_device.context", DEVICE, &[])?;
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
