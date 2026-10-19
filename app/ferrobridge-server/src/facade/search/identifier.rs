// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient identifier a chained search names, resolved to the person it
//! identifies through Cadasto's experimental AQL demographic extension.
//!
//! `Observation?patient.identifier=[system|]value` chains the `patient`
//! reference to the `identifier` parameter of `Patient`
//! (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.15). The facade holds no
//! patient of its own, so the identifier is looked up in the CDR's
//! demographic data with a configured AQL query rooted at a demographic
//! class. AQL 1.1.0 defines no demographic semantics (QUERY §3.13.2), so the
//! dialect is Cadasto's experimental extension and every query is
//! configuration, never a literal here: the defaults are the identifier
//! resolve observed on Cadasto's platform. No specification governs how a
//! FHIR identifier maps to a demographic party: our own design.

use std::collections::BTreeMap;

use openehr_its::rest::generated::query::AdhocQueryExecute;
use openehr_its::rest::generated::query::ResultSetRow;

use crate::facade::search::value::System;
use crate::facade::search::value::Token;

/// The name every surface built on the dialect carries.
pub const EXTENSION: &str = "Cadasto experimental AQL demographic extension";

/// The query parameter the identifier value is bound to.
pub const IDENTIFIER_PARAMETER: &str = "identifier";

/// The query parameter the identifier system is bound to.
pub const SYSTEM_PARAMETER: &str = "system";

/// Why a configured identifier query is refused at boot.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IdentifierQueryError {
    /// The query binds no parameter it is handed.
    #[error("the query names no ${parameter}, which the facade binds the identifier's {part} to")]
    Unbound {
        /// The parameter the query lacks.
        parameter: &'static str,
        /// The part of the identifier it carries.
        part: &'static str,
    },
}

/// The configured identifier resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierSearch {
    /// The query a value with no system runs.
    query: String,
    /// The query a value with a system runs, when one is configured.
    system_query: Option<String>,
}

impl IdentifierSearch {
    /// Returns the resolve over `query` and, when one is configured,
    /// `system_query`.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierQueryError::Unbound`] when `query` names no
    /// `$identifier`, or `system_query` names no `$identifier` or no
    /// `$system`.
    pub fn new(query: &str, system_query: Option<&str>) -> Result<Self, IdentifierQueryError> {
        require(query, IDENTIFIER_PARAMETER, "value")?;
        if let Some(text) = system_query {
            require(text, IDENTIFIER_PARAMETER, "value")?;
            require(text, SYSTEM_PARAMETER, "system")?;
        }
        Ok(Self {
            query: query.to_owned(),
            system_query: system_query.map(str::to_owned),
        })
    }

    /// Whether a value naming a system can be resolved.
    #[must_use]
    pub const fn resolves_systems(&self) -> bool {
        self.system_query.is_some()
    }

    /// Returns the request that resolves `token`, or why the configured
    /// queries cannot express it.
    ///
    /// # Errors
    ///
    /// Returns [`Inexpressible`] for a token with no value, one naming the
    /// absence of a system (`|value`), and one naming a system when no
    /// system query is configured.
    pub fn request(&self, token: &Token) -> Result<AdhocQueryExecute, Inexpressible> {
        let value = token.code.as_deref().ok_or(Inexpressible::NoValue)?;
        let mut parameters = BTreeMap::new();
        parameters.insert(
            String::from(IDENTIFIER_PARAMETER),
            serde_json::Value::String(value.to_owned()),
        );
        let text = match token.system {
            System::Any => self.query.clone(),
            System::Absent => return Err(Inexpressible::NoSystem),
            System::Is(ref system) => {
                let text = self.system_query.clone().ok_or(Inexpressible::System)?;
                parameters.insert(
                    String::from(SYSTEM_PARAMETER),
                    serde_json::Value::String(system.clone()),
                );
                text
            }
        };
        Ok(AdhocQueryExecute {
            q: text,
            offset: None,
            fetch: None,
            query_parameters: Some(parameters),
            additional_properties: BTreeMap::new(),
        })
    }
}

/// Checks that `text` names `$<parameter>`.
fn require(
    text: &str,
    parameter: &'static str,
    part: &'static str,
) -> Result<(), IdentifierQueryError> {
    if names(text, parameter) {
        Ok(())
    } else {
        Err(IdentifierQueryError::Unbound { parameter, part })
    }
}

/// Whether `text` names the AQL parameter `$<parameter>` as a whole word.
fn names(text: &str, parameter: &str) -> bool {
    let wanted = format!("${parameter}");
    text.match_indices(&wanted).any(|(at, _)| {
        text.get(at.saturating_add(wanted.len())..)
            .and_then(|rest| rest.chars().next())
            .is_none_or(|next| !(next.is_alphanumeric() || next == '_'))
    })
}

/// A token value the configured queries cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Inexpressible {
    /// `[system]|`: no identifier value.
    #[error(
        "the value names a system and no identifier value, and a patient is found by its identifier value"
    )]
    NoValue,
    /// `|[value]`: an identifier with no system.
    #[error(
        "the value asks for an identifier with no system, which the configured {EXTENSION} query cannot tell from one with a system"
    )]
    NoSystem,
    /// `[system]|[value]` with no system query configured.
    #[error(
        "the value names a system, and no facade.identifier_search.system_query is configured to match one"
    )]
    System,
}

