// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The engine run over one resource, the strict re-read of what it built,
//! and the identity record of one committed composition.

use fhirconnect::engine::library::Library;
use fhirconnect::engine::linked::LinkedCompositions;
use fhirconnect::engine::outcome::Warning;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::subject::SubjectIdentifier;
use fhirconnect::engine::traverse::error::EngineError;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_rm::v1_2::composition::composition::Composition;

use crate::cdr::Returned;
use crate::cdr::ids::EhrId;
use crate::facade::commit;
use crate::facade::engine;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::programs::Loaded;
use crate::facade::request::Inbound;
use crate::facade::status;
use crate::identity::ExternalResourceId;
use crate::identity::FhirResourceId;
use crate::identity::derive;
use crate::identity::derive::EntryKey;
use crate::identity::record::CompositionBinding;
use crate::identity::record::Identifier;
use crate::identity::record::SourceVersion;
use crate::identity::store::EntryWrite;
use crate::terminology::Direction;
use ferrohealth_server::run::chain;

use super::Delivery;
use super::Ingest;
use super::Mapped;
use super::Placed;
use super::Provenance;
use super::Refused;
use super::RepresentationError;
use super::Written;
use super::refusal::engine_refusal;
use super::refusal::store_refusal;

/// Returns the consumed-source key of one resource: its `resourceType`, `id`
/// and `meta.versionId`, or nothing when it has no `id`.
pub(super) fn resource_source(inbound: &Inbound) -> Option<SourceVersion> {
    inbound.id().map(|external| {
        SourceVersion::new(
            inbound.resource_type(),
            external.clone(),
            inbound.version_id().map(str::to_owned),
        )
    })
}

/// Returns every `Resource.identifier` of `inbound` that carries a `value`,
/// in document order.
///
/// An `identifier` search matches on the value
/// (<https://hl7.org/fhir/R4/search.html#token>), so an `Identifier` with no
/// `value` can answer no search and has nothing to record.
fn identifiers_of(inbound: &Inbound) -> Vec<Identifier> {
    crate::identity::subject::identifiers_of(inbound.document())
        .into_iter()
        .map(|(system, value)| Identifier::new(system, value))
        .collect()
}

/// Re-reads a built composition through the strict RM reader.
///
/// The built composition is re-read before it is sent, so a bad document never
/// reaches the CDR. It already carries the `FEEDER_AUDIT` the engine wrote
/// ([`engine::inbound_from`]), so the CDR stores where the content came from.
pub(super) fn strict_read(composition: &CanonicalComposition) -> Result<Composition, Refused> {
    let text = serde_json::to_string(composition.value()).map_err(|error| {
        Refused::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(format!(
                "the built composition could not be written: {error}"
            )),
        )
    })?;
    openehr_its::json::from_canonical_json::<Composition>(&text).map_err(|error| {
        Refused::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            Issue::error(IssueType::Processing).diagnosing(format!(
                "the built composition is no valid openEHR COMPOSITION: {error}"
            )),
        )
    })
}

/// Returns the composition the CDR stored, or the built one.
///
/// The CDR is the authority on what it stored, so its representation wins when
/// `Prefer: return=representation` produced one, and the built composition
/// stands only when the CDR returned none.
///
/// # Errors
///
/// Returns a [`Refused`] carrying [`RepresentationError::Unreadable`] when the
/// returned composition does not read back. It answers the status table's
/// [`status::INTERNAL`] row, as an undecodable CDR body does through
/// [`status::of_client_error`].
pub(super) fn representation(
    returned: Returned<Composition>,
    built: &CanonicalComposition,
) -> Result<CanonicalComposition, Refused> {
    match returned {
        Returned::Representation(stored) => canonical_of(stored.as_ref(), built),
        Returned::Minimal | Returned::Identifier(_) => Ok(built.clone()),
    }
}

/// Returns the canonical form of a composition the CDR served, under the
/// template and generation of `like`.
///
/// # Errors
///
/// Returns a [`Refused`] carrying [`RepresentationError::Unreadable`] when the
/// composition does not read back, on the status table's [`status::INTERNAL`]
/// row.
pub(super) fn canonical_of(
    stored: &Composition,
    like: &CanonicalComposition,
) -> Result<CanonicalComposition, Refused> {
    let value = openehr_its::json::to_canonical_json(stored)
        .parse::<serde_json::Value>()
        .map_err(|source| {
            let error = RepresentationError::Unreadable { source };
            Refused::of_answer(&status::Answer::new(status::INTERNAL, chain(&error)))
        })?;
    Ok(CanonicalComposition::new(
        value,
        like.template_id(),
        like.generation(),
    ))
}

