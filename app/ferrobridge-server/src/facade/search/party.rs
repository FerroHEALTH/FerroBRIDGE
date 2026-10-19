// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FHIR search on the resources a party context serves, through Cadasto's
//! experimental AQL demographic extension.
//!
//! The ITS-REST 1.1.0 Demographic API has no search or list operation, and
//! AQL 1.1.0 defines no demographic semantics (QUERY §3.13.2), so a party
//! type is searched by its `identifier` with a configured query rooted at a
//! demographic class, run over `POST /query/aql` as the extension's guide
//! runs it (<https://docs.cadasto.io/docs/guides/aql-archetype-query-language>,
//! §Querying demographic data). Each query is configuration per resource
//! type, the forms kept to the ones the guide documents or Cadasto observed
//! as stable (a root, `WHERE`, `ORDER BY` and the `CLUSTER` containment of
//! an identifier), and the facade pages it with the query's `offset` and
//! `fetch`. No specification governs a party search: our own design.

use std::collections::BTreeMap;

use openehr_its::rest::generated::query::AdhocQueryExecute;

use crate::facade::search::identifier::IdentifierSearch;
use crate::facade::search::identifier::Inexpressible;
use crate::facade::search::value::Token;

/// The one search parameter a party type takes, `identifier`, the token
/// parameter every party resource of R4 defines
/// (<https://hl7.org/fhir/R4/patient.html#search>).
pub const PARAMETER: &str = "identifier";

/// The configured identifier queries, by the resource type they search.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartySearch {
    /// The query of each searched type.
    types: BTreeMap<String, IdentifierSearch>,
}

impl PartySearch {
    /// Returns the search over `types`.
    #[must_use]
    pub const fn new(types: BTreeMap<String, IdentifierSearch>) -> Self {
        Self { types }
    }

    /// Returns the query `resource_type` is searched with, `None` when it is
    /// not searched.
    #[must_use]
    pub fn of_type(&self, resource_type: &str) -> Option<&IdentifierSearch> {
        self.types.get(resource_type)
    }

    /// Returns every searched type, in name order.
    pub fn types(&self) -> impl Iterator<Item = &str> {
        self.types.keys().map(String::as_str)
    }
}

/// Returns the request of one page of the search `query` runs for `token`:
/// `fetch` rows from `offset`.
///
/// # Errors
///
/// Returns [`Inexpressible`] when the configured query cannot express the
/// token.
pub fn page(
    query: &IdentifierSearch,
    token: &Token,
    offset: i64,
    fetch: i64,
) -> Result<AdhocQueryExecute, Inexpressible> {
    let mut request = query.request(token)?;
    request.offset = Some(offset);
    request.fetch = Some(fetch);
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::page;
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
        let request = page(&query, &token, 20, 11).expect("the token is expressible");
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
}
