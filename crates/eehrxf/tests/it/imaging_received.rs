// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An imaging report received in the exchange format: the document examples
//! of the vendored HL7 Europe Imaging Report 1.0.0-ballot guide read, named
//! as the imaging category by the `Composition.category` slice
//! `CompositionEuImaging` requires, and held to `BundleReportEuImaging` and
//! `CompositionEuImaging`.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::io::Read;

use eehrxf::category::{Category, PRIORITY_SYSTEM};
use eehrxf::receive::category::{DocumentType, LOINC, Uncategorised};
use eehrxf::receive::{ReceiveError, ReceivedDocument};
use flate2::read::GzDecoder;

use super::support::{imaging, imaging_package};

/// The canonical URL of the Imaging Report `Bundle` profile.
const BUNDLE_REPORT_EU_IMAGING: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/BundleReportEuImaging";

/// The canonical URL of the Imaging Report `Composition` profile.
const COMPOSITION_EU_IMAGING: &str =
    "http://hl7.eu/fhir/imaging/StructureDefinition/CompositionEuImaging";

/// The two document examples of the guide, each a `BundleReportEuImaging`.
const DOCUMENT_EXAMPLES: [&str; 2] = [
    "Bundle-bundle-report-semi-structured.json",
    "Bundle-bundle-report-structured.json",
];

/// The `Composition.category` coding of the `imaging` slice, as the
/// semi-structured example writes it.
const MEDICAL_IMAGING: &str = r#"{"coding": [{"system": "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs", "code": "Medical-Imaging"}]}"#;

/// Returns the text of the example `file` the vendored Imaging Report
/// package carries under `package/example/`.
fn example(file: &str) -> Result<String, Box<dyn Error>> {
    member(&format!("package/example/{file}"))
}

/// Returns the text of the member `wanted` of the vendored Imaging Report
/// package.
fn member(wanted: &str) -> Result<String, Box<dyn Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(imaging_package())?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.to_string_lossy() == wanted {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            return Ok(text);
        }
    }
    Err(format!("the Imaging Report package carries no {wanted}").into())
}

/// The semi-structured example with its `Composition.category` codings
/// replaced by `categories`, a JSON array, re-serialised so the document
/// stays faithful to the R4 model.
fn semi_structured_with(categories: &str) -> Result<String, Box<dyn Error>> {
    let mut bundle: serde_json::Value =
        serde_json::from_str(&example("Bundle-bundle-report-semi-structured.json")?)?;
    let composition = bundle
        .pointer_mut("/entry/0/resource")
        .ok_or("the first entry")?;
    assert_eq!(composition["resourceType"], "Composition");
    let replaced: serde_json::Value = serde_json::from_str(categories)?;
    composition["category"] = replaced;
    Ok(serde_json::to_string_pretty(&bundle)?)
}

#[test]
fn the_imaging_category_is_named_by_the_composition_category_its_profile_requires() {
    assert_eq!(Category::Imaging.document_type(), None);
    assert_eq!(
        Category::Imaging.document_category(),
        Some(DocumentType {
            system: PRIORITY_SYSTEM,
            code: "Medical-Imaging",
        })
    );
    let profiles = Category::Imaging.profiles();
    assert_eq!(
        profiles.map(|profiles| (profiles.bundle, profiles.composition)),
        Some((BUNDLE_REPORT_EU_IMAGING, COMPOSITION_EU_IMAGING))
    );
}

