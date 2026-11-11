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
//!
//! `medication.v2.yml` does not count as reached (#101): its line 25 maps the
//! `Ratio` `Medication.amount` onto a `DV_QUANTITY`, which
//! `types-of-mappings/data-type/DV_QUANTITY.adoc` does not pair. `:96` slots
//! the substance one constituent too deep, which a template that does not
//! nest that level leaves out (#1883). Its `:72` and `:85`, and
//! `medication.v2.substance.yml:43`, write a `reference` into a choice
//! (`Medication.ingredient.item[x]`, `Substance.ingredient.substance[x]`),
//! which writes the one `Reference` alternative the choice admits. The recursion is pinned
//! by the refusals each template depth raises: past these the template's
//! nesting adds or removes only the template-node refusals.

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

use super::set;

/// The template the medication context names.
const TEMPLATE: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_medication.v1.0.0";

/// The context this crate authored over the medication clusters.
const CONTEXT: &str = "ferrobridge_library_medication.context";

/// The two published medication cluster models.
const FILES: &[&str] = &[
    "model/cluster/org.openehr/medication.v2.yml",
    "model/cluster/org.openehr/medication.v2.substance.yml",
];

/// FerroBRIDGE's medication administration model and its context, relative
/// to the project directory.
const FIXTURES: &[&str] = &[
    "ferrobridge_medication_administration.yml",
    "ferrobridge_library_medication.context.yml",
];

/// The published substance model.
const SUBSTANCE: &[&str] = &["model/cluster/org.openehr/medication.v2.substance.yml"];

/// FerroBRIDGE's medication substance model and its context, relative to
/// the project directory.
const SUBSTANCE_FIXTURES: &[&str] = &[
    "ferrobridge_medication_substance.yml",
    "ferrobridge_library_substance.context.yml",
];

/// The context this crate authored over the substance model.
const SUBSTANCE_CONTEXT: &str = "ferrobridge_library_substance.context";

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
    let raw = openehr_adl::opt::create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
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

/// Returns the refusals of `diagnostics` as sorted, distinct
/// `file:line code` rows.
fn rows(diagnostics: &[Diagnostic]) -> Vec<String> {
    let mut rows: Vec<String> = diagnostics
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
    rows.sort();
    rows.dedup();
    rows
}

/// The refusals the medication model raises at every depth the template
/// nests it to (#101).
const LIBRARY_REFUSALS: [&str; 1] = ["medication.v2.yml:25 fc-unpaired-types"];

/// Returns the refusals of the medication context against the template
/// nesting `depth` constituents.
fn refused(depth: usize) -> Result<Vec<String>, Box<dyn Error>> {
    match compiled(depth)? {
        Ok(_) => Err(format!("the template nesting {depth} constituents compiles").into()),
        Err(diagnostics) => Ok(rows(&diagnostics)),
    }
}

/// Compiles the substance context against the template nesting one
/// constituent.
fn substance() -> Result<Arc<Program>, Box<dyn Error>> {
    let index = template(1)?;
    let set = set(SUBSTANCE, SUBSTANCE_FIXTURES)?;
    match compile(
        &set,
        &MappingName::new(SUBSTANCE_CONTEXT)?,
        &index,
        &SCHEMAS,
        &Library,
    ) {
        Ok(program) => Ok(program),
        Err(diagnostics) => Err(crate::resolve::render(&diagnostics).into()),
    }
}

#[test]
fn the_recursion_ends_where_the_template_stops_nesting() -> Result<(), Box<dyn Error>> {
    // Two and three constituents raise no template-node refusal: the
    // recursion descends while the template carries a node. With two, `:96`
    // enters the substance at the deepest cluster, where its ingredients are
    // left out; with three, they compile too.
    assert_eq!(refused(2)?, LIBRARY_REFUSALS, "2 constituents");
    assert_eq!(refused(3)?, LIBRARY_REFUSALS, "3 constituents");
    Ok(())
}

/// `medication.v2.yml:96` slots the substance, a second model of the
/// medication archetype, at the constituent's constituent, where its sibling
/// at `:83` names the constituent itself (#101). A template that nests one
/// constituent does not instantiate that level, so the slot ends the
/// recursion there with no refusal: the template, never the mapping, sets
/// the depth (#1883).
#[test]
fn one_constituent_leaves_the_substance_two_levels_down_out() -> Result<(), Box<dyn Error>> {
    assert_eq!(refused(1)?, LIBRARY_REFUSALS);
    Ok(())
}

/// A template with no constituent keeps the CKM cluster's constituent slot
/// open and fills it with nothing, so the `ingredients` method that slots a
/// constituent (`medication.v2.yml:50`) addresses nodes the template does not
/// carry and is ignored with a warning, as `model-mappings.adoc` §Mapping 0..0
/// fields ignores a mapping not part of the template (coordinator ruling on
/// #1546, 2026-11-11).
#[test]
fn a_template_with_no_constituent_ignores_the_constituent_slot() -> Result<(), Box<dyn Error>> {
    let Err(diagnostics) = compiled(0)? else {
        return Err("the cluster compiled with no constituent in the template".into());
    };
    let rows: Vec<String> = diagnostics
        .iter()
        .map(|diagnostic| {
            let line = diagnostic
                .position()
                .map_or(0, openehr_mapping_core::position::Position::line);
            format!("{line} {} {}", diagnostic.severity(), diagnostic.code())
        })
        .collect();
    assert_eq!(
        rows,
        [
            "25 error fc-unpaired-types",
            "50 warning fc-excluded-template-node"
        ],
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

/// FerroBRIDGE's substance model enters `medication.v2.substance.yml` at the
/// administered cluster; its line 43 writes the ingredient `reference` into
/// the choice `Substance.ingredient.substance[x]`, whose one `Reference`
/// alternative is what a `reference` writes (FHIR R4 formats.html §Choice), so
/// it compiles.
#[test]
fn the_substance_ingredient_reference_writes_its_reference_alternative()
-> Result<(), Box<dyn Error>> {
    let program = substance()?;
    assert_eq!(program.resource().as_str(), "Substance");
    Ok(())
}

/// Compiles the substance context and returns it with the published model
/// it slots.
pub(super) fn chain() -> Result<crate::support::Chain, Box<dyn Error>> {
    Ok((SUBSTANCE, substance()?))
}
