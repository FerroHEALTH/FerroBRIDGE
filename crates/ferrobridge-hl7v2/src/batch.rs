// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HL7 batch protocol: a file of batches of messages, and the response
//! batch that acknowledges them.
//!
//! HL7 v2 chapter 2 §2.10.3, "HL7 batch file structure", gives the file as
//! `[FHS] { [BHS] { [MSH ...] } [BTS] } [FTS]`: an optional file header, one
//! or more batches each with an optional header and trailer around zero or
//! more messages, and an optional file trailer. The header and trailer
//! segments are encoded by the message construction rules (§2.6), so the
//! file is split on the segment terminator and each message keeps its own
//! bytes, MSH-18 included, for [`crate::inbound::receive`].
//!
//! BTS-1 carries "the count of the individual messages contained within the
//! batch" and FTS-1 "the number of batches contained in this file"
//! (§2.14.3, §2.14.7). A count that disagrees with what the file holds is a
//! [`Discrepancy`] on the result, never a refusal, since the messages read
//! as they are: no specification governs that choice, it is our own design.
//!
//! §2.10.3.2, "Acknowledging batches", leaves the acknowledgment mode to the
//! application; the face takes its first option, "all messages are
//! acknowledged in the response batch", whose format "is a batch of
//! individual messages" ([`reply`]).

/// The segment terminator (chapter 2 §2.5.4).
const SEGMENT_END: u8 = b'\r';

/// One batch of a file: its header and trailer segments and its messages,
/// each as the bytes the file holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Batch {
    header: Option<Vec<u8>>,
    trailer: Option<Vec<u8>>,
    messages: Vec<Vec<u8>>,
}

impl Batch {
    /// Returns the BHS segment, when the batch has one.
    #[must_use]
    pub fn header(&self) -> Option<&[u8]> {
        self.header.as_deref()
    }

    /// Returns the BTS segment, when the batch has one.
    #[must_use]
    pub fn trailer(&self) -> Option<&[u8]> {
        self.trailer.as_deref()
    }

    /// Returns the messages, each ending with its segment terminator.
    #[must_use]
    pub fn messages(&self) -> &[Vec<u8>] {
        &self.messages
    }
}

/// A batch file split into its batches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct File {
    header: Option<Vec<u8>>,
    trailer: Option<Vec<u8>>,
    batches: Vec<Batch>,
    discrepancies: Vec<Discrepancy>,
}

impl File {
    /// Returns the FHS segment, when the file has one.
    #[must_use]
    pub fn header(&self) -> Option<&[u8]> {
        self.header.as_deref()
    }

    /// Returns the batches, in file order.
    #[must_use]
    pub fn batches(&self) -> &[Batch] {
        &self.batches
    }

    /// Returns every count the trailers state that the file contradicts.
    #[must_use]
    pub fn discrepancies(&self) -> &[Discrepancy] {
        &self.discrepancies
    }

    /// Returns every message of every batch, in file order.
    pub fn messages(&self) -> impl Iterator<Item = &[u8]> {
        self.batches
            .iter()
            .flat_map(|batch| batch.messages.iter().map(Vec::as_slice))
    }
}

/// A trailer count the file contradicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discrepancy {
    /// BTS-1 of the batch at `batch`, from 1, names another message count
    /// than the batch holds, or no number.
    MessageCount {
        /// The batch, from 1.
        batch: usize,
        /// BTS-1 as written.
        stated: String,
        /// The messages the batch holds.
        held: usize,
    },
    /// FTS-1 names another batch count than the file holds, or no number.
    BatchCount {
        /// FTS-1 as written.
        stated: String,
        /// The batches the file holds.
        held: usize,
    },
}

impl Discrepancy {
    /// Returns a short name of the discrepancy's kind, for a count.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::MessageCount { .. } => "batch-message-count",
            Self::BatchCount { .. } => "file-batch-count",
        }
    }
}

/// Why bytes do not read as a batch file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BatchError {
    /// The bytes open with neither FHS nor BHS.
    #[error("the bytes open with neither FHS nor BHS")]
    NotBatch,
    /// A segment stands where the batch file structure admits none.
    #[error("segment {number} ({id}) stands where the batch file structure admits none")]
    Misplaced {
        /// The segment's place in the file, from 1.
        number: usize,
        /// The segment id as written, or the first bytes of the segment.
        id: String,
    },
}

/// Whether `bytes` open with a batch envelope, FHS or BHS.
#[must_use]
pub fn is_batch(bytes: &[u8]) -> bool {
    bytes.starts_with(b"FHS") || bytes.starts_with(b"BHS")
}

