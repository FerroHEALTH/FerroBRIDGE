// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The display each coding of a document carries, held to the vendored
//! European material it is taken from (FHIR R4 `Coding.display`,
//! <https://hl7.org/fhir/R4/datatypes.html#Coding>).
//!
//! The precedence is `document::display`'s: a value set of the MyHealth@EU
//! Master Value Sets Catalogue 9.1.0, then a value set or code system of an
//! HL7 Europe guide, then an English example of one; a code none of them names
//! carries no display.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::io::Read;
use std::path::PathBuf;

use eehrxf::category::PRIORITY_SYSTEM;
use eehrxf::document::discharge::{DISCHARGE_SUMMARY, HDR_SLOTS};
use eehrxf::document::display::display;
use eehrxf::document::form::{SLOTS, Slot};
use eehrxf::document::imaging::{IMAGING_CATEGORIES, IMAGING_REPORT, IMAGING_SLOTS};
use eehrxf::document::laboratory::{LAB_SLOTS, LABORATORY_REPORT};
use eehrxf::document::{EMPTY_REASON, LOINC, PATIENT_SUMMARY};
use flate2::read::GzDecoder;
use serde_json::Value;

/// The vendored packages a display is read from, relative to `docs/specs/`.
const PACKAGES: [&str; 8] = [
    "ehdsi-mvc/myhealth.eu.fhir.mvc-package-9.1.0.tgz",
    "eu-hl7-base/hl7.fhir.eu.base-2.0.1.tgz",
    "eu-hl7-eps/hl7.fhir.eu.eps-1.0.0-ballot.tgz",
    "eu-hl7-hdr/hl7.fhir.eu.hdr-1.0.0.tgz",
    HEALTH_DATA_API,
    "eu-hl7-imaging/hl7.fhir.eu.imaging-1.0.0-ballot.tgz",
    "eu-hl7-laboratory/hl7.fhir.eu.laboratory-2.0.0.tgz",
    "eu-hl7-mpd/hl7.fhir.eu.mpd-1.0.0.tgz",
];

/// The EU Health Data API package, which holds the priority category code
/// system, relative to `docs/specs/`.
const HEALTH_DATA_API: &str = "eu-hl7-health-data-api/hl7.fhir.eu.health-data-api-1.0.0-ballot.tgz";

/// Where a vendored display stands in the precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    /// A value set of the MyHealth@EU catalogue.
    Catalogue,
    /// A value set or code system of an HL7 Europe guide.
    Guide,
    /// An English example of an HL7 Europe guide.
    Example,
}

/// Every LOINC display the vendored packages give, by code and rank.
type Displays = BTreeMap<String, BTreeMap<Rank, BTreeSet<String>>>;

/// Collects the LOINC codings and concepts under `value` into `found`.
fn collect(value: &Value, rank: Rank, found: &mut Displays) {
    match value {
        Value::Object(object) => {
            let loinc = object.get("system").and_then(Value::as_str) == Some(LOINC);
            if loinc {
                if let (Some(code), Some(text)) = (
                    object.get("code").and_then(Value::as_str),
                    object.get("display").and_then(Value::as_str),
                ) {
                    keep(found, code, rank, text);
                }
                for concept in object
                    .get("concept")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let (Some(code), Some(text)) = (
                        concept.get("code").and_then(Value::as_str),
                        concept.get("display").and_then(Value::as_str),
                    ) {
                        keep(found, code, rank, text);
                    }
                }
            }
            for child in object.values() {
                collect(child, rank, found);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect(item, rank, found);
            }
        }
        _ => {}
    }
}

/// Keeps one display, an example's only when it is English text.
fn keep(found: &mut Displays, code: &str, rank: Rank, text: &str) {
    if rank == Rank::Example && !text.is_ascii() {
        return;
    }
    found
        .entry(code.to_owned())
        .or_default()
        .entry(rank)
        .or_default()
        .insert(text.to_owned());
}

