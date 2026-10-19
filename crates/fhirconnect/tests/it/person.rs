// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Patient` round trip: the published KDS person chain of the
//! FHIRconnect mapping library under a FerroBRIDGE project context, compiled
//! against the published `KDS_Person` operational template.
//!
//! The published files load verbatim from the vendored library. Where one
//! does not load or compile, its refusal is asserted here by file and line
//! and the project directory carries a stand-in that says what it departs
//! from. A published model none of whose methods would run under the
//! context is replaced whole, so it is never counted as reached. The library
//! defects are recorded on #475 and #468.

use core::error::Error;
use std::sync::Arc;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::laws;
use crate::support::FIXTURES;
use crate::support::render;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The published files the project context loads, as the library ships them.
pub(crate) const PUBLISHED: &[&str] = &[
    "model/admin_entry/org.highmed/person_data.v0.yml",
    "model/admin_entry/org.highmed/versicherungsinformationen.v0.yml",
    "model/cluster/org.openehr/structured_name.v1.yml",
    "model/cluster/org.openehr/person_birth_data_iso.v0.yml",
    "model/cluster/org.openehr/death_details.v1.yml",
    "model/cluster/org.openehr/organisation.v1.yml",
    "projects/org.highmed/KDS/person/KDS_Composition.yml",
    "projects/org.highmed/KDS/person/KDS_gender.yml",
];

/// The project directory this crate authored for the round trip.
const PROJECT_DIR: &str = "projects/ferrobridge/kds_person";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_kds_person.context.yml",
    "ferrobridge_person.v1.yml",
    "ferrobridge_address.v1.yml",
    "ferrobridge_person.v1.Patient.yml",
    "ferrobridge_gender.v1.yml",
    "ferrobridge_kds_admin_entry_person.yml",
    "ferrobridge_kds_cluster_person.yml",
    "ferrobridge_kds_address.yml",
    "ferrobridge_kds_structured_name.yml",
];

/// The context this crate authored over the chain.
const CONTEXT: &str = "ferrobridge_kds_person.context";

/// The published files the stand-ins replace, as the library ships them.
const REPLACED: &[&str] = &[
    "model/cluster/org.openehr/person.v1.yml",
    "model/cluster/org.openehr/address.v1.yml",
    "model/composition/org.highmed/person.v1.Patient.yml",
    "model/evaluation/org.openehr/gender.v1.yml",
    "projects/org.highmed/KDS/person/KDS_admin_entry_person.yml",
    "projects/org.highmed/KDS/person/KDS_cluster_person.yml",
    "projects/org.highmed/KDS/person/KDS_address.yml",
    "projects/org.highmed/KDS/person/KDS_structured_name.v1.person_name-structured_name.yml",
];

/// The `mappingCode` the published person chain names.
const PERSON_MAPPING_CODE: &str = "ehrStatusAddExternalREF";

/// Returns the absolute path of a file of the vendored library.
fn published(file: &str) -> String {
    format!("{CORPUS}/{file}")
}

/// Returns the absolute path of a file of the project directory.
fn project(file: &str) -> String {
    format!("{FIXTURES}/{PROJECT_DIR}/{file}")
}

/// Builds the index over the published `KDS_Person` template.
pub(crate) fn kds_person_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(ferrobridge_testkit::fixtures::KDS_PERSON_OPT)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Loads the published files and the project directory into one set.
fn kds_person_set() -> Result<MappingSet, Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .map(|file| published(file))
        .chain(PROJECT.iter().map(|file| project(file)))
        .collect();
    load_set(files, &StaticMappingCodes::default())
        .map_err(|diagnostics| render(&diagnostics).into())
}

/// Compiles the project context against the published template.
pub(crate) fn kds_person_program() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let index = kds_person_template()?;
    let set = kds_person_set()?;
    let program = compile(
        &set,
        &MappingName::new(CONTEXT)?,
        &index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    Ok((program, index))
}

/// Compiles the project context and returns it with the published files it
/// loads, relative to the vendored library.
pub(crate) fn kds_person_chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, _) = kds_person_program()?;
    Ok((PUBLISHED, program))
}

/// Renders diagnostics as sorted `file:line code` rows, the file by its name.
fn rows(diagnostics: &[Diagnostic]) -> Vec<String> {
    let mut found: Vec<String> = diagnostics
        .iter()
        .map(|diagnostic| {
            let file = diagnostic
                .file()
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let line = diagnostic
                .position()
                .map_or(0, openehr_mapping_core::position::Position::line);
            format!("{file}:{line} {}", diagnostic.code())
        })
        .collect();
    found.sort();
    found
}

