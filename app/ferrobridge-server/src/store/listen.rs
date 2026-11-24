// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The invalidation of state held in memory: every replica listens on the
//! store's channels, and a write notifies them when it commits.
//!
//! PostgreSQL delivers a `NOTIFY` to every listening session when the
//! notifying transaction commits, and to none when it rolls back
//! (<https://www.postgresql.org/docs/current/sql-notify.html>). A
//! notification marks the copy stale, and the next request reads it again;
//! while the listener is not connected, every request reads it, so a lost
//! notification never serves an outdated copy. A [`Subscriber`] drops what
//! each notification names before the notification counts as received, and
//! everything when the session connects again. [`Listeners`] holds every
//! channel on one `LISTEN` session apart from the pool, so the pool may run
//! through a transaction-mode pooler that would end a `LISTEN`. No
//! specification governs the invalidation: our own design on family
//! precedent.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgListener, PgPoolOptions};

/// The `application_name` of the listener session, which tells an operator
/// a `LISTEN` session from a pooled connection in `pg_stat_activity`.
pub const SESSION_NAME: &str = "ferrobridge state listener";

/// The pause before a lost listener connects again.
const RECONNECT: Duration = Duration::from_secs(1);

/// The future a [`Subscriber`] answers with.
pub type Dropping<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// What a copy keyed by the notification payload drops when its listener
/// hears of a change.
pub trait Subscriber: fmt::Debug + Send + Sync {
    /// Drops what the notification `payload` names.
    fn notified<'a>(&'a self, payload: &'a str) -> Dropping<'a>;

    /// Drops everything: the listener connected or lost its connection, so
    /// notifications may have been missed.
    fn reset(&self) -> Dropping<'_>;
}

/// Whether this replica's copy of one kind of state may be trusted: the
/// listener on its channel, and the notifications it has received.
#[derive(Debug)]
pub struct Listened {
    /// The channel it listens on.
    channel: &'static str,
    /// Whether the listener is connected, so a request may trust the copy
    /// until a notification marks it stale.
    listening: AtomicBool,
    /// Whether a notification arrived that no read has made up yet.
    stale: AtomicBool,
    /// The notifications received.
    notified: AtomicU64,
    /// The connections lost after the listener had connected.
    losses: AtomicU64,
    /// The wake of a task waiting on the listener.
    wake: tokio::sync::Notify,
    /// The copy each notification's payload is dropped from, if any.
    subscriber: Option<Arc<dyn Subscriber>>,
}

impl Listened {
    /// The listener state of `channel`, not yet listening, so every request
    /// reads until [`Listeners::spawn`] connects it.
    #[must_use]
    pub fn new(channel: &'static str) -> Self {
        Self {
            channel,
            listening: AtomicBool::new(false),
            stale: AtomicBool::new(false),
            notified: AtomicU64::new(0),
            losses: AtomicU64::new(0),
            wake: tokio::sync::Notify::new(),
            subscriber: None,
        }
    }

    /// The listener state of `channel`, as [`Self::new`], whose every
    /// notification `subscriber` drops from its copy before it counts as
    /// received.
    #[must_use]
    pub fn with_subscriber(channel: &'static str, subscriber: Arc<dyn Subscriber>) -> Self {
        Self {
            subscriber: Some(subscriber),
            ..Self::new(channel)
        }
    }

