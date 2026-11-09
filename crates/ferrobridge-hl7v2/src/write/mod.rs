// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Composing a message by the encoding rules of HL7 v2.9 chapter 2.
//!
//! A message is composed from [`Segment`] values, each field built from
//! repetitions, components and subcomponents. The [`Writer`] joins them with
//! its [`Delimiters`] (§2.5.4), escapes every value of our own (§2.7.1: the
//! field, component, repetition, subcomponent and truncation characters and
//! the escape character itself), and leaves out the trailing empty fields,
//! components and subcomponents. A field read from a message is echoed as
//! the message wrote it, escape sequences included ([`Writer::echo`]), which
//! a reply can do because it keeps the delimiters of the message it answers.
//!
//! [`Writer::compose`] checks the text against the message structure the
//! generated `hl7v2-types` tables define for the type and version its MSH
//! declares, through the same lexer and grouping a received message takes,
//! and encodes it in the character set MSH-18 names (§2.14.9.18). A segment
//! the structure has no place for, a value beyond its definition, and a
//! required segment or field left empty refuse the message before it is
//! sent.

pub mod header;
pub mod value;

use hl7v2_types::model::Structure;

use crate::decode::{self, Charset, DecodeError};
use crate::parse::lex::{self, LexError};
use crate::parse::profile::Profile;
use crate::parse::structure::StructureError;
use crate::parse::{self, Delimiters, Message, Refusal, Unplaced};
use crate::write::value::{Component, Field, Repetition, Segment, Value};

/// A refusal to echo a value read from a message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EchoError {
    /// The value was read in delimiters other than the writer's, so its
    /// text as read would not travel as one value.
    #[error("the value was read in other delimiters than the writer's")]
    Delimiters,
}

/// A refusal to compose a message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ComposeError {
    /// The composed text is no HL7 v2 message: it opens with no MSH, or a
    /// segment id is not three upper-case letters or digits.
    #[error("the composed text is no HL7 v2 message")]
    Lex(#[source] LexError),
    /// No message structure is selected for the type and version MSH
    /// declares.
    #[error("no message structure is selected for the composed message")]
    Structure(#[source] StructureError),
    /// MSH-18 names a character set this crate does not encode.
    #[error("MSH-18 names the character set {code:?}, which is not encoded")]
    UnsupportedCharset {
        /// MSH-18 as composed.
        code: String,
    },
    /// The message does not conform to its structure.
    #[error(
        "the composed message does not conform to {structure}: {} refusals, {} values out of place",
        refusals.len(),
        unplaced.len()
    )]
    Nonconforming {
        /// The id of the structure the message was checked against.
        structure: &'static str,
        /// The refusals: a missing required segment, an escape sequence
        /// this crate does not decode.
        refusals: Vec<Refusal>,
        /// The segments and values out of place and the required fields
        /// left empty.
        unplaced: Vec<Unplaced>,
    },
}

/// A composed message, checked against its structure, with the character
/// set it travels in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composed {
    text: String,
    charset: Charset,
}

impl Composed {
    /// Returns the message text, each segment closed by a carriage return.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the message text.
    #[must_use]
    pub fn into_text(self) -> String {
        self.text
    }

    /// Returns the character set: the one MSH-18 names, or the default
    /// [`Writer::compose`] was given when MSH-18 is empty.
    #[must_use]
    pub const fn charset(&self) -> Charset {
        self.charset
    }

    /// Encodes the message in its character set.
    ///
    /// # Errors
    ///
    /// Returns [`DecodeError::Undeclared`] when a character has no byte in
    /// the set.
    pub fn encoded(&self) -> Result<Vec<u8>, DecodeError> {
        decode::encode(&self.text, self.charset)
    }
}

/// Writes segments in one set of delimiters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Writer {
    delimiters: Delimiters,
    profile: &'static Profile,
}

impl Writer {
    /// Creates the writer of a message in `delimiters`, which its MSH-1 and
    /// MSH-2 then declare, checking against the base structure.
    #[must_use]
    pub const fn new(delimiters: Delimiters) -> Self {
        Self {
            delimiters,
            profile: &Profile::BASE,
        }
    }

    /// Returns this writer checking a composed message against its structure
    /// as `profile` constrains it ([`Writer::compose`]).
    #[must_use]
    pub const fn with_profile(self, profile: &'static Profile) -> Self {
        Self { profile, ..self }
    }

    /// Returns the delimiters.
    #[must_use]
    pub const fn delimiters(&self) -> &Delimiters {
        &self.delimiters
    }

    /// Escapes `text` so it travels as one value.
    ///
    /// Each delimiter becomes its escape sequence (HL7 v2.9 chapter 2
    /// §2.7.1). A carriage return or a line feed becomes a space: the
    /// carriage return ends a segment (§2.10.2.0), and the hexadecimal form
    /// `\X0D\` is one of the sequences §2.7.1 admits in `TX`, `FT` and `CF`
    /// values only.
    #[must_use]
    pub fn escape(&self, text: &str) -> String {
        // NOTE: no specification governs a line break in a value of another type: our own design
        // writes a space for each carriage return and line feed.
        self.delimiters
            .escape(text)
            .chars()
            .map(|character| match character {
                '\r' | '\n' => ' ',
                other => other,
            })
            .collect()
    }

    /// Returns `field`, read in `read`, as a field written as read.
    ///
    /// # Errors
    ///
    /// Returns [`EchoError::Delimiters`] when `read` differs from the
    /// writer's delimiters.
    pub fn echo(&self, field: &parse::Field, read: &Delimiters) -> Result<Field, EchoError> {
        if *read != self.delimiters {
            return Err(EchoError::Delimiters);
        }
        Ok(Field::written(field.written()))
    }