/// Returns `seams` answering linked compositions through `linked`, when the
/// run is handed one.
fn linking<'s>(seams: Seams<'s>, linked: Option<&'s dyn LinkedCompositions>) -> Seams<'s> {
    match linked {
        Some(linked) => seams.with_linked(linked),
        None => seams,
    }
}

/// What one engine run answers.
#[derive(Debug)]
pub(super) struct Engined {
    /// The composition the run built.
    pub(super) composition: CanonicalComposition,
    /// The instant every defaulted time was taken from.
    pub(super) instant: String,
    /// The literal references whose resources a `slotContext` consumed.
    pub(super) consumed: Vec<String>,
    /// The identifiers the run offered as the subject of the EHR
    /// (`fhirconnect::engine::subject`).
    pub(super) subject: Vec<SubjectIdentifier>,
    /// The losses the run declared, in the order it took them
    /// (`fhirconnect::engine::outcome::Outcome::warnings`).
    pub(super) losses: Vec<Warning>,
}

impl Ingest<'_> {
    /// Runs the engine over one inbound resource, and returns what it built,
    /// the identifiers it offers as the subject of the EHR and the losses it
    /// declared among them.
    pub(super) async fn build_offering(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        provenance: &Provenance,
    ) -> Result<Engined, Refused> {
        self.run_linking(program, inbound, provenance, None)
            .await
            .map_err(|error| engine_refusal(&error))
    }

    /// Runs the engine over one inbound resource, and returns the composition
    /// with the instant it defaulted every clock-derived time from.
    ///
    /// Every `conceptmap` code is translated with `reverse=true` before the
    /// composition is built ([`crate::terminology::Translator::run`]).
    pub(super) async fn run(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        provenance: &Provenance,
    ) -> Result<(CanonicalComposition, String), EngineError> {
        self.run_linking(program, inbound, provenance, None)
            .await
            .map(|engined| (engined.composition, engined.instant))
    }

    /// Returns the origin a run of `inbound` records in its `FEEDER_AUDIT`: the
    /// bridge, the source item `provenance` names, and what the face named
    /// apart from it.
    fn origin(&self, provenance: &Provenance, inbound: &Inbound) -> engine::Recorded {
        let recorded = engine::origin_of(
            self.settings,
            provenance.source_for(inbound.document()),
            self.originating.as_deref(),
        );
        let mut origin = recorded.origin.with_feeder_items(self.feeder_items.clone());
        if let Some(items) = self.originating_items.as_ref() {
            origin = origin.with_originating_items(items.clone());
        }
        if let Some(provider) = self.provider.as_ref() {
            origin = origin.with_provider(provider.clone());
        }
        engine::Recorded {
            origin,
            facility: self.facility.clone(),
            composer: self.composer.clone(),
        }
    }

    /// Runs the engine as [`Ingest::run`] does, and also returns the literal
    /// references whose resources a `slotContext` consumed
    /// (`fhirconnect::engine::outcome::Outcome::consumed`), answering each
    /// linked composition through `linked`; with none, a `link` that names a
    /// resource refuses.
    pub(super) async fn run_linking(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        provenance: &Provenance,
        linked: Option<&dyn LinkedCompositions>,
    ) -> Result<Engined, EngineError> {
        // NOTE: no specification governs this: our own design, one instant serves
        // every defaulted time of one ingest, so each resource reads the clock once.
        let now = jiff::Timestamp::now().to_string();
        let built = self
            .translator
            .run(Direction::ToOpenehr, |terminology| {
                engine::inbound_from(
                    program.program(),
                    program.index(),
                    inbound.document(),
                    &now,
                    self.settings,
                    self.origin(provenance, inbound),
                    &linking(
                        Seams::default()
                            .with_functions(&Library)
                            .with_terminology(terminology)
                            .with_references(self.references)
                            .with_demographics(self.demographics),
                        linked,
                    ),
                )
            })
            .await;
        let built = built.inspect_err(|_| {
            if let Some(usage) = self.usage {
                crate::usage_report::record_mapping_failure(usage);
            }
        })?;
        let consumed = built.consumed().to_vec();
        let subject = built.subject().to_vec();
        let losses = built.warnings().to_vec();
        Ok(Engined {
            composition: built.into_value(),
            instant: now,
            consumed,
            subject,
            losses,
        })
    }

    /// Returns the commit headers one composition write carries.
    pub(super) fn commit_context(
        &self,
        change: commit::Change,
        composition: &CanonicalComposition,
    ) -> Result<crate::cdr::commit::CommitContext, Refused> {
        commit::context(change, &self.settings.system_id, composition.template_id()).map_err(
            |error| {
                Refused::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Issue::error(IssueType::Exception).diagnosing(chain(&error)),
                )
            },
        )
    }

    /// Records the identity of a committed write, and returns what stands.
    pub(super) async fn record(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        ehr_id: &EhrId,
        version: &ObjectVersionId,
        composition: &CanonicalComposition,
    ) -> Result<Written, Refused> {
        let entry = only_entry(program, composition)?;
        self.written_entry(
            program,
            inbound,
            (ehr_id, version),
            composition,
            &entry,
            composition.clone(),
        )
        .await
    }

    /// Records the identity of a committed update of the entry `path` names,
    /// and returns what stands, narrowed to that entry.
    ///
    /// # Errors
    ///
    /// Returns a `500` [`Refused`] when the committed composition holds no
    /// entry at `path`, and an identity store that cannot be written.
    pub(super) async fn record_entry(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        ehr_id: &EhrId,
        version: &ObjectVersionId,
        composition: &CanonicalComposition,
        path: &str,
    ) -> Result<Written, Refused> {
        let entry = engine::entry::entry_named(program.program(), program.index(), composition, path)?.ok_or_else(|| {
            Refused::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(
                    "the committed composition holds no entry where the updated resource's entry was",
                ),
            )
        })?;
        let narrowed = entry.isolate(composition);
        self.written_entry(
            program,
            inbound,
            (ehr_id, version),
            composition,
            &entry,
            narrowed,
        )
        .await
    }

    /// Binds, indexes and consumes each mapped entry at the version `stored`
    /// pairs with it, in entry order.
    ///
    /// The identity is read from the composition the CDR stored at that
    /// version, so a retry that maps the resource again binds what the first
    /// delivery committed, never a fresh build that differs from it in its
    /// clock-filled times (no specification governs the identity: our own
    /// design).
    pub(super) async fn place(
        &self,
        mapped: &[Mapped],
        sources: &[Option<SourceVersion>],
        ehr_id: &EhrId,
        stored: Vec<(ObjectVersionId, Composition)>,
    ) -> Result<Vec<Placed>, Refused> {
        let mut placed = Vec::with_capacity(stored.len());
        for ((entry, (version, held)), source) in mapped.iter().zip(stored).zip(sources) {
            let composition = canonical_of(&held, &entry.built)?;
            let first = only_entry(&entry.program, &composition)?;
            let (id, _stood) = self
                .write_entry(
                    &entry.program,
                    &entry.inbound,
                    (ehr_id, &version),
                    &composition,
                    &first,
                    source.as_ref(),
                )
                .await?;
            placed.push(Placed {
                id,
                version,
                change: None,
            });
        }
        Ok(placed)
    }

    /// Records the identity binding of one committed composition, and returns
    /// the resource id and the binding that stand.
    pub(crate) async fn bind(
        &self,
        program: &Loaded,
        resource_type: &str,
        ehr_id: &EhrId,
        version: &ObjectVersionId,
        composition: &CanonicalComposition,
    ) -> Result<(FhirResourceId, CompositionBinding), Refused> {
        let entry = only_entry(program, composition)?;
        self.bind_entry(program, resource_type, ehr_id, version, composition, &entry)
            .await
    }

    /// Records the identity binding of one entry of a committed composition,
    /// and returns the resource id and the binding that stand.
    ///
    /// Each entry of the program's archetype is a resource of its own, keyed
    /// by the composition and [`engine::entry::Entry::identity_path`].
    pub(crate) async fn bind_entry(
        &self,
        program: &Loaded,
        resource_type: &str,
        ehr_id: &EhrId,
        version: &ObjectVersionId,
        composition: &CanonicalComposition,
        entry: &engine::entry::Entry,
    ) -> Result<(FhirResourceId, CompositionBinding), Refused> {
        let bound = Bound::of(
            program,
            resource_type,
            (ehr_id, version),
            composition,
            entry,
        );
        self.store
            .record_entry(&bound.write(resource_type, &[], None))
            .await
            .map_err(|error| store_refusal(&error))
    }

    /// Records the entry of a written resource with the source it names,
    /// and returns the committed write of `narrowed`.
    async fn written_entry(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        (ehr_id, version): (&EhrId, &ObjectVersionId),
        composition: &CanonicalComposition,
        entry: &engine::entry::Entry,
        narrowed: CanonicalComposition,
    ) -> Result<Written, Refused> {
        let source = resource_source(inbound);
        let (id, _stood) = self
            .write_entry(
                program,
                inbound,
                (ehr_id, version),
                composition,
                entry,
                source.as_ref(),
            )
            .await?;
        Ok(Written {
            id,
            ehr_id: ehr_id.clone(),
            version: version.clone(),
            composition: narrowed,
            delivery: Delivery::Committed,
            losses: Vec::new(),
        })
    }

    /// Records the binding of one entry of a written resource with every
    /// `Resource.identifier` it carries, so a conditional create's
    /// `identifier` search finds it (<https://hl7.org/fhir/R4/http.html#ccreate>),
    /// and the `source` it consumed, in one store write.
    async fn write_entry(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        located: (&EhrId, &ObjectVersionId),
        composition: &CanonicalComposition,
        entry: &engine::entry::Entry,
        source: Option<&SourceVersion>,
    ) -> Result<(FhirResourceId, CompositionBinding), Refused> {
        let resource_type = inbound.resource_type();
        let bound = Bound::of(program, resource_type, located, composition, entry);
        let identifiers = identifiers_of(inbound);
        self.store
            .record_entry(&bound.write(resource_type, &identifiers, source))
            .await
            .map_err(|error| store_refusal(&error))
    }
}

