// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[mappings]` section.
//!
//! No specification governs the configuration: our own design.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The mapping set this deployment runs.
///
/// One tree of FHIRconnect mapping files and, optionally, one directory of the
/// operational templates the two operations compile against. With no
/// `templates`, the operations take each template a context names from the
/// `[cdr]`, and a `directory` with neither is refused at boot. Everything is
/// read once at boot, so a mapping that does not compile is a refusal to start
/// rather than a failure on the request that first touches it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mappings {
    /// The directory the FHIRconnect files are read from, recursively.
    pub directory: Option<PathBuf>,
    /// The directory holding the operational templates, as OPT 1.4 XML.
    ///
    /// When set, it wins over the `[cdr]` for the two operations; the facade
    /// always takes its templates from the CDR.
    pub templates: Option<PathBuf>,
    /// The directory the OMOCL files `etl run` maps with are read from,
    /// recursively.
    pub omocl: Option<PathBuf>,
}
