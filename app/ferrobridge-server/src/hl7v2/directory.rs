// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The face's `ConceptMap` directory as the operator API answers it, with
//! the messages each map served.
//!
//! The listing is read from the corpus once, when the face starts; the
//! counts grow by one per map each time a message's mapping applies it
//! (`ferrobridge_hl7v2::map::Mapped::applied`), whether or not the ingest
//! then commits the message, and start again from zero with the process.
//! No specification governs the directory: our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use ferrobridge_hl7v2::map::corpus::{Corpus, Kind, Map, Origin};
use ferrobridge_operator_api::hl7v2 as document;

use crate::config::Hl7v2Settings;

/// The directory of one running face.
#[derive(Debug)]
pub struct Directory {
    /// The directory and supplements of the settings, with no counts.
    listing: document::ConceptMaps,
    /// The messages each map served, by id.
    served: Mutex<BTreeMap<String, u64>>,
}

impl Directory {
    /// Returns the directory of `corpus`, loaded as `settings` describes,
    /// counting from the instant `since`.
    #[must_use]
    pub fn new(corpus: &Corpus, settings: &Hl7v2Settings, since: jiff::Timestamp) -> Self {
        let text = |path: &std::path::Path| path.display().to_string();
        Self {
            listing: document::ConceptMaps {
                directory: text(&settings.concept_maps),
                supplements: settings.supplements.iter().map(|path| text(path)).collect(),
                counted_since: since.to_string(),
                maps: corpus.maps().map(listed).collect(),
            },
            served: Mutex::new(BTreeMap::new()),
        }
    }

    /// Counts one message whose mapping applied the maps `applied`.
    pub fn served(&self, applied: &BTreeSet<String>) {
        let mut served = self.lock();
        for id in applied {
            let count = served.entry(id.clone()).or_insert(0);
            *count = count.saturating_add(1);
        }
    }

    /// Returns the document of the directory as the face holds it now.
    #[must_use]
    pub fn document(&self) -> document::ConceptMaps {
        let served = self.lock();
        let mut document = self.listing.clone();
        for map in &mut document.maps {
            map.messages = served.get(&map.id).copied().unwrap_or(0);
        }
        document
    }

    /// The counts, held only for an increment or a copy.
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, u64>> {
        // NOTE: no specification governs this: our own design; the lock only guards
        // whole increments, so a poisoned lock still holds a consistent count.
        self.served
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The listing of one map, with no messages counted.
fn listed(map: &Map) -> document::ConceptMap {
    document::ConceptMap {
        id: map.id.clone(),
        url: map.url.clone(),
        title: map.title.clone(),
        kind: match map.kind {
            Kind::Message => document::Kind::Message,
            Kind::Segment => document::Kind::Segment,
            Kind::Datatype => document::Kind::Datatype,
            Kind::Table => document::Kind::Table,
            Kind::Other => document::Kind::Other,
        },
        origin: match map.origin {
            Origin::Guide => document::Origin::Guide,
            Origin::Supplement { overrides: false } => document::Origin::Supplement,
            Origin::Supplement { overrides: true } => document::Origin::Override,
        },
        ends: map
            .ends()
            .into_iter()
            .map(|(source, target)| document::Ends { source, target })
            .collect(),
        rows: u64::try_from(map.rows.len()).unwrap_or(u64::MAX),
        messages: 0,
    }
}
