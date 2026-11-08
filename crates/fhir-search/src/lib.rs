// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 search value grammar (<https://hl7.org/fhir/R4/search.html>,
//! §3.1.1.4).
//!
//! A search parameter's value reaches a server percent-decoded and is then
//! read in two steps. [`escape::split`] cuts it into the alternatives of an
//! `OR` at every `,` no `\` escapes, and each alternative is read by the
//! grammar of its parameter type: [`escape::unescape`] for a string or a
//! uri, [`token::Token`] and [`token::OfType`] for a token,
//! [`date::DateValue`] for a date, whose [`prefix::Prefix`] is shared with
//! the other ordered types. [`string::StringMatch`] is how a string value
//! matches a field.
//!
//! No error and no `Debug` output carries a value: a code may be an
//! identifier and a date a birth date.
//!
//! ```
//! use fhir_search::date::DateValue;
//! use fhir_search::escape::split;
//! use fhir_search::prefix::Prefix;
//! use fhir_search::token::{System, Token};
//!
//! let tokens: Vec<Token> = split(r"http://loinc.org|8480-6,a\,b", ',')
//!     .iter()
//!     .map(|part| Token::parse(part))
//!     .collect::<Result<_, _>>()?;
//! assert_eq!(tokens[0].system(), System::Is("http://loinc.org"));
//! assert_eq!(tokens[1].code(), Some("a,b"));
//!
//! let date = DateValue::parse("ge2026-12")?;
//! assert_eq!(date.prefix(), Prefix::Ge);
//! let range = date.date().range();
//! assert_eq!(range.low().to_string(), "2026-12");
//! assert_eq!(range.high().map(ToString::to_string).as_deref(), Some("2027-01"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![doc(test(attr(deny(warnings))))]

pub mod date;
pub mod escape;
pub mod prefix;
pub mod string;
pub mod token;
