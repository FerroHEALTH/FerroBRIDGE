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
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::diagnostic::Severity;
use openehr_mapping_core::index::WebTemplateIndex;
use serde::Serialize;

use crate::config::Settings;
use crate::mappings::compile;
use crate::mappings::compile::TemplateEntry;

/// The extensions an OMOCL file carries.
const OMOCL_EXTENSIONS: [&str; 2] = ["yml", "yaml"];

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
    /// The Web Template indices the contexts compiled against, by identifier.
    indexes: BTreeMap<String, Arc<WebTemplateIndex>>,
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

    /// Returns the Web Template indices the contexts compiled against.
    #[must_use]
    pub const fn indexes(&self) -> &BTreeMap<String, Arc<WebTemplateIndex>> {
        &self.indexes
    }

    /// Returns the OMOCL files read.
    #[must_use]
    pub fn omocl_files(&self) -> &[PathBuf] {
        &self.omocl_files
    }

    /// Returns the report with `diagnostic` raised after every other.
    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.diagnostics.push(diagnostic);
        self
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
    let fhirconnect = match inputs.fhirconnect {
        Some(directory) => Some(crate::walk::mapping_files(directory).map_err(read_error)?),
        None => None,
    };
    let omocl = match inputs.omocl {
        Some(directory) => Some(omocl_files(directory)?),
        None => None,
    };
    check_files(fhirconnect.as_deref(), inputs.templates, omocl.as_deref()).await
}

/// Returns the OMOCL files under `directory`, sorted.
///
/// # Errors
///
/// Returns [`Error::Read`] when the directory cannot be read.
pub fn omocl_files(directory: &Path) -> Result<Vec<PathBuf>, Error> {
    crate::walk::files(directory, |path| {
        crate::walk::has_extension(path, &OMOCL_EXTENSIONS)
    })
    .map_err(read_error)
}

/// Checks the FHIRconnect files `fhirconnect` and the OMOCL files `omocl`,
/// each when given, and returns every diagnostic.
///
/// This is [`check`] over file lists, so a caller checks a set that is not a
/// directory on disk, such as the running set with one file replaced by a
/// draft.
///
/// # Errors
///
/// Returns what [`check`] returns.
pub async fn check_files(
    fhirconnect: Option<&[PathBuf]>,
    templates: Option<Templates<'_>>,
    omocl: Option<&[PathBuf]>,
) -> Result<Report, Error> {
    let mut report = Report::default();
    if let Some(files) = fhirconnect {
        let templates = templates.ok_or_else(|| Error::NoTemplateSource {
            directory: files
                .first()
                .and_then(|file| file.parent())
                .map(Path::to_path_buf)
                .unwrap_or_default(),
        })?;
        check_fhirconnect(files, templates, &mut report).await?;
    }
    if let Some(files) = omocl {
        report.counts.omocl_files = files.len();
        if let Err(mut diagnostics) =
            omocl::model::load::load_set(files, &omocl::model::semantic::FirstPartyConverters)
        {
            report.diagnostics.append(&mut diagnostics);
        }
        report.omocl_files = files.to_vec();
    }
    Ok(report)
}

/// Loads, resolves and compiles the FHIRconnect files `files` through the one
/// load path.
async fn check_fhirconnect(
    files: &[PathBuf],
    templates: Templates<'_>,
    report: &mut Report,
) -> Result<(), Error> {
    report.counts.fhirconnect_files = files.len();
    let set = match compile::read(files) {
        Ok(set) => set,
        Err(mut diagnostics) => {
            report.diagnostics.append(&mut diagnostics);
            return Ok(());
        }
    };
    report.counts.contexts = set.contexts().count();
    let mut resolved = match templates {
        Templates::Directory(directory) => {
            compile::directory_templates(directory).map_err(|error| match error {
                crate::mappings::Error::Read { path, source } => Error::Read { path, source },
                other => Error::Template {
                    template: String::new(),
                    source: Box::new(other),
                },
            })?
        }
        Templates::Cdr(client) => compile::cdr_templates(&set, client)
            .await
            .map_err(|source| Error::Template {
                template: match &source {
                    crate::mappings::Error::TemplateFetch { template, .. }
                    | crate::mappings::Error::TemplateRefused { template, .. } => template.clone(),
                    _ => String::new(),
                },
                source: Box::new(source),
            })?,
    };
    report.counts.templates = resolved.indexes.len();
    report.diagnostics.append(&mut resolved.diagnostics);
    let mut compiled = compile::compile(&set, &resolved.indexes, &resolved.refusals);
    report.counts.programs = compiled.programs.len();
    report.diagnostics.append(&mut compiled.diagnostics);
    report.programs = compiled.programs;
    report.templates = resolved.entries;
    report.indexes = resolved.indexes;
    report.set = Some(set);
    Ok(())
}

/// Returns the check error a refused walk describes.
fn read_error(crate::walk::Error { path, source }: crate::walk::Error) -> Error {
    Error::Read { path, source }
}