    /// The channel this copy listens on.
    #[must_use]
    pub const fn channel(&self) -> &'static str {
        self.channel
    }

    /// The session listens again: drop everything and mark the copy stale,
    /// so the notifications missed while it was down are made up, then
    /// trust the copy until the next notification.
    async fn connected(&self) {
        if let Some(subscriber) = &self.subscriber {
            subscriber.reset().await;
        }
        self.stale.store(true, Ordering::Release);
        self.listening.store(true, Ordering::Release);
        self.wake.notify_waiters();
    }

    /// A notification of `payload` arrived on the channel.
    async fn heard(&self, payload: &str) {
        if let Some(subscriber) = &self.subscriber {
            subscriber.notified(payload).await;
        }
        self.stale.store(true, Ordering::Release);
        self.notified.fetch_add(1, Ordering::AcqRel);
        self.wake.notify_waiters();
    }

    /// The session was lost: every request reads until it listens again.
    async fn lost(&self) {
        self.listening.store(false, Ordering::Release);
        self.losses.fetch_add(1, Ordering::AcqRel);
        if let Some(subscriber) = &self.subscriber {
            subscriber.reset().await;
        }
        self.wake.notify_waiters();
    }

    /// Whether a notification is pending or the listener is down, so the
    /// next request must read; the pending notification is taken.
    #[must_use]
    pub fn must_read(&self) -> bool {
        !self.is_listening() || self.stale.swap(false, Ordering::AcqRel)
    }

    /// Whether the listener is connected; while it is not, every request
    /// reads.
    #[must_use]
    pub fn is_listening(&self) -> bool {
        self.listening.load(Ordering::Acquire)
    }

    /// The notifications this replica has received.
    #[must_use]
    pub fn notifications(&self) -> u64 {
        self.notified.load(Ordering::Acquire)
    }

    /// The connections lost after the listener had connected.
    #[must_use]
    pub fn losses(&self) -> u64 {
        self.losses.load(Ordering::Acquire)
    }

    /// Wait until the listener is connected, at most `within`; whether it
    /// is.
    pub async fn listening_within(&self, within: Duration) -> bool {
        self.woken_until(within, Self::is_listening).await
    }

    /// Wait until this replica has received more than `seen` notifications,
    /// at most `within`; whether it has.
    pub async fn notified_after(&self, seen: u64, within: Duration) -> bool {
        self.woken_until(within, |listened| listened.notifications() > seen)
            .await
    }

    /// Wait until the listener has lost more than `seen` connections, at
    /// most `within`; whether it has.
    pub async fn lost_after(&self, seen: u64, within: Duration) -> bool {
        self.woken_until(within, |listened| listened.losses() > seen)
            .await
    }

    /// Wait until `holds` of this listener, looking again at each wake, at
    /// most `within`; whether it holds.
    async fn woken_until(&self, within: Duration, holds: impl Fn(&Self) -> bool) -> bool {
        tokio::time::timeout(within, async {
            loop {
                let woken = self.wake.notified();
                if holds(self) {
                    return;
                }
                woken.await;
            }
        })
        .await
        .is_ok()
    }
}

/// One `LISTEN` session and the copies whose channels it carries.
pub struct Listeners {
    /// The connect options of the session; they carry the credential, so
    /// they are never rendered.
    options: PgConnectOptions,
    /// The copies it carries, each on its own channel.
    listened: Vec<Arc<Listened>>,
}

impl fmt::Debug for Listeners {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let channels: Vec<&str> = self.listened.iter().map(|l| l.channel).collect();
        f.debug_struct("Listeners")
            .field("channels", &channels)
            .finish_non_exhaustive()
    }
}

impl Listeners {
    /// One session on `options` carrying every channel of `listened`.
    #[must_use]
    pub fn on(options: PgConnectOptions, listened: &[Arc<Listened>]) -> Self {
        Self {
            options,
            listened: listened.to_vec(),
        }
    }

    /// Listen on every channel on this replica's behalf, in a detached task
    /// that ends with the runtime and connects again whenever its connection
    /// is lost.
    pub fn spawn(self) {
        tokio::spawn(listen(self));
    }
}

/// The loop of the session: connect, listen on every channel it carries,
/// mark each copy stale so the reads missed while disconnected are made up,
/// then mark the copy a notification names stale; on any failure, fall back
/// to reading on every request and connect again.
#[expect(
    clippy::infinite_loop,
    reason = "a detached task that lives as long as the runtime"
)]
async fn listen(session: Listeners) {
    let mut channels: Vec<&'static str> = session.listened.iter().map(|l| l.channel).collect();
    channels.sort_unstable();
    channels.dedup();
    loop {
        match connect(&channels, &session.options).await {
            Ok(mut listener) => {
                for copy in &session.listened {
                    copy.connected().await;
                }
                loop {
                    match listener.try_recv().await {
                        Ok(Some(notification)) => {
                            let named = session
                                .listened
                                .iter()
                                .filter(|copy| copy.channel == notification.channel());
                            for copy in named {
                                copy.heard(notification.payload()).await;
                            }
                        }
                        Ok(None) => break,
                        Err(error) => {
                            tracing::warn!(?channels, %error, "the state listener failed");
                            break;
                        }
                    }
                }
                for copy in &session.listened {
                    copy.lost().await;
                }
            }
            Err(error) => {
                tracing::warn!(?channels, %error, "the state listener could not connect");
            }
        }
        tokio::time::sleep(RECONNECT).await;
    }
}

