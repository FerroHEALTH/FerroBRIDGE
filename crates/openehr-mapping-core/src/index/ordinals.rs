// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ordinal options an ADL 1.4 operational template constrains at each
//! `DV_ORDINAL` node.
//!
//! An OPT 1.4 `C_DV_ORDINAL` lists the `DV_ORDINAL` values a node admits,
//! each a `value` and a coded `symbol` (the openEHR OPT 1.4 XML schema,
//! `C_DV_ORDINAL`; openEHR RM Release 1.1.0, data types, the `DV_ORDINAL`
//! class). The `openehr-sdt` builder keeps each entry on the node as a coded
//! option with its ordinal and the terminology of its symbol, and the index
//! keys the list by the node's `aqlPath`.

/// One option of a `C_DV_ORDINAL` constraint: the ordinal value and the code
/// of its symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrdinalOption {
    /// The ordinal value, `DV_ORDINAL.value`.
    value: i32,
    /// The code of the symbol, `DV_ORDINAL.symbol.defining_code.code_string`.
    code: String,
    /// The terminology of the symbol, the `terminology_id` of its defining
    /// code (`local` for an archetype's own term set).
    terminology: String,
}

impl OrdinalOption {
    /// Creates an option of `value` with the symbol `code` in `terminology`.
    #[must_use]
    pub fn new(value: i32, code: &str, terminology: &str) -> Self {
        Self {
            value,
            code: code.to_owned(),
            terminology: terminology.to_owned(),
        }
    }

    /// Returns the ordinal value.
    #[must_use]
    pub const fn value(&self) -> i32 {
        self.value
    }

    /// Returns the code of the symbol.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the terminology of the symbol.
    #[must_use]
    pub fn terminology(&self) -> &str {
        &self.terminology
    }
}
