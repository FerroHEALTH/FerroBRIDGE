// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound engine's intake: what a source's request must prove, what its
//! event says, and when failed work runs again.
//!
//! Each step is driven by a source's document, so a new source is data and
//! never code. No specification governs an inbound patient-event intake
//! (ITS-REST 1.1.0 defines no change notification): our own design, built on
//! the standard of each step.

pub mod event;
pub mod retry;
pub mod verify;
