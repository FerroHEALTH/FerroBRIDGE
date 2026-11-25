// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology rules: the transcoding of an openEHR text item, the
//! `required` bindings held against vendored expansions, the displays of a
//! received document, and a terminology server that fails closed.
//!
//! Every code system and code here is synthetic, under `example.org`
//! (RFC 6761 §6.5).

mod bindings;
mod displays;
mod ehn;
#[cfg(feature = "openehr")]
mod local;
mod server;
#[cfg(feature = "openehr")]
mod system;
#[cfg(feature = "openehr")]
mod transcode;
mod translation;
#[cfg(feature = "openehr")]
mod version;
#[cfg(feature = "openehr")]
mod withheld;

use async_trait::async_trait;
use eehrxf::terminology::Candidate;
use eehrxf::terminology::Match;
use eehrxf::terminology::Operation;
use eehrxf::terminology::TerminologyError;
use eehrxf::terminology::TerminologyServer;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::primitives::Code;
use fhir_types::r4::primitives::Uri;
use http::StatusCode;

/// A preferred code system.
pub(crate) const PREFERRED: &str = "http://example.org/fhir/CodeSystem/preferred";

/// The concept map the scripted server translates with, at its version.
pub(crate) const MAP: &str = "http://example.org/fhir/ConceptMap/local-to-preferred|1.0.0";

/// A code system no deployment prefers.
pub(crate) const LOCAL: &str = "http://example.org/fhir/CodeSystem/local";

/// What the scripted server answers every operation with.
#[derive(Debug, Clone)]
pub(crate) enum Script {
    /// `$translate` answers these matches, each an equivalence, a system
    /// and a code.
    Translate(Vec<(&'static str, &'static str, &'static str)>),
    /// `$translate` answers each entry whose source system, source code and
    /// target system are asked as an `equivalent` match to its code, and no
    /// match to anything else.
    #[cfg_attr(
        not(all(feature = "openehr", feature = "patient-summary")),
        expect(dead_code, reason = "only the EPS sections suite scripts a map")
    )]
    Map(Vec<Mapped>),
    /// `$lookup` answers this display.
    Display(&'static str),
    /// `$validate-code` answers this result.
    Valid(bool),
    /// `$translate` answers no match, and `$lookup` this English
    /// designation.
    Unmapped(&'static str),
    /// Every operation is refused with this status.
    Refuse(StatusCode),
}

/// One entry of [`Script::Map`]: the source system, the source code, the
/// target system and the target code.
pub(crate) type Mapped = (&'static str, &'static str, &'static str, &'static str);

/// A terminology server that answers from a script.
#[derive(Debug, Clone)]
pub(crate) struct Scripted(pub(crate) Script);

impl Scripted {
    /// The refusal of `operation` the script gives, for an operation it
    /// does not answer.
    fn refusal(&self, operation: Operation) -> TerminologyError {
        let status = match self.0 {
            Script::Refuse(status) => status,
            _ => StatusCode::NOT_IMPLEMENTED,
        };
        TerminologyError::Status { operation, status }
    }
}

#[async_trait]
impl TerminologyServer for Scripted {
    async fn translate(
        &self,
        system: &str,
        code: &str,
        target: &str,
    ) -> Result<Vec<Match>, TerminologyError> {
        if matches!(self.0, Script::Unmapped(_)) {
            return Ok(Vec::new());
        }
        if let Script::Map(entries) = &self.0 {
            return Ok(entries
                .iter()
                .filter(|(from, value, to, _)| *from == system && *value == code && *to == target)
                .map(|(_, _, to, mapped)| Match {
                    equivalence: Some(String::from("equivalent")),
                    concept: Some(coding(to, mapped)),
                    source: Some(MAP.to_owned()),
                })
                .collect());
        }
        let Script::Translate(matches) = &self.0 else {
            return Err(self.refusal(Operation::Translate));
        };
        Ok(matches
            .iter()
            .map(|(equivalence, system, code)| Match {
                equivalence: Some((*equivalence).to_owned()),
                concept: Some(coding(system, code)),
                source: Some(MAP.to_owned()),
            })
            .collect())
    }

    async fn lookup(&self, _system: &str, _code: &str) -> Result<String, TerminologyError> {
        let Script::Display(display) = self.0 else {
            return Err(self.refusal(Operation::Lookup));
        };
        Ok(display.to_owned())
    }

    async fn designation(
        &self,
        _system: &str,
        _code: &str,
        language: &str,
    ) -> Result<Option<String>, TerminologyError> {
        let Script::Unmapped(english) = self.0 else {
            return Err(self.refusal(Operation::Lookup));
        };
        Ok((language == "en").then(|| english.to_owned()))
    }

    async fn validate_code(
        &self,
        _value_set: &str,
        _concept: &Candidate,
    ) -> Result<bool, TerminologyError> {
        let Script::Valid(result) = self.0 else {
            return Err(self.refusal(Operation::ValidateCode));
        };
        Ok(result)
    }
}

/// A coding of `system` and `code`.
pub(crate) fn coding(system: &str, code: &str) -> Coding {
    Coding {
        system: Some(Uri::from(system)),
        code: Some(Code::from(code)),
        ..Coding::default()
    }
}
