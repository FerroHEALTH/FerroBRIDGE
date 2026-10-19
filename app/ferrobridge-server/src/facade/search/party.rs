// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR search on the resources a party context serves, through Cadasto's
//! experimental AQL demographic extension.
//!
//! The ITS-REST 1.1.0 Demographic API has no search or list operation, and
//! AQL 1.1.0 defines no demographic semantics (QUERY §3.13.2), so a party
//! type is searched with configured queries rooted at a demographic class,
//! run over `POST /query/aql` as the extension's guide runs it
//! (<https://docs.cadasto.io/docs/guides/aql-archetype-query-language>,
//! §Querying demographic data): one for `identifier`, one for `name`. `_id`
//! needs no query, because the id is the party's `versioned_object_uid` and
//! the Demographic API reads it. Each query is configuration per resource
//! type, kept to the forms the guide documents or Cadasto observed as stable
//! (a root, `WHERE`, `LIKE`, `ORDER BY` and the `CLUSTER` containment of an
//! identifier), and the facade pages it with the query's `offset` and
//! `fetch`. No specification governs a party search: our own design.

use std::collections::BTreeMap;

use openehr_its::rest::generated::query::AdhocQueryExecute;

use crate::facade::search::identifier::IdentifierQueryError;
use crate::facade::search::identifier::IdentifierSearch;
use crate::facade::search::identifier::require;

/// The token parameter every party resource of R4 defines
/// (<https://hl7.org/fhir/R4/patient.html#search>).
pub const IDENTIFIER: &str = "identifier";

/// The string parameter `Patient` and `Practitioner` define over their names
/// (<https://hl7.org/fhir/R4/patient.html#search>).
pub const NAME: &str = "name";

/// The logical id parameter every resource takes
/// (<https://hl7.org/fhir/R4/search.html>, §3.1.1.4.1).
pub const ID: &str = "_id";

/// The query parameter a `name` value is bound to as typed.
pub const NAME_PARAMETER: &str = "name";

/// The query parameter a `name` value is bound to in lower case.
pub const NAME_LOWER: &str = "name_lower";

/// The query parameter a `name` value is bound to with its first letter in
/// upper case and the rest in lower case.
pub const NAME_CAPITALISED: &str = "name_capitalised";

/// The `LIKE` wildcard the dialect matches any run of characters with, as
/// the guide writes it (`LIKE '*insulin*'`).
const WILDCARD: char = '*';

/// The `LIKE` wildcard the dialect matches one character with.
const ONE: char = '?';

/// The configured `name` query of one party type.
///
/// A string parameter matches a field that "equals or starts with the
/// supplied parameter value, after both have been normalized by case and
/// combining characters" (<https://hl7.org/fhir/R4/search.html>,
/// §3.1.1.4.8). The dialect's `LIKE` is case-sensitive (observed on Cadasto's
/// engine), so the value is bound three times as a prefix pattern, as typed,
/// in lower case and capitalised, and the query compares the name with each.
/// A name whose case is none of the three, and every accent difference, is
/// a stated gap until the dialect can fold case and accents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameSearch {
    /// The query.
    query: String,
}

/// A `name` value the configured query cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NameInexpressible {
    /// An empty value.
    #[error("the value is empty, and a name is found by the start of its text")]
    Empty,
    /// A value carrying a `LIKE` wildcard, which the dialect would read as
    /// one.
    #[error("the value carries `*` or `?`, which the dialect's LIKE reads as a wildcard")]
    Wildcard,
}

impl NameSearch {
    /// Returns the search over `query`.
    ///
    /// # Errors
    ///
    /// Returns [`IdentifierQueryError::Unbound`] when `query` names no
    /// `$name`.
    pub fn new(query: &str) -> Result<Self, IdentifierQueryError> {
        require(query, NAME_PARAMETER, "value")?;
        Ok(Self {
            query: query.to_owned(),
        })
    }

    /// Returns the request that finds the parties whose name starts with
    /// `value`.
    ///
    /// # Errors
    ///
    /// Returns [`NameInexpressible`] for an empty value or one carrying a
    /// wildcard.
    pub fn request(&self, value: &str) -> Result<AdhocQueryExecute, NameInexpressible> {
        if value.is_empty() {
            return Err(NameInexpressible::Empty);
        }
        if value.contains([WILDCARD, ONE]) {
            return Err(NameInexpressible::Wildcard);
        }
        let prefix = |text: String| serde_json::Value::String(format!("{text}{WILDCARD}"));
        let parameters = BTreeMap::from([
            (String::from(NAME_PARAMETER), prefix(value.to_owned())),
            (String::from(NAME_LOWER), prefix(value.to_lowercase())),
            (String::from(NAME_CAPITALISED), prefix(capitalised(value))),
        ]);
        Ok(AdhocQueryExecute {
            q: self.query.clone(),
            offset: None,
            fetch: None,
            query_parameters: Some(parameters),
            additional_properties: BTreeMap::new(),
        })
    }
}

