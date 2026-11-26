// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound engine's intake, the one owner for every transport.
//!
//! It holds what a sender must prove (`verify`, for the HTTP intake and the
//! HL7 v2 face alike), what its event says, where it is stored before the
//! source is answered, and the one handler (`apply`) the workers on every
//! replica run.
//!
//! The workers apply every stored item through one handler,
//! [`apply::Apply`], which feeds the identity source and ensures the
//! patient's one EHR.
//!
//! The workers apply every stored item through one handler,
//! [`apply::Apply`], which feeds the identity source and ensures the
//! patient's one EHR.
//!
//! Each step is driven by a source's document, so a new source is data and
//! never code. No specification governs an inbound patient-event intake
//! (ITS-REST 1.1.0 defines no change notification): our own design, built on
//! the standard of each step.

pub mod apply;
mod clients;
pub mod event;
pub mod intake;
pub mod keeper;
pub mod recover;
pub mod store;
pub mod subscription;
pub mod verify;
pub mod worker;

/// How a failed call of the inbound engine reads its answer.
///
/// A `5xx` (RFC 9110 §15.6), `429 Too Many Requests` (RFC 6585 §4) and `423
/// Locked` (RFC 4918 §11.3) are transient, and every other failure status is
/// poisoned, so the work parks at once.
pub const STATUSES: crate::retry::Statuses = crate::retry::Statuses {
    transient: &[
        http::StatusCode::TOO_MANY_REQUESTS,
        http::StatusCode::LOCKED,
    ],
    other: crate::retry::Failure::Poisoned,
};

/// The `[inbound]` section, resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// The largest body any source may send, in bytes.
    pub body_limit: usize,
    /// How the workers run.
    pub workers: worker::Settings,
    /// How often each replica looks for a source whose recovery is due.
    pub recovery_tick: std::time::Duration,
    /// How long a parked item's event bodies are kept.
    pub parked_body_retention: std::time::Duration,
}
