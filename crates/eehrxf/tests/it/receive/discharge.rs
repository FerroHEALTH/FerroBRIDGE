// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received discharge report: the document examples the vendored HL7
//! Europe Hospital Discharge Report 1.0.0 package publishes, read, of their
//! category and checked against `bundle-eu-hdr` and `composition-eu-hdr`.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::io::Read;

use eehrxf::category::Category;
use eehrxf::crosswalk::discharge::HDR_COMPOSITION;
use eehrxf::document::discharge::{DISCHARGE_SUMMARY, HDR_BUNDLE};
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::category::{DocumentType, LOINC, Uncategorised};
use eehrxf::receive::conform::{CheckError, FindingKind};
use eehrxf::receive::validation::{Validated, ValidationError};
use ferrobridge_term::client::Client;
use flate2::read::GzDecoder;

use crate::support::{hdr, hdr_package, profile_set};

/// Every document `Bundle` example of the HDR package, by its member path.
const EXAMPLES: [&str; 5] = [
    "package/examples/Bundle-DischargeBundle-Novak-Petr-Subsections.json",
    "package/examples/Bundle-HDR-Luigi-De-Luca-Example.json",
    "package/examples/Bundle-HDR-Paolo-Marcheschi-Example.json",
    "package/examples/Bundle-HDR-Reijer-Wolff-Example.json",
    "package/examples/Bundle-bundle-swart.json",
];

/// Returns the text of the member `path` of the vendored HDR package.
fn example(path: &str) -> Result<String, Box<dyn Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(hdr_package())?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.to_string_lossy() == path {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            return Ok(text);
        }
    }
    Err(format!("the HDR package holds no {path}").into())
}

#[test]
fn every_example_document_is_in_the_package() -> Result<(), Box<dyn Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(hdr_package())?));
    let mut bundles = Vec::new();
    for entry in archive.entries()? {
        let path = entry?.path()?.to_string_lossy().into_owned();
        if path.starts_with("package/examples/Bundle-") {
            bundles.push(path);
        }
    }
    bundles.sort();
    assert_eq!(bundles, EXAMPLES, "the suite reads every Bundle example");
    Ok(())
}

#[test]
fn the_discharge_report_is_received_with_the_hdr_profiles() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        Category::Discharge.document_type(),
        Some(DocumentType {
            system: LOINC,
            code: DISCHARGE_SUMMARY,
        })
    );
    let profiles = Category::Discharge
        .profiles()
        .ok_or("the category is received")?;
    assert_eq!(profiles.bundle, HDR_BUNDLE);
    assert_eq!(profiles.composition, HDR_COMPOSITION);
    Ok(())
}

#[test]
fn every_example_is_a_discharge_report_that_keeps_its_text() -> Result<(), Box<dyn Error>> {
    for path in EXAMPLES {
        let text = example(path)?;
        let document = ReceivedDocument::read(&text).map_err(|error| format!("{path}: {error}"))?;
        assert_eq!(document.original(), text, "{path}");
        assert_eq!(document.category()?, Category::Discharge, "{path}");
    }
    Ok(())
}

#[test]
fn every_example_conforms_to_the_hdr_document_profiles() -> Result<(), Box<dyn Error>> {
    let (bundle, composition) = (hdr(HDR_BUNDLE)?, hdr(HDR_COMPOSITION)?);
    for path in EXAMPLES {
        let document = ReceivedDocument::read(&example(path)?)?;
        document
            .check(&bundle, &composition)
            .map_err(|error| format!("{path}: {error:?}"))?;
    }
    Ok(())
}

