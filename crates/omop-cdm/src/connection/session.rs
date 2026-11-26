// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The session settings every connection to one database starts with, and the
//! check that the server holds them.
//!
//! Both clients send the settings in the startup packet's `options`
//! parameter, which the server applies as `-c name=value` before the first
//! statement (PostgreSQL 19 docs, libpq "Parameter Key Words", `options`). A
//! pooler between the bridge and the server can drop the parameter, so each
//! client reads the settings back from `pg_settings` once it connects and
//! refuses a server that does not hold them.

use std::time::Duration;

/// The statement a client reads the session's timeouts back with: each
/// name and its value, an integer of milliseconds (PostgreSQL 19 docs,
/// `pg_settings`, `setting` in the setting's `unit`).
pub(crate) const SESSION_QUERY: &str = "SELECT name, setting FROM pg_settings \
     WHERE name IN ('statement_timeout', 'lock_timeout', 'idle_in_transaction_session_timeout')";

/// The largest timeout PostgreSQL accepts, in milliseconds: the three
/// settings are `integer` (PostgreSQL 19 docs, "Client Connection Defaults").
pub const MAXIMUM_TIMEOUT: Duration = Duration::from_millis(2_147_483_647);

/// The session settings of every connection to one database.
///
/// A timeout of zero disables it, as the server reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    /// The longest one statement runs before the server cancels it
    /// (`statement_timeout`).
    pub statement_timeout: Duration,
    /// The longest a statement waits for a lock (`lock_timeout`).
    pub lock_timeout: Duration,
    /// The longest a session stays idle inside an open transaction before
    /// the server ends it (`idle_in_transaction_session_timeout`).
    pub idle_in_transaction_session_timeout: Duration,
    /// How many prepared statements the pool's client keeps per connection;
    /// the writer prepares its own statements and keeps none.
    pub statement_cache_capacity: usize,
    /// The longest a pooled connection lives before the pool closes it.
    pub max_lifetime: Duration,
    /// The longest a pooled connection stays idle before the pool closes it.
    pub idle_timeout: Duration,
}

impl Default for Session {
    fn default() -> Self {
        // NOTE: PostgreSQL 19 docs, "Client Connection Defaults": the three timeouts default to 0
        // (disabled); the sqlx 0.9 docs: 100 statements, 30 minutes of life, 10 idle.
        Self {
            statement_timeout: Duration::ZERO,
            lock_timeout: Duration::ZERO,
            idle_in_transaction_session_timeout: Duration::ZERO,
            statement_cache_capacity: 100,
            max_lifetime: Duration::from_mins(30),
            idle_timeout: Duration::from_mins(10),
        }
    }
}

/// A server that does not hold the session settings its clients sent.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionError {
    /// The settings could not be read back.
    #[error("the session settings could not be read back")]
    Read {
        /// What the client reported.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// A timeout exceeds [`MAXIMUM_TIMEOUT`].
    #[error("{name} is longer than PostgreSQL's largest timeout, 2147483647 ms")]
    TooLong {
        /// The setting.
        name: &'static str,
    },
    /// The server holds another value of a setting, or none.
    #[error(
        "the server holds {name} = {found:?} where the bridge sent {sent} ms; a pooler between them \
         may drop the startup options"
    )]
    NotHeld {
        /// The setting.
        name: &'static str,
        /// The milliseconds the client sent.
        sent: u128,
        /// What the server reported, when it reported the setting.
        found: Option<String>,
    },
}

