// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The syntax tree a parsed expression holds, one node per production of the
//! N1 grammar (<https://hl7.org/fhirpath/N1/grammar.html>).

use std::fmt;

use rust_decimal::Decimal;

use crate::quantity::Unit;
use crate::temporal::{Date, DateTime, Time};

/// A literal (N1 §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    /// `true` or `false`.
    Boolean(bool),
    /// A string.
    String(String),
    /// A number without a fraction.
    Integer(i64),
    /// A number with a fraction.
    Decimal(Decimal),
    /// `@YYYY-MM-DD`, partial or whole.
    Date(Date),
    /// `@YYYY-MM-DDThh:mm:ss.fff(+|-)hh:mm`, partial or whole.
    DateTime(DateTime),
    /// `@Thh:mm:ss.fff`, partial or whole.
    Time(Time),
    /// A number and a unit.
    Quantity(Decimal, Unit),
}

/// A type specifier: a type name with an optional model qualifier (N1 §6.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSpecifier {
    /// The model name (`FHIR`, `System`), when given.
    pub namespace: Option<String>,
    /// The type name.
    pub name: String,
}

impl fmt::Display for TypeSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.namespace {
            Some(namespace) => write!(f, "{namespace}.{}", self.name),
            None => f.write_str(&self.name),
        }
    }
}

/// A binary operator (N1 §6), in the grammar's precedence groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// `*`
    Multiply,
    /// `/`
    Divide,
    /// `div`
    Div,
    /// `mod`
    Mod,
    /// `+`
    Add,
    /// `-`
    Subtract,
    /// `&`
    Concatenate,
    /// `|`
    Union,
    /// `<`
    Less,
    /// `<=`
    LessOrEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterOrEqual,
    /// `=`
    Equal,
    /// `~`
    Equivalent,
    /// `!=`
    NotEqual,
    /// `!~`
    NotEquivalent,
    /// `in`
    In,
    /// `contains`
    Contains,
    /// `and`
    And,
    /// `or`
    Or,
    /// `xor`
    Xor,
    /// `implies`
    Implies,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Div => "div",
            Self::Mod => "mod",
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Concatenate => "&",
            Self::Union => "|",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
            Self::Equal => "=",
            Self::Equivalent => "~",
            Self::NotEqual => "!=",
            Self::NotEquivalent => "!~",
            Self::In => "in",
            Self::Contains => "contains",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Implies => "implies",
        })
    }
}

/// One node of the syntax tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A literal.
    Literal(Literal),
    /// `{ }`, the empty collection.
    Empty,
    /// `%name`, an environment variable (N1 §9).
    External(String),
    /// `$this`.
    This,
    /// `$index`.
    Index,
    /// `$total`.
    Total,
    /// A member: `name` at the start of a path, or `input.name`.
    Member {
        /// The collection navigated from; `None` at the start of a path.
        input: Option<Box<Expr>>,
        /// The member name.
        name: String,
    },
    /// A function invocation: `name(args)`, or `input.name(args)`.
    Call {
        /// The input collection; `None` when the function starts a path.
        input: Option<Box<Expr>>,
        /// The function name.
        name: String,
        /// The arguments, unevaluated.
        args: Vec<Expr>,
    },
    /// `input.$this`, `input.$index` or `input.$total`.
    Invocation {
        /// The input collection.
        input: Box<Expr>,
        /// The special variable.
        variable: Box<Expr>,
    },
    /// `input[index]`.
    Indexer {
        /// The collection indexed.
        input: Box<Expr>,
        /// The index expression.
        index: Box<Expr>,
    },
    /// Unary `-`.
    Negate(Box<Expr>),
    /// Unary `+`.
    Plus(Box<Expr>),
    /// A binary operator.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// The left operand.
        left: Box<Expr>,
        /// The right operand.
        right: Box<Expr>,
    },
    /// `operand is type`.
    Is {
        /// The operand.
        operand: Box<Expr>,
        /// The type.
        type_specifier: TypeSpecifier,
    },
    /// `operand as type`.
    As {
        /// The operand.
        operand: Box<Expr>,
        /// The type.
        type_specifier: TypeSpecifier,
    },
}

impl Expr {
    /// The type specifier this expression spells, when it is a qualified
    /// identifier, as the argument of `is()`, `as()` and `ofType()` is (N1
    /// §5.2.4, §6.3.2).
    #[must_use]
    pub fn as_type_specifier(&self) -> Option<TypeSpecifier> {
        match self {
            Self::Member { input: None, name } => Some(TypeSpecifier {
                namespace: None,
                name: name.clone(),
            }),
            Self::Member {
                input: Some(input),
                name,
            } => match input.as_ref() {
                Self::Member {
                    input: None,
                    name: namespace,
                } => Some(TypeSpecifier {
                    namespace: Some(namespace.clone()),
                    name: name.clone(),
                }),
                _ => None,
            },
            _ => None,
        }
    }
}
