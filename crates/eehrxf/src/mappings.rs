// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIRconnect mappings the component ships (feature `openehr`).
//!
//! FerroBRIDGE authors one FHIRconnect 1.0.0 project per HL7 Europe profile
//! set it maps, and the crate ships each under `mappings/<project>/` in its
//! package, embedded here as text so a system that depends on the published
//! crate runs the same files the crate's tests run. A [`Project`] names its
//! files, in loading order, and the context mappings to compile from them;
//! the files are the input of [`crate::mapping::Mapping`], which a caller
//! compiles from the files written where it keeps its mappings. No
//! specification governs where a mapping is shipped: our own design.

use std::io;
use std::path::Path;
use std::path::PathBuf;

/// One shipped FHIRconnect project: its directory, its files and the
/// context mappings to compile from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Project {
    directory: &'static str,
    contexts: &'static [&'static str],
    files: &'static [File],
}

/// One shipped mapping file: its file name and its YAML text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct File {
    /// The file name, under the project's directory.
    pub name: &'static str,
    /// The file's YAML text, as the crate ships it.
    pub text: &'static str,
}

impl Project {
    /// Returns the name of the project's directory under `mappings/`.
    #[must_use]
    pub const fn directory(&self) -> &'static str {
        self.directory
    }

    /// Returns the `metadata.name` of each context mapping the project
    /// compiles.
    #[must_use]
    pub const fn contexts(&self) -> &'static [&'static str] {
        self.contexts
    }

    /// Returns the project's files, in loading order.
    #[must_use]
    pub const fn files(&self) -> &'static [File] {
        self.files
    }

    /// Writes every file of the project into `directory`, which must exist,
    /// and answers the paths written, in loading order, the `files` a
    /// [`crate::mapping::Mapping`] compiles from.
    ///
    /// # Errors
    ///
    /// Returns the I/O error of the first file that cannot be written.
    pub fn write_to(&self, directory: &Path) -> io::Result<Vec<PathBuf>> {
        self.files
            .iter()
            .map(|file| {
                let path = directory.join(file.name);
                std::fs::write(&path, file.text).map(|()| path)
            })
            .collect()
    }
}

/// Embeds the file `name` of the shipped project `directory`.
macro_rules! shipped {
    ($directory:literal, $name:literal) => {
        File {
            name: $name,
            text: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/mappings/",
                $directory,
                "/",
                $name
            )),
        }
    };
}

/// The HL7 Europe Medication Prescription and Dispense 1.0.0 project.
///
/// It maps over FerroBRIDGE's medication template: `MedicationRequest-eu-mpd`
/// from the medication order and `MedicationDispense-eu-mpd` from the
/// medication action at its dispensed careflow step, each with its
/// `Medication-eu-mpd`.
#[cfg(any(feature = "prescription", feature = "dispensation"))]
pub const MPD: Project = Project {
    directory: "mpd",
    contexts: &[
        "ferrobridge_eu_mpd_request.context",
        "ferrobridge_eu_mpd_dispense.context",
    ],
    files: &[
        shipped!("mpd", "ferrobridge_eu_mpd_request.context.yml"),
        shipped!("mpd", "ferrobridge_medication_order.v3.yml"),
        shipped!("mpd", "ferrobridge_medication.v2.yml"),
        shipped!("mpd", "ferrobridge_dosage.v2.yml"),
        shipped!("mpd", "ferrobridge_eu_mpd_request.yml"),
        shipped!("mpd", "ferrobridge_eu_mpd_medication.yml"),
        shipped!("mpd", "ferrobridge_eu_mpd_dispense.context.yml"),
        shipped!("mpd", "ferrobridge_medication.v1.yml"),
        shipped!("mpd", "ferrobridge_eu_mpd_dispense.yml"),
    ],
};

/// The HL7 Europe Laboratory Report 2.0.0 project.
///
/// It maps over the published `KDS_Laborbericht` template. Its context maps
/// the laboratory test result to the `DiagnosticReport-eu-lab` the document
/// holds beside its composition, each analyte to an
/// `Observation-resultslab-eu-lab` and each specimen to a `Specimen-eu-lab`.
/// The directory also holds `ferrobridge_eu_lab_composition.context`, which
/// maps the report composition to a `Composition-eu-lab` over the same
/// template and is compiled on its own.
#[cfg(feature = "laboratory")]
pub const LABORATORY: Project = Project {
    directory: "laboratory",
    contexts: &["ferrobridge_eu_laboratory.context"],
    files: &[
        shipped!("laboratory", "ferrobridge_eu_laboratory.context.yml"),
        shipped!("laboratory", "ferrobridge_laboratory_test_result.v1.yml"),
        shipped!("laboratory", "ferrobridge_laboratory_test_analyte.v1.yml"),
        shipped!("laboratory", "ferrobridge_specimen.v1.yml"),
        shipped!("laboratory", "ferrobridge_eu_laboratory_result.yml"),
        shipped!("laboratory", "ferrobridge_eu_laboratory_analyte.yml"),
        shipped!("laboratory", "ferrobridge_eu_laboratory_specimen.yml"),
        shipped!("laboratory", "ferrobridge_eu_lab_composition.context.yml"),
        shipped!("laboratory", "ferrobridge_report_result.v1.Composition.yml"),
        shipped!("laboratory", "ferrobridge_eu_lab_composition.yml"),
        shipped!("laboratory", "ferrobridge_eu_lab_composition_report.yml"),
    ],
};
