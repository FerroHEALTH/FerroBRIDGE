// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIRconnect mappings the component ships (feature `openehr`).
//!
//! FerroBRIDGE authors one FHIRconnect 1.0.0 project per HL7 Europe profile
//! set it maps, and the crate ships each under `mappings/<project>/` in its
//! package, embedded here as text so a system that depends on the published
//! crate runs the same files the crate's tests run. A [`Project`] names its
//! files, in loading order, and the context mappings to compile from them;
//! [`crate::mapping::Mapping::compile_project`] compiles them from their
//! text with no file written, [`Project::with_context`] narrows a project to
//! one of its contexts, and [`Project::write_to`] writes them where a
//! system keeps its mappings for [`crate::mapping::Mapping::compile`]. No
//! specification governs where a mapping is shipped: our own design.

use std::io;
use std::path::Path;
use std::path::PathBuf;

/// One shipped FHIRconnect project: its directory, its files, the context
/// mappings to compile from them and the received elements it accounts for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Project {
    directory: &'static str,
    contexts: &'static [&'static str],
    files: &'static [File],
    accounted: &'static [&'static str],
}

/// A context mapping a project does not compile, named to
/// [`Project::with_context`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the {project} project compiles no context mapping named `{context}`")]
pub struct UnknownContext {
    /// The project's directory.
    pub project: &'static str,
    /// The context name asked for.
    pub context: String,
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

    /// Returns this project narrowed to its one context mapping `name`, so
    /// [`crate::mapping::Mapping::compile_project`] compiles that context alone
    /// from the same files.
    ///
    /// A project whose contexts map one template, as `MPD`'s request and
    /// dispense contexts do, compiles one of them this way, so a run over that
    /// template has one program to select.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownContext`] when the project compiles no context mapping
    /// named `name`.
    pub fn with_context(self, name: &str) -> Result<Self, UnknownContext> {
        let contexts = self.contexts;
        let chosen = contexts
            .iter()
            .find(|held| **held == name)
            .ok_or_else(|| UnknownContext {
                project: self.directory,
                context: name.to_owned(),
            })?;
        Ok(Self {
            contexts: std::slice::from_ref(chosen),
            ..self
        })
    }

    /// Returns the project's files, in loading order.
    #[must_use]
    pub const fn files(&self) -> &'static [File] {
        self.files
    }

    /// Returns the received elements the project accounts for, each a
    /// FHIRPath of element names rooted at a resource type: a received
    /// instance of one that the run carries into no structured element is
    /// declared in the run's `OperationOutcome`
    /// ([`crate::mapping::Mapping::accounting`]).
    #[must_use]
    pub const fn accounted(&self) -> &'static [&'static str] {
        self.accounted
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
#[cfg(any(
    feature = "prescription",
    feature = "dispensation",
    feature = "imaging",
    feature = "laboratory",
    feature = "patient-summary",
    feature = "discharge"
))]
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
    accounted: &[],
};

/// The HL7 Europe Imaging Report 1.0.0-ballot project.
///
/// It maps over FerroBRIDGE's imaging report template: the imaging
/// examination result to the `DiagnosticReportEuImaging` the document holds
/// beside its composition, and the study it reports to the
/// `ImagingStudyEuImaging` the report references.
#[cfg(feature = "imaging")]
pub const IMAGING: Project = Project {
    directory: "imaging",
    contexts: &["ferrobridge_eu_imaging.context"],
    files: &[
        shipped!("imaging", "ferrobridge_eu_imaging.context.yml"),
        shipped!("imaging", "ferrobridge_imaging_exam_result.v1.yml"),
        shipped!("imaging", "ferrobridge_eu_imaging_report.yml"),
    ],
    accounted: &[],
};

/// The HL7 Europe Laboratory Report 2.0.0 project.
///
/// It maps over the published `KDS_Laborbericht` template. Its two contexts
/// map that one template, so a run compiles one of them with
/// [`Project::with_context`]: `ferrobridge_eu_laboratory.context` maps the
/// laboratory test result to the `DiagnosticReport-eu-lab` the document holds
/// beside its composition, each analyte to an `Observation-resultslab-eu-lab`
/// and each specimen to a `Specimen-eu-lab`, and
/// `ferrobridge_eu_lab_composition.context` maps the report composition to a
/// `Composition-eu-lab`.
#[cfg(feature = "laboratory")]
pub const LABORATORY: Project = Project {
    directory: "laboratory",
    contexts: &[
        "ferrobridge_eu_laboratory.context",
        "ferrobridge_eu_lab_composition.context",
    ],
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
    // NOTE: no specification governs this: our own design; a result read without its
    // reference range or the order it answers is misread, so each loss of one is declared.
    accounted: &[
        "DiagnosticReport.basedOn.reference",
        "DiagnosticReport.basedOn.identifier",
        "DiagnosticReport.basedOn.display",
        "DiagnosticReport.basedOn.extension",
        "Observation.basedOn",
        "Observation.referenceRange",
    ],
};

/// The HL7 Europe Patient Summary 1.0.0-ballot project.
///
/// It maps over FerroBRIDGE's patient summary template, which places the
/// medication as the openEHR International Patient Summary template does. Its
/// two contexts map that one template, so a run compiles one of them with
/// [`Project::with_context`]: `ferrobridge_eu_eps_medication.context` maps
/// each medication order of the template's medication summary section to a
/// `MedicationStatement-eu-eps`, the medication summary's entry, and
/// `ferrobridge_eu_eps_immunization.context` maps each medication action of
/// the template's immunisations section to an `immunization-eu-core`. The
/// template path is the placement criterion: each context starts at the
/// archetype the template holds in its section alone.
#[cfg(feature = "patient-summary")]
pub const EPS: Project = Project {
    directory: "eps",
    contexts: &[
        "ferrobridge_eu_eps_medication.context",
        "ferrobridge_eu_eps_immunization.context",
    ],
    files: &[
        shipped!("eps", "ferrobridge_eu_eps_medication.context.yml"),
        shipped!(
            "eps",
            "ferrobridge_medication_order.v3.MedicationStatement.yml"
        ),
        shipped!("eps", "ferrobridge_eu_eps_immunization.context.yml"),
        shipped!("eps", "ferrobridge_medication.v1.Immunization.yml"),
    ],
    accounted: &[],
};

/// The HL7 Europe Hospital Discharge Report 1.0.0 project.
///
/// It maps over FerroBRIDGE's discharge report template, a transfer of care
/// summary over published CKM archetypes. Its one context,
/// `ferrobridge_eu_hdr.context`, maps the composition to a
/// `composition-eu-hdr` naming an `encounter-eu-hdr` built from the
/// composition's context, the hospital stay, each problem or diagnosis of
/// the template's diagnostic summary section to a `condition-eu-core`, each
/// procedure of its significant procedures section to a `procedure-eu-core`
/// and each medication order of its discharge medications section to a
/// `medicationRequest-eu-core`, each an entry of the section of that name.
#[cfg(feature = "discharge")]
pub const HDR: Project = Project {
    directory: "hdr",
    contexts: &["ferrobridge_eu_hdr.context"],
    files: &[
        shipped!("hdr", "ferrobridge_eu_hdr.context.yml"),
        shipped!("hdr", "ferrobridge_transfer_summary.v1.Composition.yml"),
        shipped!("hdr", "ferrobridge_problem_diagnosis.v1.yml"),
        shipped!("hdr", "ferrobridge_procedure.v1.yml"),
        shipped!("hdr", "ferrobridge_medication_order.v3.yml"),
    ],
    accounted: &[],
};
