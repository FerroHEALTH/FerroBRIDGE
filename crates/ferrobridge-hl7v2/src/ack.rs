// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The original-mode application acknowledgment.
//!
//! The receiver answers `AA` when it processed the message, `AE` when it
//! sends "an error response, providing error information", and `AR` when it
//! failed to process the message "for reasons unrelated to content" (HL7
//! v2.9 chapter 2 §2.9.2; the codes are HL7 table 0008,
//! `CodeSystem/v2-0008` in the vendored `hl7.terminology` package). MSA-2
//! carries MSH-10 of the message acknowledged (§2.14.8.2), and each refusal
//! is one `ERR` segment with its location in ERR-2, its table 0357 code in
//! ERR-3 and the severity `E` in ERR-4 (§2.14.5).
//!
//! A reply's header is constructed anew (§2.9.2.1): it swaps the sending
//! and receiving application and facility of the message (MSH-3 to MSH-6),
//! keeps its delimiters, processing id, version and character set, and
//! carries a control id and a time of its own ([`Header::reply`]). The
//! acknowledgment names the message type `ACK` with the trigger event of the
//! message. The values echoed are taken as written, escapes included, so
//! they travel unchanged. Every segment is written by [`crate::write`].

use crate::decode::{Charset, DecodeError};
use crate::parse::{Delimiters, ErrorCode, Location, Refusal};
use crate::write::Writer;
use crate::write::header::Msh;
use crate::write::value::{Component, Field, Repetition, Segment};

/// An acknowledgment code of HL7 table 0008, original mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// `AA`, application accept.
    Accept,
    /// `AE`, application error.
    Error,
    /// `AR`, application reject.
    Reject,
}

impl Code {
    /// Returns the table 0008 code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "AA",
            Self::Error => "AE",
            Self::Reject => "AR",
        }
    }
}

/// The header fields of the message acknowledged, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    delimiters: Delimiters,
    fields: Vec<String>,
}

impl Header {
    /// Reads the header from the message text's first segment.
    ///
    /// `None` when the text opens with no MSH segment or its MSH-2 holds
    /// fewer than four encoding characters.
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let line = text.split('\r').next()?;
        let rest = line.strip_prefix("MSH")?;
        let field = rest.chars().next()?;
        let mut pieces = rest.get(field.len_utf8()..)?.split(field);
        let mut encoding = pieces.next()?.chars();
        let delimiters = Delimiters {
            field,
            component: encoding.next()?,
            repetition: encoding.next()?,
            escape: encoding.next()?,
            subcomponent: encoding.next()?,
            truncation: encoding.next(),
        };
        Some(Self {
            delimiters,
            fields: pieces.map(String::from).collect(),
        })
    }

    /// Reads the header from undecoded bytes, for a reject of a message that
    /// could not be decoded or framed.
    ///
    /// `None` when the first segment holds a byte outside 7-bit ASCII, since
    /// no character set is known to read it in.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let end = bytes
            .iter()
            .position(|byte| *byte == b'\r')
            .unwrap_or(bytes.len());
        let line = bytes.get(..end)?;
        if !line.is_ascii() {
            return None;
        }
        Self::from_text(core::str::from_utf8(line).ok()?)
    }

    /// Returns the delimiters MSH-1 and MSH-2 declare, which a reply keeps.
    #[must_use]
    pub const fn delimiters(&self) -> &Delimiters {
        &self.delimiters
    }

    /// Returns the field at `position` as written, from MSH-3; `None` for
    /// MSH-1 and MSH-2 and for an absent field.
    fn field(&self, position: usize) -> Option<&str> {
        self.fields
            .get(position.checked_sub(3)?)
            .map(String::as_str)
    }

    /// Returns the field at `position` as written, to be echoed unchanged.
    fn echoed(&self, position: usize) -> Field {
        Field::written(self.field(position).unwrap_or_default())
    }

    /// Returns MSH-10 as written.
    #[must_use]
    pub fn control_id(&self) -> Option<&str> {
        self.field(10).filter(|value| !value.is_empty())
    }

    /// Returns the message code and trigger event of MSH-9 as written, joined
    /// by `^` (`ORU^R01`); `None` when MSH-9.1 is empty.
    #[must_use]
    pub fn type_code(&self) -> Option<String> {
        let code = self.message_type(1).filter(|code| !code.is_empty())?;
        Some(
            match self.message_type(2).filter(|event| !event.is_empty()) {
                Some(event) => format!("{code}^{event}"),
                None => String::from(code),
            },
        )
    }

    /// Returns the component at `position` of MSH-9 as written, from 1.
    fn message_type(&self, position: usize) -> Option<&str> {
        self.field(9)?
            .split(self.delimiters.component)
            .nth(position.checked_sub(1)?)
    }

    /// Returns the header of a reply to this message (HL7 v2.9 chapter 2
    /// §2.9.2.1): MSH-3 to MSH-6 swapped, `message_type` in MSH-9, the
    /// stamp's time and control id in MSH-7 and MSH-10, and MSH-11, MSH-12
    /// and MSH-18 echoed as written.
    #[must_use]
    pub fn reply(&self, message_type: Field, stamp: Stamp<'_>) -> Segment {
        Segment::from(Msh {
            sending_application: self.echoed(5),
            sending_facility: self.echoed(6),
            receiving_application: self.echoed(3),
            receiving_facility: self.echoed(4),
            timestamp: Field::from(stamp.timestamp),
            message_type,
            control_id: Field::from(stamp.control_id),
            processing_id: self.echoed(11),
            version: self.echoed(12),
            charset: self
                .field(18)
                .filter(|charset| !charset.is_empty())
                .map(Field::written)
                .unwrap_or_default(),
        })
    }

    /// Returns the `MSA` segment of a reply to this message: `code` in
    /// MSA-1 and MSH-10 of the message, as written, in MSA-2 (§2.14.8.2).
    #[must_use]
    pub fn msa(&self, code: Code) -> Segment {
        Segment::new(
            "MSA",
            [
                Field::from(code.as_str()),
                Field::written(self.control_id().unwrap_or_default()),
            ],
        )
    }

    /// Returns a writer in the delimiters of this message.
    #[must_use]
    pub const fn writer(&self) -> Writer {
        Writer::new(self.delimiters)
    }
}

