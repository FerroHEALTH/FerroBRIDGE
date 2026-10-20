// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A batch file through the face: each message answered as a message alone,
//! and the answers returned in one response batch (HL7 v2 chapter 2
//! §2.10.3.2: "all messages are acknowledged in the response batch").

use ferrobridge_hl7v2::batch;
use ferrobridge_hl7v2::mllp::Connection;

use crate::hl7v2::Face;
use crate::hl7v2::stamped::Stamped;

impl Face {
    /// Answers a batch file with a response batch holding each message's
    /// acknowledgment, in order; `None` when the file does not split, since
    /// no header then addresses an answer.
    pub(super) async fn answer_batch(
        &self,
        bytes: &[u8],
        connection: &Connection,
    ) -> Option<Vec<u8>> {
        let file = match batch::split(bytes) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(error = %error, "the batch file does not split and is not answered");
                return None;
            }
        };
        for discrepancy in file.discrepancies() {
            tracing::warn!(
                discrepancy = discrepancy.kind(),
                "a batch trailer count disagrees with the file"
            );
        }
        let mut acks = Vec::with_capacity(file.batches().len());
        let mut messages = 0usize;
        let mut unanswered = 0usize;
        for part in file.batches() {
            let mut answers = Vec::with_capacity(part.messages().len());
            for message in part.messages() {
                messages = messages.saturating_add(1);
                match self.answer_message(message, connection).await {
                    Some(reply) => answers.push(reply),
                    None => unanswered = unanswered.saturating_add(1),
                }
            }
            acks.push(answers);
        }
        tracing::info!(
            batches = file.batches().len(),
            messages,
            unanswered,
            "batch file acknowledged"
        );
        let stamped = Stamped::now();
        Some(batch::reply(
            &file,
            &acks,
            batch::Stamp {
                control_id: &stamped.control_id,
                timestamp: &stamped.timestamp,
            },
        ))
    }
}
