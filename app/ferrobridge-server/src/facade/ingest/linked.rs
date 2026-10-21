// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The linked compositions of one write, committed before the composition
//! that links them.
//!
//! A `link` whose FHIR side names a resource runs that resource's context
//! "and [saves it] as a separate composition", linked through
//! `LINK.target` (`types-of-mappings/concept-type/concept-mappings.adoc`,
//! §Linked mappings). `LINK.target` is a `DV_EHR_URI`, and the id of a new
//! composition is the CDR's to assign: ITS-REST 1.1.0 gives a
//! `CONTRIBUTION` version (`UPDATE_VERSION`) no `uid`, and `composition_create`
//! names none in the body (`ehr-codegen.openapi.yaml`). So the write runs in
//! two passes: a probe run that commits nothing finds every linked resource,
//! each is committed through the single-create path a `POST` takes, and the
//! final run writes their URIs. No specification governs the two passes: our
//! own design.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::PoisonError;

use fhir_types::codec::Value;
use fhirconnect::engine::linked::Linked;
use fhirconnect::engine::linked::LinkedCompositions;
use fhirconnect::engine::seam::ReferenceError;
use fhirconnect::engine::subject::SubjectIdentifier;
use http::StatusCode;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_rm::v1_2::paths::EhrUri;

use crate::cdr::ids::versioned_object_uid;
use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::outcome::chain;
use crate::facade::programs::Loaded;
use crate::facade::references;
use crate::facade::request::Inbound;

use super::Ingest;
use super::Provenance;
use super::Recognised;
use super::Refused;
use super::engine_refusal;
use super::store_refusal;

/// The URI a probe run writes for every linked composition; the run is
/// discarded, so it never reaches the CDR.
const PROBE: &str =
    "ehr:/00000000-0000-4000-8000-000000000000/compositions/00000000-0000-4000-8000-000000000000";

/// The answers one run of a linking resource reads.
#[derive(Debug, Default)]
pub(crate) struct Linker {
    /// Where each linked resource's composition stands, by literal
    /// reference.
    answers: BTreeMap<String, Linked>,
    /// The linked resources this run met that [`Self::answers`] does not
    /// hold, in the order met.
    asked: Mutex<Vec<(String, Value)>>,
}

impl Linker {
    /// Returns the linker of a probe run, which answers every linked
    /// resource with [`PROBE`] and records it.
    pub(crate) fn probing() -> Self {
        Self::default()
    }

    /// Returns the linker of the final run, which answers from `answers`.
    fn answering(answers: BTreeMap<String, Linked>) -> Self {
        Self {
            answers,
            asked: Mutex::default(),
        }
    }

    /// Returns the linked resources the run met that it held no answer for.
    fn asked(self) -> Vec<(String, Value)> {
        self.asked
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl LinkedCompositions for Linker {
    fn composition(&self, reference: &str, resource: &Value) -> Result<Linked, ReferenceError> {
        if let Some(answer) = self.answers.get(reference) {
            return Ok(answer.clone());
        }
        let mut asked = self.asked.lock().unwrap_or_else(PoisonError::into_inner);
        if !asked.iter().any(|(held, _)| held == reference) {
            asked.push((String::from(reference), resource.clone()));
        }
        PROBE
            .parse::<EhrUri>()
            .map(Linked::At)
            .map_err(|source| ReferenceError::Fetch {
                reference: String::from(reference),
                source: Box::new(source),
            })
    }
}

/// A linked composition this write committed.
#[derive(Debug, Clone)]
pub(super) struct CommittedLink {
    /// The literal reference that named the linked resource.
    pub(super) reference: String,
    /// The version the commit produced.
    pub(super) version: ObjectVersionId,
}

/// The composition a linking resource maps to, and what it committed first.
#[derive(Debug)]
pub(super) struct Built {
    /// The composition, with every linked composition's URI written.
    pub(super) composition: CanonicalComposition,
    /// The linked compositions this write committed, in the order met.
    pub(super) committed: Vec<CommittedLink>,
    /// The identifiers the run offered as the subject of the EHR
    /// (`fhirconnect::engine::subject`).
    pub(super) subject: Vec<SubjectIdentifier>,
}

impl Ingest<'_> {
    /// Runs the engine over `inbound`, committing each linked composition its
    /// `link` mappings name before the final run writes their URIs.
    ///
    /// A linked resource this server already serves is linked where it
    /// stands; any other is selected by its `meta.profile` and committed as a
    /// create would commit it, so a replay links the composition the first
    /// delivery produced. A resource no context maps is a declared loss
    /// (`SkipReason::UnmappedLink`). The linked composition's own links are
    /// not followed: a `link` in it that names a resource refuses, one link
    /// level per write (no specification governs this: our own design).
    ///
    /// # Errors
    ///
    /// Returns the [`Refused`] of the probe or the final run, of a linked
    /// resource that cannot be read, selected or committed, a `409` for a
    /// linked resource the map consumed with other content, and a `500` when
    /// the final run meets a linked resource the probe did not.
    pub(super) async fn build_linked(
        &self,
        program: &Loaded,
        inbound: &Inbound,
        provenance: &Provenance,
    ) -> Result<Built, Refused> {
        // NOTE: ITS-REST 1.1.0 `UpdateVersion` carries no `uid`, so a linked composition's
        // id exists only after its commit, and the linking run waits for it (our own design).
        let probe = Linker::probing();
        let (probed, _, _, offered) = self
            .run_linking(program, inbound, provenance, Some(&probe))
            .await
            .map_err(|error| engine_refusal(&error))?;
        let asked = probe.asked();
        if asked.is_empty() {
            return Ok(Built {
                composition: probed,
                committed: Vec::new(),
                subject: offered,
            });
        }
        let mut answers = BTreeMap::new();
        let mut committed = Vec::new();
        for (reference, resource) in asked {
            let (linked, version) = self
                .commit_linked(&reference, resource)
                .await
                .map_err(|refused| with_committed(refused, &committed))?;
            if let Some(version) = version {
                committed.push(CommittedLink {
                    reference: reference.clone(),
                    version,
                });
            }
            answers.insert(reference, linked);
        }
        let last = Linker::answering(answers);
        let (composition, _, _, subject) = self
            .run_linking(program, inbound, provenance, Some(&last))
            .await
            .map_err(|error| with_committed(engine_refusal(&error), &committed))?;
        if let Some((reference, _)) = last.asked().first() {
            return Err(with_committed(
                Refused::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Issue::error(IssueType::Exception).diagnosing(format!(
                        "the final run linked {reference}, which its probe run never met"
                    )),
                ),
                &committed,
            ));
        }
        Ok(Built {
            composition,
            committed,
            subject,
        })
    }

