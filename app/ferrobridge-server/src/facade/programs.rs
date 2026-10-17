// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The loaded mapping set: one compiled program per context, ready to run.
//!
//! A mapping directory is read once at boot: every FHIRconnect file is loaded
//! and validated, the operational template each context names is fetched from
//! the CDR, and the context is compiled against it into an immutable program
//! (no specification governs this: our own design). A mapping that does not
//! compile is a boot refusal, never a failure on the request that first
//! touches it.
//!
//! Selection is by the set of `meta.profile`. The engine chapter keys on
//! `meta.url`, which R4 does not define; the element is `meta.profile`, a list
//! (<https://hl7.org/fhir/R4/resource.html#Meta>), so matching is set
//! membership and every declared profile is considered.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use fhirconnect::model::load::MappingSet;
use fhirconnect::resolve::program::Program;
use fhirconnect::resolve::program::binding::TemplateId;
use fhirconnect::resolve::program::mapping::Mapping;
use fhirconnect::resolve::program::mapping::Method;
use fhirconnect::resolve::select::SelectError;
use fhirconnect::resolve::select::select_by_profile_pinned;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::diagnostic::Severity;
use openehr_mapping_core::index::WebTemplateIndex;

use crate::mappings::compile::Refusal;
use crate::mappings::compile::TemplateCause;

/// Why the mapping set did not load.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadError {
    /// The mapping directory could not be read.
    #[error("the mapping directory {} could not be read", path.display())]
    Directory {
        /// The path that was tried.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A mapping file is not valid FHIRconnect.
    #[error("{} mapping diagnostics, the first of which is: {first}", count)]
    Mappings {
        /// How many diagnostics the load raised.
        count: usize,
        /// The first diagnostic, rendered.
        first: String,
    },
    /// A context names a template with no identifier.
    #[error("the context {context} names no template id")]
    NoTemplateId {
        /// The context that names none.
        context: String,
    },
    /// No fetched template provides the one a context names.
    #[error("no fetched template provides {template}, which the context {context} maps")]
    MissingTemplate {
        /// The template that was asked for.
        template: String,
        /// The context that names it.
        context: String,
    },
    /// The CDR does not hold the template a context names.
    #[error(
        "the CDR holds no template {template}, which the context {context} maps: it answered {status}"
    )]
    UnknownTemplate {
        /// The template that was asked for.
        template: String,
        /// The first context, in name order, that names it.
        context: String,
        /// The status the CDR answered.
        status: http::StatusCode,
    },
    /// The CDR did not serve a template a context names as a Web Template.
    #[error("the template {template}, which the context {context} maps, did not load from the CDR")]
    Fetch {
        /// The template that was asked for.
        template: String,
        /// The first context, in name order, that names it.
        context: String,
        /// What the fetch reported, the upstream status included.
        #[source]
        source: Box<crate::mappings::Error>,
    },
}

/// One compiled context, with the template index it runs against.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The compiled program.
    program: Arc<Program>,
    /// The index of the template the program was compiled against.
    index: Arc<WebTemplateIndex>,
}

impl Loaded {
    /// Returns the compiled program.
    #[must_use]
    pub fn program(&self) -> &Arc<Program> {
        &self.program
    }

    /// Returns the index of the template the program runs against.
    #[must_use]
    pub fn index(&self) -> &Arc<WebTemplateIndex> {
        &self.index
    }
}

/// Every program this deployment loaded.
#[derive(Debug, Default)]
pub struct Programs {
    /// The programs, in context-name order.
    loaded: Vec<Loaded>,
    /// The programs alone, for the selection functions of `fhirconnect`.
    programs: Vec<Arc<Program>>,
}

impl Programs {
    /// Returns a registry over `loaded`, ordered by context name.
    #[must_use]
    pub fn new(mut loaded: Vec<Loaded>) -> Self {
        loaded.sort_by(|left, right| {
            left.program
                .context()
                .as_str()
                .cmp(right.program.context().as_str())
        });
        let programs = loaded
            .iter()
            .map(|entry| Arc::clone(&entry.program))
            .collect();
        Self { loaded, programs }
    }