/// A listener on every one of `channels`, on one connection made with
/// `options` and named [`SESSION_NAME`] that no pool shares.
async fn connect(
    channels: &[&'static str],
    options: &PgConnectOptions,
) -> Result<PgListener, sqlx::Error> {
    let session = PgPoolOptions::new()
        .max_connections(1)
        .min_connections(0)
        .max_lifetime(None)
        .idle_timeout(None)
        .connect_with(options.clone().application_name(SESSION_NAME))
        .await?;
    let mut listener = PgListener::connect_with(&session).await?;
    listener.ignore_pool_close_event(true);
    listener.listen_all(channels.iter().copied()).await?;
    Ok(listener)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, PoisonError};
    use std::time::Duration;

    use super::{Dropping, Listened, Listeners, Subscriber};

    /// The longest a case waits on a state it set itself.
    const WAIT: Duration = Duration::from_secs(1);

    /// A copy that records what it was told to drop.
    #[derive(Debug, Default)]
    struct Recorded {
        payloads: Mutex<Vec<String>>,
        resets: AtomicU64,
    }

    impl Subscriber for Recorded {
        fn notified<'a>(&'a self, payload: &'a str) -> Dropping<'a> {
            Box::pin(async move {
                self.payloads
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(payload.to_owned());
            })
        }

        fn reset(&self) -> Dropping<'_> {
            Box::pin(async move {
                self.resets.fetch_add(1, Ordering::AcqRel);
            })
        }
    }

    #[tokio::test]
    async fn a_copy_is_read_while_unlistened_and_once_per_notification() {
        let recorded = Arc::new(Recorded::default());
        let listened = Listened::with_subscriber("ferrobridge_test", recorded.clone());
        assert_eq!("ferrobridge_test", listened.channel());
        assert!(listened.must_read());
        assert!(listened.must_read(), "every request reads while unlistened");
        assert!(!listened.listening_within(Duration::from_millis(10)).await);

        listened.connected().await;
        assert!(listened.listening_within(WAIT).await);
        assert!(listened.must_read(), "a connect makes up what was missed");
        assert!(!listened.must_read());

        listened.heard("row-1").await;
        assert!(listened.notified_after(0, WAIT).await);
        assert_eq!(1, listened.notifications());
        assert!(listened.must_read());
        assert!(!listened.must_read());

        listened.lost().await;
        assert!(listened.lost_after(0, WAIT).await);
        assert_eq!(1, listened.losses());
        assert!(!listened.is_listening());
        assert!(listened.must_read());
        assert_eq!(
            vec![String::from("row-1")],
            *recorded
                .payloads
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
        );
        assert_eq!(2, recorded.resets.load(Ordering::Acquire));
    }

    #[tokio::test]
    async fn a_copy_without_a_subscriber_counts_alone() {
        let listened = Listened::new("ferrobridge_test");
        listened.connected().await;
        listened.heard("row").await;
        listened.lost().await;
        assert_eq!((1, 1), (listened.notifications(), listened.losses()));
    }

    #[test]
    fn the_session_renders_its_channels_and_never_its_credential() {
        let options = sqlx::postgres::PgConnectOptions::new().password("s3cret");
        let listened = [Arc::new(Listened::new("ferrobridge_test"))];
        let rendered = format!("{:?}", Listeners::on(options, &listened));
        assert!(rendered.contains("ferrobridge_test"), "{rendered}");
        assert!(!rendered.contains("s3cret"), "{rendered}");
    }

    #[tokio::test]
    async fn an_unreachable_listener_leaves_every_request_reading() {
        let options = sqlx::postgres::PgConnectOptions::new()
            .host("127.0.0.1")
            .port(1);
        let listened = Arc::new(Listened::new("ferrobridge_test"));
        Listeners::on(options, &[Arc::clone(&listened)]).spawn();
        assert!(!listened.listening_within(Duration::from_millis(300)).await);
        assert!(listened.must_read());
        assert_eq!(0, listened.losses());
    }
}