    /// Returns where the composition of one linked resource stands, and the
    /// version this write committed for it, when it committed one.
    async fn commit_linked(
        &self,
        reference: &str,
        resource: Value,
    ) -> Result<(Linked, Option<ObjectVersionId>), Refused> {
        if let Some((resource_type, id)) = references::local(reference, &self.settings.base_url) {
            let binding = self
                .store
                .binding_of(&resource_type, &id)
                .map_err(|error| store_refusal(&error))?;
            if let Some(binding) = binding {
                let uri =
                    composition_uri(reference, &binding.ehr_id, &binding.versioned_object_uid)?;
                return Ok((Linked::At(uri), None));
            }
        }
        let linked = Inbound::of(resource, "").map_err(|error| {
            Refused::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                Issue::error(IssueType::Invalid).diagnosing(format!(
                    "the resource {reference} links cannot be read: {}",
                    chain(&error)
                )),
            )
        })?;
        if self.programs.candidates(linked.profiles()).is_empty() {
            return Ok((Linked::Unmapped, None));
        }
        let program = super::select(&self.programs, &linked, None)?;
        let claim = self.claim_resource(&linked)?;
        let written = match self
            .recognise(&claim, &linked, program, &Provenance::EachResource)
            .await?
        {
            Recognised::Replayed(written) => written,
            Recognised::Revised(_) => {
                return Err(Refused::new(
                    StatusCode::CONFLICT,
                    Issue::error(IssueType::Conflict).diagnosing(format!(
                        "the resource {reference} links was committed before with other \
                         content; update it on its own before linking it"
                    )),
                ));
            }
            Recognised::Unknown => {
                Box::pin(self.ingest_unlinked(&claim, &linked, program, &Provenance::EachResource))
                    .await?
            }
        };
        let committed =
            matches!(written.delivery, super::Delivery::Committed).then(|| written.version.clone());
        let uri = composition_uri(
            reference,
            written.ehr_id.as_str(),
            versioned_object_uid(&written.version).value(),
        )?;
        Ok((Linked::At(uri), committed))
    }
}

/// Returns the `ehr:` URI of the versioned composition `container` in the
/// EHR `ehr_id`.
///
/// BASE `paths` §EHR URIs: `ehr:/<ehr_id>/<top_level_structure_locator>`,
/// the locator named by the `EHR` attribute (`compositions`) and the
/// versioned object, and "when a URI uses the object identifier, the latest
/// trunk version is always assumed", so a later version of the linked
/// composition keeps the link.
fn composition_uri(reference: &str, ehr_id: &str, container: &str) -> Result<EhrUri, Refused> {
    format!("ehr:/{ehr_id}/compositions/{container}")
        .parse::<EhrUri>()
        .map_err(|error| {
            Refused::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                Issue::error(IssueType::Exception).diagnosing(format!(
                    "the composition {reference} was committed as names no ehr: URI: {error}"
                )),
            )
        })
}

/// Returns `refused` with one issue per linked composition this write
/// committed before it refused, so nothing committed goes unreported.
pub(super) fn with_committed(refused: Refused, committed: &[CommittedLink]) -> Refused {
    committed.iter().fold(refused, |refused, link| {
        refused.with_issue(Issue::warning(IssueType::Informational).diagnosing(format!(
            "the linked composition of {} was committed as {} before this write was refused",
            link.reference,
            link.version.value()
        )))
    })
}

/// The linked compositions of a Bundle entry, which the Bundle path refuses.
///
/// A transaction commits its entries in one `CONTRIBUTION` and answers all
/// or nothing (<https://hl7.org/fhir/R4/http.html#transaction>), and the id
/// of a linked composition is known only once the CDR has committed it, so
/// a `link` naming a resource cannot be written inside one. No
/// specification governs the refusal: our own design.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct InBundle;

impl LinkedCompositions for InBundle {
    fn composition(&self, reference: &str, _resource: &Value) -> Result<Linked, ReferenceError> {
        Err(ReferenceError::Link {
            reference: String::from(reference),
            source: Box::new(BundleLink),
        })
    }
}

/// Why a Bundle entry's `link` to a resource is refused.
#[derive(Debug, thiserror::Error)]
#[error(
    "a Bundle commits its entries in one CONTRIBUTION, and a linked composition's id is known \
     only once the CDR commits it; send the linking resource on its own"
)]
pub(super) struct BundleLink;
