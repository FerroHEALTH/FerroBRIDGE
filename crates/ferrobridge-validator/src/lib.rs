// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FHIR R4 and STU3 profile validator over FHIR packages read offline.
//!
//! [`set::PackageSet::load`] reads packages from `.tgz` files and unpacked
//! directories, checks that every dependency their manifests name is present,
//! and resolves `StructureDefinitions` by canonical into the generated
//! `fhir_types::r4` model; [`set::PackageSet::load_release`] reads them as
//! another [`release::Release`], [`release::Stu3`]. [`validate::Validator`]
//! walks an instance, the lexical document model of `fhir-types`, over the
//! release's element table and holds it to the snapshot of the profile it is
//! validated against (<https://hl7.org/fhir/R4/validation.html>,
//! <https://hl7.org/fhir/R4/profiling.html>). The result is a
//! [`report::Report`] of typed [`issue::Issue`]s that converts into an R4
//! `OperationOutcome`, and that names every constraint the validator met and
//! did not evaluate.
//!
//! ```no_run
//! use ferrobridge_validator::set::PackageSet;
//! use ferrobridge_validator::validate::Validator;
//! use fhir_types::codec::Value;
//! use fhir_types::r4::operation_outcome::OperationOutcome;
//!
//! let set = PackageSet::load(["vendor/hl7.fhir.r4.core", "packages/my.profiles.tgz"])?;
//! let validator = Validator::new(&set);
//! let patient: Value = serde_json::from_str(r#"{"resourceType": "Patient", "active": true}"#)?;
//! let report = validator.validate_against(&patient, "http://example.org/StructureDefinition/my-patient")?;
//! let outcome = OperationOutcome::from(&report);
//! assert!(!outcome.issue.is_empty());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![doc(test(attr(deny(warnings))))]

pub mod canonical;
pub mod issue;
pub mod package;
pub mod release;
pub mod report;
pub mod set;
pub mod terminology;
pub mod validate;
