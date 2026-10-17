// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The engine run over one resource, the strict re-read of what it built,
//! and the identity record of one committed composition.

use fhir_types::codec::Value;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::error::EngineError;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_rm::v1_2::composition::composition::Composition;

use crate::cdr::Returned;
use crate::cdr::ids::EhrId;
use crate::cdr::ids::versioned_object_uid;
use crate::facade::commit;
use crate::facade::engine;
use crate::facade::identity::FhirResourceId;
use crate::facade::identity::derive;
use crate::facade::identity::derive::EntryKey;
use crate::facade::identity::record::CompositionBinding;
use crate::facade::identity::record::ConsumedSource;
use crate::facade::identity::record::Identifier;
use crate::facade::identity::record::SourceVersion;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::outcome::chain;
use crate::facade::programs::Loaded;
use crate::facade::request::Inbound;
use crate::facade::status;
use crate::terminology::Direction;

use super::Delivery;
use super::Ingest;
use super::Mapped;
use super::Placed;
use super::Provenance;
use super::Refused;
use super::RepresentationError;
use super::Written;
use super::engine_refusal;
use super::store_refusal;

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
    inbound
        .document()
        .get("identifier")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .filter_map(|identifier| {
            let value = identifier.get("value").and_then(Value::as_str)?;
            let system = identifier.get("system").and_then(Value::as_str);
            Some(Identifier::new(system, value))
        })
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

