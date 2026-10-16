// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OMOP end-to-end cases over the reference CDR and a CDM v5.4
//! PostgreSQL, behind the `FERROBRIDGE_E2E` gate: the laboratory round trip
//! and a composition deleted after a run loaded it, over the shared support.

mod deletion;
mod round_trip;
mod support;
