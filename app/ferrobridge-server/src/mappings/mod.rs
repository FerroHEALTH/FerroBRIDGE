// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Loading the mapping set and the templates it compiles against.
//!
//! A FHIRconnect context is compiled once, at boot, against the operational
//! template it names, so a mapping that does not resolve refuses the start
//! rather than the request that first touches it. The templates come from
//! `[mappings] templates` when it names a directory, and otherwise from the
//! CDR, which serves each one at `GET /definition/template/adl1.4/{id}`
//! (openEHR ITS-REST 1.1.0 §Definition). [`compile`] is the one load path
//! the operations lane, the facade, the checker and the operator reload
//! share. No specification governs where the files live: our own design.

pub mod compile;

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use fhirconnect::operations::programs::ProgramSet;
use http::StatusCode;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::config::MappingSettings;
use crate::config::Settings;
use crate::mappings::compile::Refusal;
use crate::mappings::compile::TemplateCause;

/// Why a mapping set did not load.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A configured directory could not be read.
    #[error("cannot read {}", path.display())]
    Read {
        /// The directory that could not be read.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A directory holds none of the files the lane needs.
    #[error("{} holds no {what}", path.display())]
    Empty {
        /// The directory that was searched.
        path: PathBuf,
        /// What was looked for.
        what: &'static str,
    },
    /// The operations lane has a mapping directory and nowhere to take its
    /// templates from.
    #[error(
        "[mappings] directory {} is set with no [mappings] templates and no [cdr]: the FHIRconnect operations have no template to compile against",
        directory.display()
    )]
    NoTemplateSource {
        /// The mapping directory that was configured.
        directory: PathBuf,
    },
    /// A template the CDR served did not build a Web Template.
    #[error("the template `{template}` the CDR served did not load")]
    CdrTemplate {
        /// The template identifier that was fetched.
        template: String,
        /// Why it was refused.
        #[source]
        source: Box<openehr_mapping_core::template::PathError>,
    },
    /// A context names a template identifier the CDR route cannot carry.
    #[error("`{template}` is no template identifier the CDR route can carry")]
    TemplateName {
        /// The identifier a context wrote.
        template: String,
        /// What the identifier's constructor reported.
        #[source]
        source: Box<crate::cdr::ids::IdError>,
    },
    /// The template fetch did not reach a documented answer.
    #[error("the CDR did not serve the template `{template}`")]
    TemplateFetch {
        /// The template identifier that was asked for.
        template: String,
        /// What the client reported, the upstream status included.
        #[source]
        source: Box<crate::cdr::error::CdrError>,
    },
    /// The CDR holds no template with the identifier a context names.
    #[error("the CDR holds no template `{template}`: it answered {status}")]
    TemplateNotHeld {
        /// The template identifier that was asked for.
        template: String,
        /// The status the CDR answered.
        status: StatusCode,
    },
    /// The CDR refused the template fetch.
    #[error("the CDR refused the template `{template}`: it answered {upstream}")]
    TemplateRefused {
        /// The template identifier that was asked for.
        template: String,
        /// The upstream status and body.
        upstream: Box<crate::cdr::error::Upstream>,
    },
    /// The mapping files did not pass the validation layers.
    #[error("the mapping set under {} is not valid: {report}", path.display())]
    Mappings {
        /// The directory the files came from.
        path: PathBuf,
        /// One line per refusal.
        report: String,
    },
    /// A context mapping names no template identifier.
    #[error("the context `{context}` names no template id")]
    NoTemplateId {
        /// The context mapping that names none.
        context: String,
    },
    /// A context mapping did not compile into a program.
    #[error("the context `{context}` did not compile: {report}")]
    Compile {
        /// The context mapping that was refused.
        context: String,
        /// One line per refusal.
        report: String,
    },
    /// A context mapping names a template no loaded template provides.
    #[error("the context `{context}` maps `{template}`, which no loaded template provides")]
    UnknownTemplate {
        /// The context mapping that named it.
        context: String,
        /// The template it named.
        template: String,
    },
    /// A compiled program did not join the set.
    #[error("the context `{context}` did not join the compiled set")]
    Insert {
        /// The context mapping that was refused.
        context: String,
        /// Why it was refused.
        #[source]
        source: fhirconnect::operations::programs::ProgramSetError,
    },
    /// A template index was still shared when the lane took ownership of it.
    #[error("the template `{template}` is still shared and cannot join the compiled set")]
    SharedTemplate {
        /// The template identifier.
        template: String,
    },
}

