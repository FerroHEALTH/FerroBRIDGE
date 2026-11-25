// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Checking and compiling the whole mapping set, then swapping it in.
//!
//! A reload compiles the provisioned files and the running revision's files
//! together, once for the check and once more for each lane that takes its
//! templates from another source, and only a set that compiled for every
//! lane replaces the running one: the facade's programs and search
//! declarations, the operations lane's programs, the FHIR Mapping Language
//! maps and the operator snapshot, each in one pointer swap. A request takes its set at entry, so a request in flight finishes on
//! the old set. No specification governs the reload: our own design.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use fhirconnect::operations::programs::ProgramSet;

use crate::check::Report;
use crate::check::Templates;
use crate::facade::Facade;
use crate::facade::programs::Programs;
use crate::facade::search::Catalog;
use crate::mappings::files::Sources;
use crate::mappings::tree::Tree;
use crate::operations::lane::OperationsLane;
use crate::operator::snapshot::Snapshot;

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
    /// The operations lane's own load refused the set.
    #[error("the FHIRconnect operations did not load the mapping set")]
    Operations(#[source] crate::mappings::Error),
    /// The mapping directories could not be listed.
    #[error("a mapping directory could not be read")]
    Read(#[source] crate::walk::Error),
    /// The operator API reloads nothing: no `[state]` is configured.
    #[error("the operator API has no reload configured")]
    Unconfigured,
}

/// Everything a reload compiles against.
#[derive(Debug, Clone, Default)]
pub struct Reloader {
    /// The provisioned FHIRconnect directory, named in an operations lane
    /// refusal. The console's FHIRconnect files are checked whenever there
    /// are any.
    pub fhirconnect: Option<PathBuf>,
    /// The template directory, when one is configured.
    pub templates: Option<PathBuf>,
    /// The CDR the templates are fetched from otherwise.
    pub cdr: Option<crate::cdr::CdrClient>,
    /// Whether `[mappings] omocl` is configured. The console's OMOCL files
    /// are checked whenever there are any.
    pub omocl: bool,
    /// The facade, when it runs.
    pub facade: Option<Arc<Facade>>,
    /// The operations lane, when it runs.
    pub operations: Option<Arc<OperationsLane>>,
    /// The FHIR Mapping Language maps the inbound sources run.
    pub structuremaps: Option<Arc<crate::structuremap::Running>>,
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
    structuremaps: Vec<crate::structuremap::syntax::StructureMap>,
}

impl Prepared {
    /// Returns the check report of the set.
    #[must_use]
    pub const fn report(&self) -> &Report {
        self.snapshot.report()
    }
}

impl Reloader {
    /// Checks and compiles `sources` for every lane, each search declaration
    /// against the context it names.
    ///
    /// # Errors
    ///
    /// Returns [`ReloadError::Rejected`] with the report when a diagnostic is
    /// an error or a search declaration does not bind, and the other variants
    /// when a lane's own load refuses the set or an upstream does not answer.
    pub async fn prepare(&self, sources: &Sources) -> Result<Prepared, ReloadError> {
        let Sources {
            fhirconnect,
            declarations,
            omocl,
            structuremap,
        } = sources;
        let templates = match (self.templates.as_deref(), self.cdr.as_ref()) {
            (Some(directory), _) => Some(Templates::Directory(directory)),
            (None, Some(client)) => Some(Templates::Cdr(client)),
            (None, None) => None,
        };
        let report = crate::check::check_files(
            (self.fhirconnect.is_some() || !fhirconnect.is_empty()).then_some(fhirconnect),
            templates,
            (self.omocl || !omocl.is_empty()).then_some(omocl),
            (!structuremap.is_empty()).then_some(structuremap),
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
                crate::facade::programs::check_parties(
                    &programs,
                    facade.client().demographic().is_some(),
                )
                .map_err(ReloadError::Facade)?;
                crate::facade::programs::check_owned(&programs, &facade.owned_types())
                    .map_err(ReloadError::Facade)?;
                let catalog = bind(&report, declarations, &programs)?;
                Some((programs, catalog))
            }
            (None, Some(_set)) if !declarations.is_empty() => {
                let programs = crate::facade::programs::from_compiled(
                    report.programs().to_vec(),
                    report.indexes(),
                );
                bind(&report, declarations, &programs)?;
                None
            }
            _ => None,
        };
        let operations = match (self.operations.as_ref(), self.fhirconnect.as_deref()) {
            (Some(_lane), Some(directory)) => Some(
                match (self.templates.as_deref(), self.cdr.as_ref()) {
                    (Some(templates), _) => {
                        crate::mappings::load_tree(directory, fhirconnect, templates)
                    }
                    (None, Some(client)) => {
                        crate::mappings::load_tree_from_cdr(directory, fhirconnect, client).await
                    }
                    (None, None) => Err(crate::mappings::Error::NoTemplateSource(
                        crate::config::error::NoTemplateSource {
                            directory: directory.to_path_buf(),
                        },
                    )),
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
        let structuremaps = report.structuremaps().to_vec();
        Ok(Prepared {
            snapshot: Snapshot::new(report, searches),
            facade,
            operations,
            structuremaps,
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
        if let Some(running) = self.structuremaps.as_ref() {
            running.replace(prepared.structuremaps);
        }
        prepared.snapshot
    }
}

/// Binds the search declarations of `tree` to `programs`, refusing a set
/// with one that does not bind as `report` plus its diagnostic.
fn bind(report: &Report, tree: &Tree, programs: &Programs) -> Result<Catalog, ReloadError> {
    crate::facade::search::load_tree(tree, programs).map_err(|error| {
        let diagnostic = crate::mappings::compile::search_declaration(
            error.file(),
            ferrohealth_server::run::chain(&error),
        );
        ReloadError::Rejected(Box::new(report.clone().with_diagnostic(diagnostic)))
    })
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
