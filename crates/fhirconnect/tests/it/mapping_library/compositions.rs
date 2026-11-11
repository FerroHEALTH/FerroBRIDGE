// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The library's composition models (`composition-element.adoc`), each the
//! `start` of a FerroBRIDGE context against a library template rooted at its
//! composition archetype.
//!
//! The OPT2 root carries the template's own id, so a composition model finds
//! its root through the lineage the template source states (AM Release-2.3.0
//! AOM2, §3.6.4 `TEMPLATE`, §3.6.6 `OPERATIONAL_TEMPLATE`).

use core::error::Error;
use std::path::Path;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::library::Library;
use fhirconnect::resolve::compile::compile;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::paths::AqlPath;

use super::CORPUS;
use super::compiled;
use super::models;
use super::program;
use super::set;
use super::template;

/// The library medication list template.
const MEDICATION_LIST: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_medication_list.v1.0.0";

/// The library health summary template.
const HEALTH_SUMMARY: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_health_summary.v1.0.0";

/// The library report result template.
const REPORT_RESULT: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_report_result.v1.0.0";

/// The medication list composition, mapped onto `MedicationStatement`.
const MEDICATION_LIST_FILES: &[&str] =
    &["model/composition/org.openehr/medication_list.v1.MedicationStatement.yml"];

/// The health summary composition, the ad hoc section and the two
/// evaluations the section slots, mapped onto `Composition`.
const HEALTH_SUMMARY_FILES: &[&str] = &[
    "model/composition/org.openehr/health_summary.v1.Composition.yml",
    "model/evaluation/org.openehr/absence.v2.yml",
    "model/evaluation/org.openehr/exclusion_global.v1.yml",
    "model/section/org.openehr/adhoc.v1.yml",
];

/// FerroBRIDGE's section extension, relative to the project directory.
const HEALTH_SUMMARY_FIXTURES: &[&str] = &["ferrobridge_library_health_summary.yml"];

/// The composition models that refuse against their template for the
/// library defects #101 records, as `(template slug, context, file)`.
const REFUSED: &[(&str, &str, &str)] = &[
    (
        "report",
        "ferrobridge_library_report_condition.context",
        "model/composition/org.openehr/report.v1.Condition.yml",
    ),
    (
        "report",
        "ferrobridge_library_report_observation.context",
        "model/composition/org.openehr/report.v1.Observation.yml",
    ),
    (
        "report",
        "ferrobridge_library_report_procedure.context",
        "model/composition/org.openehr/report.v1.Procedure.yml",
    ),
    (
        "encounter",
        "ferrobridge_library_encounter.context",
        "model/composition/org.highmed/fall.v1.Encounter.yml",
    ),
    (
        "person",
        "ferrobridge_library_person.context",
        "model/composition/org.highmed/person.v1.Patient.yml",
    ),
    (
        "report_result",
        "ferrobridge_library_report_result.context",
        "model/composition/org.openehr/report_result.v1.DiagnosticReport.yml",
    ),
];

/// Compiles the composition contexts that compile and returns each with the
/// published files it loads.
pub(super) fn chains() -> Result<Vec<crate::support::Chain>, Box<dyn Error>> {
    Ok(vec![
        (
            MEDICATION_LIST_FILES,
            program(
                MEDICATION_LIST,
                "ferrobridge_library_medication_list.context",
                MEDICATION_LIST_FILES,
                &[],
            )?
            .0,
        ),
        (
            HEALTH_SUMMARY_FILES,
            program(
                HEALTH_SUMMARY,
                "ferrobridge_library_health_summary.context",
                HEALTH_SUMMARY_FILES,
                HEALTH_SUMMARY_FIXTURES,
            )?
            .0,
        ),
    ])
}

/// Renders one refusal as the published file, the line and the code.
fn row(diagnostic: &Diagnostic) -> String {
    let file = diagnostic.file();
    let relative = file
        .strip_prefix(Path::new(CORPUS))
        .unwrap_or(file)
        .display();
    let line = diagnostic
        .position()
        .map_or_else(String::new, |position| position.line().to_string());
    format!("{relative}:{line} {}", diagnostic.code())
}

