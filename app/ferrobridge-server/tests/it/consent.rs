// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access directive exchange's mappings through their public API: FHIR
//! `Consent` (R4 and R5), IHE BPPC and IHE APPC, each against its vendored
//! examples, with every loss refused at its element.

pub(crate) mod appc;
pub(crate) mod bppc;
pub(crate) mod fhir;