/// What a resolve found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parties {
    /// No party carries the identifier.
    None,
    /// One party carries it: its uid, without the version a versioned uid
    /// adds.
    One(String),
    /// More than one party carries it.
    Many,
}

/// A row of a resolve the facade cannot read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("row {row} of the {EXTENSION} identifier query carries no party uid in its first column")]
pub struct MalformedParty {
    /// The row, from zero.
    pub row: usize,
}

/// Reads the parties the rows of a resolve name.
///
/// Each row's first column is the party's `uid/value`. A versioned uid
/// (`object_id::creating_system_id::version_tree_id`, RM Common
/// `OBJECT_VERSION_ID`) names its party by the object id before the first
/// `::`, so two versions of one party count as one.
///
/// # Errors
///
/// Returns [`MalformedParty`] for a row whose first column is no non-empty
/// string.
pub fn parties(rows: &[ResultSetRow]) -> Result<Parties, MalformedParty> {
    let mut found: Vec<String> = Vec::new();
    for (row, values) in rows.iter().enumerate() {
        let uid = values
            .first()
            .and_then(serde_json::Value::as_str)
            .map(object_id)
            .filter(|uid| !uid.is_empty())
            .ok_or(MalformedParty { row })?;
        if !found.iter().any(|known| known == uid) {
            found.push(uid.to_owned());
        }
    }
    Ok(match found.as_slice() {
        [] => Parties::None,
        [one] => Parties::One(one.clone()),
        _ => Parties::Many,
    })
}

/// Returns the object id a possibly versioned uid names.
fn object_id(uid: &str) -> &str {
    uid.split_once("::").map_or(uid, |(object, _)| object)
}

#[cfg(test)]
mod tests {
    use super::{IdentifierQueryError, IdentifierSearch, Inexpressible, Parties, parties};
    use crate::facade::search::value::{System, Token};

    const QUERY: &str = "SELECT p/uid/value FROM PERSON p WHERE p/x = $identifier";

    fn token(system: System, code: Option<&str>) -> Token {
        Token {
            system,
            code: code.map(str::to_owned),
        }
    }

    #[test]
    fn a_query_must_bind_the_identifier() {
        assert_eq!(
            IdentifierSearch::new("SELECT p/uid/value FROM PERSON p", None),
            Err(IdentifierQueryError::Unbound {
                parameter: "identifier",
                part: "value"
            })
        );
        assert_eq!(
            IdentifierSearch::new("SELECT p FROM PERSON p WHERE p/x = $identifiers", None),
            Err(IdentifierQueryError::Unbound {
                parameter: "identifier",
                part: "value"
            })
        );
    }

    #[test]
    fn a_system_query_must_bind_the_system() {
        assert_eq!(
            IdentifierSearch::new(QUERY, Some(QUERY)),
            Err(IdentifierQueryError::Unbound {
                parameter: "system",
                part: "system"
            })
        );
    }

    #[test]
    fn a_value_binds_the_identifier() {
        let search = IdentifierSearch::new(QUERY, None).expect("a legal query");
        let request = search
            .request(&token(System::Any, Some("MRN-1")))
            .expect("a value alone resolves");
        assert_eq!(request.q, QUERY);
        assert_eq!(
            request
                .query_parameters
                .and_then(|p| p.get("identifier").cloned()),
            Some(serde_json::json!("MRN-1"))
        );
    }

    #[test]
    fn a_system_needs_a_system_query() {
        let search = IdentifierSearch::new(QUERY, None).expect("a legal query");
        let with_system = token(System::Is(String::from("urn:x")), Some("MRN-1"));
        assert_eq!(
            search.request(&with_system).err(),
            Some(Inexpressible::System)
        );
        let both = IdentifierSearch::new(
            QUERY,
            Some("SELECT p FROM PERSON p WHERE p/x = $identifier AND p/y = $system"),
        )
        .expect("a legal pair");
        let request = both.request(&with_system).expect("a system resolves");
        assert_eq!(
            request
                .query_parameters
                .and_then(|p| p.get("system").cloned()),
            Some(serde_json::json!("urn:x"))
        );
    }

    #[test]
    fn no_value_and_no_system_are_inexpressible() {
        let search = IdentifierSearch::new(QUERY, None).expect("a legal query");
        assert_eq!(
            search
                .request(&token(System::Is(String::from("urn:x")), None))
                .err(),
            Some(Inexpressible::NoValue)
        );
        assert_eq!(
            search.request(&token(System::Absent, Some("MRN-1"))).err(),
            Some(Inexpressible::NoSystem)
        );
    }

    #[test]
    fn the_rows_name_none_one_or_many_parties() {
        assert_eq!(parties(&[]), Ok(Parties::None));
        assert_eq!(
            parties(&[
                vec![serde_json::json!("p-1::cdr::1")],
                vec![serde_json::json!("p-1::cdr::2")]
            ]),
            Ok(Parties::One(String::from("p-1")))
        );
        assert_eq!(
            parties(&[
                vec![serde_json::json!("p-1::cdr::1")],
                vec![serde_json::json!("p-2")]
            ]),
            Ok(Parties::Many)
        );
        assert!(parties(&[vec![serde_json::json!(7)]]).is_err());
    }
}
