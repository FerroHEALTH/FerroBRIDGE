// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Checking and compiling the whole mapping set, then swapping it in.
//!
//! A reload compiles the provisioned files and the console's files together,
//! once for the check and once more for each lane that takes its templates
//! from another source, and only a set that compiled for every lane replaces
//! the running one: the facade's programs and search declarations, the
//! operations lane's programs and the operator snapshot, each in one pointer
//! swap. A request takes its set at entry, so a request in flight finishes on
//! the old set. No specification governs the reload: our own design.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use fhirconnect::operations::programs::ProgramSet;

use crate::check::Report;
use crate::check::Templates;
use crate::facade::Facade;
use crate::facade::programs::Programs;
use crate::facade::search::Catalog;
use crate::operator::files;
use crate::operator::files::Language;
use crate::operator::files::Root;
use crate::operator::snapshot::Snapshot;
use crate::state::OperationsLane;

/// Why a set did not replace the running one.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReloadError {
    /// The set raised an error diagnostic; nothing was swapped.
    #[error("the mapping set does not compile")]
    Rejected(Box<Report>),
    /// The check could not run: a directory or the CDR did not answer.
    #[error("the mapping set could not be checked")]
    Check(#[source] crate::check::Error),
    /// The facade's own load refused the set.
    #[error("the FHIR facade did not load the mapping set")]
    Facade(#[source] crate::facade::programs::LoadError),
    /// The facade's search declarations do not bind to the new programs.
    #[error("the search declarations do not load against the mapping set")]
    Search(#[source] crate::facade::search::declaration::DeclarationError),
    /// The operations lane's own load refused the set.
    #[error("the FHIRconnect operations did not load the mapping set")]
    Operations(#[source] crate::mappings::Error),
    /// The mapping directories could not be listed.
    #[error("a mapping directory could not be read")]
    Read(#[source] crate::walk::Error),
}

/// A draft standing in for one file of the set: the wire path it replaces
/// and the file on disk holding its bytes.
#[derive(Debug, Clone)]
pub struct Draft {
    /// The wire path the draft stands for.
    pub wire: String,
    /// The file holding the draft.
    pub path: PathBuf,
}

/// Everything a reload compiles against.
#[derive(Debug, Clone, Default)]
pub struct Reloader {
    /// The provisioned FHIRconnect directory, which holds the search
    /// declarations.
    pub fhirconnect: Option<PathBuf>,
    /// The template directory, when one is configured.
    pub templates: Option<PathBuf>,
    /// The CDR the templates are fetched from otherwise.
    pub cdr: Option<crate::cdr::CdrClient>,
    /// Whether OMOCL files are configured.
    pub omocl: bool,
    /// The facade, when it runs.
    pub facade: Option<Arc<Facade>>,
    /// The operations lane, when it runs.
    pub operations: Option<Arc<OperationsLane>>,
}

/// A set that compiled for every lane, ready to swap in.
#[derive(Debug)]
pub struct Prepared {
    /// The operator snapshot of the set.
    snapshot: Snapshot,
    /// The facade's programs and search declarations.
    facade: Option<(Programs, Catalog)>,
    /// The operations lane's programs.
    operations: Option<ProgramSet>,
}

impl Prepared {
    /// Returns the check report of the set.
    #[must_use]
    pub const fn report(&self) -> &Report {
        self.snapshot.report()
    }
}

impl Reloader {
    /// Returns the FHIRconnect and OMOCL files of `roots`, with `draft`
    /// standing in for the file at its wire path.
    ///
    /// # Errors
    ///
    /// Returns [`ReloadError::Read`] when a directory cannot be listed.
    pub fn sources(
        roots: &[Root],
        draft: Option<&Draft>,
    ) -> Result<(Vec<PathBuf>, Vec<PathBuf>), ReloadError> {
        let mut fhirconnect = Vec::new();
        let mut omocl = Vec::new();
        for located in files::list(roots).map_err(ReloadError::Read)? {
            if draft.is_some_and(|draft| draft.wire == located.wire()) {
                continue;
            }
            match located.language() {
                Language::Fhirconnect
                    if !crate::facade::search::declaration::is_declaration(located.path()) =>
                {
                    fhirconnect.push(located.path().to_path_buf());
                }
                Language::Fhirconnect => {}
                Language::Omocl => omocl.push(located.path().to_path_buf()),
            }
        }
        if let Some(draft) = draft {
            match files::relative(&draft.wire) {
                Ok(("omocl", _)) => omocl.push(draft.path.clone()),
                _ => fhirconnect.push(draft.path.clone()),
            }
        }
        Ok((fhirconnect, omocl))
    }

    /// Checks and compiles `fhirconnect` and `omocl` for every lane.
    ///
    /// # Errors
    ///
    /// Returns [`ReloadError::Rejected`] with the report when a diagnostic is
    /// an error, and the other variants when a lane's own load refuses the set
    /// or an upstream does not answer.
    pub async fn prepare(
        &self,
        fhirconnect: &[PathBuf],
        omocl: &[PathBuf],
    ) -> Result<Prepared, ReloadError> {
        let templates = match (self.templates.as_deref(), self.cdr.as_ref()) {
            (Some(directory), _) => Some(Templates::Directory(directory)),
            (None, Some(client)) => Some(Templates::Cdr(client)),
            (None, None) => None,
        };
        let report = crate::check::check_files(
            self.fhirconnect.is_some().then_some(fhirconnect),
            templates,
            self.omocl.then_some(omocl),
        )
        .await
        .map_err(ReloadError::Check)?;
        if !report.is_clean() {
            return Err(ReloadError::Rejected(Box::new(report)));
        }
        let facade = match (self.facade.as_ref(), report.fhirconnect_set()) {
            (Some(facade), Some(set)) => {
                let programs = if self.templates.is_none() {
                    crate::facade::programs::from_compiled(
                        report.programs().to_vec(),
                        report.indexes(),
                    )
                } else {
                    let indexes = crate::facade::programs::fetch_templates(set, facade.client())
                        .await
                        .map_err(ReloadError::Facade)?;
                    crate::facade::programs::compile_set(set, &indexes)
                        .map_err(ReloadError::Facade)?
                };
                let catalog = match self.fhirconnect.as_deref() {
                    Some(directory) => crate::facade::search::load(directory, &programs)
                        .map_err(ReloadError::Search)?,
                    None => Catalog::default(),
                };
                Some((programs, catalog))
            }
            _ => None,
        };
        let operations = match (self.operations.as_ref(), self.fhirconnect.as_deref()) {
            (Some(_lane), Some(directory)) => Some(
                match (self.templates.as_deref(), self.cdr.as_ref()) {
                    (Some(templates), _) => {
                        crate::mappings::load_files(directory, fhirconnect, templates)
                    }
                    (None, Some(client)) => {
                        crate::mappings::load_files_from_cdr(directory, fhirconnect, client).await
                    }
                    (None, None) => Err(crate::mappings::Error::NoTemplateSource {
                        directory: directory.to_path_buf(),
                    }),
                }
                .map_err(ReloadError::Operations)?,
            ),
            _ => None,
        };
        let searches = facade
            .as_ref()
            .map_or_else(BTreeMap::new, |(programs, catalog)| {
                searches(programs, catalog)
            });
        Ok(Prepared {
            snapshot: Snapshot::new(report, searches),
            facade,
            operations,
        })
    }

    /// Swaps `prepared` in: the facade, the operations lane, then the
    /// snapshot this returns.
    #[must_use]
    pub fn apply(&self, prepared: Prepared) -> Snapshot {
        if let (Some(facade), Some((programs, catalog))) = (self.facade.as_ref(), prepared.facade) {
            facade.replace(programs, catalog);
        }
        if let (Some(lane), Some(programs)) = (self.operations.as_ref(), prepared.operations) {
            lane.replace(programs);
        }
        prepared.snapshot
    }
}

/// Returns the search declaration file of each context `catalog` declares
/// for a type `programs` maps.
#[must_use]
pub fn searches(programs: &Programs, catalog: &Catalog) -> BTreeMap<String, PathBuf> {
    programs
        .resource_types()
        .iter()
        .flat_map(|resource_type| catalog.of_type(resource_type))
        .map(|declared| (declared.context().to_owned(), declared.file().to_path_buf()))
        .collect()
}

/// Returns how an answer names `path`: the draft's wire path, its wire path
/// under `roots`, or its file name.
#[must_use]
pub fn shown(roots: &[Root], draft: Option<&Draft>, path: &Path) -> String {
    if let Some(draft) = draft
        && draft.path == path
    {
        return draft.wire.clone();
    }
    crate::operator::snapshot::shown(roots, path)
}