    /// Returns the first segment `id` of `message` with every field written
    /// as read, `None` when the message holds no such segment.
    ///
    /// # Errors
    ///
    /// Returns [`EchoError::Delimiters`] when the message's delimiters
    /// differ from the writer's.
    pub fn echo_segment(&self, message: &Message, id: &str) -> Result<Option<Segment>, EchoError> {
        let Some(segment) = message.segments().iter().find(|segment| segment.id() == id) else {
            return Ok(None);
        };
        let skipped = if id == "MSH" { 2 } else { 0 };
        let fields = segment
            .fields()
            .iter()
            .skip(skipped)
            .map(|field| self.echo(field, message.delimiters()))
            .collect::<Result<Vec<Field>, EchoError>>()?;
        Ok(Some(Segment::new(id, fields)))
    }

    /// Writes one segment, without its terminating carriage return.
    ///
    /// The trailing empty fields are left out, up to the last field valued or
    /// echoed: an echoed field keeps its place even when the message left it
    /// empty.
    #[must_use]
    pub fn segment(&self, segment: &Segment) -> String {
        let mut parts = vec![segment.id.clone()];
        if segment.id == "MSH" {
            parts.push(self.delimiters.encoding_characters());
        }
        let last = segment
            .fields
            .iter()
            .rposition(|field| field.written.is_some() || !self.field(field).is_empty())
            .map_or(0, |index| index.saturating_add(1));
        parts.extend(
            segment
                .fields
                .iter()
                .take(last)
                .map(|field| self.field(field)),
        );
        parts.join(&self.delimiters.field.to_string())
    }

    /// Writes `segments` as one message, each segment closed by a carriage
    /// return (§2.10.2.0), unchecked.
    #[must_use]
    pub fn message(&self, segments: &[Segment]) -> String {
        let mut text = String::new();
        for segment in segments {
            text.push_str(&self.segment(segment));
            text.push('\r');
        }
        text
    }

    /// Writes `segments` as one message and checks it against the message
    /// structure `select` picks from its MSH, as [`inbound::receive`] picks
    /// it for a received message, with each member's minimum read through
    /// the writer's profile ([`Writer::with_profile`]).
    ///
    /// The message travels in the character set MSH-18 names, or in
    /// `default` when MSH-18 is empty.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError`] when the text is no HL7 v2 message, when no
    /// structure is selected, when MSH-18 names a set this crate does not
    /// encode, and when the message does not conform to its structure.
    ///
    /// [`inbound::receive`]: crate::inbound::receive
    pub fn compose(
        &self,
        segments: &[Segment],
        select: impl Fn(&Message) -> Result<&'static Structure, StructureError>,
        default: Charset,
    ) -> Result<Composed, ComposeError> {
        let text = self.message(segments);
        let lexed = lex::lex(&text, default).map_err(ComposeError::Lex)?;
        let charset = match lexed.message.header().and_then(|msh| msh.text(18)) {
            Some(code) => {
                Charset::from_code(code).ok_or_else(|| ComposeError::UnsupportedCharset {
                    code: String::from(code),
                })?
            }
            None => default,
        };
        let structure = select(&lexed.message).map_err(ComposeError::Structure)?;
        let parsed = parse::grouping::group(lexed, structure, self.profile);
        let unplaced: Vec<Unplaced> = parsed
            .unplaced()
            .iter()
            .filter(|unplaced| defective(unplaced))
            .cloned()
            .collect();
        if !parsed.refusals().is_empty() || !unplaced.is_empty() {
            return Err(ComposeError::Nonconforming {
                structure: structure.id,
                refusals: parsed.refusals().to_vec(),
                unplaced,
            });
        }
        Ok(Composed { text, charset })
    }

    /// Writes one field: as read, or its repetitions joined.
    fn field(&self, field: &Field) -> String {
        if let Some(written) = &field.written {
            return written.clone();
        }
        let repetitions: Vec<String> = field
            .repetitions
            .iter()
            .map(|repetition| self.repetition(repetition))
            .collect();
        repetitions.join(&self.delimiters.repetition.to_string())
    }

    /// Writes one repetition, its trailing empty components left out.
    fn repetition(&self, repetition: &Repetition) -> String {
        let components = repetition
            .components
            .iter()
            .map(|component| self.component(component));
        trimmed(components, self.delimiters.component)
    }

    /// Writes one component, its trailing empty subcomponents left out.
    fn component(&self, component: &Component) -> String {
        let subcomponents = component.subcomponents.iter().map(|value| match value {
            Value::Text(text) => self.escape(text),
            Value::Written(text) => text.clone(),
        });
        trimmed(subcomponents, self.delimiters.subcomponent)
    }
}

/// Joins `parts` with `separator`, the trailing empty ones left out.
fn trimmed(parts: impl Iterator<Item = String>, separator: char) -> String {
    let mut parts: Vec<String> = parts.collect();
    while parts.last().is_some_and(String::is_empty) {
        parts.pop();
    }
    parts.join(&separator.to_string())
}

/// Whether an outcome of the check is a defect of the composed message, as
/// opposed to a note on the tables it was checked against.
const fn defective(unplaced: &Unplaced) -> bool {
    match unplaced {
        Unplaced::UnknownSegment { .. }
        | Unplaced::OutOfStructure { .. }
        | Unplaced::ExtraField { .. }
        | Unplaced::ExtraRepetition { .. }
        | Unplaced::ExtraComponent { .. }
        | Unplaced::ExtraSubcomponent { .. }
        | Unplaced::MissingRequiredField { .. } => true,
        Unplaced::EarlierVersion { .. }
        | Unplaced::VersionSelected { .. }
        | Unplaced::OtherStructure { .. }
        | Unplaced::WithdrawnStructure { .. } => false,
    }
}