/// Returns the synthetic MII Person `Patient` of the testkit.
pub(crate) fn kds_patient() -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(serde_json::from_str(
        ferrobridge_testkit::fixtures::KDS_PERSON_PATIENT,
    )?)
}

/// Builds the synthetic `KDS_Person` composition from its FLAT fixture.
pub(crate) fn kds_person_composition(
    index: &WebTemplateIndex,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let text = std::fs::read_to_string(project("kds_person.flat.json"))?;
    let flat: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)?;
    Ok(index.build_from_flat(&flat, laws::NOW)?)
}

#[test]
fn the_kds_person_context_compiles_against_the_published_template() -> Result<(), Box<dyn Error>> {
    let (program, _) = kds_person_program()?;
    assert_eq!(program.template().id().as_str(), "KDS_Person");
    assert_eq!(program.resource().as_str(), "Patient");
    insta::assert_snapshot!("kds_person_program", program.to_string());
    Ok(())
}

#[test]
fn the_kds_person_flat_fixture_builds_a_composition_the_template_admits()
-> Result<(), Box<dyn Error>> {
    let index = kds_person_template()?;
    let composition = kds_person_composition(&index)?;
    assert_eq!(composition.template_id(), "KDS_Person");
    Ok(())
}

#[test]
fn putget_holds_on_the_kds_patient() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_person_program()?;
    let declared = laws::putget(&program, &index, &kds_patient()?, &laws::untouched)?;
    insta::assert_json_snapshot!("kds_person_putget", declared);
    Ok(())
}

#[test]
fn getput_holds_on_the_kds_person_composition() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_person_program()?;
    let declared = laws::getput(
        &program,
        &index,
        &kds_person_composition(&index)?,
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("kds_person_getput", declared);
    Ok(())
}

/// `versicherungsId` maps the statutory insurance `Identifier` onto
/// `items[at0006]`, an `ELEMENT` whose value admits five data types and which
/// carries `followedBy` children. `FollowedBy.adoc` runs the children after
/// the parent "has been executed", so the parent converts its value into the
/// `DV_IDENTIFIER` alternative, the class `Identifier` pairs with.
#[test]
fn the_statutory_insurance_identifier_round_trips() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_person_program()?;
    let composition = inbound(&program, &index, kds_patient()?)?;
    let flat = index.flatten(&composition)?;
    let carried: Vec<&String> = flat
        .iter()
        .filter(|&(_, value)| *value == serde_json::json!("Z000000000"))
        .map(|(key, _)| key)
        .collect();
    assert_eq!(
        carried,
        ["person/versicherungsinformationen/versicherungsnummer/identifier_value|id"],
        "the DV_IDENTIFIER alternative carries the insurance number"
    );
    let outbound = fhirconnect::engine::traverse::to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &fhirconnect::engine::seam::Seams::default(),
        &fhirconnect::engine::context::CallContext::new(),
    )?;
    let patient = outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Patient"))?;
    let identifiers = patient
        .get("identifier")
        .and_then(serde_json::Value::as_array)
        .ok_or("the Patient carries identifiers")?;
    assert!(
        identifiers.iter().any(|identifier| {
            identifier.get("system") == Some(&serde_json::json!("http://fhir.de/sid/gkv/kvid-10"))
                && identifier.get("value") == Some(&serde_json::json!("Z000000000"))
        }),
        "the insurance identifier comes back: {identifiers:?}"
    );
    Ok(())
}

/// `person.context.yaml` line 29 lists `KDS_person_data.v0`, a name no file
/// of the library carries: the file that extends `ADMIN_ENTRY.person_data.v0`
/// is named `KDS_admin_entry_person.v0`. A context's `extensions` reference
/// extension files "by their header name"
/// (`types-of-mapping-files/context-mappings.adoc` §Archetypes & Extensions).
#[test]
fn the_published_person_context_names_an_extension_no_file_carries() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(REPLACED)
        .chain(&["projects/org.highmed/KDS/person/person.context.yaml"])
        .map(|file| published(file))
        .collect();
    let codes = StaticMappingCodes::new([PERSON_MAPPING_CODE]);
    let Err(diagnostics) = load_set(&files, &codes) else {
        return Err("the published person context is expected not to load".into());
    };
    assert_eq!(
        rows(&diagnostics),
        vec!["person.context.yaml:29 fc-unknown-mapping-reference"]
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message().contains("`KDS_person_data.v0`")),
        "{}",
        render(&diagnostics)
    );
    Ok(())
}

