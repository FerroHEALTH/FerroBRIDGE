// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One FHIR resource written back as one v2 segment through the same maps.
//!
//! The v2-to-FHIR guide maps v2 to FHIR only, so this inverse is our own
//! design over its rows, read in the order the guide writes them:
//!
//! - A row reads its target and writes its source. A complex value runs the
//!   inverse of the data type map from the source type to the target type,
//!   and a primitive placed at a complex source fills its first component.
//! - A row whose assignment is a literal is a match, never a write: the FHIR
//!   instance holds that literal (`extension[1].url` selects the mother's
//!   maiden name extension). With no condition the match is required; with
//!   a condition it is a preference, since the literal is written only when
//!   that condition holds (`telecom[1].use` is `home` only when PID-13.2 is
//!   empty).
//! - Each instance of a repeating FHIR element goes to the first label, in
//!   row order, whose required matches it holds and whose preferences it
//!   holds, else to the first whose required matches it holds. A label of a
//!   segment map takes as many instances as its field repeats; a label of a
//!   data type map takes one.
//! - A field or component the tables mark `B` (backward compatibility) or `W`
//!   (withdrawn), in its own version or in v2.9.1, yields to a current one
//!   that writes the same element back: identifiers go to PID-3 and never
//!   PID-2, and a phone number to XTN-12 and never XTN-1. A deprecated field
//!   still serves where no current one of the version does, as PID-13 and
//!   PID-14 in v2.5, which has no PID-40.
//! - A row with a computable condition runs after the rows with none, and
//!   only when its condition holds over the v2 value as built and its target
//!   element is not already written back.
//! - A table value translates back through a table map from the FHIR system
//!   to the v2 one when the corpus holds one (the shipped supplements), else
//!   through the guide's map with `reverse`; several codes with none
//!   preferred are [`Outcome::Ambiguous`], never a guess.
//!
//! Every FHIR element no row writes back is an [`Outcome`], counted and
//! never dropped.

mod grid;
mod line;
mod walk;

use std::collections::BTreeSet;

use fhir_types::codec::Value;
use hl7v2_types::model::Max;

use crate::map::convert;
use crate::map::inverse::grid::Grid;
use crate::map::inverse::line::{Line, deprecated, lines, order, primitives};
use crate::map::inverse::walk::Inverse;
use crate::map::maps::Maps;
use crate::write::value::{Field, Segment};
use ferrobridge_term::client::Client;

/// Something the inverse did not write back, typed and counted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A FHIR element no row writes back, by its path in the resource.
    Element {
        /// The path, for example `telecom[2]` or `name[0].text`.
        path: String,
    },
    /// A table value no map translates back.
    Untranslated {
        /// The element's path.
        path: String,
        /// The canonical url of the guide's table map.
        concept_map: String,
    },
    /// A table value that translates back to several codes, none of them
    /// preferred.
    Ambiguous {
        /// The element's path.
        path: String,
        /// The canonical url of the map that answered.
        concept_map: String,
        /// The codes, in the map's order.
        codes: Vec<String>,
    },
    /// A table value reached with no translator.
    NoTranslator {
        /// The element's path.
        path: String,
        /// The canonical url of the guide's table map.
        concept_map: String,
    },
    /// A FHIR date or time the v2 type of its source cannot hold.
    Unconvertible {
        /// The element's path.
        path: String,
        /// Why.
        error: convert::ConvertError,
    },
}

impl Outcome {
    /// Returns a short name of the outcome's kind, for a count.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Element { .. } => "element",
            Self::Untranslated { .. } => "untranslated",
            Self::Ambiguous { .. } => "ambiguous",
            Self::NoTranslator { .. } => "no-translator",
            Self::Unconvertible { .. } => "unconvertible",
        }
    }
}

/// A refusal to write a resource back as a segment.
#[derive(Debug, thiserror::Error)]
pub enum InverseError {
    /// The value is no FHIR resource: no object with a `resourceType`.
    #[error("the value is no FHIR resource")]
    NotAResource,
    /// The corpus holds none or several segment maps from the segment to
    /// the resource's type.
    #[error("no single segment map maps {segment} to {resource}: {candidates:?}")]
    NoSegmentMap {
        /// The segment id.
        segment: String,
        /// The resource type.
        resource: String,
        /// The ids of the qualified candidates.
        candidates: Vec<String>,
    },
    /// The terminology server refused a translation or could not be reached.
    #[error("the reverse translation through {concept_map} failed")]
    Terminology {
        /// The canonical url of the map asked.
        concept_map: String,
        /// The cause, with the upstream status and body.
        #[source]
        source: Box<ferrobridge_term::error::Error>,
    },
}

