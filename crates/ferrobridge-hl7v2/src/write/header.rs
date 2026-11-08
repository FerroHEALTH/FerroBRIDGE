// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The message header segment a composed message opens with.

use crate::write::value::{Field, Segment};

/// The fields of an `MSH` segment a message is composed with.
///
/// MSH-1 and MSH-2 come from the writer's delimiters. MSH-18 names the
/// character set from HL7 table 0211 (HL7 v2.9 chapter 2 §2.14.9.18), and
/// [`Writer::compose`] encodes the message in the set it names; an empty
/// MSH-18 leaves the set to the connection's agreement.
///
/// [`Writer::compose`]: crate::write::Writer::compose
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Msh {
    /// MSH-3, the sending application.
    pub sending_application: Field,
    /// MSH-4, the sending facility.
    pub sending_facility: Field,
    /// MSH-5, the receiving application.
    pub receiving_application: Field,
    /// MSH-6, the receiving facility.
    pub receiving_facility: Field,
    /// MSH-7, the date and time of the message.
    pub timestamp: Field,
    /// MSH-9, the message code, trigger event and message structure.
    pub message_type: Field,
    /// MSH-10, the message control id.
    pub control_id: Field,
    /// MSH-11, the processing id.
    pub processing_id: Field,
    /// MSH-12, the version id.
    pub version: Field,
    /// MSH-18, the character set.
    pub charset: Field,
}

impl From<Msh> for Segment {
    fn from(msh: Msh) -> Self {
        let mut fields = vec![
            msh.sending_application,
            msh.sending_facility,
            msh.receiving_application,
            msh.receiving_facility,
            msh.timestamp,
            Field::default(),
            msh.message_type,
            msh.control_id,
            msh.processing_id,
            msh.version,
        ];
        fields.resize(15, Field::default());
        fields.push(msh.charset);
        Self {
            id: String::from("MSH"),
            fields,
        }
    }
}