/// `KDS_admin_entry_person.yml` line 23 names the PROGRAMMED function
/// `ehrStatusAddExternalREF`, which the engine does not register; a
/// `mappingCode` names "a function that is implemented by the engine"
/// (`types-of-mappings/concept-type/concept-mappings.adoc`), so a load
/// against the engine's registry refuses it.
#[test]
fn the_published_admin_entry_extension_names_a_function_the_engine_lacks()
-> Result<(), Box<dyn Error>> {
    let files = [
        published("model/admin_entry/org.highmed/person_data.v0.yml"),
        published("model/composition/org.highmed/person.v1.Patient.yml"),
        published("projects/org.highmed/KDS/person/KDS_admin_entry_person.yml"),
    ];
    let Err(diagnostics) = load_set(&files, &StaticMappingCodes::default()) else {
        return Err("the published admin entry extension is expected not to load".into());
    };
    let found = rows(&diagnostics);
    assert!(
        found.contains(&String::from(
            "KDS_admin_entry_person.yml:23 fc-unknown-mapping-code"
        )),
        "{found:?}"
    );
    Ok(())
}

/// Compiling the chain as the library publishes it, with the published
/// extension list less the name no file carries and the one PROGRAMMED
/// function registered so the files load, refuses exactly the rows the
/// snapshot pins:
///
/// - `KDS_admin_entry_person.yml:19`: a method with no `extension` key
///   (`types-of-mapping-files/extension-methods.adoc`).
/// - `person.v1.Patient.yml:18, 64, 69, 73`: `Patient.serviceProvider`, and
///   lines `23, 30, 34, 45`: `Patient.period`, neither of which R4 Patient
///   defines (#475); lines `55, 57, 59, 78, 80, 82`: `null_flavour` below a
///   `DV_DATE_TIME` and a `PARTY_PROXY`, which the RM gives none.
/// - `person_data.v0.yml:75`: `link` below a `Reference`.
/// - `person.v1.yml:39, 40`: `CLUSTER.organisation.v1`, a slot `KDS_Person`
///   leaves unfilled, with `organisation.v1.yml:19, 24` below it.
/// - `structured_name.v1.yml:20`: `items[at0001]`, two nodes of `KDS_Person`.
/// - `KDS_structured_name.v1.person_name-structured_name.yml:17, 31, 37`:
///   `items[at0006]`, which `KDS_Person` does not carry, `url` below
///   `Extension.value[x]`, and `$resource.family` on Patient.
/// - `KDS_address.yml:55, 60, 78, 82, 100, 104`: `code` below the `code`
///   `Address.type`; lines `47, 70, 92`: `defining_code/terminology_id`
///   written as a string; lines `21, 26, 31, 36`: Address elements read on
///   the Patient, because `KDS_cluster_person.yml` line 51 binds the post box
///   address to `$resource`.
#[test]
fn the_kds_person_chain_as_published_is_refused_exactly() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let context = std::fs::read_to_string(published(
        "projects/org.highmed/KDS/person/person.context.yaml",
    ))?
    .replace("    - \"KDS_person_data.v0\"\n", "")
    .replace(
        "  name:  KDS_Person.context\n",
        "  name: published_kds_person.context\n",
    );
    let context_file = directory.path().join("published_kds_person.context.yaml");
    std::fs::write(&context_file, context)?;
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(REPLACED)
        .map(|file| published(file))
        .chain([context_file.to_string_lossy().into_owned()])
        .collect();
    let codes = StaticMappingCodes::new([PERSON_MAPPING_CODE]);
    let set = load_set(files, &codes).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new("published_kds_person.context")?,
        &kds_person_template()?,
        &SCHEMAS,
        &codes,
    )
    .err()
    .ok_or("the person chain as published compiles")?;
    insta::assert_json_snapshot!("kds_person_chain_as_published", rows(&diagnostics));
    Ok(())
}

