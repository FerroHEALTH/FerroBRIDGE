// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The inbound intake's verifiers, event map and retry policy, driven
//! through their public API with no database and no network.

mod event;
mod retry;
mod verify;