/// Reads every LOINC display of the vendored packages.
fn vendored() -> Result<Displays, Box<dyn Error>> {
    let mut found = Displays::new();
    for package in PACKAGES {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/specs")
            .join(package);
        let catalogue = package.starts_with("ehdsi-mvc/");
        let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(path)?));
        for entry in archive.entries()? {
            let mut entry = entry?;
            let name = entry.path()?.to_string_lossy().into_owned();
            let file = name.rsplit('/').next().unwrap_or_default().to_owned();
            if std::path::Path::new(&file)
                .extension()
                .is_none_or(|extension| extension != "json")
            {
                continue;
            }
            let terminology = file.starts_with("ValueSet-") || file.starts_with("CodeSystem-");
            let rank = match (catalogue, terminology) {
                (true, true) => Rank::Catalogue,
                (true, false) => continue,
                (false, true) => Rank::Guide,
                (false, false) => Rank::Example,
            };
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            // NOTE: an archive member that is no JSON document (a package index) holds no coding.
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                collect(&value, rank, &mut found);
            }
        }
    }
    Ok(found)
}

/// Every LOINC code a document form writes: its type and its sections.
fn written() -> BTreeSet<&'static str> {
    let slots: Vec<&Slot> = SLOTS
        .iter()
        .chain(&LAB_SLOTS)
        .chain(&HDR_SLOTS)
        .chain(&IMAGING_SLOTS)
        .collect();
    let mut codes: BTreeSet<&'static str> = slots.iter().map(|slot| slot.code).collect();
    codes.extend([
        PATIENT_SUMMARY,
        LABORATORY_REPORT,
        DISCHARGE_SUMMARY,
        IMAGING_REPORT,
    ]);
    codes.extend(
        IMAGING_CATEGORIES
            .iter()
            .filter(|code| code.system == LOINC)
            .map(|code| code.code),
    );
    codes
}

#[test]
fn every_written_loinc_code_carries_the_display_the_vendored_material_gives()
-> Result<(), Box<dyn Error>> {
    let found = vendored()?;
    for code in written() {
        let ranked = found.get(code).and_then(|ranks| ranks.iter().next());
        let expected = match ranked {
            Some((rank, displays)) => {
                // The catalogue and the guides' terminology give one display each; examples
                // that disagree leave the code unsettled, which fails here.
                assert_eq!(
                    1,
                    displays.len(),
                    "{code}: the {rank:?} displays disagree: {displays:?}"
                );
                displays.iter().next().map(String::as_str)
            }
            None => None,
        };
        assert_eq!(display(LOINC, code), expected, "{code}");
    }
    Ok(())
}

#[test]
fn a_code_of_no_vendored_source_carries_no_display() {
    assert_eq!(display(LOINC, "00000-0"), None);
    assert_eq!(display("http://snomed.info/sct", "60591-5"), None);
}

#[test]
fn every_empty_reason_carries_its_r4_display() -> Result<(), Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../tools/fhir-codegen/vendor/hl7.fhir.r4.core/package/CodeSystem-list-empty-reason.json",
    );
    let system: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let concepts = system["concept"].as_array().ok_or("the concepts")?;
    for concept in concepts {
        let code = concept["code"].as_str().ok_or("a code")?;
        assert_eq!(
            display(EMPTY_REASON, code),
            concept["display"].as_str(),
            "{code}"
        );
    }
    Ok(())
}

/// Returns the vendored member `file` of the package `package`.
fn member(package: &str, file: &str) -> Result<Value, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs")
        .join(package);
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(path)?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.ends_with(file) {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            return Ok(serde_json::from_str(&text)?);
        }
    }
    Err(format!("{package} holds no {file}").into())
}

#[test]
fn every_priority_category_carries_its_health_data_api_display() -> Result<(), Box<dyn Error>> {
    let system = member(
        HEALTH_DATA_API,
        "CodeSystem-eehrxf-document-priority-category-cs.json",
    )?;
    assert_eq!(system["url"].as_str(), Some(PRIORITY_SYSTEM));
    let concepts = system["concept"].as_array().ok_or("the concepts")?;
    assert!(!concepts.is_empty(), "the code system names its categories");
    for concept in concepts {
        let code = concept["code"].as_str().ok_or("a code")?;
        assert_eq!(
            display(PRIORITY_SYSTEM, code),
            concept["display"].as_str(),
            "{code}"
        );
    }
    Ok(())
}
