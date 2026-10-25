// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synthetic OHDSI vocabulary, in the shape of an Athena download.
//!
//! The ten files under `fixtures/vocabulary/` are invented for the tests:
//! the nine tables an Athena download carries and `SOURCE_TO_CONCEPT_MAP`,
//! one per vocabulary table of the CDM v5.4
//! (<https://ohdsi.github.io/CommonDataModel/cdm54.html#Vocabulary_Tables>).
//! They follow the download's shape as the Athena application writes it
//! (`docs/specs/athena/PROVENANCE.md`): upper-case file names, a header row,
//! tab-separated unquoted values, `yyyyMMdd` dates. The tests load them with
//! the production loader, `omop_cdm::athena`, so the loader is what they
//! exercise. `fixtures/vocabulary/README.md` lists what each row exercises.

use std::path::PathBuf;

/// Returns the directory that holds the synthetic download.
#[must_use]
pub fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/vocabulary")
}
