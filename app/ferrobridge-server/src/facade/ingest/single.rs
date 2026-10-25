// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The single create and update: what the identity map knows of one
//! resource, the first version of its composition, and a later one.

use fhirconnect::engine::subject::SubjectIdentifier;
use http::StatusCode;
use openehr_its::rest::generated::ehr::client::CompositionCreateOutcome;
use openehr_its::rest::generated::ehr::client::CompositionUpdateOutcome;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_rm::v1_2::composition::composition::Composition;

use crate::cdr::Prefer;
use crate::cdr::ids::EhrId;
use crate::facade::commit;
use crate::facade::ehr;
use crate::facade::engine;
use crate::facade::identity::claims::Claim;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::outcome::chain;
use crate::facade::programs::Loaded;
use crate::facade::request::Inbound;
use crate::facade::status;

use super::Ingest;
use super::Provenance;
use super::Recognised;
use super::Refused;
use super::Revision;
use super::Standing;
use super::Written;
use super::binding::Engined;
use super::binding::canonical_of;
use super::binding::representation;
use super::binding::resource_source;
use super::binding::strict_read;
use super::claim::unclaimed;
use super::linked::Built;
use super::linked::Linker;
use super::linked::with_committed;
use super::pairing::same_content;
use super::reconcile::one_composition;
use super::refusal::cdr_refusal;
use super::refusal::ehr_refusal;
use super::refusal::engine_refusal;
use super::refusal::store_refusal;
use super::refusal::upstream_refusal;

