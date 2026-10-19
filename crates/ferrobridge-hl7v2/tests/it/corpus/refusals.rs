// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The corpus messages the face refuses correctly, read from
//! `expected-refusals.txt`, each with the acknowledgment it is owed.
//!
//! HL7 v2 chapter 2 answers a message the receiver cannot take with an
//! application acknowledgment carrying an error: `AE` "an error response,
//! providing error information", `AR` a rejection "for reasons unrelated to
//! content" (HL7 table 0008), with the condition in ERR-3 from HL7 table
//! 0357 (`CodeSystem/v2-0357` in the vendored `hl7.terminology` package). A
//! listed message the face answers that way is conformant, so its case
//! passes; one the face maps, or answers otherwise, fails.

use std::collections::BTreeMap;

use ferrobridge_hl7v2::ack::Code;

/// Why a listed message is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reason {
    /// A `ReportStream` mapping-inventory fragment: a message cut down to
    /// the segments one data type test needs, lacking a group or a segment
    /// its structure requires (a segment sequence error, table 0357 `100`),
    /// or an MSH-9 that names no trigger event, so no structure is selected
    /// (`200`).
    Fragment,
    /// A sample that breaks the structure its MSH-9 declares: a required
    /// segment absent or segments out of the structure's order.
    SampleStructure,
    /// An AIRA MQE example built to fail: a VXU^V04 whose ORDER lacks the
    /// RXA the structure requires.
    AiraFatal,
    /// Bytes outside the character set MSH-18 declares: "If the field is
    /// left blank, the character set in use is understood to be the 7-bit
    /// ASCII set" (HL7 v2 chapter 2, MSH-18), or a first repetition that is
    /// no code of HL7 table 0211.
    Charset,
}

impl Reason {
    /// The reason a line of the list names.
    fn parse(text: &str) -> Option<Self> {
        match text {
            "fragment" => Some(Self::Fragment),
            "sample-structure" => Some(Self::SampleStructure),
            "aira-fatal" => Some(Self::AiraFatal),
            "charset" => Some(Self::Charset),
            _ => None,
        }
    }

    /// The outcome kind a passing case records.
    pub(super) const fn kind(self) -> &'static str {
        match self {
            Self::Fragment => "expected-refusal:fragment",
            Self::SampleStructure => "expected-refusal:sample-structure",
            Self::AiraFatal => "expected-refusal:aira-fatal",
            Self::Charset => "expected-refusal:charset",
        }
    }
}

/// The acknowledgment a listed message is owed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Expected {
    /// MSA-1.
    pub(super) code: Code,
    /// The table 0357 code one ERR carries in ERR-3.1.
    pub(super) error: String,
    /// Why.
    pub(super) reason: Reason,
}

/// The list, by case id, or the line that does not read.
pub(super) fn expected() -> Result<BTreeMap<String, Expected>, String> {
    let mut list = BTreeMap::new();
    for line in include_str!("expected-refusals.txt").lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [id, code, error, reason] = fields.as_slice() else {
            return Err(format!("an expected refusal has four fields: {line}"));
        };
        let code = match *code {
            "AE" => Code::Error,
            "AR" => Code::Reject,
            _ => return Err(format!("an expected refusal is AE or AR: {line}")),
        };
        let reason = Reason::parse(reason).ok_or_else(|| format!("an unknown reason: {line}"))?;
        let entry = Expected {
            code,
            error: String::from(*error),
            reason,
        };
        if list.insert(String::from(*id), entry).is_some() {
            return Err(format!("a case listed twice: {id}"));
        }
    }
    Ok(list)
}

/// The ERR-3.1 codes of an acknowledgment, read with its own delimiters.
pub(super) fn error_codes(reply: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(reply);
    let Some(header) = text.split('\r').next() else {
        return Vec::new();
    };
    let mut characters = header.chars().skip(3);
    let (Some(field), Some(component)) = (characters.next(), characters.next()) else {
        return Vec::new();
    };
    text.split('\r')
        .filter(|segment| segment.starts_with("ERR"))
        .filter_map(|segment| segment.split(field).nth(3))
        .filter_map(|condition| condition.split(component).next())
        .map(String::from)
        .collect()
}

#[test]
fn every_listed_line_reads() -> Result<(), String> {
    let list = expected()?;
    assert!(!list.is_empty());
    assert!(
        list.keys().all(|id| !id.contains(' ')),
        "a case id writes each space as %20"
    );
    Ok(())
}

#[test]
fn the_error_codes_are_read_from_err_3() {
    let reply = b"MSH|^~\\&|EHR|S|LAB|N|20261019||ACK^R01^ACK|A-1|P|2.5.1\rMSA|AE|M-1\rERR||PID^1^3|101^Required field missing^HL70357|E\r";
    assert_eq!(error_codes(reply), ["101"]);
}
