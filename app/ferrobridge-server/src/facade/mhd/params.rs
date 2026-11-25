// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The parameters of one MHD search, read against the set a type declares,
//! and the matching of each kind against the element it searches.
//!
//! A repeated parameter is an `AND` and a comma inside one value an `OR`
//! (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.17). A parameter outside
//! the set is ignored, or refused under `Prefer: handling=strict`
//! (§3.1.1.3); a modifier the set does not support is refused, which R4
//! states as a SHALL (§3.1.1.4.4).

use fhir_search::date::DateValue;
use fhir_search::date::Instants;
use fhir_search::date::SearchDate;
use fhir_search::prefix::Prefix;
use fhir_search::string::StringMatch;
use fhir_search::token::System;
use fhir_search::token::Token;
use http::HeaderMap;
use http::StatusCode;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde_json::Value;

use crate::facade::outcome::Issue;
use crate::facade::outcome::IssueType;
use crate::facade::reply;
use crate::facade::reply::Refusal;

/// The parameter that names the first match of a page.
///
/// R4 lets a server "provide its own parameters in the links that it uses to
/// manage the state of the search" (§3.1.1.5.3); the name is our own design.
const CURSOR: &str = "_cursor";

/// The page size of a search that names no `_count`, and the largest it
/// answers (§3.1.1.5.3: a server "may return less than the client
/// requested").
const PAGE: usize = 100;

/// The kind of one declared parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A token (§3.1.1.4.10).
    Token,
    /// A date (§3.1.1.4.7).
    Date,
    /// A string (§3.1.1.4.9).
    Text,
    /// A reference (§3.1.1.4.11).
    Reference,
}

/// One declared parameter: its name, its kind, and the canonical of its
/// definition the `CapabilityStatement` names.
pub(super) type Declared = (&'static str, &'static str, &'static str);

/// The values of one occurrence of a parameter, any of which matches.
#[derive(Debug, Clone)]
pub(super) enum Values {
    /// Token values.
    Tokens(Vec<Token>),
    /// Date values.
    Dates(Vec<DateValue>),
    /// String values, under one match.
    Texts(StringMatch, Vec<String>),
    /// Reference values, as sent.
    References(Vec<String>),
}

/// One occurrence of a parameter the search applies.
#[derive(Debug, Clone)]
pub(super) struct Occurrence {
    /// The declared name.
    pub(super) name: &'static str,
    /// The values, any of which matches.
    pub(super) values: Values,
}

/// What one request asks of a search.
#[derive(Debug, Clone)]
pub(super) struct Read {
    /// The occurrences the search applies, in received order.
    pub(super) applied: Vec<Occurrence>,
    /// The applied pairs as received, for the `self` link.
    pairs: Vec<(String, String)>,
    /// The parameters ignored, by name.
    pub(super) ignored: Vec<String>,
    /// The page size.
    pub(super) count: usize,
    /// The first match of the page.
    pub(super) offset: usize,
}

impl Read {
    /// Returns the query of the page at `offset` of size `count`: the applied
    /// parameters, then the page.
    pub(super) fn query(&self, offset: usize, count: usize) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (name, value) in &self.pairs {
            query.append_pair(name, value);
        }
        query.append_pair("_count", &count.to_string());
        if offset > 0 {
            query.append_pair(CURSOR, &offset.to_string());
        }
        query.finish()
    }
}

/// Whether the client asked for `Prefer: handling=strict`; lenient handling
/// is the default MHD asks for (ITI TF-2: 3.67.4.1.2).
pub(super) fn strict(headers: &HeaderMap) -> bool {
    let mut strict = false;
    for value in headers.get_all("prefer") {
        let Ok(text) = value.to_str() else {
            continue;
        };
        for part in text.split(',') {
            match part.trim().to_ascii_lowercase().as_str() {
                "handling=strict" => strict = true,
                "handling=lenient" => strict = false,
                _ => {}
            }
        }
    }
    strict
}

