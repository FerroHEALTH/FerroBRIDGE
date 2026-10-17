// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mapping checker: every diagnostic a mapping set raises, without
//! starting a lane.
//!
//! `ferrobridge mapping check` and the operator API run the same check. It
//! loads the FHIRconnect tree, compiles every context against the template
//! it names, and loads the OMOCL files, collecting every
//! [`Diagnostic`] instead of stopping at the first refusal. A diagnostic is
//! about the content of a file; a directory that cannot be read or a CDR that
//! does not answer is an [`Error`], because the check did not run. No
//! specification governs the checker: our own design.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use fhirconnect::model::load::MappingSet;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::diagnostic::DiagnosticCode;
use openehr_mapping_core::diagnostic::LanguageCode;
use openehr_mapping_core::diagnostic::Severity;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;
use serde::Serialize;

use crate::config::Settings;

/// The extension an operational template file carries.
const TEMPLATE_EXTENSION: &str = "opt";

/// The extensions an OMOCL file carries.
const OMOCL_EXTENSIONS: [&str; 2] = ["yml", "yaml"];

/// The code of a context whose template no template source provides.
const UNKNOWN_TEMPLATE: &str = "fb-unknown-template";

/// The code of a context that names no template identifier.
const NO_TEMPLATE_ID: &str = "fb-no-template-id";

/// Why a check did not run.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Neither `[mappings] directory` nor `[mappings] omocl` is set.
    #[error("there is nothing to check: set [mappings] directory, [mappings] omocl, or both")]
    NothingConfigured,
    /// The FHIRconnect tree is set with nowhere to take its templates from.
    #[error(
        "[mappings] directory {} is set with no [mappings] templates and no [cdr]: the contexts have no template to compile against",
        directory.display()
    )]
    NoTemplateSource {
        /// The FHIRconnect directory that was configured.
        directory: PathBuf,
    },
    /// A configured directory or a file in it could not be read.
    #[error("cannot read {}", path.display())]
    Read {
        /// The path that could not be read.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// The CDR did not answer a template fetch with a template or a `404`.
    #[error("the CDR did not serve the template `{template}`")]
    Template {
        /// The template identifier that was asked for.
        template: String,
        /// What the fetch reported, the upstream status included.
        #[source]
        source: Box<crate::mappings::Error>,
    },
}

/// Where the operational templates the contexts compile against come from.
#[derive(Debug, Clone, Copy)]
pub enum Templates<'a> {
    /// Every `.opt` file under a directory, as OPT 1.4 XML.
    Directory(&'a Path),
    /// The CDR, one fetch per template identifier a context names.
    Cdr(&'a crate::cdr::CdrClient),
}

/// What one check reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct Inputs<'a> {
    /// The FHIRconnect tree, when one is checked.
    pub fhirconnect: Option<&'a Path>,
    /// Where the FHIRconnect contexts take their templates from.
    pub templates: Option<Templates<'a>>,
    /// The OMOCL directory, when one is checked.
    pub omocl: Option<&'a Path>,
}

/// How much one check read and compiled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// The FHIRconnect files read.
    pub fhirconnect_files: usize,
    /// The FHIRconnect context files of a set that loaded.
    pub contexts: usize,
    /// The contexts that compiled into a program.
    pub programs: usize,
    /// The templates the contexts compiled against.
    pub templates: usize,
    /// The OMOCL files read.
    pub omocl_files: usize,
}

/// Every diagnostic one check raised, in the order it raised them.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The diagnostics, FHIRconnect first, then OMOCL.
    diagnostics: Vec<Diagnostic>,
    /// What was read.
    counts: Counts,
    /// The FHIRconnect set, when it loaded.
    set: Option<MappingSet>,
    /// The programs the contexts compiled into, in context-name order.
    programs: Vec<Arc<Program>>,
    /// The templates the contexts compiled against.
    templates: Vec<TemplateEntry>,
    /// The OMOCL files read.
    omocl_files: Vec<PathBuf>,
}

/// Where one template came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateOrigin {
    /// An `.opt` file of the template directory.
    File(PathBuf),
    /// The CDR.
    Cdr,
}

/// One template the contexts compiled against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateEntry {
    /// The template identifier.
    id: String,
    /// Where it came from.
    origin: TemplateOrigin,
}

