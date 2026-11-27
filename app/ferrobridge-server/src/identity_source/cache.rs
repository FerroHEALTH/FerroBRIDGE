// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The replica's cache of the pseudonyms the identity source answered.
//!
//! An answer changes when the identity source merges, unmerges, links or
//! deletes a patient: after a merge the subsumed patient's identifiers
//! resolve to the survivor's pseudonym, and after an unmerge each resolves
//! to its own person again (`PIXm` 3.1.0 §2:3.83.4.2.2.5). The cache
//! therefore serves only while the identity source's PMIR feed reaches this
//! deployment: the subscription's holder ([`super::subscription`]) reads its
//! status and announces it on [`PSEUDONYMS`], and every ITI-93 message the
//! receiver ([`super::feed`]) takes forgets the identifiers it names on every
//! replica. While the subscription is not `active`, its status is stale, or
//! the replica's `LISTEN` session is down, every lookup asks the source, and
//! nothing cached before such a gap is served after it. An
//! entry is keyed by the HMAC of the source identifier under the state
//! store's lookup key, so no identifier value is held, and lives at most
//! `max_age` whatever the feed says. No specification governs the cache: our
//! own design.

use core::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ihe_iti::pixm::identifier::SourceIdentifier;
use secrecy::ExposeSecret as _;
use sqlx::postgres::PgPool;

use crate::identity::PersonId;
use crate::identity::cache::Bounded;
use crate::store::listen::{Dropping, Listened, PSEUDONYMS, Subscriber};
use crate::store::seal::LookupKey;

/// The digest purpose of a source identifier.
const SOURCE: &str = "pseudonym.source";

/// The payload prefix that forgets one source identifier.
const FORGET: &str = "forget:";

/// The payload prefix that announces the subscription's status.
const STATUS: &str = "status:";

/// The status code of a subscription the feed is sent to.
pub(super) const ACTIVE: &str = "active";

/// What the cache is configured to hold.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    /// The most pseudonyms it keeps.
    pub entries: usize,
    /// The longest one pseudonym is kept.
    pub max_age: Duration,
    /// How often the subscription's holder announces its status; an
    /// announcement older than two of these is stale.
    pub status_every: Duration,
}

/// The entries and the subscription's gate, which the `LISTEN` session's
/// notifications reach.
struct Shared {
    /// Each source identifier's digest to the instant it was put and its
    /// pseudonym.
    entries: Bounded<Vec<u8>, (Instant, PersonId)>,
    /// Until when the last `active` announcement holds; `None` while the
    /// subscription is not active or no announcement came.
    active_until: Mutex<Option<Instant>>,
    /// How long one `active` announcement holds.
    announcement: Duration,
    /// The pool the subscription's row is read from after a reconnect.
    pool: PgPool,
    /// How many times an entry was forgotten or the cache emptied, so an
    /// answer asked for before a change is never cached after it.
    changes: AtomicU64,
}

impl Shared {
    /// Returns the gate, whatever a panicking holder left: it is one value.
    fn gate(&self) -> std::sync::MutexGuard<'_, Option<Instant>> {
        self.active_until
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Holds the gate open for one announcement when `status` is `active`,
    /// and closes it otherwise; every entry is dropped unless the gate was
    /// open and stays open, since a feed message may have been missed while
    /// it was closed.
    fn announced(&self, status: &str) {
        let until = (status == ACTIVE)
            .then(|| Instant::now().checked_add(self.announcement))
            .flatten();
        let held = {
            let mut gate = self.gate();
            let held = until.is_some() && gate.is_some_and(|open| Instant::now() < open);
            *gate = until;
            held
        };
        if !held {
            self.clear();
        }
    }

    /// Whether an `active` announcement still holds.
    fn active(&self) -> bool {
        self.gate().is_some_and(|until| Instant::now() < until)
    }

    /// Counts one change.
    fn changed(&self) {
        self.changes.fetch_add(1, Ordering::AcqRel);
    }

    /// Drops every entry.
    fn clear(&self) {
        self.changed();
        self.entries.clear();
    }

    /// Drops the entry of `digest`.
    fn drop_digest(&self, digest: &[u8]) {
        self.changed();
        self.entries.remove(&digest.to_vec());
    }

    /// Forgets the source identifier whose digest `payload` encodes.
    fn forget(&self, payload: &str) {
        // NOTE: no specification governs this: our own design; a payload this
        // version cannot read empties the cache, the safe reading of a change.
        match URL_SAFE_NO_PAD.decode(payload) {
            Ok(digest) => self.drop_digest(&digest),
            Err(_unreadable) => self.clear(),
        }
    }
}