/// A resource written back as a segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inverted {
    segment: Segment,
    outcomes: Vec<Outcome>,
    translations: usize,
}

impl Inverted {
    /// Returns the segment, to write with [`crate::write::Writer`].
    #[must_use]
    pub const fn segment(&self) -> &Segment {
        &self.segment
    }

    /// Returns the segment, consuming the result.
    #[must_use]
    pub fn into_segment(self) -> Segment {
        self.segment
    }

    /// Returns every outcome, in the order the inverse met them.
    #[must_use]
    pub fn outcomes(&self) -> &[Outcome] {
        &self.outcomes
    }

    /// Returns how many translations the inverse asked for.
    #[must_use]
    pub const fn translations(&self) -> usize {
        self.translations
    }
}

/// Writes `resource` back as the segment `definition` describes.
///
/// The resource runs back through the segment map `corpus` holds from that
/// segment to the resource's type, its data type maps and its table maps,
/// translating through `translator`.
///
/// `definition` is the segment table of the version the segment is written
/// in (`hl7v2_types::segment::find` for v2.9.1, or the segment a structure
/// of `hl7v2_types::legacy` places), whose field optionality decides which
/// fields are written.
///
/// # Errors
///
/// Returns [`InverseError::NotAResource`] when `resource` is no FHIR
/// resource, [`InverseError::NoSegmentMap`] when the corpus holds none or
/// several segment maps to its type, and [`InverseError::Terminology`] when
/// the terminology server refuses a translation or cannot be reached.
pub async fn segment(
    resource: &Value,
    definition: &'static hl7v2_types::model::Segment,
    corpus: &Maps,
    translator: Option<&Client>,
) -> Result<Inverted, InverseError> {
    let resource_type = resource
        .get("resourceType")
        .and_then(Value::as_str)
        .ok_or(InverseError::NotAResource)?;
    let map = corpus
        .find("segment", definition.id, resource_type)
        .map_err(|candidates| InverseError::NoSegmentMap {
            segment: String::from(definition.id),
            resource: String::from(resource_type),
            candidates,
        })?;
    let mut inverse = Inverse {
        corpus,
        translator,
        outcomes: Vec::new(),
        translations: 0,
        consumed: BTreeSet::new(),
    };
    let latest = hl7v2_types::segment::find(definition.id);
    let lines = lines(map, &|position| {
        let index = position.checked_sub(1)?;
        let field = definition.fields.get(index)?;
        let withdrawn = latest
            .and_then(|latest| latest.fields.get(index))
            .is_some_and(|later| deprecated(later.optionality));
        Some((
            deprecated(field.optionality) || withdrawn,
            field.data_type.map(|data| data.code()),
        ))
    });
    let capacity = |line: &Line<'_>| {
        definition
            .fields
            .get(
                line.source
                    .first()
                    .copied()
                    .unwrap_or_default()
                    .saturating_sub(1),
            )
            .map_or(1, |field| match field.cardinality.max {
                Max::Unbounded => usize::MAX,
                Max::Bounded(bound) => usize::try_from(bound).unwrap_or(usize::MAX),
            })
    };
    let refs: Vec<&Line<'_>> = lines.iter().collect();
    let leaves = inverse.assign(&refs, 0, resource, "", None, &capacity, true);
    let mut fields = inverse.fields(order(leaves), definition.id).await?;
    let mut unwritten = Vec::new();
    primitives(resource, "", true, &mut unwritten);
    for path in unwritten {
        if !inverse.consumed.contains(&path) {
            inverse.outcomes.push(Outcome::Element { path });
        }
    }
    let last = fields.keys().next_back().copied().unwrap_or_default();
    let written: Vec<Field> = (1..=last)
        .map(|position| {
            fields
                .remove(&position)
                .unwrap_or_default()
                .into_iter()
                .filter(|grid| !grid.is_empty())
                .map(Grid::into_repetition)
                .collect()
        })
        .collect();
    Ok(Inverted {
        segment: Segment::new(definition.id, written),
        outcomes: inverse.outcomes,
        translations: inverse.translations,
    })
}
