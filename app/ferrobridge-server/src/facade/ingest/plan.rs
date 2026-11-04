// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a Bundle write plans before it commits: each mapped entry, where it
//! is placed, and what stands recorded for it.

use super::binding::canonical_of;
use super::{Consumed, Delivery, Refused, Skipped, Written};
use crate::cdr::ids::EhrId;
use crate::facade::commit;
use crate::facade::identity::FhirResourceId;
use crate::facade::identity::PersonId;
use crate::facade::identity::record::CommittedSource;
use crate::facade::identity::record::ConsumedSource;
use crate::facade::outcome::Issue;
use crate::facade::programs::Loaded;
use crate::facade::request::Inbound;
use fhirconnect::engine::outcome::Warning;
use fhirconnect::engine::subject::SubjectIdentifier;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_rm::v1_2::composition::composition::Composition;

/// One Bundle entry that mapped, ready to commit.
#[derive(Debug)]
pub(super) struct Mapped {
    /// The `fullUrl` the Bundle gave the entry, for a refusal that names it.
    pub(super) full_url: String,
    /// The entry's position in the Bundle.
    pub(super) position: usize,
    /// The resource the entry carries.
    pub(super) inbound: Inbound,
    /// The program the entry ran.
    pub(super) program: Loaded,
    /// The template of the composition the entry produced.
    pub(super) template_id: String,
    /// The composition as the engine built it.
    pub(super) built: CanonicalComposition,
    /// The composition as the strict reader read it back.
    pub(super) composition: Box<Composition>,
    /// The instant the engine defaulted every clock-derived time from.
    pub(super) instant: String,
    /// The literal references whose resources a `slotContext` consumed.
    pub(super) consumed: Vec<String>,
    /// The identifiers the entry offers as the subject of the EHR.
    pub(super) offered: Vec<SubjectIdentifier>,
    /// The losses the entry's mapping declared.
    pub(super) losses: Vec<Warning>,
}

/// Which versions of a committed contribution the entries being bound
/// account for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    /// Every version is the composition of one of the entries.
    Whole,
    /// The entries are some of the versions, as one resource of a
    /// transaction is; a version no entry matches is passed over.
    Part,
}

/// What mapping one Bundle entry produced.
#[derive(Debug)]
pub(super) enum Mapping {
    /// The entry mapped, for the subject it names.
    Mapped(Box<Mapped>, PersonId),
    /// No program ran: the skip, and the issue that refuses the Bundle.
    Unmapped(Skipped, Issue),
}

/// Where one entry of a Bundle stands before the commit.
#[derive(Debug)]
pub(super) enum Pending {
    /// The entry mapped: its index among the mapped entries.
    Mapped(usize),
    /// No program ran over the entry.
    Skipped(Skipped),
    /// Another entry's mapping consumed the entry.
    Consumed(Consumed),
}

/// Every entry of a Bundle, mapped and partitioned.
#[derive(Debug, Default)]
pub(super) struct Partition {
    /// The entries that mapped, in Bundle order.
    pub(super) mapped: Vec<Mapped>,
    /// One slot per entry that mapped or was skipped, in Bundle order.
    pub(super) pending: Vec<Pending>,
    /// The issues that refuse the Bundle.
    pub(super) failures: Vec<Issue>,
    /// The one subject every mapped entry names.
    pub(super) subject: Option<PersonId>,
}

/// Where one committed entry stands.
#[derive(Debug)]
pub(super) struct Placed {
    /// The logical id the entry's resource has.
    pub(super) id: FhirResourceId,
    /// The composition version the entry stands at.
    pub(super) version: ObjectVersionId,
    /// The change this delivery committed for the entry, when it committed
    /// one.
    pub(super) change: Option<commit::Change>,
}

/// What the identity map records of one Bundle entry's source.
#[derive(Debug, Default)]
pub(super) struct Recorded {
    /// What consumed the exact source.
    pub(super) consumed: Option<ConsumedSource>,
    /// The contribution that committed the exact source.
    pub(super) committed: Option<CommittedSource>,
    /// What consumed another version of the source's `id`, whose composition
    /// the entry revises.
    pub(super) revised: Option<ConsumedSource>,
}

/// Where the composition a consumed source produced stands now.
#[derive(Debug)]
pub(super) struct Standing {
    /// The EHR the composition lives in.
    pub(super) ehr_id: EhrId,
    /// The logical id the resource has.
    pub(super) id: FhirResourceId,
    /// The version the composition stands at.
    pub(super) version: ObjectVersionId,
    /// The composition the CDR holds at that version.
    pub(super) composition: Composition,
}

impl Standing {
    /// Returns the answer of a write that found this composition and
    /// committed nothing, with `built` as the mapping the answer renders
    /// through.
    pub(super) fn written(self, built: &CanonicalComposition) -> Result<Written, Refused> {
        let composition = canonical_of(&self.composition, built)?;
        Ok(self.replayed(composition))
    }

    /// Returns the answer of a write that found this composition and
    /// committed nothing, rendering `composition`.
    pub(super) fn replayed(self, composition: CanonicalComposition) -> Written {
        Written {
            id: self.id,
            ehr_id: self.ehr_id,
            version: self.version,
            composition,
            delivery: Delivery::Replayed,
            losses: Vec::new(),
        }
    }
}
