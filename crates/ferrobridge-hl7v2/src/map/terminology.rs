// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology server a run translates and resolves on, with the
//! assigning authorities it has resolved held across messages.
//!
//! The identifier registry search
//! (`GET [base]/NamingSystem?kind=identifier&value=<authority>`,
//! <https://hl7.org/fhir/R4/namingsystem.html#search>) answers the same way
//! for an authority until the server's registrations change, so a steady
//! feed from one sender would otherwise send it again on every message. Each
//! answer is held for a max age, 10 minutes unless
//! [`Terminology::with_max_age`] sets another, and at most 4096 authorities
//! are held unless [`Terminology::with_capacity`] sets another. No
//! specification governs the cache: our own design.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ferrobridge_term::client::Client;

/// A terminology client with the assigning authorities it resolved.
///
/// The cache is keyed by the authority and belongs to this one client, so it
/// is keyed by the terminology server too. A clone shares the client and the
/// cache, so every clone resolves an authority once per max age. A refused
/// or unreachable search is never held.
#[derive(Debug, Clone)]
pub struct Terminology {
    client: Client,
    authorities: Arc<Authorities>,
}

impl Terminology {
    /// The max age of a resolution unless [`Terminology::with_max_age`]
    /// sets another: 10 minutes.
    pub const MAX_AGE: Duration = Duration::from_secs(600);

    /// The number of authorities held unless
    /// [`Terminology::with_capacity`] sets another: 4096.
    pub const CAPACITY: NonZeroUsize = NonZeroUsize::new(4096).expect("4096 should not be zero");

    /// Returns `client` with an empty cache, holding each resolution for
    /// [`Terminology::MAX_AGE`] and at most [`Terminology::CAPACITY`]
    /// authorities.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            authorities: Arc::new(Authorities::new(Self::MAX_AGE, Self::CAPACITY)),
        }
    }

    /// Holds each resolution for `max_age`, after which the next message
    /// that names the authority searches again; a zero max age holds none.
    /// The cache starts empty.
    #[must_use]
    pub fn with_max_age(self, max_age: Duration) -> Self {
        let capacity = self.authorities.capacity;
        Self {
            client: self.client,
            authorities: Arc::new(Authorities::new(max_age, capacity)),
        }
    }

    /// Holds at most `capacity` authorities, the stalest left out first.
    /// The cache starts empty.
    #[must_use]
    pub fn with_capacity(self, capacity: NonZeroUsize) -> Self {
        let max_age = self.authorities.max_age;
        Self {
            client: self.client,
            authorities: Arc::new(Authorities::new(max_age, capacity)),
        }
    }

    /// Returns the terminology client.
    #[must_use]
    pub const fn client(&self) -> &Client {
        &self.client
    }

    /// Returns the max age of a resolution.
    #[must_use]
    pub fn max_age(&self) -> Duration {
        self.authorities.max_age
    }

    /// Returns the systems held for `authority`, when they were resolved
    /// within the max age.
    pub(super) fn held(&self, authority: &str) -> Option<Vec<String>> {
        self.authorities.held(authority, Instant::now())
    }

    /// Holds `systems` as the resolution of `authority`, resolved now.
    pub(super) fn hold(&self, authority: &str, systems: &[String]) {
        self.authorities.hold(authority, systems, Instant::now());
    }
}

/// The resolutions, each with the instant it was answered.
#[derive(Debug)]
struct Authorities {
    max_age: Duration,
    capacity: NonZeroUsize,
    held: Mutex<BTreeMap<String, Resolution>>,
}

/// One authority's systems: one, none, or more than one.
#[derive(Debug)]
struct Resolution {
    systems: Vec<String>,
    answered: Instant,
}

impl Authorities {
    const fn new(max_age: Duration, capacity: NonZeroUsize) -> Self {
        Self {
            max_age,
            capacity,
            held: Mutex::new(BTreeMap::new()),
        }
    }

    fn fresh(&self, resolution: &Resolution, now: Instant) -> bool {
        now.saturating_duration_since(resolution.answered) < self.max_age
    }

    fn held(&self, authority: &str, now: Instant) -> Option<Vec<String>> {
        // A poisoned lock leaves the map whole, since no write panics part
        // way, so the cache keeps serving it.
        let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        held.get(authority)
            .filter(|resolution| self.fresh(resolution, now))
            .map(|resolution| resolution.systems.clone())
    }

    fn hold(&self, authority: &str, systems: &[String], now: Instant) {
        if self.max_age.is_zero() {
            return;
        }
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if !held.contains_key(authority) && held.len() >= self.capacity.get() {
            held.retain(|_, resolution| self.fresh(resolution, now));
            if held.len() >= self.capacity.get() {
                let stalest = held
                    .iter()
                    .min_by_key(|(_, resolution)| resolution.answered)
                    .map(|(authority, _)| authority.clone());
                if let Some(stalest) = stalest {
                    held.remove(&stalest);
                }
            }
        }
        held.insert(
            authority.to_owned(),
            Resolution {
                systems: systems.to_vec(),
                answered: now,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn systems(system: &str) -> Vec<String> {
        vec![system.to_owned()]
    }

    #[test]
    fn a_resolution_is_held_for_the_max_age_and_no_longer() {
        let authorities = Authorities::new(Duration::from_secs(60), Terminology::CAPACITY);
        let start = Instant::now();
        authorities.hold("NORTHHOSP", &systems("urn:a"), start);
        let within = start + Duration::from_secs(59);
        assert_eq!(
            Some(systems("urn:a")),
            authorities.held("NORTHHOSP", within)
        );
        let after = start + Duration::from_secs(60);
        assert_eq!(None, authorities.held("NORTHHOSP", after));
    }

    #[test]
    fn none_and_several_systems_are_held_like_one() {
        let authorities = Authorities::new(Duration::from_secs(60), Terminology::CAPACITY);
        let now = Instant::now();
        authorities.hold("NONE", &[], now);
        let two = vec!["urn:a".to_owned(), "urn:b".to_owned()];
        authorities.hold("TWO", &two, now);
        assert_eq!(Some(Vec::new()), authorities.held("NONE", now));
        assert_eq!(Some(two), authorities.held("TWO", now));
    }

    #[test]
    fn a_full_cache_leaves_out_the_expired_then_the_stalest() {
        let capacity = NonZeroUsize::new(2).expect("two");
        let authorities = Authorities::new(Duration::from_secs(60), capacity);
        let start = Instant::now();
        authorities.hold("A", &systems("urn:a"), start);
        authorities.hold("B", &systems("urn:b"), start + Duration::from_secs(10));
        let later = start + Duration::from_secs(20);
        authorities.hold("C", &systems("urn:c"), later);
        assert_eq!(None, authorities.held("A", later), "the stalest left out");
        assert!(authorities.held("B", later).is_some());
        assert!(authorities.held("C", later).is_some());
        let expired = start + Duration::from_secs(75);
        authorities.hold("D", &systems("urn:d"), expired);
        assert_eq!(None, authorities.held("B", expired), "B expired first");
        assert!(authorities.held("C", expired).is_some());
        assert!(authorities.held("D", expired).is_some());
        assert_eq!(2, authorities.held.lock().expect("a lock").len());
    }

    #[test]
    fn a_zero_max_age_holds_nothing() {
        let authorities = Authorities::new(Duration::ZERO, Terminology::CAPACITY);
        let now = Instant::now();
        authorities.hold("A", &systems("urn:a"), now);
        assert_eq!(None, authorities.held("A", now));
        assert!(authorities.held.lock().expect("a lock").is_empty());
    }
}