impl fmt::Debug for Shared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shared")
            .field("entries", &self.entries.len())
            .field("active", &self.active())
            .finish_non_exhaustive()
    }
}

impl Subscriber for Shared {
    fn notified<'a>(&'a self, payload: &'a str) -> Dropping<'a> {
        Box::pin(async move {
            if let Some(digest) = payload.strip_prefix(FORGET) {
                self.forget(digest);
            } else if let Some(status) = payload.strip_prefix(STATUS) {
                self.announced(status);
            } else {
                self.clear();
            }
        })
    }

    fn reset(&self) -> Dropping<'_> {
        Box::pin(async move {
            self.clear();
            *self.gate() = None;
            match super::subscription::recorded_active(&self.pool, self.announcement).await {
                Ok(true) => self.announced(ACTIVE),
                Ok(false) => {}
                Err(error) => tracing::warn!(
                    error = %ferrohealth_server::run::chain(&error),
                    "the PMIR subscription's status could not be read; the pseudonym cache waits for the next announcement"
                ),
            }
        })
    }
}

/// The cache of one replica.
#[derive(Clone)]
pub struct PseudonymCache {
    /// The entries and the gate.
    shared: Arc<Shared>,
    /// The listener on [`PSEUDONYMS`].
    listened: Arc<Listened>,
    /// The key a source identifier is digested under.
    lookup: LookupKey,
    /// The longest one pseudonym is kept.
    max_age: Duration,
}

impl fmt::Debug for PseudonymCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PseudonymCache")
            .field("shared", &self.shared)
            .field("max_age", &self.max_age)
            .finish_non_exhaustive()
    }
}

impl PseudonymCache {
    /// Returns an empty cache within `bounds`, keyed under `lookup`, whose
    /// subscription row and notifications go through `pool`; it serves
    /// nothing until its listener connects and the subscription is
    /// announced `active`.
    #[must_use]
    pub fn new(bounds: Bounds, lookup: LookupKey, pool: PgPool) -> Self {
        let shared = Arc::new(Shared {
            entries: Bounded::new(bounds.entries),
            active_until: Mutex::new(None),
            announcement: bounds.status_every.saturating_mul(2),
            pool,
            changes: AtomicU64::new(0),
        });
        let subscriber: Arc<dyn Subscriber> = shared.clone();
        let listened = Arc::new(Listened::with_subscriber(PSEUDONYMS.name(), subscriber));
        Self {
            shared,
            listened,
            lookup,
            max_age: bounds.max_age,
        }
    }

    /// Returns the listener state the replica's `LISTEN` session carries.
    #[must_use]
    pub fn listened(&self) -> Arc<Listened> {
        Arc::clone(&self.listened)
    }

    /// Whether the cache may answer: its listener is connected and the
    /// subscription's last announcement said `active` and still holds.
    #[must_use]
    pub fn serving(&self) -> bool {
        self.listened.is_listening() && self.shared.active()
    }

    /// Returns the digest `source` is keyed by.
    fn key(&self, source: &SourceIdentifier) -> Vec<u8> {
        self.digest(source.system(), source.value().expose_secret())
    }

    /// Returns the digest of the identifier `value` in `system`.
    fn digest(&self, system: &str, value: &str) -> Vec<u8> {
        self.lookup
            .digest(SOURCE, &format!("{system}\u{1f}{value}"))
    }

    /// Returns the pseudonym cached for `source`, while the cache serves
    /// and the entry is younger than `max_age`.
    #[must_use]
    pub fn get(&self, source: &SourceIdentifier) -> Option<PersonId> {
        if !self.serving() {
            return None;
        }
        let key = self.key(source);
        let (put, person) = self.shared.entries.get(&key)?;
        if put.elapsed() < self.max_age {
            return Some(person);
        }
        self.shared.entries.remove(&key);
        None
    }

    /// Returns the change count to hand [`Self::put`] once the identity
    /// source has answered, read before it is asked.
    #[must_use]
    pub fn changes(&self) -> u64 {
        self.shared.changes.load(Ordering::Acquire)
    }

    /// Caches `person` as the pseudonym of `source`, while the cache serves
    /// and nothing changed since `changes` was read.
    pub fn put(&self, changes: u64, source: &SourceIdentifier, person: &PersonId) {
        if self.serving() {
            self.shared
                .entries
                .put_if(self.key(source), (Instant::now(), person.clone()), || {
                    self.changes() == changes
                });
        }
    }

