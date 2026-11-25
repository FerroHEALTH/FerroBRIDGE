// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound engine's intake: what a source's request must prove, what its
//! event says, where it is stored before the source is answered, and how the
//! workers on every replica apply it.
//!
//! Each step is driven by a source's document, so a new source is data and
//! never code. No specification governs an inbound patient-event intake
//! (ITS-REST 1.1.0 defines no change notification): our own design, built on
//! the standard of each step.

pub mod apply;
pub mod event;
pub mod intake;
pub mod keeper;
pub mod recover;
pub mod retry;
pub mod store;
pub mod subscription;
pub mod verify;
pub mod worker;

/// The `[inbound]` section, resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// The largest body any source may send, in bytes.
    pub body_limit: usize,
    /// How the workers run.
    pub workers: worker::Settings,
    /// How often each replica looks for a source whose recovery is due.
    pub recovery_tick: std::time::Duration,
}
