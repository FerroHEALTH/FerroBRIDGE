// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR `Consent` of a KDS study participation: the published informed
//! consent model of the FHIRconnect mapping library and the study
//! participation cluster it slots, under a FerroBRIDGE project context
//! compiled against the published `Studienteilnahme` operational template.
//!
//! The published `study_details.v1.yml` carries a `null` `mappings` and is
//! replaced by a stand-in with an empty list, so the chain reaches the
//! informed consent model past the cascade the published context stops on.

use core::error::Error;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::support::FIXTURES;
use crate::support::render;
use crate::support::rows;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The published `Studienteilnahme` operational template of the testkit.
const TEMPLATE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt/kds-studienteilnahme/Studienteilnahme.opt"
);

/// The published files the project context loads, as the library ships them.
const PUBLISHED: &[&str] = &[
    "model/action/org.openehr/informed_consent.v0.yml",
    "model/cluster/org.highmed/study_participation.v1.yml",
];

/// The project directory this crate authored for the chain.
const PROJECT_DIR: &str = "projects/ferrobridge/kds_studienteilnahme";

/// The files of the project directory, the context first.
const PROJECT: &[&str] = &[
    "ferrobridge_kds_studienteilnahme.context.yml",
    "ferrobridge_study_details.v1.yml",
];

/// The context this crate authored over the chain.
const CONTEXT: &str = "ferrobridge_kds_studienteilnahme.context";

/// Builds the index over the published `Studienteilnahme` template.
fn studienteilnahme_template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(&std::fs::read_to_string(TEMPLATE)?)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// The published informed consent model over the published study
/// participation cluster, with only the study details stand-in, is refused
/// exactly; each refusal is recorded on #101:
///
/// - `informed_consent.v0.yml:35`, `:58`, `:80`, `:102`, `:124`, `:146`: the
///   `ISMTransition` manuals write the string `openehr` onto
///   `defining_code/terminology_id`, a `TERMINOLOGY_ID` object, where
///   `manual.adoc` writes `defining_code/terminology_id/value`.
#[test]
fn the_kds_studienteilnahme_chain_as_published_is_refused_exactly() -> Result<(), Box<dyn Error>> {
    let files: Vec<String> = PUBLISHED
        .iter()
        .map(|file| format!("{CORPUS}/{file}"))
        .chain(
            PROJECT
                .iter()
                .map(|file| format!("{FIXTURES}/{PROJECT_DIR}/{file}")),
        )
        .collect();
    let codes = StaticMappingCodes::default();
    let set = load_set(files, &codes).map_err(|d| render(&d))?;
    let diagnostics = compile(
        &set,
        &MappingName::new(CONTEXT)?,
        &studienteilnahme_template()?,
        &SCHEMAS,
        &codes,
    )
    .err()
    .ok_or("the study participation chain as published compiles")?;
    insta::assert_json_snapshot!(
        "kds_studienteilnahme_chain_as_published",
        rows(&diagnostics)
    );
    Ok(())
}