/// Loads the mapping set the operations lane of `settings` runs, if any.
///
/// The lane is off when `[operations] enabled` is false or no
/// `[mappings] directory` is set. Otherwise the templates come from
/// `[mappings] templates` when it is set, and from the CDR `cdr` reaches when
/// it is not.
///
/// # Errors
///
/// Returns [`Error::NoTemplateSource`] when a mapping directory is set with
/// neither a template directory nor a CDR, and the errors of [`load`] and
/// [`load_from_cdr`] otherwise.
pub async fn load_configured(
    settings: &Settings,
    cdr: Option<&crate::cdr::CdrClient>,
) -> Result<Option<ProgramSet>, Error> {
    if !settings.operations.enabled {
        return Ok(None);
    }
    // NOTE: no specification governs this: our own design; a template
    // directory is the operator's explicit choice, so it wins over the CDR.
    if let Some(ref mappings) = settings.mappings {
        return load(mappings).map(Some);
    }
    match (settings.mapping_directory.as_deref(), cdr) {
        (Some(directory), Some(client)) => load_from_cdr(directory, client).await.map(Some),
        (Some(directory), None) => Err(Error::NoTemplateSource {
            directory: directory.to_path_buf(),
        }),
        (None, _) => Ok(None),
    }
}

/// Loads the mapping set `settings` names, compiled and ready to run.
///
/// Every `.opt` file under the template directory is read, and each context
/// compiles against the one it names.
///
/// # Errors
///
/// Returns [`Error`] when a directory cannot be read or holds nothing, when a
/// template does not build, when the mapping files do not validate, and when a
/// context does not compile against its template.
pub fn load(settings: &MappingSettings) -> Result<ProgramSet, Error> {
    let files = mapping_files(&settings.directory)?;
    let set = read(&settings.directory, &files)?;
    let templates = compile::directory_templates(&settings.templates)?;
    if !templates.diagnostics.is_empty() {
        return Err(Error::Mappings {
            path: settings.templates.clone(),
            report: compile::render(&templates.diagnostics),
        });
    }
    if templates.indexes.is_empty() {
        return Err(Error::Empty {
            path: settings.templates.clone(),
            what: "operational template",
        });
    }
    program_set(&set, templates.indexes, &templates.refusals)
}

/// Loads the mapping set under `directory`, compiled against the templates
/// the CDR `client` reaches serves.
///
/// The context files are read first, and every template identifier they name
/// is fetched once, in order, before any context compiles. A template the CDR
/// does not serve refuses the whole set, so the lane never starts half
/// loaded.
///
/// # Errors
///
/// Returns [`Error::Read`], [`Error::Empty`] and [`Error::Mappings`] for the
/// mapping files, [`Error::NoTemplateId`] for a context naming no template,
/// [`Error::TemplateName`], [`Error::TemplateFetch`],
/// [`Error::TemplateNotHeld`], [`Error::TemplateRefused`] and
/// [`Error::CdrTemplate`] for a template the CDR
/// does not serve as a Web Template, and the compile errors of [`load`].
pub async fn load_from_cdr(
    directory: &Path,
    client: &crate::cdr::CdrClient,
) -> Result<ProgramSet, Error> {
    let files = mapping_files(directory)?;
    let set = read(directory, &files)?;
    let templates = compile::cdr_templates(&set, client).await?;
    if let Some(refusal) = templates.refusals.into_iter().next() {
        return Err(refused(refusal, ""));
    }
    program_set(&set, templates.indexes, &[])
}

/// Returns the FHIRconnect files under `directory`, refusing an empty tree.
fn mapping_files(directory: &Path) -> Result<Vec<PathBuf>, Error> {
    let files = crate::walk::mapping_files(directory)
        .map_err(|crate::walk::Error { path, source }| Error::Read { path, source })?;
    if files.is_empty() {
        return Err(Error::Empty {
            path: directory.to_path_buf(),
            what: "mapping file",
        });
    }
    Ok(files)
}

/// Reads and validates `files`, the mapping files under `directory`.
fn read(
    directory: &Path,
    files: &[PathBuf],
) -> Result<fhirconnect::model::load::MappingSet, Error> {
    compile::read(files).map_err(|diagnostics| Error::Mappings {
        path: directory.to_path_buf(),
        report: compile::render(&diagnostics),
    })
}

