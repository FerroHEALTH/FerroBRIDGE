// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FHIRPath evaluator: FHIRPath Normative Release 2.0.0
//! (<https://hl7.org/fhirpath/N1/>) with the additions FHIR R4 makes to it
//! (<https://hl7.org/fhir/R4/fhirpath.html>), over the lexical document model
//! and the generated element table of `fhir-types`.
//!
//! [`expression::Expression::parse`] parses an expression once; it is then
//! checked against a context type with [`expression::Expression::check`] and
//! evaluated any number of times with
//! [`expression::Expression::evaluate`], in an [`eval::Environment`] on an
//! [`eval::Focus`].
//!
//! ```
//! use ferrobridge_fhirpath::eval::{Environment, Focus};
//! use ferrobridge_fhirpath::expression::Expression;
//! use ferrobridge_fhirpath::model::r4::R4;
//! use ferrobridge_fhirpath::value::Item;
//! use fhir_types::codec::Value;
//!
//! let patient: Value = serde_json::from_str(
//!     r#"{"resourceType": "Patient", "name": [{"given": ["Ada"]}]}"#,
//! )?;
//! let expression = Expression::parse("Patient.name.given.first() = 'Ada'")?;
//! let environment = Environment::new(&R4);
//! let result = expression.evaluate(&environment, Focus::resource(&patient))?;
//! assert_eq!(result, vec![Item::Boolean(true)]);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Decimals are exact; dates and times keep the precision they were written
//! to. An evaluation error and an empty result are distinct outcomes. The
//! functions that reach outside the document call seams the caller supplies
//! ([`seam`]), and a call without its seam is an error naming the function.

#![doc(test(attr(deny(warnings))))]

pub mod ast;
pub mod check;
pub mod eval;
pub mod expression;
mod html;
mod lex;
pub mod model;
pub mod parse;
pub mod quantity;
pub mod seam;
pub mod temporal;
pub mod ucum;
pub mod value;