impl Ingest<'_> {
    /// Returns what the identity map knows of the source a single create of
    /// `inbound` carries.
    ///
    /// The source key is the `resourceType`, the `id` and the
    /// `meta.versionId` ([`SourceVersion`]). No specification governs the
    /// rule that follows: our own design, since R4 `create` says nothing of a
    /// repeated create with a client-assigned `id`
    /// (<https://hl7.org/fhir/R4/http.html#create>) and a `meta.versionId`
    /// changes each time the sender's resource changes
    /// (<https://hl7.org/fhir/R4/resource.html#Meta>).
    ///
    /// - A source whose `id` and `meta.versionId` the map consumed is
    ///   [`Recognised::Replayed`]: nothing is committed.
    /// - A source whose `id` the map consumed at another `meta.versionId` is
    ///   [`Recognised::Revised`].
    /// - A source with no `meta.versionId` whose `id` the map consumed at any
    ///   version is compared with the composition as it stands, masking what
    ///   two mappings of one resource differ in (the `uid` the CDR assigns and
    ///   every time the run filled from its clock, as `pair` masks them). In
    ///   a composition holding several entries of the archetype, the
    ///   comparison is of the entry the `id` is bound to. The same content is
    ///   [`Recognised::Replayed`], answering that entry, and other content
    ///   [`Recognised::Revised`].
    /// - A source whose key a transaction committed and left unbound, or whose
    ///   `id` the map never consumed, is [`Recognised::Unknown`].
    ///
    /// `claim` is the one [`Ingest::claim_resource`] took for `inbound`.
    ///
    /// # Errors
    ///
    /// Returns a `409` [`Refused`] when the map binds the `id` to more than
    /// one composition, a `422` for a resource the program cannot map or a
    /// composition the strict reader refuses, every CDR refusal of the
    /// composition read through the status table, a `500` when `claim` does
    /// not hold the resource's source key, and a `500` when the identity
    /// store cannot be read or holds an identifier this version cannot read.
    ///
    /// [`SourceVersion`]: crate::facade::identity::record::SourceVersion
    pub async fn recognise(
        &self,
        claim: &Claim<'_>,
        inbound: &Inbound,
        program: &Loaded,
        provenance: &Provenance,
    ) -> Result<Recognised, Refused> {
        let Some(source) = resource_source(inbound) else {
            return Ok(Recognised::Unknown);
        };
        if !claim.holds(&source.storage_key()) {
            return Err(unclaimed());
        }
        let exact = self
            .store
            .consumed(&source)
            .map_err(|error| store_refusal(&error))?;
        let known = if let Some(known) = exact {
            if source.version_id().is_some() {
                let built = self
                    .run_linking(program, inbound, provenance, Some(&Linker::probing()))
                    .await
                    .map_err(|error| engine_refusal(&error))?
                    .composition;
                let standing = self.standing(&known).await?;
                let written = match self.standing_entry(program, inbound, &standing, &built)? {
                    Some((whole, entry)) => standing.replayed(entry.isolate(&whole)),
                    None => standing.written(&built)?,
                };
                return Ok(Recognised::Replayed(written));
            }
            known
        } else {
            if self
                .store
                .committed(&source)
                .map_err(|error| store_refusal(&error))?
                .is_some()
            {
                return Ok(Recognised::Unknown);
            }
            let versions = self
                .store
                .consumed_versions(&source)
                .map_err(|error| store_refusal(&error))?;
            let Some(known) = one_composition(&source, &versions)? else {
                return Ok(Recognised::Unknown);
            };
            if source.version_id().is_some() {
                return Ok(Recognised::Revised(known));
            }
            known
        };
        // NOTE: no specification governs this: our own design, recognition commits
        // nothing, so a link reads as the probe URI and a LINKED resource is revised.
        let Engined {
            composition: built,
            instant,
            ..
        } = self
            .run_linking(program, inbound, provenance, Some(&Linker::probing()))
            .await
            .map_err(|error| engine_refusal(&error))?;
        let sent = strict_read(&built)?;
        let standing = self.standing(&known).await?;
        let Some((whole, entry)) = self.standing_entry(program, inbound, &standing, &built)? else {
            return Ok(Recognised::Revised(known));
        };
        let Some(committed) = would_commit(program, &whole, &entry, &built, sent)? else {
            return Ok(Recognised::Revised(known));
        };
        if same_content(&committed, &instant, &standing.composition) {
            return Ok(Recognised::Replayed(
                standing.replayed(entry.isolate(&whole)),
            ));
        }
        Ok(Recognised::Revised(known))
    }

    /// Returns the standing composition, canonical, with the entry the
    /// consumed resource is bound to, or nothing when the composition no
    /// longer holds that entry.
    fn standing_entry(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        standing: &Standing,
        built: &CanonicalComposition,
    ) -> Result<Option<(CanonicalComposition, engine::entry::Entry)>, Refused> {
        let binding = self
            .store
            .binding_of(inbound.resource_type(), &standing.id)
            .map_err(|error| store_refusal(&error))?
            .ok_or_else(|| {
                Refused::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Issue::error(IssueType::Exception).diagnosing(format!(
                        "the identity map consumed a source as {}/{} and binds that id to no entry",
                        inbound.resource_type(),
                        standing.id.as_str()
                    )),
                )
            })?;
        let whole = canonical_of(&standing.composition, built)?;
        Ok(
            engine::entry::entry_named(program.program(), &whole, &binding.entry_path)?
                .map(|entry| (whole, entry)),
        )
    }

    /// Commits the first version of a composition for `inbound`.
    ///
    /// The EHR is the subject's, resolved or created under the configured
    /// policy; the identity binding and the consumed source version are
    /// recorded once the CDR stored the composition.
    ///
    /// A source the identity map records as committed by a transaction and
    /// not consumed is bound as a re-sent transaction binds it: from the
    /// CONTRIBUTION read back, with the version matched to the resource by
    /// its `FEEDER_AUDIT`, committing nothing, and answering
    /// [`Delivery::Reconciled`] with the composition the first delivery
    /// produced. No specification governs the redelivery rule: our own
    /// design.
    ///
    /// `claim` is the one [`Ingest::claim_resource`] took for `inbound`,
    /// held by the caller from before its identity lookup until the answer,
    /// so a concurrent delivery of the same source cannot commit beside this
    /// one.
    ///
    /// # Errors
    ///
    /// Returns a [`Refused`] for a resource that names no usable subject, an
    /// EHR that cannot be resolved, a resource the program cannot map, a
    /// composition the strict reader refuses, every CDR refusal through the
    /// status table, a `500` naming the recorded contribution when it cannot
    /// be read back or none of its versions matches the resource, a `500`
    /// when `claim` does not hold the resource's source key, and an identity
    /// store that cannot be read or written.
    ///
    /// [`Delivery::Reconciled`]: super::Delivery::Reconciled
    pub async fn ingest_resource(
        &self,
        claim: &Claim<'_>,
        inbound: &Inbound,
        program: &Loaded,
        provenance: &Provenance,
    ) -> Result<Written, Refused> {
        self.ingest_linking(claim, inbound, program, provenance, Linking::Commit)
            .await
    }

    /// Commits the first version of a composition for a linked resource, as
    /// [`Ingest::ingest_resource`] does with its own `link` mappings left
    /// unfollowed.
    pub(super) async fn ingest_unlinked(
        &self,
        claim: &Claim<'_>,
        inbound: &Inbound,
        program: &Loaded,
        provenance: &Provenance,
    ) -> Result<Written, Refused> {
        self.ingest_linking(claim, inbound, program, provenance, Linking::Refuse)
            .await
    }

    /// Commits the first version of a composition for `inbound`, following
    /// its `link` mappings as `linking` says.
    async fn ingest_linking(
        &self,
        claim: &Claim<'_>,
        inbound: &Inbound,
        program: &Loaded,
        provenance: &Provenance,
        linking: Linking,
    ) -> Result<Written, Refused> {
        if let Some(source) = resource_source(inbound)
            && !claim.holds(&source.storage_key())
        {
            return Err(unclaimed());
        }
        let subject = inbound.subject(&self.settings.subject).map_err(|error| {
            Refused::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                Issue::error(IssueType::Required)
                    .diagnosing(chain(&error))
                    .at(format!("{}.subject", inbound.resource_type())),
            )
        })?;
        if let Some(written) = self
            .reconcile_resource(inbound, program, provenance)
            .await?
        {
            return Ok(written);
        }
        let ehr_id = ehr::resolve(
            &self.client,
            self.store,
            &subject,
            &self.settings.subject,
            self.settings.ehr_policy,
        )
        .await
        .map_err(|error| ehr_refusal(&error))?;
        let built = match linking {
            Linking::Commit => self.build_linked(program, inbound, provenance).await?,
            Linking::Refuse => {
                let engined = self.build_offering(program, inbound, provenance).await?;
                Built {
                    composition: engined.composition,
                    committed: Vec::new(),
                    subject: engined.subject,
                    losses: engined.losses,
                }
            }
        };
        self.create(program, inbound, &ehr_id, built.composition, &built.subject)
            .await
            .map(|written| written.with_losses(built.losses))
            .map_err(|refused| with_committed(refused, &built.committed))
    }

    /// Commits `composition` as the first version of a composition in
    /// `ehr_id`, after writing the subject `offered` names into its
    /// `EHR_STATUS`, and records the identity of `inbound`.
    async fn create(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        ehr_id: &EhrId,
        composition: CanonicalComposition,
        offered: &[SubjectIdentifier],
    ) -> Result<Written, Refused> {
        let rm = strict_read(&composition)?;
        Box::pin(self.write_subject(ehr_id, offered)).await?;
        let context = self.commit_context(commit::Change::Creation, &composition)?;
        let answered = self
            .client
            .create_composition(ehr_id, &rm, &context, Prefer::Representation)
            .await
            .map_err(|error| cdr_refusal(&error))?;
        let (version, stored) = match answered.outcome {
            CompositionCreateOutcome::Created { body, headers } => {
                let version = crate::cdr::version_from_etag(
                    "composition_create",
                    StatusCode::CREATED,
                    headers.etag.as_deref(),
                )
                .map_err(|error| cdr_refusal(&error))?;
                let returned = crate::cdr::returned(body, Prefer::Representation);
                (version, representation(returned, &composition)?)
            }
            CompositionCreateOutcome::NoContent { headers } => {
                let version = crate::cdr::version_from_etag(
                    "composition_create",
                    StatusCode::NO_CONTENT,
                    headers.etag.as_deref(),
                )
                .map_err(|error| cdr_refusal(&error))?;
                (version, composition.clone())
            }
            CompositionCreateOutcome::UnprocessableEntity { body } => {
                return Err(upstream_refusal(
                    status::UNPROCESSABLE,
                    StatusCode::UNPROCESSABLE_ENTITY,
                    &body,
                ));
            }
            CompositionCreateOutcome::NotFound { body } => {
                return Err(upstream_refusal(
                    status::NOT_FOUND,
                    StatusCode::NOT_FOUND,
                    &body,
                ));
            }
            CompositionCreateOutcome::BadRequest { body } => {
                return Err(upstream_refusal(
                    status::BAD_REQUEST,
                    StatusCode::BAD_REQUEST,
                    &body,
                ));
            }
        };
        self.record(program, inbound, ehr_id, &version, &stored)
    }

    /// Commits a later version of the composition `revision` names, in which
    /// the entry it names is the one `inbound` maps to.
    ///
    /// R4 update replaces the resource named
    /// (<https://hl7.org/fhir/R4/http.html#update>), and every other entry of
    /// the program's archetype is a resource of its own, so the new version is
    /// the standing one with that entry's item replaced and every other item
    /// and field as it stood. A composition whose only entry is the updated
    /// one is the composition the resource maps to, built whole. No
    /// specification governs how one entry is spliced into a composition: our
    /// own design.
    ///
    /// # Errors
    ///
    /// Returns a [`Refused`] for a resource the program cannot map, a `404`
    /// when the standing composition no longer holds the entry, a composition
    /// the strict reader refuses, every CDR refusal through the status table
    /// (a `412` carrying the latest version as its `ETag`), and an identity
    /// store that cannot be written.
    pub async fn revise_resource(
        &self,
        inbound: &Inbound,
        program: &Loaded,
        revision: &Revision<'_>,
        provenance: &Provenance,
    ) -> Result<Written, Refused> {
        let linked = self.build_linked(program, inbound, provenance).await?;
        self.revise(inbound, program, revision, &linked)
            .await
            .map(|written| written.with_losses(linked.losses))
            .map_err(|refused| with_committed(refused, &linked.committed))
    }

    /// Commits the version of the composition `revision` names that splices
    /// in the composition of `linked`, after writing the subject it offers
    /// into the `EHR_STATUS`, and records the identity of `inbound`.
    async fn revise(
        &self,
        inbound: &Inbound,
        program: &Loaded,
        revision: &Revision<'_>,
        linked: &Built,
    ) -> Result<Written, Refused> {
        let built = &linked.composition;
        let offered = linked.subject.as_slice();
        let Revision {
            ehr_id,
            container,
            preceding,
            ..
        } = *revision;
        let composition = splice(program, built, revision)?;
        let rm = strict_read(&composition)?;
        Box::pin(self.write_subject(ehr_id, offered)).await?;
        let context = self.commit_context(commit::Change::Modification, &composition)?;
        let answered = self
            .client
            .update_composition(
                ehr_id,
                container,
                preceding,
                &rm,
                &context,
                Prefer::Representation,
            )
            .await
            .map_err(|error| cdr_refusal(&error))?;
        let (version, stored) = match answered.outcome {
            CompositionUpdateOutcome::Ok { body, headers } => {
                let version = crate::cdr::version_from_etag(
                    "composition_update",
                    StatusCode::OK,
                    headers.etag.as_deref(),
                )
                .map_err(|error| cdr_refusal(&error))?;
                let returned = crate::cdr::returned(Some(body), Prefer::Representation);
                (version, representation(returned, &composition)?)
            }
            CompositionUpdateOutcome::NoContent { headers } => {
                let version = crate::cdr::version_from_etag(
                    "composition_update",
                    StatusCode::NO_CONTENT,
                    headers.etag.as_deref(),
                )
                .map_err(|error| cdr_refusal(&error))?;
                (version, composition.clone())
            }
            CompositionUpdateOutcome::UnprocessableEntity { body } => {
                return Err(upstream_refusal(
                    status::UNPROCESSABLE,
                    StatusCode::UNPROCESSABLE_ENTITY,
                    &body,
                ));
            }
            CompositionUpdateOutcome::PreconditionFailed { body, headers } => {
                let latest = crate::cdr::optional_version_from_etag(
                    "composition_update",
                    headers.etag.as_deref(),
                )
                .map_err(|error| cdr_refusal(&error))?;
                let mut answer = status::Answer::new(
                    status::PRECONDITION_FAILED,
                    status::diagnostics(StatusCode::PRECONDITION_FAILED, &body),
                );
                if let Some(latest) = latest {
                    answer = answer.with_entity_tag(String::from(latest.version_tree_id().value()));
                }
                return Err(Refused::of_answer(&answer));
            }
            CompositionUpdateOutcome::NotFound { body } => {
                return Err(upstream_refusal(
                    status::NOT_FOUND,
                    StatusCode::NOT_FOUND,
                    &body,
                ));
            }
            CompositionUpdateOutcome::BadRequest { body } => {
                return Err(upstream_refusal(
                    status::BAD_REQUEST,
                    StatusCode::BAD_REQUEST,
                    &body,
                ));
            }
        };
        self.record_entry(
            program,
            inbound,
            ehr_id,
            &version,
            &stored,
            revision.entry_path,
        )
    }
}

