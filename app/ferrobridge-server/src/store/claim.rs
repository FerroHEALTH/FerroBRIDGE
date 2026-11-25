// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The row claim on shared work: the statement that takes the due rows of a
//! table for one replica and marks them with an expiry, after which another
//! replica may take them again.
//!
//! The due rows are locked with `FOR UPDATE SKIP LOCKED`, so two replicas
//! never take one row
//! (<https://www.postgresql.org/docs/current/sql-select.html#SQL-FOR-UPDATE-SHARE>).
//! The inbound work items, the inbound slots and the audit outbox claim
//! through it. No specification governs the claim: our own design.

use std::time::Duration;

/// One table's claim: which rows are due, in what order, and how a claimed
/// row is marked.
#[derive(Debug, Clone, Copy)]
pub struct Claim<'a> {
    /// The table.
    pub table: &'a str,
    /// The columns that name one row.
    pub keys: &'a [&'a str],
    /// The condition a row meets while it may be taken.
    pub due: &'a str,
    /// The order the due rows are taken in.
    pub order: &'a str,
    /// How many rows one claim takes, a number or a parameter.
    pub limit: &'a str,
    /// The assignments that mark a claimed row, its expiry among them.
    pub mark: &'a str,
    /// What the statement returns, each column of the claimed row named as
    /// `claimed.<column>`.
    pub returning: &'a str,
}

impl Claim<'_> {
    /// Returns the statement that takes the due rows and marks them.
    #[must_use]
    pub fn statement(&self) -> String {
        let keys = self.keys.join(", ");
        let same = self
            .keys
            .iter()
            .map(|key| format!("claimed.{key} = due.{key}"))
            .collect::<Vec<_>>()
            .join(" AND ");
        format!(
            "WITH due AS MATERIALIZED (SELECT {keys} FROM {table} WHERE ({due}) \
             ORDER BY {order} LIMIT {limit} FOR UPDATE SKIP LOCKED) \
             UPDATE {table} claimed SET {mark} FROM due WHERE {same} RETURNING {returning}",
            table = self.table,
            due = self.due,
            order = self.order,
            limit = self.limit,
            mark = self.mark,
            returning = self.returning,
        )
    }
}

/// Returns `period` in milliseconds, as a statement binds an expiry.
#[must_use]
pub fn millis(period: Duration) -> i64 {
    // NOTE: no specification governs this: our own design; a period past
    // i64 milliseconds is held as the longest the column takes.
    i64::try_from(period.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Claim, millis};

    #[test]
    fn a_claim_locks_its_due_rows_and_joins_them_on_every_key() {
        let claim = Claim {
            table: "work",
            keys: &["source", "key"],
            due: "a OR b",
            order: "at",
            limit: "$1",
            mark: "holder = $2",
            returning: "claimed.key",
        };
        assert_eq!(
            "WITH due AS MATERIALIZED (SELECT source, key FROM work WHERE (a OR b) \
             ORDER BY at LIMIT $1 FOR UPDATE SKIP LOCKED) \
             UPDATE work claimed SET holder = $2 FROM due \
             WHERE claimed.source = due.source AND claimed.key = due.key RETURNING claimed.key",
            claim.statement()
        );
    }

    #[test]
    fn a_period_binds_in_milliseconds_and_saturates() {
        assert_eq!(30_000, millis(Duration::from_secs(30)));
        assert_eq!(i64::MAX, millis(Duration::MAX));
    }
}
