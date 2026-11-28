// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The replica's copy of the identity map's record-once rows.
//!
//! The rows it holds (a person's EHR, an EHR's sealed person, a sender's id
//! and a resource's binding) are written once and never updated, so a row
//! found once stays true: a copy needs no expiry, only a bound on its size,
//! and it drops its oldest entry first. A row the store did not hold is
//! never copied, since another replica may write it next. The copy serves
//! only while the replica's `LISTEN` session is connected, and every connect
//! or loss of that session drops it whole, as the state held in memory does
//! everywhere else (`crate::store::listen`). No specification governs the
//! copy: our own design.

use core::fmt;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::cdr::ids::EhrId;
use crate::identity::FhirResourceId;
use crate::identity::record::CompositionBinding;
use crate::store::listen::{Dropping, IDENTITY, Listened, Subscriber};
use crate::store::seal::KeyId;

/// A map of at most `capacity` entries that drops the oldest entry first.
///
/// An entry is never replaced: the rows it copies are written once.
pub struct Bounded<K, V> {
    /// The most entries it holds; `0` holds none.
    capacity: usize,
    /// The entries and their insertion order.
    entries: Mutex<Entries<K, V>>,
}

/// The entries of a [`Bounded`] map, each with the sequence number it was
/// put under, and the keys in that order.
struct Entries<K, V> {
    /// Each key's sequence number and value.
    values: BTreeMap<K, (u64, V)>,
    /// Each sequence number's key, oldest first.
    order: BTreeMap<u64, K>,
    /// The next sequence number.
    next: u64,
}

impl<K: Ord + Clone, V: Clone> Bounded<K, V> {
    /// Returns an empty map of at most `capacity` entries.
    #[must_use]
    pub const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Mutex::new(Entries {
                values: BTreeMap::new(),
                order: BTreeMap::new(),
                next: 0,
            }),
        }
    }

    /// Returns the entries, whatever a panicking holder left: an entry is
    /// either whole or absent, so a poisoned map is still a correct copy.
    fn entries(&self) -> std::sync::MutexGuard<'_, Entries<K, V>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns the value of `key`, when the map holds it.
    #[must_use]
    pub fn get(&self, key: &K) -> Option<V> {
        self.entries()
            .values
            .get(key)
            .map(|(_, value)| value.clone())
    }

    /// Puts `value` under `key` when the map does not hold `key`, dropping
    /// the oldest entry when the map is full.
    pub fn put(&self, key: K, value: V) {
        self.put_if(key, value, || true);
    }

    /// Puts `value` under `key` as [`Self::put`] does, when `holds` answers
    /// `true` while the map is locked, so no change the caller counts can
    /// land between the check and the entry.
    pub fn put_if(&self, key: K, value: V, holds: impl FnOnce() -> bool) {
        if self.capacity == 0 {
            return;
        }
        let mut entries = self.entries();
        if !holds() || entries.values.contains_key(&key) {
            return;
        }
        while entries.values.len() >= self.capacity {
            let Some((_, oldest)) = entries.order.pop_first() else {
                break;
            };
            entries.values.remove(&oldest);
        }
        let sequence = entries.next;
        entries.next = sequence.wrapping_add(1);
        entries.order.insert(sequence, key.clone());
        entries.values.insert(key, (sequence, value));
    }

    /// Drops the entry of `key`, when the map holds one.
    pub fn remove(&self, key: &K) {
        let mut entries = self.entries();
        if let Some((sequence, _)) = entries.values.remove(key) {
            entries.order.remove(&sequence);
        }
    }

    /// Returns how many entries the map holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries().values.len()
    }

    /// Returns whether the map holds no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops every entry.
    pub fn clear(&self) {
        let mut entries = self.entries();
        entries.values.clear();
        entries.order.clear();
    }
}

impl<K, V> fmt::Debug for Bounded<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bounded")
            .field("capacity", &self.capacity)
            .finish_non_exhaustive()
    }
}

/// A person as the state store seals it: the id of the key that sealed it
/// and the sealed bytes, opened on each use.
#[derive(Clone)]
pub struct SealedPerson {
    /// The id of the sealing key.
    pub key_id: KeyId,
    /// The sealed person.
    pub sealed: Vec<u8>,
}