impl TemplateEntry {
    /// Returns the template identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns where it came from.
    #[must_use]
    pub const fn origin(&self) -> &TemplateOrigin {
        &self.origin
    }
}

impl Report {
    /// Returns every diagnostic, errors and warnings, in the order raised.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Returns how much the check read and compiled.
    #[must_use]
    pub const fn counts(&self) -> Counts {
        self.counts
    }

    /// Returns the FHIRconnect set, when it loaded.
    #[must_use]
    pub const fn fhirconnect_set(&self) -> Option<&MappingSet> {
        self.set.as_ref()
    }

    /// Returns the programs the contexts compiled into.
    #[must_use]
    pub fn programs(&self) -> &[Arc<Program>] {
        &self.programs
    }

    /// Returns the templates the contexts compiled against, by identifier.
    #[must_use]
    pub fn templates(&self) -> &[TemplateEntry] {
        &self.templates
    }

    /// Returns the OMOCL files read.
    #[must_use]
    pub fn omocl_files(&self) -> &[PathBuf] {
        &self.omocl_files
    }

    /// Returns how many diagnostics carry `severity`.
    #[must_use]
    pub fn count(&self, severity: Severity) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity() == severity)
            .count()
    }

    /// Whether the set loads: no diagnostic is an error.
    ///
    /// A warning is a disagreement the compiler accepted, so a set with
    /// warnings and no error is clean, the same rule `serve` starts by.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.count(Severity::Error) == 0
    }

    /// Returns the report as JSON: `clean`, `counts` and one object per
    /// diagnostic.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "clean": self.is_clean(),
            "counts": self.counts,
            "diagnostics": self.diagnostics.iter().map(|diagnostic| diagnostic_json(diagnostic, &diagnostic.file().display().to_string())).collect::<Vec<serde_json::Value>>(),
        })
    }
}

impl fmt::Display for Report {
    /// Renders one line per diagnostic, then a summary line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for diagnostic in &self.diagnostics {
            writeln!(f, "{diagnostic}")?;
        }
        let counts = self.counts;
        write!(
            f,
            "checked {} FHIRconnect files ({} contexts, {} programs over {} templates) and {} OMOCL files: {} errors, {} warnings",
            counts.fhirconnect_files,
            counts.contexts,
            counts.programs,
            counts.templates,
            counts.omocl_files,
            self.count(Severity::Error),
            self.count(Severity::Warning),
        )
    }
}

/// Returns one diagnostic in the serde form of [`Diagnostic`], its file named
/// `file`, so a wire answer never names a host directory.
///
/// # Panics
///
/// Never in practice: the serde form of a [`Diagnostic`] is a flat object of
/// strings, numbers and a string array, which `serde_json` always renders.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "the serde form of a Diagnostic is a flat object of strings, numbers and a string array, which serde_json always renders"
)]
pub fn diagnostic_json(diagnostic: &Diagnostic, file: &str) -> serde_json::Value {
    let mut value =
        serde_json::to_value(diagnostic).expect("a diagnostic should render as a JSON object");
    if let Some(object) = value.as_object_mut() {
        object.insert(String::from("file"), serde_json::Value::from(file));
    }
    value
}

/// Returns the inputs `settings` configures, the CDR `cdr` reaches serving
/// the templates when `[mappings] templates` is unset.
///
/// # Errors
///
/// Returns [`Error::NothingConfigured`] when neither mapping directory is
/// set, and [`Error::NoTemplateSource`] when the FHIRconnect tree is set with
/// neither a template directory nor a CDR.
pub fn inputs<'a>(
    settings: &'a Settings,
    cdr: Option<&'a crate::cdr::CdrClient>,
) -> Result<Inputs<'a>, Error> {
    let fhirconnect = settings.mapping_directory.as_deref();
    let omocl = settings.omocl_directory.as_deref();
    if fhirconnect.is_none() && omocl.is_none() {
        return Err(Error::NothingConfigured);
    }
    // NOTE: no specification governs this: our own design; a template
    // directory is the operator's explicit choice, so it wins over the CDR.
    let templates = match (settings.mappings.as_ref(), cdr) {
        (Some(mappings), _) => Some(Templates::Directory(&mappings.templates)),
        (None, Some(client)) => Some(Templates::Cdr(client)),
        (None, None) => None,
    };
    if let (Some(directory), None) = (fhirconnect, templates) {
        return Err(Error::NoTemplateSource {
            directory: directory.to_path_buf(),
        });
    }
    Ok(Inputs {
        fhirconnect,
        templates,
        omocl,
    })
}

