// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A parsed expression: parsed once, then checked and evaluated many times.

use std::fmt;
use std::str::FromStr;

use crate::ast::Expr;
use crate::check::{SemanticError, check, check_any};
use crate::eval::error::EvalError;
use crate::eval::{Environment, Focus, evaluate};
use crate::model::{FhirType, Model};
use crate::parse::{ParseError, parse};
use crate::value::Collection;

/// A FHIRPath expression, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    source: String,
    tree: Expr,
}

impl Expression {
    /// Parses `text`.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] when `text` is no N1 expression.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        Ok(Self {
            source: String::from(text),
            tree: parse(text)?,
        })
    }

    /// The text the expression was parsed from.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The syntax tree.
    #[must_use]
    pub const fn tree(&self) -> &Expr {
        &self.tree
    }

    /// Checks the expression against the type its context will have: every
    /// member a path names must be one the types along the path define, and
    /// a path that starts with a type name must start with the context's (N1
    /// §3, §11).
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] for the first member or type name the model
    /// refuses.
    pub fn check(&self, model: &dyn Model, context: FhirType) -> Result<(), SemanticError> {
        check(&self.tree, model, context)
    }

    /// Checks the expression against a context that may hold any of the
    /// types `contexts`, as an occurrence of a choice element does (R4
    /// §2.9.1.1, [`crate::model::path_types`]): a member is accepted when one
    /// of them defines it, and refused when none does.
    ///
    /// # Errors
    ///
    /// Returns [`SemanticError`] for the first member or type name the model
    /// refuses.
    pub fn check_any(&self, model: &dyn Model, contexts: &[FhirType]) -> Result<(), SemanticError> {
        check_any(&self.tree, model, contexts)
    }

    /// Evaluates the expression in `env` on `focus`.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError`] when the evaluation signals an error; an empty
    /// result is `Ok` with an empty collection.
    pub fn evaluate<'a>(
        &self,
        env: &Environment<'a>,
        focus: Focus<'a>,
    ) -> Result<Collection<'a>, EvalError> {
        evaluate(&self.tree, env, focus)
    }
}

impl FromStr for Expression {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}