    /// Returns how many programs were loaded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.loaded.len()
    }

    /// Whether no program was loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty()
    }

    /// Returns every loaded program, in context-name order.
    #[must_use]
    pub fn loaded(&self) -> &[Loaded] {
        &self.loaded
    }

    /// Returns the resource types at least one program maps, in order.
    ///
    /// The `CapabilityStatement` names exactly these, and a type outside the
    /// set answers `404` with `not-supported` on the wire (no specification
    /// governs this: our own design).
    #[must_use]
    pub fn resource_types(&self) -> BTreeSet<String> {
        self.loaded
            .iter()
            .map(|entry| String::from(entry.program.resource().as_str()))
            .collect()
    }

    /// Whether any program maps `resource_type`.
    #[must_use]
    pub fn supports(&self, resource_type: &str) -> bool {
        self.loaded
            .iter()
            .any(|entry| entry.program.resource().as_str() == resource_type)
    }

    /// Returns the resource types a `reference` mapping of some program
    /// creates and no program maps, in order.
    ///
    /// Going out of openEHR a `reference` mapping creates the resource it
    /// names (`types-of-mappings/concept-type/Reference.adoc`), and the facade
    /// serves it on its own, so these types are readable although no context
    /// maps them (no specification governs this: our own design).
    #[must_use]
    pub fn created_types(&self) -> BTreeSet<String> {
        let mut created = BTreeSet::new();
        for entry in &self.loaded {
            referenced(entry.program.mappings(), &mut created);
        }
        let mapped = self.resource_types();
        created.retain(|resource_type| !mapped.contains(resource_type));
        created
    }

    /// Whether `resource_type` is mapped by a program or created by one, so a
    /// read of it can answer.
    #[must_use]
    pub fn readable(&self, resource_type: &str) -> bool {
        self.supports(resource_type) || self.created_types().contains(resource_type)
    }

    /// Returns the program the claimed profiles select, pinned by `template`.
    ///
    /// # Errors
    ///
    /// Returns [`SelectError::NoMatch`] when no program claims one of the
    /// profiles, [`SelectError::Ambiguous`] when several do and `template`
    /// pins none, and [`SelectError::ProfileVersion`] when the only claimants
    /// pin another profile version.
    pub fn select(
        &self,
        claimed: &[String],
        template: Option<&TemplateId>,
    ) -> Result<&Loaded, SelectError> {
        let chosen = select_by_profile_pinned(&self.programs, claimed, template)?;
        self.loaded
            .iter()
            .find(|entry| Arc::ptr_eq(&entry.program, chosen))
            .ok_or_else(|| SelectError::NoMatch {
                wanted: format!("the profiles [{}]", claimed.join(", ")),
            })
    }

    /// Returns the program compiled from the context named `context`.
    #[must_use]
    pub fn by_context(&self, context: &str) -> Option<&Loaded> {
        self.loaded
            .iter()
            .find(|entry| entry.program.context().as_str() == context)
    }

    /// Returns every candidate context that claims one of `claimed`.
    ///
    /// An ambiguous selection reports these, so a caller learns which
    /// `templateId` would pin the choice.
    #[must_use]
    pub fn candidates(&self, claimed: &[String]) -> Vec<String> {
        self.loaded
            .iter()
            .filter(|entry| {
                claimed.iter().any(|entry_url| {
                    entry_url.split('|').next().unwrap_or(entry_url)
                        == entry.program.profile().url().as_str()
                })
            })
            .map(|entry| {
                format!(
                    "{} ({})",
                    entry.program.context(),
                    entry.program.template().id()
                )
            })
            .collect()
    }
}

/// Adds every resource type a `reference` mapping among `mappings` names to
/// `created`, through slots, nested references and `followedBy`.
fn referenced(mappings: &[Mapping], created: &mut BTreeSet<String>) {
    for mapping in mappings {
        match *mapping.method() {
            Method::Reference {
                ref resource,
                ref mappings,
            } => {
                created.insert(String::from(resource.as_str()));
                referenced(mappings, created);
            }
            Method::Slot { ref mappings, .. } => referenced(mappings, created),
            Method::Value
            | Method::Link { .. }
            | Method::Programmed { .. }
            | Method::Participation { .. } => {}
        }
        referenced(mapping.followed_by(), created);
    }
}

/// Returns the FHIRconnect files under `directory`, sorted.
///
/// # Errors
///
/// Returns [`LoadError::Directory`] when the directory cannot be listed.
pub fn mapping_files(directory: &Path) -> Result<Vec<PathBuf>, LoadError> {
    crate::walk::mapping_files(directory)
        .map_err(|crate::walk::Error { path, source }| LoadError::Directory { path, source })
}

/// Reads and validates the mapping set under `directory`.
///
/// # Errors
///
/// Returns [`LoadError::Directory`] when the directory cannot be listed and
/// [`LoadError::Mappings`] when a file is not valid FHIRconnect.
pub fn read_set(directory: &Path) -> Result<MappingSet, LoadError> {
    let files = mapping_files(directory)?;
    crate::mappings::compile::read(&files).map_err(|diagnostics| mappings_error(&diagnostics))
}

/// Compiles every context of `set` against the templates in `indexes`.
///
/// `indexes` is keyed by template identifier; a context whose template is
/// absent is a refusal, because a mapping cannot name a template the CDR has
/// not loaded (no specification governs this: our own design).
///
/// # Errors
///
/// Returns [`LoadError::NoTemplateId`] for a context naming no template,
/// [`LoadError::MissingTemplate`] for one whose template is absent, and
/// [`LoadError::Mappings`] for a context that does not compile.
pub fn compile_set(
    set: &MappingSet,
    indexes: &BTreeMap<String, Arc<WebTemplateIndex>>,
) -> Result<Programs, LoadError> {
    let compiled = crate::mappings::compile::compile(set, indexes, &[]);
    if let Some(refusal) = compiled.refusals.into_iter().next() {
        return Err(match refusal {
            Refusal::NoTemplateId { file, .. } => LoadError::NoTemplateId {
                context: file.display().to_string(),
            },
            Refusal::Template {
                context, template, ..
            } => LoadError::MissingTemplate { template, context },
            Refusal::Compile { .. } => mappings_error(
                &compiled
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.severity() == Severity::Error)
                    .cloned()
                    .collect::<Vec<Diagnostic>>(),
            ),
        });
    }
    Ok(from_compiled(compiled.programs, indexes))
}