impl Session {
    /// Returns each timeout by its server name.
    fn timeouts(&self) -> [(&'static str, Duration); 3] {
        [
            ("statement_timeout", self.statement_timeout),
            ("lock_timeout", self.lock_timeout),
            (
                "idle_in_transaction_session_timeout",
                self.idle_in_transaction_session_timeout,
            ),
        ]
    }

    /// Returns each timeout as the `name` and the milliseconds a client
    /// sends.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::TooLong`] for a timeout past
    /// [`MAXIMUM_TIMEOUT`].
    pub fn parameters(&self) -> Result<Vec<(&'static str, String)>, SessionError> {
        self.timeouts()
            .into_iter()
            .map(|(name, timeout)| {
                if timeout > MAXIMUM_TIMEOUT {
                    return Err(SessionError::TooLong { name });
                }
                Ok((name, timeout.as_millis().to_string()))
            })
            .collect()
    }

    /// Reads the timeouts back from a connection of `pool` and holds them to
    /// these.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::Read`] when they cannot be read, and the
    /// errors of [`Session::verify`].
    pub async fn held_by(&self, pool: &sqlx::PgPool) -> Result<(), SessionError> {
        let found: Vec<(String, String)> = sqlx::query_as(SESSION_QUERY)
            .fetch_all(pool)
            .await
            .map_err(|source| SessionError::Read {
                source: Box::new(source),
            })?;
        self.verify(&found)
    }

    /// Holds `found`, the rows of [`SESSION_QUERY`], to the timeouts.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::NotHeld`] naming the first timeout the server
    /// holds another value of, or does not report.
    pub fn verify(&self, found: &[(String, String)]) -> Result<(), SessionError> {
        for (name, timeout) in self.timeouts() {
            let sent = timeout.as_millis();
            let held = found
                .iter()
                .find(|(setting, _)| setting == name)
                .map(|(_, value)| value);
            if held.and_then(|value| value.parse::<u128>().ok()) != Some(sent) {
                return Err(SessionError::NotHeld {
                    name,
                    sent,
                    found: held.cloned(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MAXIMUM_TIMEOUT, Session, SessionError};
    use std::time::Duration;

    fn session() -> Session {
        Session {
            statement_timeout: Duration::from_secs(30),
            lock_timeout: Duration::from_secs(5),
            idle_in_transaction_session_timeout: Duration::from_secs(60),
            ..Session::default()
        }
    }

    fn rows(values: [(&str, &str); 3]) -> Vec<(String, String)> {
        values
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect()
    }

    #[test]
    fn each_timeout_is_sent_in_milliseconds() {
        assert_eq!(
            session().parameters().expect("every timeout fits"),
            [
                ("statement_timeout", String::from("30000")),
                ("lock_timeout", String::from("5000")),
                ("idle_in_transaction_session_timeout", String::from("60000")),
            ]
        );
    }

    #[test]
    fn a_timeout_past_the_servers_largest_is_refused() {
        let mut session = session();
        session.lock_timeout = MAXIMUM_TIMEOUT + Duration::from_millis(1);
        assert!(matches!(
            session.parameters(),
            Err(SessionError::TooLong {
                name: "lock_timeout"
            })
        ));
    }

    #[test]
    fn a_server_holding_every_timeout_passes() {
        let found = rows([
            ("idle_in_transaction_session_timeout", "60000"),
            ("lock_timeout", "5000"),
            ("statement_timeout", "30000"),
        ]);
        assert!(session().verify(&found).is_ok());
    }

    #[test]
    fn a_server_holding_another_value_or_none_is_refused() {
        let other = rows([
            ("statement_timeout", "0"),
            ("lock_timeout", "5000"),
            ("idle_in_transaction_session_timeout", "60000"),
        ]);
        assert!(matches!(
            session().verify(&other),
            Err(SessionError::NotHeld {
                name: "statement_timeout",
                sent: 30_000,
                found: Some(ref value),
            }) if value == "0"
        ));
        let missing = &rows([
            ("statement_timeout", "30000"),
            ("lock_timeout", "5000"),
            ("idle_in_transaction_session_timeout", "60000"),
        ])[..2];
        assert!(matches!(
            session().verify(missing),
            Err(SessionError::NotHeld {
                name: "idle_in_transaction_session_timeout",
                sent: 60_000,
                found: None,
            })
        ));
    }
}
