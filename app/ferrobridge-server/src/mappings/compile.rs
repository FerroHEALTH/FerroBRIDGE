// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one load path of a FHIRconnect set: read the files, resolve the
//! templates the contexts name, compile every context.
//!
//! Each step reports everything it found, every diagnostic and every refusal
//! with its typed cause, so the checker reports all of it and a lane refuses
//! on the first refusal with the error it has always answered. The operations
//! lane, the facade, the checker and the operator reload all load through
//! here. No specification governs the load path: our own design.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use fhirconnect::engine::library::Library;
use fhirconnect::model::ast::keyword::ContextScope;
use fhirconnect::model::load::MappingSet;
use fhirconnect::resolve::program::Program;
use http::StatusCode;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::diagnostic::DiagnosticCode;
use openehr_mapping_core::diagnostic::LanguageCode;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use crate::mappings::Error;

/// The extension an operational template file carries.
const TEMPLATE_EXTENSION: &str = "opt";

/// The code of a context whose template no template source provides.
pub const UNKNOWN_TEMPLATE: &str = "fb-unknown-template";

/// The code of a context that names no template identifier.
pub const NO_TEMPLATE_ID: &str = "fb-no-template-id";

/// The code of a search declaration that does not bind to the set.
pub const SEARCH_DECLARATION: &str = "fb-search-declaration";

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

/// Why a template a context names is missing from the compiled set.
#[derive(Debug)]
pub enum TemplateCause {
    /// No template source provides it: no `.opt` file of the directory
    /// carries the identifier.
    NotProvided,
    /// The CDR answered the fetch with this status and no template.
    NotHeld(StatusCode),
    /// The fetch reached an answer that is no Web Template, or the identifier
    /// is none the CDR route can carry.
    Unusable(Box<Error>),
}

/// One context a load refused, with its typed cause.
#[derive(Debug)]
pub enum Refusal {
    /// The context names no template identifier.
    NoTemplateId {
        /// The context's `metadata.name`.
        context: String,
        /// The context file.
        file: PathBuf,
    },
    /// The template the context names is missing.
    Template {
        /// The context's `metadata.name`.
        context: String,
        /// The template identifier it names.
        template: String,
        /// Why it is missing.
        cause: TemplateCause,
    },
    /// The context did not compile; the diagnostics say why.
    Compile {
        /// The context's `metadata.name`.
        context: String,
    },
}

/// The templates one source provided.
#[derive(Debug, Default)]
pub struct Templates {
    /// The Web Template indices, by template identifier.
    pub indexes: BTreeMap<String, Arc<WebTemplateIndex>>,
    /// Where each came from, in the order they were read.
    pub entries: Vec<TemplateEntry>,
    /// The templates a context names and the source did not provide.
    pub refusals: Vec<Refusal>,
    /// A diagnostic per template that did not build or was not provided.
    pub diagnostics: Vec<Diagnostic>,
}

/// The programs one set compiled into.
#[derive(Debug, Default)]
pub struct Compiled {
    /// The programs, in context-name order.
    pub programs: Vec<Arc<Program>>,
    /// The contexts that did not compile.
    pub refusals: Vec<Refusal>,
    /// Every refusal and every warning, in context-name order.
    pub diagnostics: Vec<Diagnostic>,
}

impl Compiled {
    /// Whether no context was refused.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.refusals.is_empty()
    }
}

/// Loads and validates the FHIRconnect files `files` names as one set.
///
/// # Errors
///
/// Returns every diagnostic of the loader, the schemas and the semantic
/// rules when the set does not load.
pub fn read(files: &[PathBuf]) -> Result<MappingSet, Vec<Diagnostic>> {
    fhirconnect::model::load::load_set(files, &Library)
}

