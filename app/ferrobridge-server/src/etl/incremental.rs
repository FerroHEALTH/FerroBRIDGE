// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The incremental OMOP load: a change-feed consumer that commits each
//! changed composition through the batch run's per-composition step.
//!
//! An event names the compositions a contribution changed. For each one the
//! load reads the composition's latest version over ITS-REST 1.1.0
//! `composition_get` with its versioned object id and acts on the answer:
//! `200` is validated, mapped and committed by `etl::commit_one`,
//! the step `etl run` commits each composition with; `204`, a deletion
//! (`204_deleted_at_time`), removes the composition's rows under the
//! `[etl] deletions` policy; `404` keeps what the CDM holds, as the batch
//! run's deletion sweep does. An erasure removes the rows of every
//! composition it names, or of every composition of an erased EHR. The
//! visits are read once per pass, before its first commit or its
//! derivation, and the derived tables are rebuilt once per pass, before the
//! pass acknowledges anything ([`Consumer::settle`]). A batch run after an
//! incremental run therefore writes the same rows. No specification governs
//! the incremental mode: our own design.

use omop_cdm::graph::key::Source;
use omop_cdm::writer::CdmWriter;
use omop_cdm::writer::error::WriteError;
use omop_cdm::writer::input::RunId;
use omop_cdm::writer::removal::{Committed, Removal};
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_base::v1_3::base_types::identification::uid_based_id::UidBasedId;
use openehr_its::rest::generated::ehr::client::CompositionGetOutcome;

use crate::cdr::CdrClient;
use crate::cdr::error::CdrError;
use crate::cdr::ids::EhrId;
use crate::change_feed::Event;
use crate::change_feed::ReaderName;
use crate::change_feed::follow::Consumer;
use crate::change_feed::follow::ConsumerError;
use crate::change_feed::health::Following;
use crate::config::CdmSettings;
use crate::etl::deletions::DeletionPolicy;
use crate::etl::job::JobError;
use crate::etl::mapper::{CdmVocabulary, OmoclMapper, read_roots};
use crate::etl::report::RunReport;
use crate::etl::{EtlSettings, MappingContext, Read, RunError, Step, Templates, Visited};

/// The name the load reports to readiness and the logs under.
pub const NAME: &str = "omop-incremental";

/// The RM type name of the versioned objects the load commits.
const COMPOSITION: &str = "COMPOSITION";

/// The RM type name of an erased EHR in an erasure.
const EHR: &str = "EHR";

/// The incremental load, resolved from `[omop_incremental]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The reader name the load follows the change feed under.
    pub reader: ReaderName,
}