impl Ingest<'_> {
    /// Runs the engine over one inbound resource.
    pub(super) async fn build(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        provenance: &Provenance,
    ) -> Result<CanonicalComposition, Refused> {
        self.run(program, inbound, provenance)
            .await
            .map(|(composition, _instant)| composition)
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
                    provenance.source_for(inbound.document()),
                    &Seams::default()
                        .with_terminology(terminology)
                        .with_references(self.references),
                )
            })
            .await;
        let built = built.inspect_err(|_| {
            if let Some(usage) = self.usage {
                usage.record_mapping_failure();
            }
        })?;
        Ok((built.into_value(), now))
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
    pub(super) fn record(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        ehr_id: &EhrId,
        version: &ObjectVersionId,
        composition: &CanonicalComposition,
    ) -> Result<Written, Refused> {
        let (id, stood) = self.bind(
            program,
            inbound.resource_type(),
            ehr_id,
            version,
            composition,
        )?;
        self.finish_write(inbound, id, &stood, ehr_id, version, composition.clone())
    }

    /// Records the identity of a committed update of the entry `path` names,
    /// and returns what stands, narrowed to that entry.
    ///
    /// # Errors
    ///
    /// Returns a `500` [`Refused`] when the committed composition holds no
    /// entry at `path`, and an identity store that cannot be written.
    pub(super) fn record_entry(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        ehr_id: &EhrId,
        version: &ObjectVersionId,
        composition: &CanonicalComposition,
        path: &str,
    ) -> Result<Written, Refused> {
        let entry = engine::entry::entry_named(program.program(), composition, path)?.ok_or_else(|| {
            Refused::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(
                    "the committed composition holds no entry where the updated resource's entry was",
                ),
            )
        })?;
        let (id, stood) = self.bind_entry(
            program,
            inbound.resource_type(),
            ehr_id,
            version,
            composition,
            &entry,
        )?;
        self.finish_write(
            inbound,
            id,
            &stood,
            ehr_id,
            version,
            entry.isolate(composition),
        )
    }

    /// Indexes and consumes the source of one bound write, and returns what
    /// stands.
    fn finish_write(
        &self,
        inbound: &Inbound,
        id: FhirResourceId,
        stood: &CompositionBinding,
        ehr_id: &EhrId,
        version: &ObjectVersionId,
        composition: CanonicalComposition,
    ) -> Result<Written, Refused> {
        self.index(inbound, &id)?;
        if let Some(source) = resource_source(inbound) {
            self.consume(&source, stood, &id)?;
        }
        Ok(Written {
            id,
            ehr_id: ehr_id.clone(),
            version: version.clone(),
            composition,
            delivery: Delivery::Committed,
        })
    }

    /// Binds and consumes each mapped entry at the version `stored` pairs with
    /// it, in entry order.
    ///
    /// The identity is read from the composition the CDR stored at that
    /// version, so a retry that maps the resource again binds what the first
    /// delivery committed, never a fresh build that differs from it in its
    /// clock-filled times (no specification governs the identity: our own
    /// design).
    pub(super) fn place(
        &self,
        mapped: &[Mapped<'_>],
        sources: &[Option<SourceVersion>],
        ehr_id: &EhrId,
        stored: Vec<(ObjectVersionId, Composition)>,
    ) -> Result<Vec<Placed>, Refused> {
        let mut placed = Vec::with_capacity(stored.len());
        for ((entry, (version, held)), source) in mapped.iter().zip(stored).zip(sources) {
            let composition = canonical_of(&held, &entry.built)?;
            let (id, stood) = self.bind(
                entry.program,
                entry.inbound.resource_type(),
                ehr_id,
                &version,
                &composition,
            )?;
            self.index(&entry.inbound, &id)?;
            if let Some(source) = source {
                self.consume(source, &stood, &id)?;
            }
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
    pub(crate) fn bind(
        &self,
        program: &Loaded,
        resource_type: &str,
        ehr_id: &EhrId,
        version: &ObjectVersionId,
        composition: &CanonicalComposition,
    ) -> Result<(FhirResourceId, CompositionBinding), Refused> {
        let entry = engine::entry::entry_of(program.program(), composition).ok_or_else(|| {
            Refused::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(
                    "the committed composition carries no entry of the archetype the program maps",
                ),
            )
        })?;
        self.bind_entry(program, resource_type, ehr_id, version, composition, &entry)
    }

    /// Records the identity binding of one entry of a committed composition,
    /// and returns the resource id and the binding that stand.
    ///
    /// Each entry of the program's archetype is a resource of its own, keyed
    /// by the composition and [`engine::entry::Entry::identity_path`].
    pub(crate) fn bind_entry(
        &self,
        program: &Loaded,
        resource_type: &str,
        ehr_id: &EhrId,
        version: &ObjectVersionId,
        composition: &CanonicalComposition,
        entry: &engine::entry::Entry,
    ) -> Result<(FhirResourceId, CompositionBinding), Refused> {
        // NOTE: no specification governs this: our own design, every further resource
        // of a split is bound as a created resource of its own when the entry is
        // rendered, so the entry's identity is its first occurrence.
        let path = entry.identity_path();
        let key = EntryKey::new(versioned_object_uid(version), path.as_str(), 0);
        let map_key = derive::map_key(&key);
        let mut recorded = self
            .store
            .internal_of(resource_type, &map_key)
            .map_err(|error| store_refusal(&error))?;
        if recorded.is_none()
            && let Some(legacy) = entry.legacy_path()
        {
            // The map wins once written: an id an earlier build recorded under
            // the legacy path of the entry still stands, when that path names
            // this entry and no other.
            let earlier = EntryKey::new(versioned_object_uid(version), legacy.as_str(), 0);
            let held = self
                .store
                .internal_of(resource_type, &derive::map_key(&earlier))
                .map_err(|error| store_refusal(&error))?;
            if held.is_some()
                && engine::entry::entry_named(program.program(), composition, &legacy)?
                    .is_some_and(|named| named == *entry)
            {
                recorded = held;
            }
        }
        let id = if let Some(known) = recorded {
            known
        } else {
            let (fresh, _source) = derive::derive(&key, entry.uid());
            self.store
                .record_internal(resource_type, &map_key, &fresh)
                .map_err(|error| store_refusal(&error))?
        };
        let binding = CompositionBinding {
            ehr_id: String::from(ehr_id.as_str()),
            versioned_object_uid: String::from(versioned_object_uid(version).value()),
            template_id: String::from(composition.template_id()),
            resource_type: String::from(resource_type),
            entry_path: path,
            split: key.split(),
            context: program.program().context().to_string(),
            derivation: Some(derive::DERIVATION),
            created: None,
        };
        let stood = self
            .store
            .record_binding(resource_type, &id, &binding)
            .map_err(|error| store_refusal(&error))?;
        Ok((id, stood))
    }

    /// Records every `Resource.identifier` of `inbound` against `id`, so a
    /// conditional create's `identifier` search finds the resource
    /// (<https://hl7.org/fhir/R4/http.html#ccreate>).
    fn index(&self, inbound: &Inbound, id: &FhirResourceId) -> Result<(), Refused> {
        for identifier in identifiers_of(inbound) {
            self.store
                .record_identifier(inbound.resource_type(), &identifier, id)
                .map_err(|error| store_refusal(&error))?;
        }
        Ok(())
    }

    /// Records that `source` was consumed by the composition `stood` binds.
    fn consume(
        &self,
        source: &SourceVersion,
        stood: &CompositionBinding,
        id: &FhirResourceId,
    ) -> Result<(), Refused> {
        self.store
            .record_consumed(
                source,
                &ConsumedSource {
                    ehr_id: stood.ehr_id.clone(),
                    versioned_object_uid: stood.versioned_object_uid.clone(),
                    internal_id: String::from(id.as_str()),
                    context: stood.context.clone(),
                },
            )
            .map_err(|error| store_refusal(&error))?;
        Ok(())
    }
}
