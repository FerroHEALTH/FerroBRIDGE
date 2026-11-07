// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings of the HL7 v2 face that `[hl7v2]` resolves into.

/// One sending facility the face accepts, matched against MSH-4.
///
/// MSH-4 is an `HD`: a namespace id in HD.1, or a universal id in HD.2 with
/// its type in HD.3 (HL7 v2.5.1 chapter 2, `HD` data type). No specification
/// governs an allow list: our own design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sender {
    /// A facility named by its namespace id, HD.1.
    Namespace(String),
    /// A facility named by its universal id, HD.2, of the type HD.3.
    Universal {
        /// HD.2.
        id: String,
        /// HD.3, such as `ISO`.
        kind: String,
    },
}

/// A sender declared `origin = "device"`: an analyser feed with no human,
/// whose results commit as acts of the device. No specification governs the
/// declaration: our own design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// The sending facility the device sends as.
    pub sender: Sender,
    /// The namespace its device identifiers are issued in.
    pub identifier_issuer: String,
}