/// Reads `pairs` against `declared`.
///
/// # Errors
///
/// Returns the `400` of a malformed value, a modifier the parameter does not
/// take, and, when `strict`, a parameter outside `declared`.
pub(super) fn read(
    pairs: &[(String, String)],
    declared: &[Declared],
    strict: bool,
) -> Result<Read, Refusal> {
    let mut read = Read {
        applied: Vec::new(),
        pairs: Vec::new(),
        ignored: Vec::new(),
        count: PAGE,
        offset: 0,
    };
    for (key, value) in pairs {
        let (name, modifier) = match key.split_once(':') {
            Some((name, modifier)) => (name, Some(modifier)),
            None => (key.as_str(), None),
        };
        match name {
            "_format" | "_pretty" => continue,
            "_count" => {
                let count = crate::facade::media::page_size(value)
                    .map_err(|error| invalid(key, &error.to_string()))?;
                read.count = usize::try_from(count).unwrap_or(PAGE).min(PAGE);
                continue;
            }
            CURSOR => {
                read.offset = value
                    .parse()
                    .map_err(|_unparsed| invalid(key, &format!("{CURSOR} is a match position")))?;
                continue;
            }
            _ => {}
        }
        let Some(&(declared_name, _, kind)) = declared.iter().find(|entry| entry.0 == name) else {
            if strict {
                return Err(reply::refusal(
                    StatusCode::BAD_REQUEST,
                    Issue::error(IssueType::NotSupported)
                        .diagnosing(format!(
                            "{name} is not a search parameter of this type, and Prefer: \
                             handling=strict asks for the search to be refused"
                        ))
                        .at(key.clone()),
                ));
            }
            read.ignored.push(key.clone());
            continue;
        };
        let values = values(key, kind_of(kind), modifier, value)?;
        read.applied.push(Occurrence {
            name: declared_name,
            values,
        });
        read.pairs.push((key.clone(), value.clone()));
    }
    Ok(read)
}

/// Returns the kind a `CapabilityStatement` search parameter type names.
fn kind_of(kind: &str) -> Kind {
    match kind {
        "date" => Kind::Date,
        "string" => Kind::Text,
        "reference" => Kind::Reference,
        _ => Kind::Token,
    }
}

/// Reads the values of one occurrence.
fn values(key: &str, kind: Kind, modifier: Option<&str>, value: &str) -> Result<Values, Refusal> {
    let parts = fhir_search::escape::split(value, ',');
    let unmodified = || {
        modifier.map_or(Ok(()), |modifier| {
            Err(reply::refusal(
                StatusCode::BAD_REQUEST,
                Issue::error(IssueType::NotSupported)
                    .diagnosing(format!("the modifier :{modifier} is not supported here"))
                    .at(key.to_owned()),
            ))
        })
    };
    match kind {
        Kind::Token => {
            unmodified()?;
            parts
                .iter()
                .map(|part| Token::parse(part).map_err(|error| invalid(key, &error.to_string())))
                .collect::<Result<_, _>>()
                .map(Values::Tokens)
        }
        Kind::Date => {
            unmodified()?;
            parts
                .iter()
                .map(|part| {
                    DateValue::parse(part).map_err(|error| invalid(key, &error.to_string()))
                })
                .collect::<Result<_, _>>()
                .map(Values::Dates)
        }
        Kind::Reference => {
            unmodified()?;
            parts
                .iter()
                .map(|part| {
                    fhir_search::escape::unescape(part)
                        .map_err(|error| invalid(key, &error.to_string()))
                })
                .collect::<Result<_, _>>()
                .map(Values::References)
        }
        Kind::Text => {
            let matching = StringMatch::from_modifier(modifier).ok_or_else(|| {
                reply::refusal(
                    StatusCode::BAD_REQUEST,
                    Issue::error(IssueType::NotSupported)
                        .diagnosing(format!(
                            "the modifier :{} is not supported on a string",
                            modifier.unwrap_or_default()
                        ))
                        .at(key.to_owned()),
                )
            })?;
            parts
                .iter()
                .map(|part| {
                    fhir_search::escape::unescape(part)
                        .map_err(|error| invalid(key, &error.to_string()))
                })
                .collect::<Result<_, _>>()
                .map(|texts| Values::Texts(matching, texts))
        }
    }
}

/// Returns the `400` of a malformed value of `key`.
fn invalid(key: &str, text: &str) -> Refusal {
    reply::refusal(
        StatusCode::BAD_REQUEST,
        Issue::error(IssueType::Invalid)
            .diagnosing(format!("{key}: {text}"))
            .at(key.to_owned()),
    )
}

/// Whether `token` matches the code `code` of `system` (§3.1.1.4.10).
pub(super) fn coded(token: &Token, system: Option<&str>, code: Option<&str>) -> bool {
    let system_matches = match token.system() {
        System::Any => true,
        System::Absent => system.is_none(),
        System::Is(named) => system == Some(named),
    };
    system_matches && token.code().is_none_or(|named| code == Some(named))
}

/// Whether any of `tokens` matches a coding of the `CodeableConcept`s or
/// `Coding`s in `concepts`.
pub(super) fn tokens_code<'a>(tokens: &[Token], concepts: impl Iterator<Item = &'a Value>) -> bool {
    let codings: Vec<&Value> = concepts
        .flat_map(
            |concept| match concept.get("coding").and_then(Value::as_array) {
                Some(codings) => codings.iter().collect::<Vec<_>>(),
                None => vec![concept],
            },
        )
        .collect();
    tokens.iter().any(|token| {
        codings.iter().any(|coding| {
            coded(
                token,
                coding.get("system").and_then(Value::as_str),
                coding.get("code").and_then(Value::as_str),
            )
        })
    })
}