/// Checks the mapping set `inputs` names and returns every diagnostic.
///
/// The FHIRconnect files are loaded and validated as one set; when the set
/// loads, every context is compiled against its template and every warning
/// of a compiled program is reported too. The OMOCL files are loaded and
/// validated; OMOCL names no template, so they are compiled when a run first
/// meets a composition, never here.
///
/// # Errors
///
/// Returns [`Error::NoTemplateSource`] when a FHIRconnect tree comes with no
/// template source, [`Error::Read`] when a directory cannot be read, and
/// [`Error::Template`] when the CDR answers a template fetch with neither the
/// template nor a `404`.
pub async fn check(inputs: Inputs<'_>) -> Result<Report, Error> {
    let mut report = Report::default();
    if let Some(directory) = inputs.fhirconnect {
        let templates = inputs.templates.ok_or_else(|| Error::NoTemplateSource {
            directory: directory.to_path_buf(),
        })?;
        check_fhirconnect(directory, templates, &mut report).await?;
    }
    if let Some(directory) = inputs.omocl {
        check_omocl(directory, &mut report)?;
    }
    Ok(report)
}

/// Loads, resolves and compiles the FHIRconnect tree under `directory`.
async fn check_fhirconnect(
    directory: &Path,
    templates: Templates<'_>,
    report: &mut Report,
) -> Result<(), Error> {
    let files = crate::walk::mapping_files(directory).map_err(read_error)?;
    report.counts.fhirconnect_files = files.len();
    let set = match fhirconnect::model::load::load_set(&files, &StaticMappingCodes::default()) {
        Ok(set) => set,
        Err(mut diagnostics) => {
            report.diagnostics.append(&mut diagnostics);
            return Ok(());
        }
    };
    report.counts.contexts = set.contexts().count();
    let indexes = match templates {
        Templates::Directory(templates) => directory_templates(templates, report)?,
        Templates::Cdr(client) => cdr_templates(&set, client, report).await?,
    };
    report.counts.templates = indexes.len();
    compile_contexts(&set, &indexes, report);
    report.set = Some(set);
    Ok(())
}

