// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The items a collection holds: the FHIRPath system values (N1 §4.1), the
//! FHIR elements of the document under evaluation, and the type descriptions
//! `type()` returns (N1 §10.2).

pub mod node;

use std::fmt;

use rust_decimal::Decimal;

use crate::quantity::Quantity;
use crate::temporal::{Date, DateTime, Time};
use node::Node;

/// An ordered collection, the result of every expression (N1 §3.1).
pub type Collection<'a> = Vec<Item<'a>>;

/// One item of a collection.
#[derive(Debug, Clone, PartialEq)]
pub enum Item<'a> {
    /// `System.Boolean`.
    Boolean(bool),
    /// `System.String`.
    String(String),
    /// `System.Integer`, held in the 32-bit range of N1 §4.1.3.
    Integer(i64),
    /// `System.Decimal`.
    Decimal(Decimal),
    /// `System.Date`.
    Date(Date),
    /// `System.DateTime`.
    DateTime(DateTime),
    /// `System.Time`.
    Time(Time),
    /// `System.Quantity`.
    Quantity(Quantity),
    /// An element or resource of the document.
    Node(Node<'a>),
    /// A type description from `type()` (N1 §10.2).
    Type(TypeInfo),
}

/// A type description (N1 §10.2): `SimpleTypeInfo` or `ClassInfo` with a
/// name, `TupleTypeInfo` for a backbone element, which has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    /// `System` or `FHIR`.
    pub namespace: &'static str,
    /// The type name; `None` for an anonymous type.
    pub name: Option<String>,
}

/// The system types of N1 §4.1, the `System` namespace.
pub const SYSTEM_TYPES: [&str; 9] = [
    "Any", "Boolean", "String", "Integer", "Decimal", "Date", "DateTime", "Time", "Quantity",
];

impl Item<'_> {
    /// The name of the system type a system value has, `None` for a node or a
    /// type description.
    #[must_use]
    pub const fn system_type(&self) -> Option<&'static str> {
        match self {
            Self::Boolean(_) => Some("Boolean"),
            Self::String(_) => Some("String"),
            Self::Integer(_) => Some("Integer"),
            Self::Decimal(_) => Some("Decimal"),
            Self::Date(_) => Some("Date"),
            Self::DateTime(_) => Some("DateTime"),
            Self::Time(_) => Some("Time"),
            Self::Quantity(_) => Some("Quantity"),
            Self::Node(_) | Self::Type(_) => None,
        }
    }

    /// The node this item is, when it is one.
    #[must_use]
    pub const fn as_node(&self) -> Option<&Node<'_>> {
        match self {
            Self::Node(node) => Some(node),
            _ => None,
        }
    }
}

impl fmt::Display for Item<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boolean(value) => write!(f, "{value}"),
            Self::String(value) => f.write_str(value),
            Self::Integer(value) => write!(f, "{value}"),
            Self::Decimal(value) => write!(f, "{value}"),
            Self::Date(value) => write!(f, "{value}"),
            Self::DateTime(value) => write!(f, "{value}"),
            Self::Time(value) => write!(f, "{value}"),
            Self::Quantity(value) => write!(f, "{value}"),
            Self::Node(node) => write!(f, "{node}"),
            Self::Type(info) => match &info.name {
                Some(name) => write!(f, "{}.{name}", info.namespace),
                None => write!(f, "{}.(anonymous)", info.namespace),
            },
        }
    }
}