    /// Forgets the identifiers each `(system, value)` names on this replica
    /// at once and on every other through [`PSEUDONYMS`].
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the notification cannot be sent; this
    /// replica has forgotten them already.
    pub async fn forget<'a>(
        &self,
        identifiers: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<(), sqlx::Error> {
        let payloads: Vec<String> = identifiers
            .into_iter()
            .map(|(system, value)| {
                let digest = self.digest(system, value);
                self.shared.drop_digest(&digest);
                format!("{FORGET}{}", URL_SAFE_NO_PAD.encode(digest))
            })
            .collect();
        if payloads.is_empty() {
            return Ok(());
        }
        let mut connection = self.shared.pool.acquire().await?;
        for payload in &payloads {
            PSEUDONYMS.notify(&mut connection, payload).await?;
        }
        Ok(())
    }

    /// Announces the subscription's `status` to every replica.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] when the notification cannot be sent.
    pub(super) async fn announce(&self, status: &str) -> Result<(), sqlx::Error> {
        self.shared.announced(status);
        let mut connection = self.shared.pool.acquire().await?;
        PSEUDONYMS
            .notify(&mut connection, &format!("{STATUS}{status}"))
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ACTIVE, Bounds, PseudonymCache, Shared};
    use crate::store::listen::Subscriber as _;
    use crate::store::seal::{LookupKey, SealingKey};

    /// The base64 of 32 bytes of `A`, made for the tests.
    const KEY: &str = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=";

    fn cache() -> PseudonymCache {
        let key = SealingKey::from_base64(KEY).expect("a 32-byte key");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://bridge@127.0.0.1:1/state?sslmode=disable")
            .expect("a lazy pool");
        PseudonymCache::new(
            Bounds {
                entries: 4,
                max_age: Duration::from_secs(60),
                status_every: Duration::from_secs(30),
            },
            LookupKey::new(&key),
            pool,
        )
    }

    fn shared(cache: &PseudonymCache) -> &Shared {
        cache.shared.as_ref()
    }

    #[tokio::test]
    async fn the_gate_opens_only_on_an_active_announcement() {
        let cache = cache();
        assert!(!shared(&cache).active());
        shared(&cache).notified("status:active").await;
        assert!(shared(&cache).active());
        shared(&cache).notified("status:error").await;
        assert!(!shared(&cache).active());
    }

    #[tokio::test]
    async fn a_forget_drops_one_digest_and_an_unreadable_one_empties_the_cache() {
        let cache = cache();
        let person = crate::identity::PersonId::new("urn:oid:2.999.9", "p-1").expect("a person");
        let one = cache.digest("urn:oid:2.999.1", "a");
        let two = cache.digest("urn:oid:2.999.1", "b");
        shared(&cache)
            .entries
            .put(one.clone(), (std::time::Instant::now(), person.clone()));
        shared(&cache)
            .entries
            .put(two.clone(), (std::time::Instant::now(), person));
        let payload = format!(
            "forget:{}",
            base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, &one)
        );
        shared(&cache).notified(&payload).await;
        assert!(shared(&cache).entries.get(&one).is_none());
        assert!(shared(&cache).entries.get(&two).is_some());
        shared(&cache).notified("forget:not base64!").await;
        assert!(shared(&cache).entries.is_empty());
    }

    #[tokio::test]
    async fn only_an_unbroken_run_of_active_announcements_keeps_the_entries() {
        let cache = cache();
        let person = crate::identity::PersonId::new("urn:oid:2.999.9", "p-1").expect("a person");
        let one = cache.digest("urn:oid:2.999.1", "a");
        let put = || {
            shared(&cache)
                .entries
                .put(one.clone(), (std::time::Instant::now(), person.clone()));
        };
        shared(&cache).announced(ACTIVE);
        put();
        shared(&cache).announced(ACTIVE);
        assert!(shared(&cache).entries.get(&one).is_some());
        shared(&cache).announced("unreachable");
        assert!(shared(&cache).entries.is_empty());
        put();
        shared(&cache).announced(ACTIVE);
        assert!(
            shared(&cache).entries.is_empty(),
            "an entry from before the gap is not served after it"
        );
    }

    #[tokio::test]
    async fn nothing_is_served_while_the_listener_is_down() {
        let cache = cache();
        shared(&cache).announced(ACTIVE);
        assert!(!cache.serving());
    }
}
