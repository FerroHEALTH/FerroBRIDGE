// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The running load as the operator API reports it: the compiled programs,
//! the files they came from, the templates and every diagnostic.
//!
//! A path in a document is a wire path (`fhirconnect/...`, `omocl/...`) when
//! the file is under a mapping root, and the file name alone otherwise, so a
//! document never names a host directory. The documents are the
//! `ferrobridge_operator_api::operator` types the operator console reads. No
//! specification governs the operator surface: our own design.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use ferrobridge_operator_api::operator::{
    EngineState, Extension, Listed, Model, Omocl, Program, Template,
};
use openehr_mapping_core::diagnostic::Severity;

use crate::check::Report;
use crate::mappings::compile::TemplateOrigin;
use crate::operator::files;
use crate::operator::files::Language;
use crate::operator::files::Owner;
use crate::operator::files::Root;

/// The running load: the boot check's report and the search declaration
/// each context answers searches with.
#[derive(Debug)]
pub struct Snapshot {
    /// What the check loaded and raised.
    report: Report,
    /// The search declaration file of each context, by context name.
    searches: BTreeMap<String, PathBuf>,
}

impl Snapshot {
    /// Returns the snapshot of `report`, with `searches` naming each
    /// context's search declaration.
    #[must_use]
    pub const fn new(report: Report, searches: BTreeMap<String, PathBuf>) -> Self {
        Self { report, searches }
    }

    /// Returns the check report of the running load.
    #[must_use]
    pub const fn report(&self) -> &Report {
        &self.report
    }

    /// Returns the `GET /operator/state` document over `roots`.
    #[must_use]
    pub fn document(&self, roots: &[Root]) -> EngineState {
        let shown = |path: &Path| shown(roots, path);
        let set = self.report.fhirconnect_set();
        let file_of = |name: &openehr_mapping_core::header::metadata::MappingName| {
            set.and_then(|set| set.file_of(name)).map(&shown)
        };
        let contexts = self
            .report
            .programs()
            .iter()
            .map(|program| Program {
                context: program.context().as_str().to_owned(),
                file: file_of(program.context()),
                template: program.template().id().as_str().to_owned(),
                profile: program.profile().url().as_str().to_owned(),
                profile_version: program.profile().version().to_string(),
                resource_type: program.resource().as_str().to_owned(),
                models: program
                    .models()
                    .iter()
                    .map(|model| Model {
                        name: model.name().as_str().to_owned(),
                        file: file_of(model.name()),
                        extensions: model
                            .extensions()
                            .iter()
                            .map(|name| Extension {
                                name: name.as_str().to_owned(),
                                file: file_of(name),
                            })
                            .collect(),
                    })
                    .collect(),
                search: self
                    .searches
                    .get(program.context().as_str())
                    .map(|path| shown(path)),
            })
            .collect();
        let templates = self
            .report
            .templates()
            .iter()
            .map(|template| match template.origin() {
                TemplateOrigin::File(path) => Template::Directory {
                    id: template.id().to_owned(),
                    file: shown(path),
                },
                TemplateOrigin::Cdr => Template::Cdr {
                    id: template.id().to_owned(),
                },
            })
            .collect();
        let omocl_files = self
            .report
            .omocl_files()
            .iter()
            .map(|path| shown(path))
            .collect();
        let omocl_valid = !self.report.diagnostics().iter().any(|diagnostic| {
            diagnostic.severity() == Severity::Error
                && self
                    .report
                    .omocl_files()
                    .iter()
                    .any(|file| file == diagnostic.file())
        });
        EngineState {
            contexts,
            templates,
            omocl: Omocl {
                files: omocl_files,
                valid: omocl_valid,
            },
            counts: self.report.counts(),
            diagnostics: self
                .report
                .diagnostics()
                .iter()
                .map(|diagnostic| {
                    crate::check::shown_diagnostic(diagnostic, &shown(diagnostic.file()))
                })
                .collect(),
        }
    }

    /// Returns every mapping file under `roots` as `GET /operator/mappings`
    /// lists it.
    ///
    /// # Errors
    ///
    /// Returns [`crate::walk::Error`] when a directory or a file cannot be
    /// read.
    pub fn listing(&self, roots: &[Root]) -> Result<Vec<Listed>, crate::walk::Error> {
        let mut listed = Vec::new();
        for located in files::list(roots)? {
            let bytes = std::fs::read(located.path()).map_err(|source| crate::walk::Error {
                path: located.path().to_path_buf(),
                source,
            })?;
            let diagnostics = self
                .report
                .diagnostics()
                .iter()
                .filter(|diagnostic| diagnostic.file() == located.path())
                .map(|diagnostic| crate::check::shown_diagnostic(diagnostic, located.wire()))
                .collect();
            listed.push(Listed {
                path: located.wire().to_owned(),
                language: located.language().into(),
                kind: self.kind(&located).map(str::to_owned),
                sha256: files::sha256(&bytes),
                owner: located.owner().into(),
                read_only: located.owner() == Owner::Provisioned,
                diagnostics,
            });
        }
        Ok(listed)
    }

    /// Returns the kind the running load read `located` as.
    fn kind(&self, located: &files::Located) -> Option<&'static str> {
        let path = located.path();
        match located.language() {
            Language::Omocl => self
                .report
                .omocl_files()
                .iter()
                .any(|file| file == path)
                .then_some("mapping"),
            Language::Fhirconnect => {
                if crate::facade::search::declaration::is_declaration(path) {
                    return Some("search");
                }
                let set = self.report.fhirconnect_set()?;
                if set.contexts().any(|context| context.file() == path) {
                    return Some("context");
                }
                set.models()
                    .find(|model| model.file() == path)
                    .and_then(|model| model.header().mapping_type())
                    .map(|kind| kind.value().as_str())
            }
        }
    }
}

/// Returns how a document names `path`: its wire path under `roots`, or its
/// file name.
#[must_use]
pub fn shown(roots: &[Root], path: &Path) -> String {
    files::wire_path_in(roots, path).unwrap_or_else(|| {
        path.file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
            .to_owned()
    })
}
