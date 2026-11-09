// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One segment mapped to one resource, outside a message `Bundle`.
//!
//! A consumer that holds a parsed message and needs one segment as one
//! resource (a PID as the `Patient` a FHIR source would send) runs the
//! segment map from that segment to that resource alone: the guide's segment
//! map, its data type maps and its table maps, through the same run as
//! [`map`] and with the same typed outcomes. The resources a `(Type)` row
//! creates on the way are returned beside it, each with the `urn:uuid` full
//! url its reference names.
//!
//! [`map`]: crate::map::map

use fhir_types::codec::Value;

use crate::map::maps::Maps;
use crate::map::{MapError, Outcome, run};
use crate::parse::Parsed;
use ferrobridge_term::client::Client;

/// A refusal to map one segment.
#[derive(Debug, thiserror::Error)]
pub enum SegmentError {
    /// No placed segment is at the index named.
    #[error("no placed segment is at index {index} of the message")]
    Unplaced {
        /// The index in the message's segments.
        index: usize,
    },
    /// The corpus holds none or several segment maps from the segment to
    /// the resource named.
    #[error("no single segment map maps {segment} to {resource}: {candidates:?}")]
    NoSegmentMap {
        /// The segment id.
        segment: String,
        /// The resource type.
        resource: String,
        /// The ids of the qualified candidates, none or several.
        candidates: Vec<String>,
    },
    /// The run refused the segment.
    #[error(transparent)]
    Map(#[from] MapError),
}

/// One resource a single-segment map built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    /// The resource type.
    pub resource_type: String,
    /// The `urn:uuid` full url a reference to it names.
    pub full_url: String,
    /// The resource.
    pub resource: Value,
}

/// A segment mapped to one resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segmented {
    resource: Option<Built>,
    created: Vec<Built>,
    outcomes: Vec<Outcome>,
    translations: usize,
}

impl Segmented {
    /// Returns the resource the segment mapped to; `None` when it was left
    /// out for lacking a required element or not decoding as R4, which
    /// [`Segmented::outcomes`] then records.
    #[must_use]
    pub const fn resource(&self) -> Option<&Built> {
        self.resource.as_ref()
    }

    /// Returns the resources a `(Type)` row created, in the order the run
    /// created them.
    #[must_use]
    pub fn created(&self) -> &[Built] {
        &self.created
    }

    /// Returns every outcome, in the order the run met them: the parse
    /// outcomes of the message first.
    #[must_use]
    pub fn outcomes(&self) -> &[Outcome] {
        &self.outcomes
    }

    /// Returns how many translations the run asked for.
    #[must_use]
    pub const fn translations(&self) -> usize {
        self.translations
    }
}

/// Maps the placed segment at `index` of `parsed` to one `resource_type`
/// through the segment map `corpus` holds from that segment to it,
/// translating table values through `translator` as [`map`] does.
///
/// # Errors
///
/// Returns [`SegmentError::Unplaced`] when no placed segment is at `index`,
/// [`SegmentError::NoSegmentMap`] when the corpus holds none or several
/// segment maps from the segment to `resource_type`, and
/// [`SegmentError::Map`] with [`MapError::Stopped`] when a `NOT VALUED ERROR`
/// condition holds or [`MapError::Terminology`] when the terminology server
/// refuses a translation or cannot be reached.
///
/// [`map`]: crate::map::map
pub async fn segment(
    parsed: &Parsed,
    index: usize,
    resource_type: &str,
    corpus: &Maps,
    translator: Option<&Client>,
) -> Result<Segmented, SegmentError> {
    let mut run = run::Run::new(corpus, parsed);
    let root = run.single(index, resource_type)?;
    let translations = run.translate(translator).await?;
    let (entries, outcomes) = run.resources();
    let mut resource = None;
    let mut created = Vec::new();
    for (resource_type, full_url, document) in entries {
        let built = Built {
            resource_type,
            full_url,
            resource: document,
        };
        if built.full_url == root {
            resource = Some(built);
        } else {
            created.push(built);
        }
    }
    Ok(Segmented {
        resource,
        created,
        outcomes,
        translations,
    })
}
