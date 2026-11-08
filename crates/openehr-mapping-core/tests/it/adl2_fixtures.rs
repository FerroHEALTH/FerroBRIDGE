// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ADL 2 twin of the testkit's ADL 1.4 minimal evaluation: the two
//! at-coded sources under `tests/fixtures/adl2/` compiled into an
//! operational template with `openehr-adl`, indexed as an `adl2`
//! template, and held node for node to the OPT 1.4 the testkit carries.
//!
//! ADL 2.4 admits at-codes as node identifiers and has the at-code system
//! used by systems conformant to the openEHR Reference Model
//! (<https://specifications.openehr.org/releases/AM/Release-2.4.0/ADL2.html>,
//! §ADL 2.4), so the at-coded twin builds the same Web Template paths.

use std::collections::BTreeSet;
use std::error::Error;

use ferrobridge_testkit::fixtures::MINIMAL_EVALUATION_OPT;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::assemble::parse_artefact;
use openehr_adl::opt::RawOpt;
use openehr_adl::opt::create_opt;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::Generation;
use openehr_mapping_core::template::TemplateSource;

/// The ADL 2 archetype of the synthetic note.
const NOTE: &str = include_str!("../fixtures/adl2/note_evaluation.adls");

/// The ADL 2 template that slots it into a composition.
const MINIMAL: &str = include_str!("../fixtures/adl2/minimal_evaluation.adls");

/// Compiles the two ADL 2 sources into an operational template and indexes
/// it.
fn adl2_index() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let note = parse_artefact(NOTE, Dialect::Adl2).map_err(|errors| format!("{errors:?}"))?;
    let minimal = parse_artefact(MINIMAL, Dialect::Adl2).map_err(|errors| format!("{errors:?}"))?;
    let mut repository = ArchetypeRepository::default();
    repository.insert(note);
    let raw = create_opt(&minimal, &repository)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
        resolved_id: String::from("openEHR-EHR-COMPOSITION.ferrobridge_minimal.v1.0.0"),
    })?)
}

/// Returns the RM type and the path below the composition of every node
/// under the composition's content.
fn content_nodes(index: &WebTemplateIndex) -> BTreeSet<(String, String)> {
    index
        .nodes()
        .filter(|node| node.aql_path().as_str().starts_with("/content"))
        .map(|node| (node.rm_type().to_owned(), node.aql_path().to_string()))
        .collect()
}

#[test]
fn the_adl2_twin_indexes_as_an_adl2_template() -> Result<(), Box<dyn Error>> {
    let index = adl2_index()?;
    assert_eq!(index.generation(), Generation::Adl2);
    assert!(
        index
            .nodes()
            .any(|node| node.aql_path().as_str().ends_with("/items[at0002]/value")),
        "the at-coded note element is a node"
    );
    Ok(())
}

#[test]
fn the_adl2_twin_builds_the_content_paths_of_the_adl14_template() -> Result<(), Box<dyn Error>> {
    let adl2 = adl2_index()?;
    let opt = openehr_its::opt14::from_xml(MINIMAL_EVALUATION_OPT)?;
    let adl14 = WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(opt)))?;
    assert_eq!(
        content_nodes(&adl2),
        content_nodes(&adl14),
        "the two generations agree on every content node"
    );
    Ok(())
}

/// Compiles the two ADL 2 sources into an operational template.
fn adl2_opt() -> Result<RawOpt, Box<dyn Error>> {
    let note = parse_artefact(NOTE, Dialect::Adl2).map_err(|errors| format!("{errors:?}"))?;
    let minimal = parse_artefact(MINIMAL, Dialect::Adl2).map_err(|errors| format!("{errors:?}"))?;
    let mut repository = ArchetypeRepository::default();
    repository.insert(note);
    Ok(create_opt(&minimal, &repository)?)
}

#[test]
fn an_opt2_root_carries_the_parent_its_template_states() -> Result<(), Box<dyn Error>> {
    // AM Release-2.3.0 AOM2 §3.6.6: an OPERATIONAL_TEMPLATE is specialised.
    let mut raw = adl2_opt()?;
    raw.template.parent_archetype_id = Some(String::from("openEHR-EHR-COMPOSITION.report.v1"));
    let index = WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
        resolved_id: String::from("openEHR-EHR-COMPOSITION.minimal.v1.0.0"),
    })?;
    assert_eq!(index.root_lineage(), ["openEHR-EHR-COMPOSITION.report.v1"]);
    Ok(())
}

#[test]
fn an_opt2_that_states_no_parent_carries_the_lineage_a_caller_passes() -> Result<(), Box<dyn Error>>
{
    let raw = adl2_opt()?;
    let index = WebTemplateIndex::build(&TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
        resolved_id: String::from("openEHR-EHR-COMPOSITION.minimal.v1.0.0"),
    })?;
    assert_eq!(index.root_lineage().len(), 0, "{:?}", index.root_lineage());
    let index = index.with_root_lineage(vec![
        String::from("openEHR-EHR-COMPOSITION.report-result.v1"),
        String::from("openEHR-EHR-COMPOSITION.report.v1"),
    ]);
    assert_eq!(
        index.root_lineage(),
        [
            "openEHR-EHR-COMPOSITION.report-result.v1",
            "openEHR-EHR-COMPOSITION.report.v1"
        ]
    );
    Ok(())
}