/// Why the load did not finish an event.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadError {
    /// The CDR did not answer the composition read.
    #[error("the CDR did not answer the composition read")]
    Cdr(#[source] CdrError),
    /// The composition answer is no canonical JSON.
    #[error("the CDR's composition answer is no canonical JSON")]
    Body(#[source] serde_json::Error),
    /// The composition answer names no version in its `ETag` or its `uid`.
    #[error("the CDR's composition answer names no version")]
    NoVersion,
    /// The event or the answer names an identifier that is no openEHR
    /// identifier.
    #[error("the {what} `{value}` is no openEHR identifier")]
    Identifier {
        /// What the identifier names.
        what: &'static str,
        /// The identifier.
        value: String,
    },
    /// The run step stopped: a template the CDR did not serve, the visit
    /// query, or the CDM database.
    #[error("the commit step stopped")]
    Run(#[source] RunError),
    /// The CDM database refused a removal or a read of the side table.
    #[error("the CDM database refused the removal")]
    Write(#[source] WriteError),
    /// The CDM writer could not connect.
    #[error("the CDM writer could not connect")]
    Connect(#[source] WriteError),
    /// The mapper or the writer refused the composition.
    #[error("the composition was refused: {reason}")]
    Refused {
        /// Why, as the run report states it.
        reason: String,
    },
}

/// What the load keeps between events of one pass.
#[derive(Debug, Default)]
struct State {
    /// The writer, connected on first use and again after it closed.
    writer: Option<CdmWriter>,
    /// The templates read this pass.
    templates: Templates,
    /// The visits read this pass, when `[etl]` derives visits.
    visited: Option<Visited>,
    /// Whether this pass changed the CDM, so the derived tables are rebuilt.
    changed: bool,
}

/// The change-feed consumer that loads OMOP incrementally.
#[derive(Debug)]
pub struct Incremental {
    /// The CDR the compositions and templates are read from.
    cdr: CdrClient,
    /// The `[etl]` settings the batch run reads.
    etl: EtlSettings,
    /// The CDM database.
    cdm: CdmSettings,
    /// The OMOCL mapper.
    mapper: OmoclMapper<CdmVocabulary>,
    /// The run every watermark the load writes names.
    run_id: RunId,
    /// What the load keeps between events.
    state: tokio::sync::Mutex<State>,
    /// The status the committed compositions are counted in, when reported.
    following: Option<Following>,
}

impl Incremental {
    /// Returns the load over `settings`, reading through `cdr`: the OMOCL
    /// set loads and the concept resolver connects here, so a mapping that
    /// does not load or a CDM database that does not answer refuses the
    /// start; the writer connects on the first event.
    ///
    /// # Errors
    ///
    /// Returns [`JobError::Missing`] when `[etl]`, `[cdm]` or the OMOCL set
    /// is not configured, [`JobError::Mappings`] when the set does not load,
    /// the connection variants when the resolver cannot connect, and
    /// [`JobError::RunId`] when the run identifier cannot be formed.
    pub async fn start(
        settings: &crate::config::Settings,
        cdr: CdrClient,
    ) -> Result<Self, JobError> {
        let roots = crate::etl::job::omocl_roots(settings);
        let (Some(etl), Some(cdm), false) = (
            settings.etl.as_ref(),
            settings.cdm.as_ref(),
            roots.is_empty(),
        ) else {
            return Err(JobError::Missing("[etl], [cdm] and [mappings] omocl"));
        };
        let set = read_roots(&roots).map_err(JobError::Mappings)?;
        let (mapper, _pool) = crate::etl::job::mapper(etl, cdm, set).await?;
        let run_id =
            RunId::new(format!("{NAME}-{}", uuid::Uuid::new_v4())).map_err(JobError::RunId)?;
        Ok(Self {
            cdr,
            etl: etl.clone(),
            cdm: cdm.clone(),
            mapper,
            run_id,
            state: tokio::sync::Mutex::new(State::default()),
            following: None,
        })
    }

    /// Returns this load counting each committed composition in the status
    /// `following` holds.
    #[must_use]
    pub fn with_following(mut self, following: Following) -> Self {
        self.following = Some(following);
        self
    }

    /// Returns the connected writer of `state`, connecting it first when it
    /// has none or its connection closed.
    async fn writer<'s>(
        &self,
        state: &'s mut Option<CdmWriter>,
    ) -> Result<&'s mut CdmWriter, LoadError> {
        let writer = match state.take() {
            Some(writer) if !writer.is_closed() => writer,
            _ => self.connect().await?,
        };
        Ok(state.insert(writer))
    }

    /// Connects a writer to the CDM database, pseudonymising when configured.
    async fn connect(&self) -> Result<CdmWriter, LoadError> {
        {
            let mut writer = CdmWriter::connect_with(
                &self.cdm.connection,
                self.cdm.schema.clone(),
                self.cdm.bridge_schema.clone(),
                self.cdm.person_policy,
            )
            .await
            .map_err(LoadError::Connect)?;
            if let Some(pseudonymiser) = self.cdm.pseudonymiser.as_ref() {
                writer = writer.pseudonymise(pseudonymiser.clone());
            }
            Ok(writer)
        }
    }

    /// Reads the visits and writes the ones they derive, once per pass, when
    /// `[etl]` derives visits.
    async fn visits(
        &self,
        writer: &mut CdmWriter,
        visited: &mut Option<Visited>,
        report: &mut RunReport,
    ) -> Result<(), LoadError> {
        if visited.is_none() && self.etl.visits.is_some() {
            *visited = crate::etl::read_visits(&self.etl, &self.cdr, writer, report)
                .await
                .map_err(LoadError::Run)?;
        }
        Ok(())
    }

    /// Loads the latest version of the composition `container` of `ehr`.
    async fn composition(
        &self,
        state: &mut State,
        ehr: &EhrId,
        container: &HierObjectId,
    ) -> Result<(), LoadError> {
        let answered = self
            .cdr
            .composition(ehr, &UidBasedId::HierObjectId(container.clone()), None)
            .await
            .map_err(LoadError::Cdr)?;
        let headers = match answered.outcome {
            CompositionGetOutcome::Ok { headers, .. } => headers,
            CompositionGetOutcome::NoContent => return self.retract(state, container).await,
            CompositionGetOutcome::NotFound { .. } => {
                tracing::debug!(
                    lane = NAME,
                    composition = container.value(),
                    "the CDR knows no such composition; the CDM keeps what it holds"
                );
                return Ok(());
            }
        };
        let value: serde_json::Value =
            serde_json::from_str(answered.upstream.body()).map_err(LoadError::Body)?;
        let version = version(headers.etag.as_deref(), &value)?;
        let ehr_id = HierObjectId::new(ehr.as_str()).map_err(|_refused| LoadError::Identifier {
            what: "EHR id",
            value: ehr.as_str().to_owned(),
        })?;
        let source = Source::new(ehr_id, version);
        let State {
            writer,
            templates,
            visited,
            changed,
        } = state;
        let writer = self.writer(writer).await?;
        let mut report = RunReport::new(self.run_id.as_str());
        self.visits(writer, visited, &mut report).await?;
        let read = Read {
            source: &source,
            context: MappingContext {
                type_concept_id: self.etl.type_concept_id,
            },
            windows: visited.as_ref().map(|visited| &visited.windows),
        };
        let step = Step {
            cdr: &self.cdr,
            mapper: &self.mapper,
            run_id: &self.run_id,
        };
        let refused = crate::etl::commit_one(&step, templates, &read, value, writer, &mut report)
            .await
            .map_err(LoadError::Run)?;
        *changed = true;
        if let Some(refusal) = refused {
            return Err(LoadError::Refused {
                reason: refusal.reason().to_owned(),
            });
        }
        if let Some(following) = self.following.as_ref() {
            following.delivered();
        }
        Ok(())
    }

    /// Removes the rows of the composition `container` the CDR reports
    /// deleted, under the `[etl] deletions` policy.
    async fn retract(&self, state: &mut State, container: &HierObjectId) -> Result<(), LoadError> {
        let writer = self.writer(&mut state.writer).await?;
        let Some(committed) = writer
            .committed(container)
            .await
            .map_err(LoadError::Write)?
        else {
            return Ok(());
        };
        self.remove(state, &[committed]).await
    }

    /// Removes the rows of each of `committed` under the `[etl] deletions`
    /// policy.
    async fn remove(&self, state: &mut State, committed: &[Committed]) -> Result<(), LoadError> {
        if self.etl.deletions == DeletionPolicy::Report {
            for kept in committed {
                tracing::warn!(
                    lane = NAME,
                    composition = kept.versioned_object_uid().value(),
                    "the CDR reports this composition gone; [etl] deletions = report keeps its rows"
                );
            }
            return Ok(());
        }
        let writer = self.writer(&mut state.writer).await?;
        for one in committed {
            match writer.remove(one).await.map_err(LoadError::Write)? {
                Removal::Removed(rows) => {
                    tracing::info!(
                        lane = NAME,
                        composition = one.versioned_object_uid().value(),
                        tables = rows.len(),
                        "removed the rows of a composition the CDR no longer holds"
                    );
                    state.changed = true;
                }
                Removal::Superseded => {}
            }
        }
        Ok(())
    }

    /// Removes the rows of everything the erasure `event` names: each erased
    /// composition, and every composition of an erased EHR.
    async fn erase(&self, state: &mut State, event: &Event) -> Result<(), LoadError> {
        let Some(ehr) = event.ehr_id.as_ref() else {
            return Ok(());
        };
        let ehr_id = HierObjectId::new(ehr.as_str()).map_err(|_refused| LoadError::Identifier {
            what: "EHR id",
            value: ehr.as_str().to_owned(),
        })?;
        let mut erased = Vec::new();
        for version in &event.versions {
            match (version.kind.as_str(), version.vo_id) {
                (COMPOSITION, Some(vo_id)) => {
                    let container = container(vo_id)?;
                    let writer = self.writer(&mut state.writer).await?;
                    erased.extend(
                        writer
                            .committed(&container)
                            .await
                            .map_err(LoadError::Write)?,
                    );
                }
                (EHR, _) => {
                    let writer = self.writer(&mut state.writer).await?;
                    erased.extend(
                        writer
                            .committed_in(&ehr_id)
                            .await
                            .map_err(LoadError::Write)?,
                    );
                }
                _ => {}
            }
        }
        self.remove(state, &erased).await
    }
}

/// Returns the versioned object id `vo_id` names.
fn container(vo_id: uuid::Uuid) -> Result<HierObjectId, LoadError> {
    HierObjectId::new(vo_id.to_string()).map_err(|_refused| LoadError::Identifier {
        what: "versioned object id",
        value: vo_id.to_string(),
    })
}

/// Returns the version a composition answer names: its `ETag`, or the
/// composition's own `uid` (openEHR RM Common 1.1.0, `LOCATABLE.uid`).
fn version(
    etag: Option<&str>,
    composition: &serde_json::Value,
) -> Result<ObjectVersionId, LoadError> {
    if let Some(version) =
        crate::cdr::optional_version_from_etag("composition_get", etag).map_err(LoadError::Cdr)?
    {
        return Ok(version);
    }
    let uid = composition
        .pointer("/uid/value")
        .and_then(serde_json::Value::as_str)
        .ok_or(LoadError::NoVersion)?;
    ObjectVersionId::new(uid).map_err(|_refused| LoadError::Identifier {
        what: "composition uid",
        value: uid.to_owned(),
    })
}

/// Returns the consumer failure `error` is: a fault of the event or of what
/// the CDR holds for it is poisoned, an upstream that did not answer is
/// transient.
fn failure(error: LoadError) -> ConsumerError {
    match error {
        LoadError::Body(_)
        | LoadError::NoVersion
        | LoadError::Identifier { .. }
        | LoadError::Refused { .. } => ConsumerError::poisoned(error),
        LoadError::Cdr(_) | LoadError::Run(_) | LoadError::Write(_) | LoadError::Connect(_) => {
            ConsumerError::transient(error)
        }
    }
}

#[async_trait::async_trait]
impl Consumer for Incremental {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn process(&self, event: &Event) -> Result<(), ConsumerError> {
        let mut state = self.state.lock().await;
        if event.erasure {
            return self.erase(&mut state, event).await.map_err(failure);
        }
        let Some(ehr) = event.ehr_id.as_ref() else {
            return Ok(());
        };
        for version in &event.versions {
            let (COMPOSITION, Some(vo_id)) = (version.kind.as_str(), version.vo_id) else {
                continue;
            };
            let container = container(vo_id).map_err(failure)?;
            self.composition(&mut state, ehr, &container)
                .await
                .map_err(failure)?;
        }
        Ok(())
    }

    async fn settle(&self) -> Result<(), ConsumerError> {
        let mut state = self.state.lock().await;
        if !state.changed {
            return Ok(());
        }
        let State {
            writer, visited, ..
        } = &mut *state;
        let writer = self.writer(writer).await.map_err(failure)?;
        let mut report = RunReport::new(self.run_id.as_str());
        self.visits(writer, visited, &mut report)
            .await
            .map_err(failure)?;
        let answered = visited
            .as_ref()
            .and_then(|visited| visited.answered.as_deref());
        crate::etl::derive(&self.etl, writer, &mut report, answered)
            .await
            .map_err(|error| failure(LoadError::Run(error)))?;
        *state = State {
            writer: state.writer.take(),
            ..State::default()
        };
        Ok(())
    }
}