/// Builds the index of every `.opt` file under `directory`, reporting each
/// one that does not build.
fn directory_templates(
    directory: &Path,
    report: &mut Report,
) -> Result<BTreeMap<String, Arc<WebTemplateIndex>>, Error> {
    let files = crate::walk::files(directory, |path| {
        crate::walk::has_extension(path, &[TEMPLATE_EXTENSION])
    })
    .map_err(read_error)?;
    let mut indexes = BTreeMap::new();
    for path in files {
        let xml = std::fs::read_to_string(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        let built = TemplateSource::opt14(&xml).and_then(|source| WebTemplateIndex::build(&source));
        match built {
            Ok(index) => {
                report.templates.push(TemplateEntry {
                    id: index.template_id().to_owned(),
                    origin: TemplateOrigin::File(path.clone()),
                });
                indexes.insert(index.template_id().to_owned(), Arc::new(index));
            }
            Err(error) => report.diagnostics.push(Diagnostic::error(
                &path,
                DiagnosticCode::TemplateBuild,
                error.to_string(),
            )),
        }
    }
    Ok(indexes)
}

/// Fetches every template a context of `set` names from the CDR, once each,
/// reporting a template the CDR does not hold against the first context, in
/// name order, that names it.
async fn cdr_templates(
    set: &MappingSet,
    client: &crate::cdr::CdrClient,
    report: &mut Report,
) -> Result<BTreeMap<String, Arc<WebTemplateIndex>>, Error> {
    let mut wanted: BTreeMap<String, PathBuf> = BTreeMap::new();
    for context in set.contexts() {
        if let Some(template) = template_of(context) {
            wanted
                .entry(template)
                .or_insert_with(|| context.file().to_path_buf());
        }
    }
    let mut indexes = BTreeMap::new();
    for (template, file) in wanted {
        match crate::mappings::template_index(client, &template).await {
            Ok(index) => {
                report.templates.push(TemplateEntry {
                    id: template.clone(),
                    origin: TemplateOrigin::Cdr,
                });
                indexes.insert(template, Arc::new(index));
            }
            Err(crate::mappings::Error::TemplateNotHeld { status, .. }) => {
                report.diagnostics.push(Diagnostic::error(
                    &file,
                    language(UNKNOWN_TEMPLATE),
                    format!("the CDR holds no template `{template}`: it answered {status}"),
                ));
            }
            Err(error @ crate::mappings::Error::TemplateName { .. }) => {
                report.diagnostics.push(Diagnostic::error(
                    &file,
                    language(UNKNOWN_TEMPLATE),
                    error.to_string(),
                ));
            }
            Err(crate::mappings::Error::CdrTemplate { source, .. }) => {
                report.diagnostics.push(Diagnostic::error(
                    &file,
                    DiagnosticCode::TemplateBuild,
                    format!("the template `{template}` the CDR served did not load: {source}"),
                ));
            }
            Err(source) => {
                return Err(Error::Template {
                    template,
                    source: Box::new(source),
                });
            }
        }
    }
    Ok(indexes)
}

/// Compiles every context of `set` against the template it names, reporting
/// every refusal and every warning.
fn compile_contexts(
    set: &MappingSet,
    indexes: &BTreeMap<String, Arc<WebTemplateIndex>>,
    report: &mut Report,
) {
    for context in set.contexts() {
        let file = context.file();
        let name = context.header().name().value();
        let Some(template) = template_of(context) else {
            report.diagnostics.push(
                Diagnostic::error(
                    file,
                    language(NO_TEMPLATE_ID),
                    "the context names no template id",
                )
                .with_mapping_name(name.clone()),
            );
            continue;
        };
        let Some(index) = indexes.get(&template) else {
            // NOTE: a template the CDR does not hold was reported once, at
            // its fetch, so only a template directory leaves a gap here.
            if !report.diagnostics.iter().any(|diagnostic| {
                diagnostic.file() == file && diagnostic.code() == &language(UNKNOWN_TEMPLATE)
            }) {
                report.diagnostics.push(
                    Diagnostic::error(
                        file,
                        language(UNKNOWN_TEMPLATE),
                        format!("the context maps `{template}`, which no loaded template provides"),
                    )
                    .with_mapping_name(name.clone()),
                );
            }
            continue;
        };
        match compile(
            set,
            name,
            index,
            &fhir_types::r4::schema::SCHEMAS,
            &StaticMappingCodes::default(),
        ) {
            Ok(program) => {
                report.counts.programs = report.counts.programs.saturating_add(1);
                report
                    .diagnostics
                    .extend(program.warnings().iter().cloned());
                report.programs.push(program);
            }
            Err(mut diagnostics) => report.diagnostics.append(&mut diagnostics),
        }
    }
}

/// Loads and validates the OMOCL files under `directory`.
fn check_omocl(directory: &Path, report: &mut Report) -> Result<(), Error> {
    let files = crate::walk::files(directory, |path| {
        crate::walk::has_extension(path, &OMOCL_EXTENSIONS)
    })
    .map_err(read_error)?;
    report.counts.omocl_files = files.len();
    let loaded =
        omocl::model::load::load_set(&files, &omocl::model::semantic::FirstPartyConverters);
    report.omocl_files = files;
    if let Err(mut diagnostics) = loaded {
        report.diagnostics.append(&mut diagnostics);
    }
    Ok(())
}
/// Returns the template identifier one context names, if it names one.
fn template_of(context: &fhirconnect::model::ast::ContextMappingFile) -> Option<String> {
    context
        .context()
        .template
        .id
        .as_ref()
        .map(|located| located.value().clone())
}

/// Returns the checker's own diagnostic code `code`.
#[expect(
    clippy::expect_used,
    reason = "every caller passes one of this module's constants, each lowercase letters and hyphens"
)]
fn language(code: &str) -> DiagnosticCode {
    DiagnosticCode::Language(
        LanguageCode::new(code).expect("the checker's codes should be in the code alphabet"),
    )
}

/// Returns the check error a refused walk describes.
fn read_error(crate::walk::Error { path, source }: crate::walk::Error) -> Error {
    Error::Read { path, source }
}

#[cfg(test)]
mod tests {
    use super::{NO_TEMPLATE_ID, UNKNOWN_TEMPLATE, language};

    #[test]
    fn the_checker_codes_are_in_the_code_alphabet() {
        for code in [NO_TEMPLATE_ID, UNKNOWN_TEMPLATE] {
            assert_eq!(code, language(code).as_str());
        }
    }
}
