// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The evaluation errors: every case where N1 says "the evaluation will end
//! and signal an error to the calling environment", and the seams' failures.
//! An error is never an empty result.

use crate::seam::SeamError;
use crate::temporal::arithmetic::ArithmeticError;

/// Why an evaluation ended without a result.
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    /// An operation that takes one item met several (N1 §4.5).
    #[error("{operation} expects a single item, the collection holds {count}")]
    Singleton {
        /// The operator or function.
        operation: String,
        /// The number of items found.
        count: usize,
    },
    /// An operation met an item of a type it does not take.
    #[error("{operation} does not apply to {found}")]
    Type {
        /// The operator or function.
        operation: String,
        /// What it met.
        found: String,
    },
    /// A function N1 and FHIR R4 do not define.
    #[error("the function {name}() is not defined")]
    UnknownFunction {
        /// The function name.
        name: String,
    },
    /// A function called with the wrong number of arguments.
    #[error("{function}() takes {expected} argument(s), {found} given")]
    Arity {
        /// The function name.
        function: String,
        /// The admitted counts.
        expected: &'static str,
        /// The count given.
        found: usize,
    },
    /// A member no type of the input defines (N1 §3).
    #[error("{type_name} has no element {element}")]
    UnknownElement {
        /// The type, or the types, navigated from.
        type_name: String,
        /// The member name.
        element: String,
    },
    /// The expression starts with a type name that is not the context's
    /// type (N1 §3).
    #[error("the expression names the type {named}, the context is a {context}")]
    ContextType {
        /// The type the expression names.
        named: String,
        /// The type of the context.
        context: String,
    },
    /// A type specifier that names no type (N1 §6.3.1).
    #[error("{name} names no type of the model")]
    UnknownType {
        /// The specifier as written.
        name: String,
    },
    /// An environment variable the environment does not define (N1 §9).
    #[error("the environment variable %{name} is not defined")]
    UnknownVariable {
        /// The variable name.
        name: String,
    },
    /// A function whose seam the environment does not supply.
    #[error("{function}() needs a seam the environment does not supply")]
    NoSeam {
        /// The function name.
        function: &'static str,
    },
    /// A seam failed.
    #[error("the seam behind {function}() failed")]
    Seam {
        /// The function name.
        function: &'static str,
        /// The seam's failure.
        #[source]
        source: SeamError,
    },
    /// A regular expression that does not compile.
    #[error("the regular expression {pattern:?} does not compile")]
    Regex {
        /// The pattern.
        pattern: String,
        /// The compiler's failure.
        #[source]
        source: regex::Error,
    },
    /// Date and time arithmetic outside its rules (N1 §6.7).
    #[error("date and time arithmetic failed")]
    Arithmetic {
        /// The failure.
        #[source]
        source: ArithmeticError,
    },
    /// The document contradicts the model, as a string where an object
    /// belongs.
    #[error("the element {element} contradicts the model: {reason}")]
    Document {
        /// The element path.
        element: String,
        /// What is wrong.
        reason: String,
    },
    /// An element carries a modifier extension `checkModifiers()` does not
    /// admit (FHIR R4 §2.9.1.5).
    #[error("the element carries the modifier extension {url}, which the call does not admit")]
    Modifier {
        /// The extension's url.
        url: String,
    },
    /// An argument outside what the function admits.
    #[error("{function}() received an invalid argument: {reason}")]
    Argument {
        /// The function name.
        function: String,
        /// What is wrong.
        reason: String,
    },
}

impl EvalError {
    /// A type error for `operation` meeting `found`.
    pub(crate) fn type_error(operation: impl Into<String>, found: impl Into<String>) -> Self {
        Self::Type {
            operation: operation.into(),
            found: found.into(),
        }
    }

    /// A singleton error for `operation` meeting `count` items.
    pub(crate) fn singleton(operation: impl Into<String>, count: usize) -> Self {
        Self::Singleton {
            operation: operation.into(),
            count,
        }
    }
}