/// Returns the composition an update of one entry commits: the standing
/// composition with that entry's item replaced by the one `built` carries.
///
/// The standing `uid` names the version the update follows, so the spliced
/// composition leaves it out, as a built one carries none (no specification
/// governs this: our own design).
fn splice(
    program: &Loaded,
    built: &CanonicalComposition,
    revision: &Revision<'_>,
) -> Result<CanonicalComposition, Refused> {
    let standing = canonical_of(revision.standing, built)?;
    let target = engine::entry::entry_named(program.program(), &standing, revision.entry_path)?
        .ok_or_else(|| {
            Refused::new(
                StatusCode::NOT_FOUND,
                Issue::error(IssueType::NotFound).diagnosing(format!(
                    "the composition version {} no longer holds the entry the resource names",
                    revision.preceding.value()
                )),
            )
        })?;
    if engine::entry::entries_of(program.program(), &standing).len() <= 1 {
        return Ok(built.clone());
    }
    let item = engine::entry::entry_of(program.program(), built)
        .and_then(|entry| entry.item(built).cloned())
        .ok_or_else(|| {
            Refused::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                Issue::error(IssueType::Processing)
                    .diagnosing("the resource maps to no entry of the archetype the program maps"),
            )
        })?;
    let spliced = target.replaced(&standing, item);
    let mut value = spliced.value().clone();
    if let Some(members) = value.as_object_mut() {
        members.remove("uid");
    }
    Ok(CanonicalComposition::new(
        value,
        spliced.template_id(),
        spliced.generation(),
    ))
}

