// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the suites share: the synthetic template of the testkit, the loader
//! for the mapping fixtures beside them, and the diagnostic renderer.

use core::error::Error;
use std::sync::Arc;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::model::ast::ContextMappingFile;
use fhirconnect::model::ast::ModelMappingFile;
use fhirconnect::model::load::MappingSet;
use fhirconnect::model::parse::lower_context;
use fhirconnect::model::parse::lower_model;
use fhirconnect::model::semantic::MappingCodeRegistry;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

/// The fixtures this crate's tests ship.
pub(crate) const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// The FHIRconnect mappings the `eehrxf` crate ships, one directory per
/// project, which these suites check as they check their own.
pub(crate) const SHIPPED: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/eehrxf/mappings");

/// Builds the index over the testkit's diagnosis template.
pub(crate) fn template() -> Result<WebTemplateIndex, Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(ferrobridge_testkit::fixtures::DIAGNOSE_OPT)?;
    Ok(WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(
        opt,
    )))?)
}

/// Compiles the context and model fixture pair named `fixture`.
pub(crate) fn compiled(fixture: &str) -> Result<Arc<Program>, Box<dyn Error>> {
    compiled_against(fixture, &template()?)
}

/// Compiles the context and model fixture pair named `fixture` against `index`.
pub(crate) fn compiled_against(
    fixture: &str,
    index: &WebTemplateIndex,
) -> Result<Arc<Program>, Box<dyn Error>> {
    compiled_with(fixture, index, &StaticMappingCodes::default())
}

/// Compiles the context and model fixture pair named `fixture` against
/// `index`, with `codes` as the `mappingCode` registry.
pub(crate) fn compiled_with(
    fixture: &str,
    index: &WebTemplateIndex,
    codes: &dyn MappingCodeRegistry,
) -> Result<Arc<Program>, Box<dyn Error>> {
    compiled_models(fixture, &[fixture], index, codes)
}

/// Compiles the context fixture named `fixture` with the model fixtures
/// `models` against `index`, with `codes` as the `mappingCode` registry.
pub(crate) fn compiled_models(
    fixture: &str,
    models: &[&str],
    index: &WebTemplateIndex,
    codes: &dyn MappingCodeRegistry,
) -> Result<Arc<Program>, Box<dyn Error>> {
    let mut set = MappingSet::default();
    let mut files = vec![(
        format!("contexts/{fixture}.context.yml"),
        std::fs::read_to_string(format!("{FIXTURES}/contexts/{fixture}.context.yml"))?,
    )];
    for model in models {
        files.push((
            format!("model/{model}.yml"),
            std::fs::read_to_string(format!("{FIXTURES}/model/{model}.yml"))?,
        ));
    }
    for (name, text) in files {
        let name = name.as_str();
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
    let context = MappingName::new(format!("{fixture}.context"))?;
    compile(&set, &context, index, &SCHEMAS, codes)
        .map_err(|diagnostics| Box::<dyn Error>::from(render(&diagnostics)))
}

/// Renders a diagnostic list as one error message.
pub(crate) fn render(diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{} {} {}: {}",
                diagnostic.file().display(),
                diagnostic.severity(),
                diagnostic.code(),
                diagnostic.message()
            )
        })
        .collect::<Vec<String>>()
        .join("; ")
}

/// Renders diagnostics as sorted `file:line severity code` rows, the file by
/// its name, so a warning never reads as a refusal.
pub(crate) fn rows(diagnostics: &[Diagnostic]) -> Vec<String> {
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
            format!(
                "{file}:{line} {} {}",
                diagnostic.severity(),
                diagnostic.code()
            )
        })
        .collect();
    found.sort();
    found
}

/// A compiled program with the published files of the library it loads,
/// relative to the vendored library.
pub(crate) type Chain = (&'static [&'static str], Arc<Program>);

/// A compiled program with the library files its set loaded, owned, for a
/// file list computed at run time.
pub(crate) type OwnedChain = (Vec<String>, Arc<Program>);

/// The #101 comment that records each refusal a chain pins as a library
/// defect, keyed by `file:line`.
///
/// The suite cannot read the tracker, so each chain pins the id of the
/// comment beside the row, checked by hand when it is added.
pub(crate) type Recorded = &'static [(&'static str, u64)];

/// Returns the `file:line` of every error row with no recorded comment, and
/// every recorded `file:line` no error row names any more.
pub(crate) fn unrecorded(rows: &[String], recorded: Recorded) -> (Vec<String>, Vec<String>) {
    let errors: std::collections::BTreeSet<String> = rows
        .iter()
        .filter_map(|row| {
            let (place, rest) = row.split_once(' ')?;
            rest.starts_with("error ").then(|| place.to_owned())
        })
        .collect();
    let pinned: std::collections::BTreeSet<String> = recorded
        .iter()
        .map(|&(place, _)| place.to_owned())
        .collect();
    (
        errors.difference(&pinned).cloned().collect(),
        pinned.difference(&errors).cloned().collect(),
    )
}