/// Splits a batch file into its batches and messages.
///
/// # Errors
///
/// Returns [`BatchError::NotBatch`] when the bytes open with neither FHS nor
/// BHS, and [`BatchError::Misplaced`] when a segment stands outside the
/// structure chapter 2 §2.10.3 gives: a second FHS, a segment before the
/// first MSH of a batch other than its BHS, or one after the FTS.
pub fn split(bytes: &[u8]) -> Result<File, BatchError> {
    if !is_batch(bytes) {
        return Err(BatchError::NotBatch);
    }
    let separator = bytes.get(3).copied().unwrap_or(b'|');
    let mut file = File::default();
    let mut current: Option<Batch> = None;
    let mut message: Option<Vec<u8>> = None;
    let segments = bytes
        .split(|byte| *byte == SEGMENT_END)
        .filter(|segment| !segment.is_empty());
    for (offset, segment) in segments.enumerate() {
        let number = offset.saturating_add(1);
        let misplaced = || BatchError::Misplaced {
            number,
            id: String::from_utf8_lossy(segment.get(..3).unwrap_or(segment)).into_owned(),
        };
        if file.trailer.is_some() {
            return Err(misplaced());
        }
        let id = segment.get(..3).unwrap_or_default();
        match id {
            b"FHS" => {
                if number != 1 {
                    return Err(misplaced());
                }
                file.header = Some(segment.to_vec());
            }
            b"BHS" => {
                close_message(&mut current, &mut message);
                if let Some(batch) = current.take() {
                    file.batches.push(batch);
                }
                current = Some(Batch {
                    header: Some(segment.to_vec()),
                    ..Batch::default()
                });
            }
            b"MSH" => {
                close_message(&mut current, &mut message);
                let batch = current.get_or_insert_with(Batch::default);
                if batch.trailer.is_some() {
                    let closed = core::mem::take(batch);
                    file.batches.push(closed);
                }
                let mut opened = segment.to_vec();
                opened.push(SEGMENT_END);
                message = Some(opened);
            }
            b"BTS" => {
                close_message(&mut current, &mut message);
                let batch = current.get_or_insert_with(Batch::default);
                if batch.trailer.is_some() {
                    return Err(misplaced());
                }
                batch.trailer = Some(segment.to_vec());
            }
            b"FTS" => {
                close_message(&mut current, &mut message);
                if let Some(batch) = current.take() {
                    file.batches.push(batch);
                }
                file.trailer = Some(segment.to_vec());
            }
            _ => match message.as_mut() {
                Some(open) => {
                    open.extend_from_slice(segment);
                    open.push(SEGMENT_END);
                }
                None => return Err(misplaced()),
            },
        }
    }
    close_message(&mut current, &mut message);
    if let Some(batch) = current.take() {
        file.batches.push(batch);
    }
    file.discrepancies = discrepancies(&file, separator);
    Ok(file)
}

/// Adds the open message, when there is one, to the open batch.
fn close_message(current: &mut Option<Batch>, message: &mut Option<Vec<u8>>) {
    if let Some(done) = message.take() {
        current
            .get_or_insert_with(Batch::default)
            .messages
            .push(done);
    }
}

/// The field at `position` of `segment`, from 1, split on `separator`.
fn field(segment: &[u8], separator: u8, position: usize) -> Option<&[u8]> {
    segment.split(|byte| *byte == separator).nth(position)
}

/// The trailer counts the file contradicts.
fn discrepancies(file: &File, separator: u8) -> Vec<Discrepancy> {
    let mut found = Vec::new();
    for (offset, batch) in file.batches.iter().enumerate() {
        let Some(trailer) = &batch.trailer else {
            continue;
        };
        let stated = field(trailer, separator, 1).unwrap_or_default();
        if !counts(stated, batch.messages.len()) {
            found.push(Discrepancy::MessageCount {
                batch: offset.saturating_add(1),
                stated: String::from_utf8_lossy(stated).into_owned(),
                held: batch.messages.len(),
            });
        }
    }
    if let Some(trailer) = &file.trailer {
        let stated = field(trailer, separator, 1).unwrap_or_default();
        if !counts(stated, file.batches.len()) {
            found.push(Discrepancy::BatchCount {
                stated: String::from_utf8_lossy(stated).into_owned(),
                held: file.batches.len(),
            });
        }
    }
    found
}

/// Whether `stated` is empty or the decimal number `held`.
// NOTE: HL7 v2 chapter 2 §2.14.3 makes BTS-1 optional, so an empty count states nothing to
// contradict; our own reading.
fn counts(stated: &[u8], held: usize) -> bool {
    stated.is_empty()
        || core::str::from_utf8(stated)
            .ok()
            .and_then(|text| text.trim().parse::<usize>().ok())
            == Some(held)
}

/// What a response header carries of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp<'a> {
    /// FHS-11 and BHS-11 of the response, the control id.
    pub control_id: &'a str,
    /// FHS-7 and BHS-7 of the response, a v2 `DTM`.
    pub timestamp: &'a str,
}

