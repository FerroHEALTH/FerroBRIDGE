// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parameters of one party search.
//!
//! A party type takes one of `identifier`, `name` and `_id`, plus `_count`
//! and the paging cursor. R4 ANDs every parameter
//! (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.17), and one configured
//! query answers one parameter, so a second search parameter is one the
//! server "does not support" for this search: it is refused, or ignored and
//! reported when the client sends `Prefer: handling=lenient` (§3.1.1.3). No
//! specification governs which parameters a party type takes: our own
//! design.

use crate::facade::Facade;
use crate::facade::handlers::Refusal;
use crate::facade::search::criteria::CURSOR;
use crate::facade::search::criteria::Handling;
use crate::facade::search::cursor;
use crate::facade::search::party::ID;
use crate::facade::search::party::IDENTIFIER;
use crate::facade::search::party::NAME;
use crate::facade::search::party::PartyQueries;
use crate::facade::search::value;
use crate::facade::search::value::Token;

use super::bad_request;

/// The page-size parameter (<https://hl7.org/fhir/R4/search.html>,
/// §3.1.1.5.3).
const COUNT: &str = "_count";

/// The one criterion a party search matches by.
#[derive(Debug, Clone)]
pub(super) enum Criterion {
    /// `identifier`.
    Identifier(Token),
    /// `name`.
    Name(String),
    /// `_id`.
    Id(String),
}

/// What one party search asks.
#[derive(Debug)]
pub(super) struct Request {
    /// The criterion.
    pub(super) criterion: Criterion,
    /// The parameters the search used, for the links and the cursor.
    pub(super) used: Vec<(String, String)>,
    /// The page size; `0` asks for the count alone.
    pub(super) count: u32,
    /// The first row of the page.
    pub(super) offset: i64,
    /// The parameters ignored as `Prefer: handling=lenient` asks.
    pub(super) ignored: Vec<String>,
}

/// Reads the parameters of one search on `resource_type`, which `queries`
/// configures.
pub(super) fn read(
    facade: &Facade,
    resource_type: &str,
    queries: &PartyQueries,
    pairs: &[(String, String)],
    handling: Handling,
) -> Result<Request, Refusal> {
    let mut criterion = None;
    let mut used = Vec::new();
    let mut ignored = Vec::new();
    let mut count = None;
    let mut cursor_text = None;
    let taken = queries.parameters();
    for (name, text) in pairs {
        match name.as_str() {
            COUNT => count = Some(page_size(resource_type, text)?),
            CURSOR => cursor_text = Some(text.clone()),
            parameter if taken.contains(&parameter) && criterion.is_none() => {
                criterion = Some(criterion_of(parameter, text)?);
                used.push((name.clone(), text.clone()));
            }
            other if handling == Handling::Lenient => ignored.push(other.to_owned()),
            other if taken.contains(&other) => {
                return Err(bad_request(&format!(
                    "{other} follows another search parameter, and a {resource_type} is searched by one of {} at a time",
                    taken.join(", ")
                )));
            }
            other => {
                return Err(bad_request(&format!(
                    "{other} is no search parameter of {resource_type}, which is searched by one of {}",
                    taken.join(", ")
                )));
            }
        }
    }
    let criterion = criterion.ok_or_else(|| {
        bad_request(&format!(
            "a {resource_type} is searched by one of {}, and the search names none",
            taken.join(", ")
        ))
    })?;
    let limits = facade.search_limits();
    let count = count.unwrap_or(limits.default_count).min(limits.max_count);
    let fingerprint = cursor::fingerprint(resource_type, &used);
    let offset = match cursor_text {
        None => 0,
        Some(text) => {
            let position = cursor::decode(&text, &fingerprint)
                .map_err(|error| bad_request(&format!("{CURSOR}: {error}")))?;
            if position.declaration != 0 || position.skip != 0 {
                return Err(bad_request(&format!(
                    "{CURSOR}: `{text}` is no cursor of a {resource_type} search"
                )));
            }
            position.offset
        }
    };
    Ok(Request {
        criterion,
        used,
        count,
        offset,
        ignored,
    })
}

/// Reads one value of `parameter` as the criterion it names.
fn criterion_of(parameter: &str, text: &str) -> Result<Criterion, Refusal> {
    if value::split(text, ',').len() > 1 {
        return Err(bad_request(&format!(
            "{parameter} names more than one value, and a party search matches one"
        )));
    }
    match parameter {
        IDENTIFIER => value::token(text)
            .map(Criterion::Identifier)
            .map_err(|error| bad_request(&format!("{IDENTIFIER}: {error}"))),
        NAME => value::unescape(text)
            .map(Criterion::Name)
            .map_err(|error| bad_request(&format!("{NAME}: {error}"))),
        ID => Ok(Criterion::Id(text.to_owned())),
        other => Err(bad_request(&format!(
            "{other} is no search parameter of a party type"
        ))),
    }
}

/// Reads a `_count` value: `0` asks for the count alone, as R4 treats it as
/// `_summary=count` (<https://hl7.org/fhir/R4/search.html>, §3.1.1.5.3).
fn page_size(resource_type: &str, text: &str) -> Result<u32, Refusal> {
    text.parse::<u32>().map_err(|_unparsed| {
        bad_request(&format!(
            "{COUNT} is {text}, and a {resource_type} search takes a page size of 0 or more"
        ))
    })
}