#[test]
fn the_profile_requires_the_category_the_component_reads() -> Result<(), Box<dyn Error>> {
    let json: serde_json::Value = serde_json::from_str(&member(
        "package/StructureDefinition-CompositionEuImaging.json",
    )?)?;
    assert_eq!(json["url"], COMPOSITION_EU_IMAGING);
    let slice = json["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?
        .iter()
        .find(|element| element["id"] == "Composition.category:imaging")
        .ok_or("the imaging category slice")?;
    assert_eq!(slice["min"], 1);
    assert_eq!(
        slice["patternCodeableConcept"]["coding"][0]["system"],
        PRIORITY_SYSTEM
    );
    assert_eq!(
        slice["patternCodeableConcept"]["coding"][0]["code"],
        "Medical-Imaging"
    );
    let r#type = json["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?
        .iter()
        .find(|element| element["id"] == "Composition.type")
        .ok_or("Composition.type")?;
    assert_eq!(r#type["binding"]["strength"], "preferred");
    Ok(())
}

#[test]
fn every_document_example_of_the_guide_is_an_imaging_report() -> Result<(), Box<dyn Error>> {
    for file in DOCUMENT_EXAMPLES {
        let document = ReceivedDocument::read(&example(file)?)?;
        assert_eq!(document.category(), Ok(Category::Imaging), "{file}");
    }
    Ok(())
}

#[test]
fn every_document_example_of_the_guide_holds_to_the_imaging_profiles() -> Result<(), Box<dyn Error>>
{
    let profiles = Category::Imaging
        .profiles()
        .ok_or("the imaging category is received")?;
    let bundle = imaging(profiles.bundle)?;
    let composition = imaging(profiles.composition)?;
    for file in DOCUMENT_EXAMPLES {
        let document = ReceivedDocument::read(&example(file)?)?;
        let conformance = document.check(&bundle, &composition);
        assert!(conformance.is_ok(), "{file}: {conformance:?}");
    }
    Ok(())
}

#[test]
fn an_imaging_type_without_the_category_names_no_category() -> Result<(), Box<dyn Error>> {
    // NOTE: CompositionEuImaging binds Composition.type to its value set at strength preferred
    // only, so an imaging report type alone names no category.
    let text = semi_structured_with(&format!(
        r#"[{{"coding": [{{"system": "{LOINC}", "code": "85430-7"}}]}}]"#
    ))?;
    let document = ReceivedDocument::read(&text)?;
    assert_eq!(document.category(), Err(Uncategorised::Unknown));
    Ok(())
}

#[test]
fn a_document_without_the_imaging_category_breaks_the_profile() -> Result<(), Box<dyn Error>> {
    let text = semi_structured_with(&format!(
        r#"[{{"coding": [{{"system": "{LOINC}", "code": "85430-7"}}]}}]"#
    ))?;
    let document = ReceivedDocument::read(&text)?;
    let refused = document.check(
        &imaging(BUNDLE_REPORT_EU_IMAGING)?,
        &imaging(COMPOSITION_EU_IMAGING)?,
    );
    assert!(refused.is_err(), "{refused:?}");
    Ok(())
}

#[test]
fn the_category_alone_names_an_imaging_report() -> Result<(), Box<dyn Error>> {
    let text = semi_structured_with(&format!("[{MEDICAL_IMAGING}]"))?;
    assert_eq!(
        ReceivedDocument::read(&text)?.category(),
        Ok(Category::Imaging)
    );
    Ok(())
}

#[cfg(feature = "laboratory")]
#[test]
fn a_document_naming_two_categories_is_of_neither() -> Result<(), Box<dyn Error>> {
    let mut bundle: serde_json::Value =
        serde_json::from_str(&example("Bundle-bundle-report-semi-structured.json")?)?;
    let composition = bundle
        .pointer_mut("/entry/0/resource")
        .ok_or("the first entry")?;
    composition["type"] = serde_json::json!({"coding": [{"system": LOINC, "code": "11502-2"}]});
    let document = ReceivedDocument::read(&serde_json::to_string_pretty(&bundle)?)?;
    assert_eq!(document.category(), Err(Uncategorised::Several));
    Ok(())
}

#[test]
fn the_minimal_metadata_example_is_no_document() -> Result<(), Box<dyn Error>> {
    // NOTE: HL7 Europe Imaging Report 1.0.0-ballot, BundleReportMinimalMetadataEuImaging is a
    // collection without a Composition, so it is no document R4 bundle.html defines.
    let refused = ReceivedDocument::read(&example(
        "Bundle-bundle-report-minimal-metadata-unstructured.json",
    )?);
    assert!(
        matches!(refused, Err(ReceiveError::NotADocument { ref found }) if found.as_deref() == Some("collection")),
        "{refused:?}"
    );
    Ok(())
}
