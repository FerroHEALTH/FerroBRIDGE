// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The library's IPS medication sub-context, entered through `slotContext`
//! from a FerroBRIDGE root context against the library health summary
//! template built from the CKM archetypes.
//!
//! The published `IPS.context.yml` names the template the library ships,
//! which the template reader refuses (#101), and three of its four section
//! extensions write a path under `$reference` (#1436). The medication
//! sub-context needs neither, so FerroBRIDGE's context enters it alone
//! (`types-of-mapping-files/sub-contexts.adoc`, §`slotContext`). It starts
//! at `SECTION.adhoc.v1`, whose published model does not compile under a
//! slot (#101), so the project directory carries a stand-in under the
//! published name.

use core::error::Error;
use std::sync::Arc;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;

use super::program;

/// The library health summary template.
const HEALTH_SUMMARY: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_health_summary.v1.0.0";

/// The context FerroBRIDGE authored over the medication sub-context.
const CONTEXT: &str = "ferrobridge_library_ips_medication.context";

/// The published files the program reaches: the sub-context and its section
/// extension.
const FILES: &[&str] = &[
    "projects/org.openehr/IPS/medication/IPS_medication.context.yml",
    "projects/org.openehr/IPS/medication/IPS_medication_section.yml",
];

/// The published models the program compiles beside them.
const MODELS: &[&str] = &[
    "model/composition/org.openehr/health_summary.v1.Composition.yml",
    "model/evaluation/org.openehr/absence.v2.yml",
    "model/evaluation/org.openehr/exclusion_global.v1.yml",
];

/// Compiles the medication context against the library health summary
/// template.
fn medication() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let files: Vec<&str> = FILES.iter().chain(MODELS).copied().collect();
    program(
        HEALTH_SUMMARY,
        CONTEXT,
        &files,
        &[
            "ferrobridge_ips_section_adhoc.v1.yml",
            "ferrobridge_ips_medication.yml",
        ],
    )
}

/// Returns a synthetic composition of the library health summary template
/// whose section holds one absence statement.
fn medication_composition(
    index: &WebTemplateIndex,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let flat = serde_json::json!({
        "library_health_summary/language|code": "en",
        "library_health_summary/language|terminology": "ISO_639-1",
        "library_health_summary/territory|code": "NL",
        "library_health_summary/territory|terminology": "ISO_3166-1",
        "library_health_summary/composer|name": "FerroBRIDGE synthetic composer",
        "library_health_summary/category|code": "433",
        "library_health_summary/category|value": "event",
        "library_health_summary/category|terminology": "openehr",
        "library_health_summary/context/start_time": "2026-10-01T09:00:00Z",
        "library_health_summary/context/setting|code": "238",
        "library_health_summary/context/setting|value": "other care",
        "library_health_summary/context/setting|terminology": "openehr",
        "library_health_summary/section/absence_of_information/language|code": "en",
        "library_health_summary/section/absence_of_information/language|terminology": "ISO_639-1",
        "library_health_summary/section/absence_of_information/encoding|code": "UTF-8",
        "library_health_summary/section/absence_of_information/encoding|terminology": "IANA_character-sets",
        "library_health_summary/section/absence_of_information/absence_statement": "No synthetic medication"
    });
    let serde_json::Value::Object(flat) = flat else {
        return Err("the FLAT composition is no object".into());
    };
    Ok(index.build_from_flat(&flat, crate::laws::NOW)?)
}

/// Compiles the medication context and runs it out of openEHR over the
/// synthetic composition, and returns it with the published files it
/// reaches: a context counts once it runs.
pub(super) fn chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    let (program, index) = medication()?;
    to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &medication_composition(&index)?,
        &Seams::default(),
        &CallContext::new(),
    )?;
    Ok((FILES, program))
}

#[test]
fn the_medication_sub_context_compiles_its_section_extension() -> Result<(), Box<dyn Error>> {
    let (program, _) = medication()?;
    assert_eq!(program.resource().as_str(), "Composition");
    let section = program
        .models()
        .iter()
        .find(|model| model.name().as_str() == "SECTION.adhoc.v1")
        .ok_or("the program compiles no ad hoc section")?;
    assert_eq!(
        section
            .extensions()
            .iter()
            .map(MappingName::as_str)
            .collect::<Vec<_>>(),
        ["IPS_medication_section"]
    );
    Ok(())
}

/// The library template is built from the CKM archetypes, and its OPT names
/// no embedded template at the section, so the sub-context's `template.id`
/// cannot be compared (`sub-contexts.adoc`, §Rules) and the compiler warns.
#[test]
fn the_sub_context_template_is_undecided() -> Result<(), Box<dyn Error>> {
    let (program, _) = medication()?;
    let codes: Vec<&str> = program
        .warnings()
        .iter()
        .map(|warning| warning.code().as_str())
        .collect();
    assert_eq!(codes, ["fc-sub-context-template-undecided"]);
    Ok(())
}

/// Going out of openEHR, the section carries the title and the LOINC code
/// `IPS_medication_section.yml` writes.
#[test]
fn the_medication_section_goes_out_with_its_title_and_code() -> Result<(), Box<dyn Error>> {
    let (program, index) = medication()?;
    let composition = medication_composition(&index)?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let document = outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Composition"))?;
    insta::assert_json_snapshot!(
        "library_ips_medication_section",
        document.pointer("/section")
    );
    Ok(())
}