impl fmt::Debug for SealedPerson {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SealedPerson")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

/// The copied rows, one bounded map per table.
pub struct Rows {
    /// A person, as its namespace and the digest of its id, to its EHR.
    pub ehrs: Bounded<(String, Vec<u8>), EhrId>,
    /// An EHR to the person it was first recorded for, sealed.
    pub subjects: Bounded<String, SealedPerson>,
    /// A resource type and a sender's id to the internal id.
    pub internals: Bounded<(String, String), FhirResourceId>,
    /// A resource type and an internal id to its binding.
    pub bindings: Bounded<(String, String), CompositionBinding>,
}

impl Rows {
    /// Returns empty maps of at most `entries` entries each.
    #[must_use]
    pub const fn new(entries: usize) -> Self {
        Self {
            ehrs: Bounded::new(entries),
            subjects: Bounded::new(entries),
            internals: Bounded::new(entries),
            bindings: Bounded::new(entries),
        }
    }

    /// Drops every entry of every map.
    pub fn clear(&self) {
        self.ehrs.clear();
        self.subjects.clear();
        self.internals.clear();
        self.bindings.clear();
    }
}

impl fmt::Debug for Rows {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rows")
            .field("ehrs", &self.ehrs.len())
            .field("subjects", &self.subjects.len())
            .field("internals", &self.internals.len())
            .field("bindings", &self.bindings.len())
            .finish()
    }
}

impl Subscriber for Rows {
    fn notified<'a>(&'a self, _payload: &'a str) -> Dropping<'a> {
        Box::pin(async move { self.clear() })
    }

    fn reset(&self) -> Dropping<'_> {
        Box::pin(async move { self.clear() })
    }
}

/// The copy and the listener state that says whether it may serve.
#[derive(Debug, Clone)]
pub struct IdentityCopy {
    /// The copied rows.
    rows: Arc<Rows>,
    /// The listener on [`IDENTITY`], which drops the rows on every connect
    /// and loss of the session.
    listened: Arc<Listened>,
}

impl IdentityCopy {
    /// Returns an empty copy of at most `entries` entries per table, which
    /// serves nothing until its listener connects; `None` for `0`.
    #[must_use]
    pub fn new(entries: usize) -> Option<Self> {
        if entries == 0 {
            return None;
        }
        let rows = Arc::new(Rows::new(entries));
        let subscriber: Arc<dyn Subscriber> = rows.clone();
        let listened = Arc::new(Listened::with_subscriber(IDENTITY.name(), subscriber));
        Some(Self { rows, listened })
    }

    /// Returns the rows while the listener is connected, and `None` while it
    /// is not, so every lookup then reads the store.
    #[must_use]
    pub fn serving(&self) -> Option<&Rows> {
        self.listened.is_listening().then_some(self.rows.as_ref())
    }

    /// Returns the listener state the replica's `LISTEN` session carries.
    #[must_use]
    pub fn listened(&self) -> Arc<Listened> {
        Arc::clone(&self.listened)
    }
}

#[cfg(test)]
mod tests {
    use super::{Bounded, IdentityCopy, Rows};
    use crate::store::listen::Subscriber as _;

    #[test]
    fn a_full_map_drops_its_oldest_entry_first() {
        let map = Bounded::new(2);
        map.put("a", 1);
        map.put("b", 2);
        map.put("c", 3);
        assert_eq!(map.get(&"a"), None);
        assert_eq!(map.get(&"b"), Some(2));
        assert_eq!(map.get(&"c"), Some(3));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn an_entry_is_never_replaced_and_keeps_its_place() {
        let map = Bounded::new(2);
        map.put("a", 1);
        map.put("b", 2);
        map.put("a", 9);
        assert_eq!(map.get(&"a"), Some(1));
        map.put("c", 3);
        assert_eq!(map.get(&"a"), None);
        assert_eq!(map.get(&"b"), Some(2));
    }

    #[test]
    fn a_map_of_no_capacity_holds_nothing() {
        let map = Bounded::new(0);
        map.put("a", 1);
        assert!(map.is_empty());
    }

    #[test]
    fn a_copy_of_no_entries_is_none() {
        assert!(IdentityCopy::new(0).is_none());
    }

    #[test]
    fn a_copy_serves_nothing_until_its_listener_connects() {
        let copy = IdentityCopy::new(4).expect("a copy of four entries");
        assert!(copy.serving().is_none());
    }

    #[tokio::test]
    async fn a_reset_drops_every_table() {
        let rows = Rows::new(4);
        rows.internals.put(
            ("Condition".to_owned(), "x".to_owned()),
            crate::identity::FhirResourceId::new("a").expect("an id"),
        );
        rows.subjects.put(
            "ehr".to_owned(),
            super::SealedPerson {
                key_id: crate::store::seal::KeyId::new("k").expect("a key id"),
                sealed: vec![1],
            },
        );
        rows.reset().await;
        assert!(rows.internals.is_empty());
        assert!(rows.subjects.is_empty());
    }
}
