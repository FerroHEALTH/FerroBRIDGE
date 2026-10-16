// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The paging cursor a `next` link carries.
//!
//! "Typically, a server will provide its own parameters in the links that it
//! uses to manage the state of the search as pages are retrieved. These
//! parameters do not need to be understood or processed by the client"
//! (<https://hl7.org/fhir/R4/search.html>, §3.1.1.5.3). The cursor names the
//! declaration a page starts in, the row offset inside it, how many entries
//! of the composition at that row an earlier page answered, and a fingerprint
//! of the search it belongs to, so a cursor pasted onto another search is
//! refused rather than read as a row number of the wrong query. No
//! specification governs the shape: our own design.

use sha2::Digest;

/// How many bytes of the digest the fingerprint keeps, two hex characters
/// each.
const FINGERPRINT_BYTES: usize = 8;

/// Where one page starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// The index of the declaration, in context-name order.
    pub declaration: usize,
    /// The row offset inside that declaration's query.
    pub offset: i64,
    /// How many entries of the composition at `offset` an earlier page
    /// answered, in the order its rows first name them.
    pub skip: usize,
}

impl Position {
    /// The first row of the first declaration.
    pub const START: Self = Self {
        declaration: 0,
        offset: 0,
        skip: 0,
    };
}

/// Why a cursor is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CursorError {
    /// The cursor is not one this server writes.
    #[error("`{0}` is no paging cursor this server wrote")]
    Malformed(String),
    /// The cursor belongs to a search with other parameters.
    #[error("the paging cursor belongs to another search; follow the next link as it was answered")]
    Foreign,
}

/// Returns the fingerprint of the search `used` describes on `resource_type`.
#[must_use]
pub fn fingerprint(resource_type: &str, used: &[(String, String)]) -> String {
    let mut hasher = sha2::Sha256::new();
    for field in std::iter::once(resource_type).chain(
        used.iter()
            .flat_map(|(name, value)| [name.as_str(), value.as_str()]),
    ) {
        hasher.update(u64::try_from(field.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(field.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .take(FINGERPRINT_BYTES)
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

/// Returns the cursor of `position` in the search `fingerprint` names.
#[must_use]
pub fn encode(position: Position, fingerprint: &str) -> String {
    format!(
        "{}.{}.{}.{fingerprint}",
        position.declaration, position.offset, position.skip
    )
}

/// Reads a cursor of the search `fingerprint` names.
///
/// # Errors
///
/// Returns [`CursorError::Malformed`] for a value this server does not write
/// and [`CursorError::Foreign`] for one of another search.
pub fn decode(text: &str, fingerprint: &str) -> Result<Position, CursorError> {
    let malformed = || CursorError::Malformed(text.to_owned());
    let mut parts = text.split('.');
    let (Some(declaration), Some(offset), Some(skip), Some(print), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return Err(malformed());
    };
    let declaration = declaration.parse::<usize>().map_err(|_parse| malformed())?;
    let offset = offset
        .parse::<i64>()
        .ok()
        .filter(|offset| *offset >= 0)
        .ok_or_else(malformed)?;
    let skip = skip.parse::<usize>().map_err(|_parse| malformed())?;
    if print != fingerprint {
        return Err(CursorError::Foreign);
    }
    Ok(Position {
        declaration,
        offset,
        skip,
    })
}

#[cfg(test)]
mod tests {
    use super::{CursorError, Position, decode, encode, fingerprint};

    fn used(value: &str) -> Vec<(String, String)> {
        vec![(String::from("code"), value.to_owned())]
    }

    #[test]
    fn a_cursor_round_trips_inside_its_own_search() {
        let print = fingerprint("Condition", &used("44054006"));
        let position = Position {
            declaration: 1,
            offset: 40,
            skip: 2,
        };
        assert_eq!(Ok(position), decode(&encode(position, &print), &print));
    }

    #[test]
    fn a_cursor_of_another_search_is_refused() {
        let print = fingerprint("Condition", &used("44054006"));
        let other = fingerprint("Condition", &used("73211009"));
        assert_ne!(print, other);
        let cursor = encode(Position::START, &print);
        assert_eq!(Err(CursorError::Foreign), decode(&cursor, &other));
    }

    #[test]
    fn a_cursor_this_server_did_not_write_is_refused() {
        let print = fingerprint("Condition", &[]);
        for text in [
            "",
            "1",
            "1.2",
            "1.2.x",
            "a.2.0.x",
            "1.-2.0.x",
            "1.2.-1.x",
            "1.2.y.x",
            "1.2.0.x.y",
        ] {
            assert!(
                matches!(decode(text, &print), Err(CursorError::Malformed(_))),
                "{text}"
            );
        }
    }
}