#[test]
fn a_template_root_carries_the_lineage_its_source_states() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        template(MEDICATION_LIST)?.root_lineage(),
        ["openEHR-EHR-COMPOSITION.medication_list.v1"]
    );
    assert_eq!(
        template(REPORT_RESULT)?.root_lineage(),
        [
            "openEHR-EHR-COMPOSITION.report-result.v1",
            "openEHR-EHR-COMPOSITION.report.v1"
        ]
    );
    Ok(())
}

/// The ADL 1.4 `report-result.v1` restates the `context` of `report.v1`, so
/// its template carries that context once (#1549).
#[test]
fn the_report_result_template_carries_one_context() -> Result<(), Box<dyn Error>> {
    let index = template(REPORT_RESULT)?;
    let contexts: Vec<&str> = index
        .nodes()
        .filter(|node| node.rm_type() == "EVENT_CONTEXT")
        .map(|node| node.aql_path().as_str())
        .collect();
    assert_eq!(contexts, ["/context"]);
    assert_eq!(
        index.node(&AqlPath::new("/context"))?.rm_type(),
        "EVENT_CONTEXT"
    );
    Ok(())
}

#[test]
fn a_composition_model_with_no_lineage_finds_no_root() -> Result<(), Box<dyn Error>> {
    let index = template(MEDICATION_LIST)?.with_root_lineage(Vec::new());
    let set = set(
        MEDICATION_LIST_FILES,
        &["ferrobridge_library_medication_list.context.yml"],
    )?;
    let Err(diagnostics) = compile(
        &set,
        &MappingName::new("ferrobridge_library_medication_list.context")?,
        &index,
        &SCHEMAS,
        &Library,
    ) else {
        return Err("a composition model compiled with no lineage".into());
    };
    let codes: Vec<String> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect();
    assert_eq!(codes, ["fc-unresolved-archetype-root"]);
    Ok(())
}

#[test]
fn the_medication_list_compiles_onto_medication_statement() -> Result<(), Box<dyn Error>> {
    let (program, _) = program(
        MEDICATION_LIST,
        "ferrobridge_library_medication_list.context",
        MEDICATION_LIST_FILES,
        &[],
    )?;
    assert_eq!(program.resource().as_str(), "MedicationStatement");
    assert_eq!(
        models(&program),
        ["COMPOSITION.medication_list.v1.MedicationStatement"]
    );
    Ok(())
}

/// The section slots FerroBRIDGE's stand-in for `SECTION.adhoc.v1`, which
/// slots the two published evaluations.
#[test]
fn the_health_summary_compiles_its_section_evaluations() -> Result<(), Box<dyn Error>> {
    let (program, _) = program(
        HEALTH_SUMMARY,
        "ferrobridge_library_health_summary.context",
        HEALTH_SUMMARY_FILES,
        HEALTH_SUMMARY_FIXTURES,
    )?;
    assert_eq!(program.resource().as_str(), "Composition");
    assert_eq!(
        models(&program),
        [
            "COMPOSITION.health_summary.v1",
            "SECTION.adhoc.v1",
            "EVALUATION.absence.v2",
            "EVALUATION.exclusion_global.v1"
        ]
    );
    Ok(())
}

/// Each finds its root through the template lineage and refuses only on the
/// defects #101 records: `null_flavour` below a `DV_DATE_TIME` and a
/// `PARTY_PROXY`, two methods of one name, FHIR elements the resource does
/// not define, and a `participations` list walked as a tail.
#[test]
fn the_defective_composition_models_refuse_as_pinned() -> Result<(), Box<dyn Error>> {
    let mut refused = serde_json::Map::new();
    for (slug, context, file) in REFUSED {
        let id = format!("openEHR-EHR-COMPOSITION.ferrobridge_library_{slug}.v1.0.0");
        let Err(diagnostics) = compiled(&id, context, &[file], &[])? else {
            return Err(format!("{file} compiled").into());
        };
        let rows: Vec<String> = diagnostics.iter().map(row).collect();
        assert!(
            rows.iter()
                .all(|row| !row.ends_with("fc-unresolved-archetype-root")),
            "{rows:?}"
        );
        refused.insert((*file).to_owned(), serde_json::json!(rows));
    }
    insta::assert_json_snapshot!("library_composition_refusals", refused);
    Ok(())
}
