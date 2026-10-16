// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `Bundle.total`: the number of matches a search counts.
//!
//! A count reads no composition wherever one AQL count agrees with the
//! entries the search answers, and reads the rows only where it cannot
//! (<https://hl7.org/fhir/R4/search.html>, §3.1.1.5.2). No specification
//! governs how the count is made: our own design.

use std::collections::BTreeSet;

use crate::cdr::CdrClient;
use crate::facade::Facade;
use crate::facade::handlers::Refusal;
use crate::facade::handlers::search::Plan;
use crate::facade::handlers::search::malformed_answer;
use crate::facade::handlers::search::matched;
use crate::facade::handlers::search::read_row;
use crate::facade::handlers::search::rows_of;
use crate::facade::search::aql;
use crate::facade::search::aql::Conditions;
use crate::facade::search::criteria::Criteria;
use crate::facade::search::criteria::Total;
use crate::facade::search::cursor::Position;
use crate::facade::search::declaration::Node;

/// Returns the `Bundle.total` of one page, when it is known or asked for.
///
/// A first page that leaves no next page holds every match, so its size is
/// the exact total at no cost. Otherwise the matches are counted only when
/// the client asked for a total (<https://hl7.org/fhir/R4/search.html>,
/// §3.1.1.5.2), and `_total=none` never gets one.
pub(super) async fn total_of(
    facade: &Facade,
    client: &CdrClient,
    plans: &[Plan<'_>],
    criteria: &Criteria,
    start: Position,
    next: Option<Position>,
    answered: usize,
) -> Result<Option<u64>, Refusal> {
    match criteria.total {
        Some(Total::None) => Ok(None),
        _ if start == Position::START && next.is_none() => {
            Ok(Some(u64::try_from(answered).unwrap_or(u64::MAX)))
        }
        Some(Total::Accurate | Total::Estimate) => Ok(Some(
            count_all(facade, client, plans, Accuracy::of(criteria)).await?,
        )),
        None => Ok(None),
    }
}

/// How exact a count must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Accuracy {
    /// `_total=accurate`, and the count `_count=0` and `_summary=count` ask
    /// for unless `_total=estimate` is also given.
    Accurate,
    /// `_total=estimate`: "a rough estimate of the number of matching
    /// resources is sufficient" (R4 search.html §3.1.1.5.2).
    Estimate,
}

impl Accuracy {
    /// Returns the accuracy `criteria` ask of a count.
    pub(super) fn of(criteria: &Criteria) -> Self {
        if criteria.total == Some(Total::Estimate) {
            Self::Estimate
        } else {
            Self::Accurate
        }
    }
}

/// Returns the number of matches across every declaration.
///
/// A declaration whose matches one AQL count gives exactly, and every
/// declaration under [`Accuracy::Estimate`], costs one count query that reads
/// no composition. Any other declaration reads its rows and counts each
/// entry [`matched`] answers once.
pub(super) async fn count_all(
    facade: &Facade,
    client: &CdrClient,
    plans: &[Plan<'_>],
    accuracy: Accuracy,
) -> Result<u64, Refusal> {
    let mut total: u64 = 0;
    for plan in plans {
        let Some(conditions) = plan.conditions.as_ref() else {
            continue;
        };
        let counted = if accuracy == Accuracy::Estimate || counts_exactly(plan, conditions) {
            let entry = plan.declared.placement().map(|placement| &placement.entry);
            count_query(client, conditions, entry).await?
        } else {
            let entries = count_entries(facade, client, plan, conditions).await?;
            u64::try_from(entries).unwrap_or(u64::MAX)
        };
        total = total.saturating_add(counted);
    }
    Ok(total)
}

/// Whether one AQL count gives the exact number of entries one declaration
/// matches.
///
/// It does when the query binds no node, no `_id` narrows the entries, and
/// the template places the context's entry only among the composition's own
/// `content` items, so each entry `CONTAINS` finds is one [`matched`]
/// answers. A bound node can stand outside every entry and an `_id` names
/// entries by their place, so only the rows tell those apart.
fn counts_exactly(plan: &Plan<'_>, conditions: &Conditions) -> bool {
    conditions.node_count() == 0
        && plan.ids.is_empty()
        && plan
            .declared
            .placement()
            .is_some_and(|placement| placement.top_level)
}

/// Runs the count query of one declaration and returns its number.
async fn count_query(
    client: &CdrClient,
    conditions: &Conditions,
    entry: Option<&Node>,
) -> Result<u64, Refusal> {
    let rows = rows_of(client, &aql::count_request(conditions, entry)).await?;
    rows.first()
        .and_then(|row| row.first())
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| malformed_answer("the CDR answered the count query with no readable count"))
}

/// Returns the number of entries the rows of one declaration answer.
async fn count_entries(
    facade: &Facade,
    client: &CdrClient,
    plan: &Plan<'_>,
    conditions: &Conditions,
) -> Result<usize, Refusal> {
    let fetch = i64::from(facade.search_limits().max_count.max(1));
    let mut counted: BTreeSet<(String, usize)> = BTreeSet::new();
    let mut offset: i64 = 0;
    loop {
        let request = aql::rows_request(conditions, &plan.order, offset, fetch);
        let rows = rows_of(client, &request).await?;
        for (position, row) in rows.iter().enumerate() {
            let read = read_row(facade, plan, row, position)?;
            for entry in matched(plan, &read, row, position)? {
                counted.insert((read.version.value().to_owned(), entry.ordinal()));
            }
        }
        let fetched = i64::try_from(rows.len()).unwrap_or(i64::MAX);
        if fetched < fetch {
            return Ok(counted.len());
        }
        offset = offset.saturating_add(fetched);
    }
}