/// Compiles every context of `set` into the operations lane's program set.
fn program_set(
    set: &fhirconnect::model::load::MappingSet,
    indexes: std::collections::BTreeMap<String, Arc<WebTemplateIndex>>,
    resolved: &[Refusal],
) -> Result<ProgramSet, Error> {
    let compiled = compile::compile(set, &indexes, resolved);
    let report = compile::render(&compiled.diagnostics);
    if let Some(refusal) = compiled.refusals.into_iter().next() {
        return Err(refused(refusal, &report));
    }
    let mut programs = ProgramSet::new();
    for (template, index) in indexes {
        // NOTE: no specification governs this: our own design; a program
        // holds no index, so the load path's handle is the only one left.
        let index = Arc::try_unwrap(index).map_err(|_shared| Error::SharedTemplate {
            template: template.clone(),
        })?;
        programs.insert_template(index);
    }
    for program in compiled.programs {
        log_warnings(&program);
        let context = program.context().as_str().to_owned();
        programs
            .insert_program(program)
            .map_err(|source| Error::Insert { context, source })?;
    }
    Ok(programs)
}

/// Returns the lane error the first refusal of a load describes, `report`
/// carrying the compile diagnostics.
fn refused(refusal: Refusal, report: &str) -> Error {
    match refusal {
        Refusal::NoTemplateId { context, .. } => Error::NoTemplateId { context },
        Refusal::Compile { context } => Error::Compile {
            context,
            report: report.to_owned(),
        },
        Refusal::Template {
            context,
            template,
            cause,
        } => match cause {
            TemplateCause::NotProvided => Error::UnknownTemplate { context, template },
            TemplateCause::NotHeld(status) => Error::TemplateNotHeld { template, status },
            TemplateCause::Unusable(error) => *error,
        },
    }
}
/// Fetches the template `template` names from the CDR and builds its Web
/// Template index.
///
/// This is the one CDR template fetch at boot, which the operations lane and
/// the facade both call.
///
/// # Errors
///
/// Returns [`Error::TemplateName`] for an identifier the route cannot carry,
/// [`Error::TemplateNotHeld`] with the status when the CDR holds no such
/// template, [`Error::TemplateRefused`] with the upstream answer for a `400`,
/// [`Error::TemplateFetch`] when the call reaches no documented answer, and
/// [`Error::CdrTemplate`] when the template does not build a Web Template.
pub async fn template_index(
    client: &crate::cdr::CdrClient,
    template: &str,
) -> Result<WebTemplateIndex, Error> {
    let source = fetch(client, template).await?;
    WebTemplateIndex::build(&source).map_err(|error| Error::CdrTemplate {
        template: template.to_owned(),
        source: Box::new(error),
    })
}

/// Fetches the template `template` names from the CDR.
async fn fetch(client: &crate::cdr::CdrClient, template: &str) -> Result<TemplateSource, Error> {
    let id = crate::cdr::ids::template_id(template).map_err(|source| Error::TemplateName {
        template: template.to_owned(),
        source: Box::new(source),
    })?;
    let outcome = client
        .template(&id)
        .await
        .map_err(|source| Error::TemplateFetch {
            template: template.to_owned(),
            source: Box::new(source),
        })?;
    match outcome {
        crate::cdr::template::TemplateOutcome::Found(
            crate::cdr::template::TemplateSource::Opt14(opt),
        ) => Ok(TemplateSource::Opt14(opt)),
        crate::cdr::template::TemplateOutcome::Found(
            crate::cdr::template::TemplateSource::Opt2 {
                template: opt,
                resolved_id,
            },
        ) => Ok(TemplateSource::Opt2 {
            template: opt,
            resolved_id: resolved_id.physical_id(),
        }),
        // NOTE: the client reports `UnknownTemplate` only for a `404` from both
        // definition routes (`definition-codegen.openapi.yaml`, ITS-REST 1.1.0).
        crate::cdr::template::TemplateOutcome::UnknownTemplate => Err(Error::TemplateNotHeld {
            template: template.to_owned(),
            status: StatusCode::NOT_FOUND,
        }),
        crate::cdr::template::TemplateOutcome::BadRequest(upstream) => {
            Err(Error::TemplateRefused {
                template: template.to_owned(),
                upstream: Box::new(upstream),
            })
        }
    }
}

/// Logs every warning a compiled program carries, once, as it loads.
///
/// A warning is a disagreement the compiler accepted, so the program runs and
/// the operator learns of it here. The record names the context, the file,
/// the line and the diagnostic code, and never the content of a mapping.
pub fn log_warnings(program: &fhirconnect::resolve::program::Program) {
    for warning in program.warnings() {
        tracing::warn!(
            context = %program.context(),
            file = %warning.file().display(),
            line = warning
                .position()
                .map_or(0, openehr_mapping_core::position::Position::line),
            code = %warning.code(),
            "a mapping context compiled with a warning"
        );
    }
}