#[test]
fn a_discharge_report_without_its_encounter_is_found() -> Result<(), Box<dyn Error>> {
    let text = example("package/examples/Bundle-HDR-Paolo-Marcheschi-Example.json")?;
    let mut tree: serde_json::Value = serde_json::from_str(&text)?;
    let composition = tree
        .pointer_mut("/entry/0/resource")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("the Composition entry")?;
    assert!(
        composition.remove("encounter").is_some(),
        "the example names its encounter"
    );
    let document = ReceivedDocument::read(&serde_json::to_string(&tree)?)?;
    match document.check(&hdr(HDR_BUNDLE)?, &hdr(HDR_COMPOSITION)?) {
        Err(CheckError::NonConformant { findings }) => {
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.element == "Composition.encounter"
                        && matches!(finding.kind, FindingKind::TooFew { .. })),
                "{findings:?}"
            );
            Ok(())
        }
        other => Err(format!("the document without its encounter passed: {other:?}").into()),
    }
}

#[test]
fn a_discharge_code_in_another_system_names_no_category() -> Result<(), Box<dyn Error>> {
    let text = example("package/examples/Bundle-HDR-Paolo-Marcheschi-Example.json")?;
    let mut tree: serde_json::Value = serde_json::from_str(&text)?;
    let coding = tree
        .pointer_mut("/entry/0/resource/type/coding/0/system")
        .ok_or("the Composition.type coding")?;
    *coding = serde_json::Value::from("http://example.org/loinc");
    let document = ReceivedDocument::read(&serde_json::to_string(&tree)?)?;
    assert_eq!(document.category(), Err(Uncategorised::Unknown));
    Ok(())
}

/// Validates the HDR example at `path`, with `edit` applied to its tree,
/// against `bundle-eu-hdr` over every fetched profile package, with no
/// terminology server (a binding the set cannot decide stays listed);
/// `None` when a local run opts out of the absent packages.
fn validated(
    path: &str,
    edit: impl FnOnce(&mut serde_json::Value) -> Result<(), Box<dyn Error>>,
) -> Result<Option<Result<Validated, ValidationError>>, Box<dyn Error>> {
    let Some(set) = profile_set()? else {
        return Ok(None);
    };
    let mut tree: serde_json::Value = serde_json::from_str(&example(path)?)?;
    edit(&mut tree)?;
    let document = ReceivedDocument::read(&serde_json::to_string(&tree)?)?;
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    Ok(Some(runtime.block_on(document.validate(
        set,
        HDR_BUNDLE,
        None::<&Client>,
    ))))
}

// NOTE: HDR 1.0.0 declares the R5 `xtehr.eu.ehds.models` as a dependency, which the set holds
// unread (packages.html 6.0.0-ballot4 §2.1.10.3 is silent on it; our own design, #1604).
#[test]
fn a_discharge_report_validates_against_bundle_eu_hdr() -> Result<(), Box<dyn Error>> {
    let path = "package/examples/Bundle-HDR-Paolo-Marcheschi-Example.json";
    let Some(outcome) = validated(path, |_| Ok(()))? else {
        return Ok(());
    };
    outcome.map_err(|error| format!("{path}: {error:?}"))?;
    Ok(())
}

#[test]
fn a_discharge_report_without_its_encounter_is_refused_by_the_validator()
-> Result<(), Box<dyn Error>> {
    let path = "package/examples/Bundle-HDR-Paolo-Marcheschi-Example.json";
    let Some(outcome) = validated(path, |tree| {
        let composition = tree
            .pointer_mut("/entry/0/resource")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or("the Composition entry")?;
        composition
            .remove("encounter")
            .ok_or("the example names its encounter")?;
        Ok(())
    })?
    else {
        return Ok(());
    };
    match outcome {
        Err(ValidationError::NonConformant { findings }) => {
            assert!(
                findings.iter().any(|finding| finding.location == "Bundle.entry[0].resource"
                    && matches!(&finding.kind, FindingKind::Issue { code: "required", message, .. }
                        if message.starts_with("Composition.encounter occurs 0 times"))),
                "{findings:?}"
            );
            Ok(())
        }
        other => Err(format!("the document without its encounter passed: {other:?}").into()),
    }
}
