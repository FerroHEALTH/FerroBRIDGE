// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The functions of N1 §5, §7 and §10.2, and the functions FHIR R4 adds
//! (<https://hl7.org/fhir/R4/fhirpath.html#2.9.1.5>).

mod boundary;
mod collections;
mod conversion;
mod fhir;
mod math;
mod sorting;
mod strings;
mod text;
mod tree;

use super::error::EvalError;
use super::{Engine, Frame, STU3_FUNCTIONS};
use crate::ast::Expr;
use crate::value::{Collection, Item};

/// A count of items or characters as a `System.Integer`.
#[expect(
    clippy::expect_used,
    reason = "a collection or a string held in memory has fewer than i64::MAX items"
)]
fn length(count: usize) -> i64 {
    i64::try_from(count).expect("a count in memory should fit an i64")
}

/// One function invocation: its input, its unevaluated arguments and the
/// frame they are evaluated in.
pub(crate) struct Call<'c, 'e, 'a> {
    engine: &'c Engine<'e, 'a>,
    name: &'c str,
    input: Collection<'a>,
    args: &'c [Expr],
    frame: &'c Frame<'a>,
}

impl<'a> Call<'_, '_, 'a> {
    /// Checks the argument count against `min..=max`.
    fn arity(&self, min: usize, max: usize, expected: &'static str) -> Result<(), EvalError> {
        if (min..=max).contains(&self.args.len()) {
            Ok(())
        } else {
            Err(EvalError::Arity {
                function: String::from(self.name),
                expected,
                found: self.args.len(),
            })
        }
    }

    /// Evaluates argument `index` in the frame of the call, `None` when it is
    /// not given.
    fn arg(&self, index: usize) -> Result<Option<Collection<'a>>, EvalError> {
        self.args
            .get(index)
            .map(|arg| self.engine.eval(arg, self.frame))
            .transpose()
    }

    /// Evaluates argument `index` in the frame of the call, empty when not
    /// given.
    fn arg_or_empty(&self, index: usize) -> Result<Collection<'a>, EvalError> {
        Ok(self.arg(index)?.unwrap_or_default())
    }

    /// Evaluates argument `index` with `item` as `$this` and `position` as
    /// `$index` (N1 §5), and `total` as `$total` (N1 §7).
    fn lambda(
        &self,
        index: usize,
        item: &Item<'a>,
        position: usize,
        total: Option<Collection<'a>>,
    ) -> Result<Collection<'a>, EvalError> {
        let Some(arg) = self.args.get(index) else {
            return Ok(Vec::new());
        };
        let frame = Frame {
            focus: vec![item.clone()],
            index: Some(position),
            total,
        };
        self.engine.eval(arg, &frame)
    }

    /// The model.
    fn model(&self) -> &'a dyn crate::model::Model {
        self.engine.model()
    }
}

impl<'a> Engine<'_, 'a> {
    /// Calls the function `name` on `input`.
    pub(crate) fn call(
        &self,
        name: &str,
        input: Collection<'a>,
        args: &[Expr],
        frame: &Frame<'a>,
    ) -> Result<Collection<'a>, EvalError> {
        if STU3_FUNCTIONS.contains(&name) && !self.env.stu3_functions {
            return Err(EvalError::UnknownFunction {
                name: String::from(name),
            });
        }
        let call = Call {
            engine: self,
            name,
            input,
            args,
            frame,
        };
        if let Some(result) = boundary::call(&call)? {
            return Ok(result);
        }
        if let Some(result) = sorting::call(&call)? {
            return Ok(result);
        }
        if let Some(result) = text::call(&call)? {
            return Ok(result);
        }
        if let Some(result) = collections::call(&call)? {
            return Ok(result);
        }
        if let Some(result) = conversion::call(&call)? {
            return Ok(result);
        }
        if let Some(result) = strings::call(&call)? {
            return Ok(result);
        }
        if let Some(result) = math::call(&call)? {
            return Ok(result);
        }
        if let Some(result) = tree::call(&call)? {
            return Ok(result);
        }
        if let Some(result) = fhir::call(&call)? {
            return Ok(result);
        }
        Err(EvalError::UnknownFunction {
            name: String::from(name),
        })
    }
}
