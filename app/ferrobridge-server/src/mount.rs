// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the server mounts the surfaces more than one module serves under.

/// The path every facade route sits under.
///
/// The FHIR service base is `[base]` of the R4 HTTP chapter
/// (<https://hl7.org/fhir/R4/http.html>), and this deployment mounts it at
/// `/fhir` so the health routes and the FHIRconnect operations keep their own
/// space (no specification governs the mount point: our own design). The
/// SMART configuration is served under it as well.
pub const BASE_PATH: &str = "/fhir";
