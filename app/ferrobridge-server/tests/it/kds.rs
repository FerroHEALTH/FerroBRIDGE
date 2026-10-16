// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The KDS diagnosis and person round trips' mapping sets, laid out as the
//! server reads a mapping directory.
//!
//! The files are the ones the `fhirconnect` round-trip suites compile: the
//! published model mappings and extensions of the vendored FHIRconnect
//! mapping library, verbatim, and the FerroBRIDGE project directories beside
//! those suites, with the published `KDS_Diagnose` and `KDS_Person`
//! templates of the testkit.

use std::error::Error as StdError;
use std::path::Path;

/// The vendored mapping library, relative to this crate's manifest.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/fhirconnect-mapping-lib"
);

/// The project directory the `fhirconnect` round-trip suite authored.
const PROJECT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/fhirconnect/tests/fixtures/projects/ferrobridge/kds_diagnose"
);

/// The published files the project context loads.
const PUBLISHED: &[&str] = &[
    "model/evaluation/org.openehr/problem_diagnosis.v1.yml",
    "model/cluster/org.openehr/anatomical_location.v1.yml",
    "model/cluster/org.openehr/problem_qualifier.v2.yml",
    "model/cluster/org.highmed/multiple_coding_icd10gm.v1.yml",
    "model/cluster/org.openehr/case_identification.v0.yml",
    "model/composition/org.openehr/report.v1.Condition.yml",
    "projects/org.highmed/KDS/diagnose/KDS_problem_qualifier.yml",
    "projects/org.highmed/KDS/diagnose/KDS_anatomical_location.yml",
    "projects/org.highmed/KDS/diagnose/KDS_lebensphase.yml",
];

/// The project files, the context first.
const PROJECT_FILES: &[&str] = &[
    "ferrobridge_kds_diagnose.context.yml",
    "ferrobridge_kds_problem_diagnose.yml",
    "ferrobridge_kds_problem_qualifier.yml",
    "ferrobridge_lebensphase.v0.yml",
    "ferrobridge_kds_composition.Condition.yml",
];

/// The profile the project context claims.
pub(crate) const PROFILE: &str = "https://www.medizininformatik-initiative.de/fhir/core/modul-diagnose/StructureDefinition/Diagnose";

/// Copies every mapping file of the round trip into `directory`, flat.
///
/// # Errors
///
/// Returns the I/O error of a file that cannot be copied.
pub(crate) fn write_mappings(directory: &Path) -> Result<(), Box<dyn StdError>> {
    std::fs::create_dir_all(directory)?;
    for file in PUBLISHED {
        let source = Path::new(CORPUS).join(file);
        let name = source.file_name().ok_or("a published file has a name")?;
        std::fs::copy(&source, directory.join(name))?;
    }
    for file in PROJECT_FILES {
        std::fs::copy(Path::new(PROJECT).join(file), directory.join(file))?;
    }
    Ok(())
}

/// The project directory the `fhirconnect` person round-trip suite authored.
const PERSON_PROJECT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/fhirconnect/tests/fixtures/projects/ferrobridge/kds_person"
);

/// The published files the person project context loads.
const PERSON_PUBLISHED: &[&str] = &[
    "model/admin_entry/org.highmed/person_data.v0.yml",
    "model/admin_entry/org.highmed/versicherungsinformationen.v0.yml",
    "model/cluster/org.openehr/structured_name.v1.yml",
    "model/cluster/org.openehr/person_birth_data_iso.v0.yml",
    "model/cluster/org.openehr/death_details.v1.yml",
    "model/cluster/org.openehr/organisation.v1.yml",
    "projects/org.highmed/KDS/person/KDS_Composition.yml",
    "projects/org.highmed/KDS/person/KDS_gender.yml",
];

/// The person project files, the context first.
const PERSON_PROJECT_FILES: &[&str] = &[
    "ferrobridge_kds_person.context.yml",
    "ferrobridge_person.v1.yml",
    "ferrobridge_address.v1.yml",
    "ferrobridge_person.v1.Patient.yml",
    "ferrobridge_gender.v1.yml",
    "ferrobridge_kds_admin_entry_person.yml",
    "ferrobridge_kds_cluster_person.yml",
    "ferrobridge_kds_address.yml",
    "ferrobridge_kds_structured_name.yml",
];

/// The profile the person project context claims.
pub(crate) const PERSON_PROFILE: &str = "https://www.medizininformatik-initiative.de/fhir/core/modul-person/StructureDefinition/Patient";

/// Copies every mapping file of the person round trip into `directory`, flat.
///
/// # Errors
///
/// Returns the I/O error of a file that cannot be copied.
pub(crate) fn write_person_mappings(directory: &Path) -> Result<(), Box<dyn StdError>> {
    std::fs::create_dir_all(directory)?;
    for file in PERSON_PUBLISHED {
        let source = Path::new(CORPUS).join(file);
        let name = source.file_name().ok_or("a published file has a name")?;
        std::fs::copy(&source, directory.join(name))?;
    }
    for file in PERSON_PROJECT_FILES {
        std::fs::copy(Path::new(PERSON_PROJECT).join(file), directory.join(file))?;
    }
    Ok(())
}

/// Returns the synthetic `KDS_Diagnose` composition as a FLAT document.
///
/// # Errors
///
/// Returns the I/O or JSON error of a fixture that cannot be read.
pub(crate) fn flat_composition()
-> Result<serde_json::Map<String, serde_json::Value>, Box<dyn StdError>> {
    let text = std::fs::read_to_string(Path::new(PROJECT).join("kds_diagnose.flat.json"))?;
    Ok(serde_json::from_str(&text)?)
}