/// What the acknowledgment's own header carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp<'a> {
    /// MSH-10 of the acknowledgment.
    pub control_id: &'a str,
    /// MSH-7 of the acknowledgment, a v2 `DTM`.
    pub timestamp: &'a str,
}

/// One `ERR` segment of an acknowledgment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// ERR-2, the location, when the error has one.
    pub location: Option<Location>,
    /// ERR-3, the table 0357 code.
    pub code: ErrorCode,
    /// ERR-8, the user message, without message content.
    pub text: String,
}

impl From<&Refusal> for Error {
    fn from(refusal: &Refusal) -> Self {
        Self {
            location: Some(refusal.location.clone())
                .filter(|location| !location.segment.is_empty()),
            code: refusal.code,
            text: refusal.detail.clone(),
        }
    }
}

/// Writes the error as one `ERR` segment: ERR-2 the location, ERR-3 the
/// table 0357 code, ERR-4 the severity `E` and ERR-8 the text (HL7 v2.9
/// chapter 2 §2.14.5).
impl From<&Error> for Segment {
    fn from(error: &Error) -> Self {
        let location = error
            .location
            .as_ref()
            .map(|location| location.components().into_iter().collect::<Repetition>())
            .map(Field::from)
            .unwrap_or_default();
        let condition = Repetition::from_iter([error.code.code(), error.code.display(), "HL70357"]);
        Self::new(
            "ERR",
            [
                Field::default(),
                location,
                Field::from(condition),
                Field::from("E"),
                Field::default(),
                Field::default(),
                Field::default(),
                Field::from(error.text.as_str()),
            ],
        )
    }
}

/// Renders the acknowledgment of the message `header` names.
///
/// # Examples
///
/// ```
/// use ferrobridge_hl7v2::ack::{Code, Error, Header, Stamp, render};
/// use ferrobridge_hl7v2::parse::ErrorCode;
///
/// let header = Header::from_text("MSH|^~\\&|LAB|NORTHLAB|EHR|SOUTHCLINIC|20260925||ORU^R01|MSG00001|P|2.5.1\r")
///     .expect("an MSH segment");
/// let refusal = Error {
///     location: None,
///     code: ErrorCode::UnsupportedMessageType,
///     text: String::from("no structure for ORU^R99"),
/// };
/// let stamp = Stamp { control_id: "ACK00001", timestamp: "20260925143001" };
/// let reply = render(&header, Code::Reject, &[refusal], stamp);
/// assert!(reply.contains("\rMSA|AR|MSG00001"));
/// assert!(reply.contains("\rERR|"));
/// ```
#[must_use]
pub fn render(header: &Header, code: Code, errors: &[Error], stamp: Stamp<'_>) -> String {
    let event = Component::written(header.message_type(2).unwrap_or_default());
    let message_type =
        Repetition::from_iter([Component::from("ACK"), event, Component::from("ACK")]);
    let mut segments = vec![
        header.reply(Field::from(message_type), stamp),
        header.msa(code),
    ];
    segments.extend(errors.iter().map(Segment::from));
    header.writer().message(&segments)
}