/// Whether any of `tokens` matches one of the `Identifier`s `identifiers`.
pub(super) fn tokens_identifier<'a>(
    tokens: &[Token],
    identifiers: impl Iterator<Item = &'a Value>,
) -> bool {
    let identifiers: Vec<&Value> = identifiers.collect();
    tokens.iter().any(|token| {
        identifiers.iter().any(|identifier| {
            coded(
                token,
                identifier.get("system").and_then(Value::as_str),
                identifier.get("value").and_then(Value::as_str),
            )
        })
    })
}

/// Whether any of `tokens` names the plain code `code` of `system`.
pub(super) fn tokens_plain(tokens: &[Token], system: &str, code: Option<&str>) -> bool {
    tokens
        .iter()
        .any(|token| code.is_some() && coded(token, Some(system), code))
}

/// Whether any of `texts` matches one of `fields` under `matching`.
pub(super) fn texts(matching: StringMatch, texts: &[String], fields: &[&str]) -> bool {
    texts
        .iter()
        .any(|text| fields.iter().any(|field| matching.matches(field, text)))
}

/// The instants one element covers, open where it names no bound.
#[derive(Debug, Clone, Copy)]
pub(super) struct Span {
    /// The first instant, `None` before every instant.
    from: Option<Timestamp>,
    /// The first instant after, `None` after every instant.
    until: Option<Timestamp>,
}

impl Span {
    /// Returns the span of a FHIR `date`, `dateTime` or `instant`, `None`
    /// for text that is none.
    pub(super) fn of_date(text: &str) -> Option<Self> {
        let instants = instants(text)?;
        Some(Self {
            from: Some(instants.from()),
            until: instants.until(),
        })
    }

    /// Returns the span of a FHIR `Period`: from its start to the end of
    /// its end, open on the side it leaves out (R4 `datatypes.html#Period`).
    pub(super) fn of_period(period: &Value) -> Option<Self> {
        let start = period.get("start").and_then(Value::as_str);
        let end = period.get("end").and_then(Value::as_str);
        if start.is_none() && end.is_none() {
            return None;
        }
        Some(Self {
            from: match start {
                Some(text) => Some(instants(text)?.from()),
                None => None,
            },
            until: match end {
                Some(text) => instants(text)?.until(),
                None => None,
            },
        })
    }
}

/// Returns the instants `text`, a FHIR date, dateTime or instant, covers,
/// read in UTC where it names no offset.
// NOTE: R4 search.html §3.1.1.4.7 assumes the server's local zone for a value without one;
// this server's zone is UTC.
fn instants(text: &str) -> Option<Instants> {
    SearchDate::parse(text).ok()?.instants(&TimeZone::UTC).ok()
}

/// Whether any of `dates` matches `span` (§3.1.1.4.5, the prefix table).
pub(super) fn dates(dates: &[DateValue], span: Option<Span>) -> bool {
    let Some(target) = span else {
        return false;
    };
    dates.iter().any(|date| {
        let searched = match date.prefix() {
            Prefix::Ap => date
                .date()
                .approximate(&TimeZone::UTC, Timestamp::now())
                .ok(),
            _ => date.date().instants(&TimeZone::UTC).ok(),
        };
        searched.is_some_and(|searched| compare(date.prefix(), searched, target))
    })
}

/// Whether `target` stands to `searched` as `prefix` asks.
fn compare(prefix: Prefix, searched: Instants, target: Span) -> bool {
    let low = searched.from();
    let high = searched.until();
    let starts_before = target.from.is_none_or(|from| from < low);
    let ends_after = match (target.until, high) {
        (None, Some(_)) => true,
        (Some(until), Some(high)) => until > high,
        (_, None) => false,
    };
    let contained = !starts_before && !ends_after;
    match prefix {
        Prefix::Eq => contained,
        Prefix::Ne => !contained,
        Prefix::Gt => ends_after,
        Prefix::Lt => starts_before,
        Prefix::Ge => ends_after || contained,
        Prefix::Le => starts_before || contained,
        Prefix::Sa => matches!((target.from, high), (Some(from), Some(high)) if from >= high),
        Prefix::Eb => target.until.is_some_and(|until| until <= low),
        Prefix::Ap => {
            let opens_before_high = match (target.from, high) {
                (Some(from), Some(high)) => from < high,
                _ => true,
            };
            opens_before_high && target.until.is_none_or(|until| until > low)
        }
    }
}
