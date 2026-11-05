// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The library's medication clusters, whose constituents are medication
//! clusters again, compiled through a recursive slot against a template that
//! nests the CKM medication cluster a given number of times.
//!
//! `medication.v2.yml` and `medication.v2.substance.yml` slot their own
//! archetype, as the CKM cluster's constituent slot (`at0138`) admits. The
//! recursion descends while the template carries a node and ends where it
//! carries none (#1551; FHIRconnect v1.0.0 is silent: our own design).

use core::error::Error;
use std::fmt::Write;
use std::sync::Arc;

use ferrobridge_testkit::fixtures::CKM_MEDICATION_ARCHETYPES;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::library::Library;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use fhirconnect::resolve::program::SLOT_DEPTH_LIMIT;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use super::models;
use super::set;

/// The template the medication context names.
const TEMPLATE: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_medication.v1.0.0";

/// The context this crate authored over the medication clusters.
const CONTEXT: &str = "ferrobridge_library_medication.context";

/// The two published medication cluster models.
pub(super) const FILES: &[&str] = &[
    "model/cluster/org.openehr/medication.v2.yml",
    "model/cluster/org.openehr/medication.v2.substance.yml",
];

/// FerroBRIDGE's medication administration model and its context, relative
/// to the project directory.
const FIXTURES: &[&str] = &[
    "ferrobridge_medication_administration.yml",
    "ferrobridge_library_medication.context.yml",
];

/// The `language` and `description` sections every artefact of the template
/// carries.
const HEADER: &str = r#"language
    original_language = <[ISO_639-1::en]>

description
    lifecycle_state = <"unmanaged">
    original_author = <
        ["name"] = <"FerroBRIDGE">
    >
    details = <
        ["en"] = <
            language = <[ISO_639-1::en]>
            purpose = <"The medication cluster recursion of FerroBRIDGE's mapping library suite.">
        >
    >
"#;

/// Returns one artefact of the template: `kind` and `id` over `parent`, its
/// root `rm` code `at0000.1` holding `slot` filled by `filler`.
fn artefact(
    kind: &str,
    id: &str,
    parent: &str,
    rm: &str,
    at: &str,
    slot: &str,
    filler: &str,
) -> String {
    format!(
        "{kind}\n    {id}\n\nspecialize\n    {parent}\n\n{HEADER}\ndefinition\n    {rm}[at0000.1] matches {{\n        \
         {at} matches {{\n            use_archetype {filler}\n        }}\n    }}\n\nterminology\n    \
         term_definitions = <\n        [\"en\"] = <\n            [\"at0000.1\"] = <\n                text = \
         <\"{rm}\">\n                description = <\"{rm}\">\n            >\n            [\"{slot}\"] = <\n      \
         text = <\"Slot\">\n                description = <\"Slot\">\n            >\n        >\n    >\n"
    )
}

/// Returns FerroBRIDGE's ADL 2 template holding one medication ACTION whose
/// medication cluster nests `depth` constituent medication clusters.
///
/// No specification governs this template: our own design, built over the
/// vendored CKM medication archetypes.
fn nested_template(depth: usize) -> Result<String, Box<dyn Error>> {
    let filler = |index: usize| {
        if index >= depth {
            String::from("openEHR-EHR-CLUSTER.medication.v2")
        } else {
            format!("openEHR-EHR-CLUSTER.medication-ferrobridge_level{index}.v2")
        }
    };
    let mut text = artefact(
        "template (adl_version=2.4.0; rm_release=1.1.0)",
        TEMPLATE,
        "openEHR-EHR-COMPOSITION.encounter.v1",
        "COMPOSITION",
        "/content",
        "at0000.2",
        "ACTION[at0000.2, openEHR-EHR-ACTION.medication-ferrobridge.v1]",
    );
    let separator = "------------------------------------------------------------------------";
    write!(
        text,
        "\n{separator}\n{}",
        artefact(
            "template_overlay",
            "openEHR-EHR-ACTION.medication-ferrobridge.v1.0.0",
            "openEHR-EHR-ACTION.medication.v1",
            "ACTION",
            "/description[at0017]/items",
            "at0104.1",
            &format!("CLUSTER[at0104.1, {}]", filler(0)),
        )
    )?;
    for index in 0..depth {
        write!(
            text,
            "\n{separator}\n{}",
            artefact(
                "template_overlay",
                &format!("{}.0.0", filler(index)),
                "openEHR-EHR-CLUSTER.medication.v2",
                "CLUSTER",
                "/items",
                "at0138.1",
                &format!("CLUSTER[at0138.1, {}]", filler(index.saturating_add(1))),
            )
        )?;
    }
    Ok(text)
}