/// Returns `value` with its first letter in upper case and the rest in lower
/// case.
fn capitalised(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.as_str().to_lowercase().chars())
            .collect(),
        None => String::new(),
    }
}

/// The configured queries of one party type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartyQueries {
    /// The `identifier` query, when one is configured.
    identifier: Option<IdentifierSearch>,
    /// The `name` query, when one is configured.
    name: Option<NameSearch>,
}

impl PartyQueries {
    /// Returns the queries `identifier` and `name`.
    #[must_use]
    pub const fn new(identifier: Option<IdentifierSearch>, name: Option<NameSearch>) -> Self {
        Self { identifier, name }
    }

    /// Returns the `identifier` query, when one is configured.
    #[must_use]
    pub const fn identifier(&self) -> Option<&IdentifierSearch> {
        self.identifier.as_ref()
    }

    /// Returns the `name` query, when one is configured.
    #[must_use]
    pub const fn name(&self) -> Option<&NameSearch> {
        self.name.as_ref()
    }

    /// Returns the search parameters the type takes, in name order: `_id`
    /// always, `identifier` and `name` when their query is configured.
    #[must_use]
    pub fn parameters(&self) -> Vec<&'static str> {
        let mut parameters = vec![ID];
        if self.identifier.is_some() {
            parameters.push(IDENTIFIER);
        }
        if self.name.is_some() {
            parameters.push(NAME);
        }
        parameters
    }
}

/// The configured queries, by the resource type they search.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartySearch {
    /// The queries of each searched type.
    types: BTreeMap<String, PartyQueries>,
}

impl PartySearch {
    /// Returns the search over `types`.
    #[must_use]
    pub const fn new(types: BTreeMap<String, PartyQueries>) -> Self {
        Self { types }
    }

    /// Returns the queries `resource_type` is searched with, `None` when it
    /// is not searched.
    #[must_use]
    pub fn of_type(&self, resource_type: &str) -> Option<&PartyQueries> {
        self.types.get(resource_type)
    }

    /// Returns every searched type, in name order.
    pub fn types(&self) -> impl Iterator<Item = &str> {
        self.types.keys().map(String::as_str)
    }
}

/// Returns `request` reading `fetch` rows from `offset`.
#[must_use]
pub fn page(mut request: AdhocQueryExecute, offset: i64, fetch: i64) -> AdhocQueryExecute {
    request.offset = Some(offset);
    request.fetch = Some(fetch);
    request
}

#[cfg(test)]
mod tests {
    use super::{NameInexpressible, NameSearch, page};
    use crate::facade::search::identifier::IdentifierSearch;
    use crate::facade::search::value::{System, Token};

    #[test]
    fn a_page_binds_the_identifier_and_pages_the_query() {
        let query = IdentifierSearch::new(
            "SELECT p/uid/value FROM PERSON p WHERE x = $identifier",
            None,
        )
        .expect("the query binds the identifier");
        let token = Token {
            system: System::Any,
            code: Some(String::from("SYN-0001")),
        };
        let request = page(
            query.request(&token).expect("the token is expressible"),
            20,
            11,
        );
        assert_eq!(request.offset, Some(20));
        assert_eq!(request.fetch, Some(11));
        assert_eq!(
            request
                .query_parameters
                .as_ref()
                .and_then(|parameters| parameters.get("identifier")),
            Some(&serde_json::Value::from("SYN-0001"))
        );
    }

    /// R4 search.html §3.1.1.4.8: a string matches a field that starts with
    /// the value after both are normalized by case.
    #[test]
    fn a_name_is_bound_as_three_case_variants_of_a_prefix() {
        let query = NameSearch::new("SELECT p/uid/value FROM PERSON p WHERE n LIKE $name")
            .expect("the query binds the name");
        let request = query
            .request("sYNthetic")
            .expect("the value is expressible");
        let parameters = request.query_parameters.expect("parameters");
        assert_eq!(parameters["name"], "sYNthetic*");
        assert_eq!(parameters["name_lower"], "synthetic*");
        assert_eq!(parameters["name_capitalised"], "Synthetic*");
    }

    #[test]
    fn a_name_carrying_a_wildcard_or_nothing_is_inexpressible() {
        let query = NameSearch::new("SELECT p/uid/value FROM PERSON p WHERE n LIKE $name")
            .expect("the query binds the name");
        assert_eq!(
            query.request("Syn*").err(),
            Some(NameInexpressible::Wildcard)
        );
        assert_eq!(query.request("").err(), Some(NameInexpressible::Empty));
    }

    #[test]
    fn a_name_query_binding_no_name_is_refused() {
        assert!(
            NameSearch::new("SELECT p/uid/value FROM PERSON p WHERE n LIKE $name_lower").is_err()
        );
    }
}