/// The published address model, compiled without the KDS extension that
/// overwrites its one method, refuses its own `Address.type` paths: the
/// conditions and FHIR paths read `code` below the `code` primitive, whose
/// value R4 names `value` (`StructureDefinition-code`, `code.value`), and the
/// manual paths write a string onto `TERMINOLOGY_ID` (`manual.adoc` writes
/// `defining_code/terminology_id/value`).
#[test]
fn the_published_address_model_is_refused_for_its_type_paths() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let context = std::fs::read_to_string(project("ferrobridge_kds_person.context.yml"))?
        .replace("    - \"ferrobridge_kds_address\"\n", "");
    let context_file = directory.path().join("ferrobridge_kds_person.context.yml");
    std::fs::write(&context_file, context)?;
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(&["model/cluster/org.openehr/address.v1.yml"])
        .map(|file| published(file))
        .chain(
            PROJECT
                .iter()
                .filter(|file| {
                    !matches!(
                        **file,
                        "ferrobridge_kds_person.context.yml"
                            | "ferrobridge_address.v1.yml"
                            | "ferrobridge_kds_address.yml"
                    )
                })
                .map(|file| project(file)),
        )
        .chain([context_file.to_string_lossy().into_owned()])
        .collect();
    let set = load_set(files, &StaticMappingCodes::default()).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new(CONTEXT)?,
        &kds_person_template()?,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .err()
    .ok_or("the published address model compiles")?;
    insta::assert_json_snapshot!("kds_person_address_as_published", rows(&diagnostics));
    Ok(())
}

/// Maps `patient` into a `KDS_Person` composition through `program`.
fn inbound(
    program: &Program,
    index: &WebTemplateIndex,
    patient: serde_json::Value,
) -> Result<CanonicalComposition, fhirconnect::engine::traverse::error::EngineError> {
    fhirconnect::engine::traverse::to_openehr(
        program,
        &SCHEMAS,
        index,
        &fhir_types::codec::Value::from_serde_json(patient),
        &fhirconnect::engine::seam::Seams::default(),
        &laws::defaults(),
        &fhirconnect::engine::context::CallContext::new(),
    )
    .map(|outcome| outcome.value().clone())
}

/// The published gender model compiles, and its one method maps the `code`
/// `Patient.gender` onto the `DV_CODED_TEXT` `items[at0022]`, a conversion
/// `types-of-mappings/data-type/DV_CODED_TEXT.adoc` §STRING calls not
/// possible, so a run that carries a gender refuses with no cell.
#[test]
fn the_published_gender_model_refuses_a_gender_at_run() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .chain(&["model/evaluation/org.openehr/gender.v1.yml"])
        .map(|file| published(file))
        .chain(
            PROJECT
                .iter()
                .filter(|file| **file != "ferrobridge_gender.v1.yml")
                .map(|file| project(file)),
        )
        .collect();
    let set = load_set(files, &StaticMappingCodes::default()).map_err(|d| render(&d))?;
    let index = kds_person_template()?;
    let program = compile(
        &set,
        &MappingName::new(CONTEXT)?,
        &index,
        &SCHEMAS,
        &StaticMappingCodes::default(),
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    let Err(error) = inbound(&program, &index, kds_patient()?) else {
        return Err("a gender is expected to be refused by the published model".into());
    };
    let source = Error::source(&error).map(ToString::to_string);
    assert_eq!(
        source.as_deref(),
        Some("no data-type cell pairs DV_CODED_TEXT with string going fhir->openehr"),
        "{error}"
    );
    Ok(())
}

/// Every address of `KDS_Person` carries `land` (`items[at0006]`, 1..1), a
/// `DV_CODED_TEXT`. The KDS address extension maps no country, and FHIR
/// `Address.country` is a `string`, which `DV_CODED_TEXT.adoc` §STRING cannot
/// convert, so a Patient with an address builds no composition the template
/// admits.
#[test]
fn a_patient_address_builds_no_kds_person_composition() -> Result<(), Box<dyn Error>> {
    let (program, index) = kds_person_program()?;
    let mut patient = kds_patient()?;
    if let Some(object) = patient.as_object_mut() {
        object.insert(
            String::from("address"),
            serde_json::json!([{
                "type": "both",
                "line": ["Beispielweg 1"],
                "city": "Musterstadt",
                "postalCode": "00000",
                "country": "DE"
            }]),
        );
    }
    let Err(error) = inbound(&program, &index, patient) else {
        return Err("an address is expected to be refused by the template".into());
    };
    let source = Error::source(&error).map(ToString::to_string);
    assert!(
        matches!(
            error,
            fhirconnect::engine::traverse::error::EngineError::Build { .. }
        ),
        "{error}: {source:?}"
    );
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("'Straßenanschrift']/items[at0006]/value: mandatory node is missing"),
        "{rendered}"
    );
    Ok(())
}