/// Returns the registry of `programs`, each with the index of the template it
/// compiled against.
#[must_use]
pub fn from_compiled(
    programs: Vec<Arc<Program>>,
    indexes: &BTreeMap<String, Arc<WebTemplateIndex>>,
) -> Programs {
    let loaded = programs
        .into_iter()
        .filter_map(|program| {
            crate::mappings::log_warnings(&program);
            // NOTE: no specification governs this: our own design; the load
            // path compiled each program against an index of this map.
            let index = indexes
                .get(program.template().id().as_str())
                .map(Arc::clone)?;
            Some(Loaded { program, index })
        })
        .collect();
    Programs::new(loaded)
}

/// Returns the templates every context of `set` names, fetched from the CDR.
///
/// Each template is fetched once, through [`crate::mappings::template_index`],
/// and a refusal names the first context, in name order, that maps it.
///
/// # Errors
///
/// Returns [`LoadError::NoTemplateId`] for a context naming no template,
/// [`LoadError::UnknownTemplate`] with the upstream status when the CDR holds
/// no such template, and [`LoadError::Fetch`] for every other fetch that does
/// not yield a Web Template.
pub async fn fetch_templates(
    set: &MappingSet,
    client: &crate::cdr::CdrClient,
) -> Result<BTreeMap<String, Arc<WebTemplateIndex>>, LoadError> {
    if let Some(context) = set
        .contexts()
        .find(|context| crate::mappings::compile::template_of(context).is_none())
    {
        return Err(LoadError::NoTemplateId {
            context: context.file().display().to_string(),
        });
    }
    let templates = match crate::mappings::compile::cdr_templates(set, client).await {
        Ok(templates) => templates,
        Err(source) => {
            let template = match &source {
                crate::mappings::Error::TemplateFetch { template, .. }
                | crate::mappings::Error::TemplateRefused { template, .. } => template.clone(),
                _ => String::new(),
            };
            let context = set
                .contexts()
                .find(|context| {
                    crate::mappings::compile::template_of(context).as_deref()
                        == Some(template.as_str())
                })
                .map(|context| context.header().name().value().as_str().to_owned())
                .unwrap_or_default();
            return Err(LoadError::Fetch {
                template,
                context,
                source: Box::new(source),
            });
        }
    };
    if let Some(Refusal::Template {
        context,
        template,
        cause,
    }) = templates.refusals.into_iter().next()
    {
        return Err(match cause {
            TemplateCause::NotHeld(status) => LoadError::UnknownTemplate {
                template,
                context,
                status,
            },
            TemplateCause::Unusable(source) => LoadError::Fetch {
                template,
                context,
                source,
            },
            TemplateCause::NotProvided => LoadError::MissingTemplate { template, context },
        });
    }
    Ok(templates.indexes)
}

/// Returns the refusal a diagnostic list describes.
fn mappings_error(diagnostics: &[Diagnostic]) -> LoadError {
    let first = diagnostics.first().map_or_else(
        || String::from("no diagnostic was reported"),
        |diagnostic| {
            format!(
                "{} {}: {}",
                diagnostic.file().display(),
                diagnostic.code(),
                diagnostic.message()
            )
        },
    );
    LoadError::Mappings {
        count: diagnostics.len(),
        first,
    }
}

#[cfg(test)]
mod tests {
    use super::{Programs, mapping_files};

    #[test]
    fn an_empty_registry_supports_no_type() {
        let programs = Programs::default();
        assert!(programs.is_empty());
        assert_eq!(0, programs.len());
        assert!(!programs.supports("Condition"));
        assert!(programs.resource_types().is_empty());
    }

    #[test]
    fn only_yaml_files_are_read_and_the_order_is_stable() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        for name in ["b.yml", "a.yaml", "notes.txt", "c.YML"] {
            std::fs::write(directory.path().join(name), "x: 1\n").expect("a fixture file");
        }
        std::fs::create_dir_all(directory.path().join("nested")).expect("a nested directory");
        std::fs::write(directory.path().join("nested/d.yml"), "x: 1\n").expect("a fixture file");
        let found = mapping_files(directory.path()).expect("the directory lists");
        let names: Vec<String> = found
            .iter()
            .filter_map(|path| path.file_name().and_then(std::ffi::OsStr::to_str))
            .map(str::to_owned)
            .collect();
        assert_eq!(
            vec![
                String::from("a.yaml"),
                String::from("b.yml"),
                String::from("c.YML"),
                String::from("d.yml"),
            ],
            names
        );
    }

    #[test]
    fn a_directory_that_does_not_exist_is_a_refusal() {
        let error = mapping_files(std::path::Path::new("/ferrobridge/no/such/directory"))
            .expect_err("an absent directory is refused");
        assert!(format!("{error}").contains("could not be read"), "{error}");
    }
}