/// Builds the response batch to `file`, holding the acknowledgments `acks`.
///
/// Each response header swaps the sending and receiving application and
/// facility and echoes the header's control id in its reference control id;
/// each batch holds, in order, the acknowledgments `acks` has for it.
///
/// The response has a file header and trailer when `file` has them, a batch
/// header for each batch that has one, and a trailer for every batch, whose
/// BTS-1 counts the acknowledgments it holds. BHS-11 "can be echoed back in
/// BHS-12 Reference Batch Control ID if an answering batch is needed", and
/// FHS-11 likewise in FHS-12 (chapter 2 §2.14.2.11, §2.14.6.11).
#[must_use]
pub fn reply(file: &File, acks: &[Vec<Vec<u8>>], stamp: Stamp<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(header) = &file.header {
        push_segment(&mut out, &answer_header(header, stamp));
    }
    for (offset, batch) in file.batches.iter().enumerate() {
        let answers = acks.get(offset).map(Vec::as_slice).unwrap_or_default();
        let separator = batch
            .header
            .as_deref()
            .or(file.header.as_deref())
            .and_then(|header| header.get(3).copied())
            .unwrap_or(b'|');
        if let Some(header) = &batch.header {
            push_segment(&mut out, &answer_header(header, stamp));
        }
        for ack in answers {
            out.extend_from_slice(ack);
            if !ack.ends_with(&[SEGMENT_END]) {
                out.push(SEGMENT_END);
            }
        }
        let mut trailer = b"BTS".to_vec();
        trailer.push(separator);
        trailer.extend_from_slice(answers.len().to_string().as_bytes());
        push_segment(&mut out, &trailer);
    }
    if let Some(header) = &file.header {
        let separator = header.get(3).copied().unwrap_or(b'|');
        let mut trailer = b"FTS".to_vec();
        trailer.push(separator);
        trailer.extend_from_slice(file.batches.len().to_string().as_bytes());
        push_segment(&mut out, &trailer);
    }
    out
}

/// Appends `segment` and its terminator.
fn push_segment(out: &mut Vec<u8>, segment: &[u8]) {
    out.extend_from_slice(segment);
    out.push(SEGMENT_END);
}

/// The response's header for the FHS or BHS `header`: fields 3 to 6
/// swapped, field 7 and field 11 from `stamp`, field 12 the header's own
/// field 11, the rest left empty.
fn answer_header(header: &[u8], stamp: Stamp<'_>) -> Vec<u8> {
    let separator = header.get(3).copied().unwrap_or(b'|');
    let echoed = |position: usize| field(header, separator, position).unwrap_or_default();
    let id = header.get(..3).unwrap_or(b"BHS");
    let fields: [&[u8]; 12] = [
        id,
        echoed(1),
        echoed(4),
        echoed(5),
        echoed(2),
        echoed(3),
        stamp.timestamp.as_bytes(),
        b"",
        b"",
        b"",
        stamp.control_id.as_bytes(),
        echoed(10),
    ];
    let mut out = Vec::new();
    for (index, value) in fields.iter().enumerate() {
        if index > 0 {
            out.push(separator);
        }
        out.extend_from_slice(value);
    }
    while out.last() == Some(&separator) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{BatchError, Discrepancy, Stamp, answer_header, split};

    #[test]
    fn the_response_header_swaps_the_parties_and_references_the_control_id() {
        let header = b"BHS|^~\\&|LAB|NORTH|EHR|SOUTH|20261019100000+0200||||B-17";
        let stamp = Stamp {
            control_id: "R-1",
            timestamp: "20261019100100+0000",
        };
        assert_eq!(
            answer_header(header, stamp),
            b"BHS|^~\\&|EHR|SOUTH|LAB|NORTH|20261019100100+0000||||R-1|B-17".to_vec()
        );
    }

    #[test]
    fn a_segment_before_the_first_message_is_misplaced() {
        assert_eq!(
            split(b"BHS|^~\\&\rPID|1\rMSH|^~\\&\r"),
            Err(BatchError::Misplaced {
                number: 2,
                id: String::from("PID")
            })
        );
    }

    #[test]
    fn a_count_the_file_contradicts_is_a_discrepancy() {
        let file =
            split(b"FHS|^~\\&\rBHS|^~\\&\rMSH|^~\\&|A\rBTS|2\rFTS|x\r").expect("a batch file");
        assert_eq!(
            file.discrepancies(),
            [
                Discrepancy::MessageCount {
                    batch: 1,
                    stated: String::from("2"),
                    held: 1
                },
                Discrepancy::BatchCount {
                    stated: String::from("x"),
                    held: 1
                },
            ]
        );
    }
}