/// Builds the index over the template nesting `depth` constituents.
fn template(depth: usize) -> Result<WebTemplateIndex, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    for (file, source) in CKM_MEDICATION_ARCHETYPES {
        let archetype = openehr_adl::assemble::parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    let text = nested_template(depth)?;
    let template = openehr_adl::assemble::parse_artefact(&text, Dialect::Adl2)
        .map_err(|errors| format!("the template: {errors:?}"))?;
    let opt = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(opt),
        resolved_id: String::from(TEMPLATE),
    })?)
}

/// Compiles the medication context against the template nesting `depth`
/// constituents.
fn compiled(depth: usize) -> Result<Result<Arc<Program>, Vec<Diagnostic>>, Box<dyn Error>> {
    let index = template(depth)?;
    let set = set(FILES, FIXTURES)?;
    Ok(compile(
        &set,
        &MappingName::new(CONTEXT)?,
        &index,
        &SCHEMAS,
        &Library,
    ))
}

/// Compiles the medication context against a template nesting two
/// constituents, for the corpus reach.
pub(super) fn program() -> Result<Arc<Program>, Box<dyn Error>> {
    compiled(2)?.map_err(|diagnostics| crate::resolve::render(&diagnostics).into())
}

#[test]
fn the_medication_clusters_compile_through_their_recursion() -> Result<(), Box<dyn Error>> {
    let program = program()?;
    assert_eq!(program.resource().as_str(), "MedicationAdministration");
    assert_eq!(
        models(&program),
        [
            "ferrobridge_medication_administration",
            "CLUSTER.medication.v2",
            "CLUSTER.medication.v2.substance"
        ]
    );
    Ok(())
}

#[test]
fn the_recursion_ends_where_the_template_stops_nesting() -> Result<(), Box<dyn Error>> {
    for depth in [2, 3] {
        if let Err(diagnostics) = compiled(depth)? {
            return Err(format!(
                "the template nesting {depth} constituents: {}",
                crate::resolve::render(&diagnostics)
            )
            .into());
        }
    }
    Ok(())
}

/// `medication.v2.yml:96` slots the substance at the constituent's
/// constituent, where its sibling at `:83` names the constituent itself (#101),
/// so a template that nests one constituent carries no node there.
#[test]
fn one_constituent_refuses_the_substance_two_levels_down() -> Result<(), Box<dyn Error>> {
    let Err(diagnostics) = compiled(1)? else {
        return Err("the substance compiled one level short".into());
    };
    let rows: Vec<String> = diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{}:{} {}",
                diagnostic
                    .file()
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                diagnostic
                    .position()
                    .map_or_else(String::new, |position| position.line().to_string()),
                diagnostic.code()
            )
        })
        .collect();
    assert_eq!(rows, ["medication.v2.yml:96 fc-unknown-template-node"]);
    Ok(())
}

#[test]
fn a_template_with_no_constituent_refuses_the_constituent_path() -> Result<(), Box<dyn Error>> {
    // A cluster entered once is no recursion, so a path the template does not
    // carry there is refused as every other unknown node is.
    let Err(diagnostics) = compiled(0)? else {
        return Err("the cluster compiled with no constituent in the template".into());
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "fc-unknown-template-node"),
        "{}",
        crate::resolve::render(&diagnostics)
    );
    Ok(())
}

#[test]
fn a_recursion_past_the_depth_limit_refuses() -> Result<(), Box<dyn Error>> {
    let Err(diagnostics) = compiled(SLOT_DEPTH_LIMIT.saturating_add(1))? else {
        return Err("the recursion compiled past the limit".into());
    };
    let depth: Vec<&Diagnostic> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code().as_str() == "fc-slot-depth")
        .collect();
    assert!(
        !depth.is_empty(),
        "{}",
        crate::resolve::render(&diagnostics)
    );
    assert!(
        depth.iter().all(|diagnostic| diagnostic
            .message()
            .contains(&format!("past the limit of {SLOT_DEPTH_LIMIT}"))),
        "{}",
        crate::resolve::render(&diagnostics)
    );
    Ok(())
}