/// Returns the composition a revision of `entry` by `built` would commit, or
/// nothing when `built` maps no entry of the program's archetype.
///
/// A replay is a create that would change nothing, so it is judged against
/// what [`Ingest::revise_resource`] commits: `sent`, the built composition,
/// when `whole` holds one entry of the archetype, and otherwise `whole` with
/// the entry's item replaced, so the entries beside it and the composition's
/// own fields stand as they are (no specification governs this: our own
/// design).
///
/// # Errors
///
/// Returns a `422` [`Refused`] when the spliced composition is no valid
/// openEHR `COMPOSITION`.
fn would_commit(
    program: &Loaded,
    whole: &CanonicalComposition,
    entry: &engine::entry::Entry,
    built: &CanonicalComposition,
    sent: Composition,
) -> Result<Option<Composition>, Refused> {
    if engine::entry::entries_of(program.program(), whole).len() <= 1 {
        return Ok(Some(sent));
    }
    let Some(item) = engine::entry::entry_of(program.program(), built)
        .and_then(|mapped| mapped.item(built).cloned())
    else {
        return Ok(None);
    };
    strict_read(&entry.replaced(whole, item)).map(Some)
}

/// Whether a create follows the `link` mappings of the resource it maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Linking {
    /// Each linked resource is committed first and its URI written.
    Commit,
    /// A `link` that names a resource refuses: the resource is itself a
    /// linked one, and one link level is followed per write.
    Refuse,
}