/// Renders and encodes the acknowledgment in the message's character set.
///
/// # Errors
///
/// Returns [`DecodeError::Undeclared`] when an echoed value has no byte in
/// `charset`.
pub fn encoded(
    header: &Header,
    code: Code,
    errors: &[Error],
    stamp: Stamp<'_>,
    charset: Charset,
) -> Result<Vec<u8>, DecodeError> {
    crate::decode::encode(&render(header, code, errors, stamp), charset)
}

/// Builds the `AR` answer to a malformed frame from the bytes read before the
/// refusal, `None` when no header can be read from them.
#[must_use]
pub fn reject_frame(partial: &[u8], detail: &str, stamp: Stamp<'_>) -> Option<Vec<u8>> {
    let header = Header::from_bytes(partial)?;
    let error = Error {
        location: None,
        code: ErrorCode::Application,
        text: String::from(detail),
    };
    Some(render(&header, Code::Reject, &[error], stamp).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::{Code, Error, Header, Stamp, render};
    use crate::parse::{ErrorCode, Location};

    const MESSAGE: &str =
        "MSH|^~\\&|LAB|NORTH|EHR|SOUTH|20260925120000+0200||ORU^R01^ORU_R01|MSG-1|P|2.5.1\rPID|1";

    fn stamp() -> Stamp<'static> {
        Stamp {
            control_id: "ACK-1",
            timestamp: "20260925120001+0200",
        }
    }

    #[test]
    fn an_accept_swaps_the_applications_and_echoes_msh_10() {
        let header = Header::from_text(MESSAGE).expect("a header");
        assert_eq!(
            render(&header, Code::Accept, &[], stamp()),
            "MSH|^~\\&|EHR|SOUTH|LAB|NORTH|20260925120001+0200||ACK^R01^ACK|ACK-1|P|2.5.1\rMSA|AA|MSG-1\r"
        );
    }

    #[test]
    fn an_error_carries_one_err_segment_per_refusal() {
        let header = Header::from_text(MESSAGE).expect("a header");
        let error = Error {
            location: Some(Location::segment("PID", 1).with_field(3)),
            code: ErrorCode::RequiredFieldMissing,
            text: String::from("PID.3 is required"),
        };
        let ack = render(&header, Code::Error, &[error], stamp());
        assert!(
            ack.ends_with("MSA|AE|MSG-1\rERR||PID^1^3|101^Required field missing^HL70357|E||||PID.3 is required\r"),
            "{ack}"
        );
    }

    #[test]
    fn an_unknown_key_identifier_is_err_3_204() {
        let header = Header::from_text(MESSAGE).expect("a header");
        let error = Error {
            location: Some(Location::segment("QPD", 1).with_field(3)),
            code: ErrorCode::UnknownKeyIdentifier,
            text: String::from("the identifier is not known"),
        };
        let ack = render(&header, Code::Error, &[error], stamp());
        assert!(
            ack.contains("|204^Unknown key identifier^HL70357|"),
            "{ack}"
        );
    }

    #[test]
    fn the_type_code_joins_the_message_code_and_the_event() {
        let header = Header::from_text(MESSAGE).expect("a header");
        assert_eq!(header.type_code().as_deref(), Some("ORU^R01"));
        let bare = Header::from_text("MSH|^~\\&|LAB||||||ACK|MSG-2|P|2.5.1").expect("a header");
        assert_eq!(bare.type_code().as_deref(), Some("ACK"));
    }

    #[test]
    fn a_header_with_a_byte_outside_ascii_is_not_read() {
        assert!(Header::from_bytes(b"MSH|^~\\&|L\xC4B").is_none());
        assert!(Header::from_bytes(b"MSH|^~\\&|LAB").is_some());
    }
}