/// Builds the index of every `.opt` file under `directory`.
///
/// A file that does not build is a `template-build` diagnostic on that file,
/// and the other files still serve.
///
/// # Errors
///
/// Returns [`Error::Read`] when the directory or a file cannot be read.
pub fn directory_templates(directory: &Path) -> Result<Templates, Error> {
    let files = crate::walk::files(directory, |path| {
        crate::walk::has_extension(path, &[TEMPLATE_EXTENSION])
    })
    .map_err(|crate::walk::Error { path, source }| Error::Read { path, source })?;
    let mut templates = Templates::default();
    for path in files {
        let xml = std::fs::read_to_string(&path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?;
        match TemplateSource::opt14(&xml).and_then(|source| WebTemplateIndex::build(&source)) {
            Ok(index) => {
                templates.entries.push(TemplateEntry {
                    id: index.template_id().to_owned(),
                    origin: TemplateOrigin::File(path.clone()),
                });
                templates
                    .indexes
                    .insert(index.template_id().to_owned(), Arc::new(index));
            }
            Err(error) => templates.diagnostics.push(Diagnostic::error(
                &path,
                DiagnosticCode::TemplateBuild,
                error.to_string(),
            )),
        }
    }
    Ok(templates)
}

/// Fetches every template a context of `set` names from the CDR `client`
/// reaches, once each, in identifier order.
///
/// A template the CDR does not hold, one that does not build and an
/// identifier the route cannot carry are refusals against the first
/// context, in name order, that names it.
///
/// # Errors
///
/// Returns the fetch's [`Error`] when the CDR answers with neither a
/// template nor a `404`, so an upstream failure never reads as a missing
/// template.
pub async fn cdr_templates(
    set: &MappingSet,
    client: &crate::cdr::CdrClient,
) -> Result<Templates, Error> {
    let mut wanted: BTreeMap<String, (String, PathBuf)> = BTreeMap::new();
    for context in roots(set) {
        if let Some(template) = template_of(context) {
            wanted.entry(template).or_insert_with(|| {
                (
                    context.header().name().value().as_str().to_owned(),
                    context.file().to_path_buf(),
                )
            });
        }
    }
    let mut templates = Templates::default();
    for (template, (context, file)) in wanted {
        let cause = match crate::mappings::template_index(client, &template).await {
            Ok(index) => {
                templates.entries.push(TemplateEntry {
                    id: template.clone(),
                    origin: TemplateOrigin::Cdr,
                });
                templates.indexes.insert(template, Arc::new(index));
                continue;
            }
            Err(Error::TemplateNotHeld { status, .. }) => TemplateCause::NotHeld(status),
            Err(error @ (Error::TemplateName { .. } | Error::CdrTemplate { .. })) => {
                TemplateCause::Unusable(Box::new(error))
            }
            Err(error) => return Err(error),
        };
        let (code, message) = match &cause {
            TemplateCause::NotHeld(status) => (
                language(UNKNOWN_TEMPLATE),
                format!("the CDR holds no template `{template}`: it answered {status}"),
            ),
            TemplateCause::Unusable(error) if matches!(**error, Error::CdrTemplate { .. }) => (
                DiagnosticCode::TemplateBuild,
                crate::facade::outcome::chain(error.as_ref()),
            ),
            TemplateCause::Unusable(error) => (
                language(UNKNOWN_TEMPLATE),
                crate::facade::outcome::chain(error.as_ref()),
            ),
            TemplateCause::NotProvided => (language(UNKNOWN_TEMPLATE), String::new()),
        };
        templates
            .diagnostics
            .push(Diagnostic::error(&file, code, message));
        templates.refusals.push(Refusal::Template {
            context,
            template,
            cause,
        });
    }
    Ok(templates)
}

/// Compiles every context of `set` against the template it names in
/// `indexes`.
///
/// A context whose template `skip` names was already refused where the
/// template was resolved, so it is passed over without a second refusal.
#[must_use]
pub fn compile(
    set: &MappingSet,
    indexes: &BTreeMap<String, Arc<WebTemplateIndex>>,
    skip: &[Refusal],
) -> Compiled {
    let mut compiled = Compiled::default();
    for context in roots(set) {
        let file = context.file();
        let name = context.header().name().value();
        let Some(template) = template_of(context) else {
            compiled.diagnostics.push(
                Diagnostic::error(
                    file,
                    language(NO_TEMPLATE_ID),
                    "the context names no template id",
                )
                .with_mapping_name(name.clone()),
            );
            compiled.refusals.push(Refusal::NoTemplateId {
                context: name.as_str().to_owned(),
                file: file.to_path_buf(),
            });
            continue;
        };
        let Some(index) = indexes.get(&template) else {
            let reported = skip.iter().any(|refusal| {
                matches!(refusal, Refusal::Template { template: missing, .. } if *missing == template)
            });
            if !reported {
                compiled.diagnostics.push(
                    Diagnostic::error(
                        file,
                        language(UNKNOWN_TEMPLATE),
                        format!("the context maps `{template}`, which no loaded template provides"),
                    )
                    .with_mapping_name(name.clone()),
                );
                compiled.refusals.push(Refusal::Template {
                    context: name.as_str().to_owned(),
                    template,
                    cause: TemplateCause::NotProvided,
                });
            }
            continue;
        };
        match fhirconnect::resolve::compile::compile(
            set,
            name,
            index,
            &fhir_types::r4::schema::SCHEMAS,
            &Library,
        ) {
            Ok(program) => {
                compiled
                    .diagnostics
                    .extend(program.warnings().iter().cloned());
                compiled.programs.push(program);
            }
            Err(mut diagnostics) => {
                compiled.diagnostics.append(&mut diagnostics);
                compiled.refusals.push(Refusal::Compile {
                    context: name.as_str().to_owned(),
                });
            }
        }
    }
    compiled
}

/// Returns the root contexts of `set`: every context but a `scope: sub` one.
///
/// A sub-context "is never a root" and maps into the template of the context
/// that enters it
/// (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mapping-files/sub-contexts.adoc`,
/// §`scope` and §Rules), so it is compiled inside that context's program and
/// its own template is never fetched.
pub fn roots(
    set: &MappingSet,
) -> impl Iterator<Item = &fhirconnect::model::ast::ContextMappingFile> {
    set.contexts()
        .filter(|context| context.context().scope() == ContextScope::Root)
}

/// Returns the template identifier one context names, if it names one.
#[must_use]
pub fn template_of(context: &fhirconnect::model::ast::ContextMappingFile) -> Option<String> {
    context
        .context()
        .template
        .id
        .as_ref()
        .map(|located| located.value().clone())
}

/// Returns the load path's own diagnostic code `code`.
#[expect(
    clippy::expect_used,
    reason = "every caller passes one of this module's constants, each lowercase letters and hyphens"
)]
#[must_use]
fn language(code: &str) -> DiagnosticCode {
    DiagnosticCode::Language(
        LanguageCode::new(code).expect("the load path's codes should be in the code alphabet"),
    )
}

/// Returns the error diagnostic of a search declaration `file` that does not
/// bind to the set, `message` saying why.
#[must_use]
pub fn search_declaration(file: &Path, message: String) -> Diagnostic {
    Diagnostic::error(file, language(SEARCH_DECLARATION), message)
}

/// Renders the error diagnostics of `diagnostics` as one line.
#[must_use]
pub fn render(diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.severity() == openehr_mapping_core::diagnostic::Severity::Error
        })
        .map(|diagnostic| {
            format!(
                "{} {}: {}",
                diagnostic.file().display(),
                diagnostic.code(),
                diagnostic.message()
            )
        })
        .collect::<Vec<String>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::{NO_TEMPLATE_ID, SEARCH_DECLARATION, UNKNOWN_TEMPLATE, language};

    #[test]
    fn the_load_path_codes_are_in_the_code_alphabet() {
        for code in [NO_TEMPLATE_ID, SEARCH_DECLARATION, UNKNOWN_TEMPLATE] {
            assert_eq!(code, language(code).as_str());
        }
    }
}