/// The first entry of the program's archetype in `composition`.
fn only_entry(
    program: &Loaded,
    composition: &CanonicalComposition,
) -> Result<engine::entry::Entry, Refused> {
    engine::entry::entry_of(program.program(), program.index(), composition)?.ok_or_else(|| {
        Refused::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            Issue::error(IssueType::Exception).diagnosing(
                "the committed composition carries no entry of the archetype the program maps",
            ),
        )
    })
}

/// The key, the derived id and the binding one committed entry is recorded
/// under.
struct Bound {
    /// The key the internal id is recorded under.
    key: ExternalResourceId,
    /// The internal id derived for the entry.
    derived: FhirResourceId,
    /// Where the entry lives.
    binding: CompositionBinding,
}

impl Bound {
    /// Returns the record of `entry` in the composition committed at
    /// `located`, served as `resource_type`.
    fn of(
        program: &Loaded,
        resource_type: &str,
        (ehr_id, version): (&EhrId, &ObjectVersionId),
        composition: &CanonicalComposition,
        entry: &engine::entry::Entry,
    ) -> Self {
        // NOTE: no specification governs this: our own design, every further resource
        // of a split is bound as a created resource of its own when the entry is
        // rendered, so the entry's identity is its first occurrence.
        let path = entry.identity_path().to_owned();
        let key = EntryKey::new(version.versioned_object_id(), path.as_str());
        Self {
            key: derive::map_key(&key),
            derived: derive::entry_id(&key),
            binding: CompositionBinding {
                ehr_id: String::from(ehr_id.as_str()),
                versioned_object_uid: String::from(version.versioned_object_id().value()),
                template_id: String::from(composition.template_id()),
                resource_type: String::from(resource_type),
                entry_path: path,
                split: 0,
                context: program.program().context().to_string(),
                derivation: Some(derive::DERIVATION),
                created: None,
            },
        }
    }

    /// Returns the store write of this record with `identifiers` and the
    /// consumed `source`.
    fn write<'a>(
        &'a self,
        resource_type: &'a str,
        identifiers: &'a [Identifier],
        source: Option<&'a SourceVersion>,
    ) -> EntryWrite<'a> {
        EntryWrite {
            resource_type,
            key: &self.key,
            derived: &self.derived,
            binding: &self.binding,
            identifiers,
            source,
        }
    }
}
