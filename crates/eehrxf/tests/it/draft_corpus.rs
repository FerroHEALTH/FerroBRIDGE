// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `EEHRxF` draft corpus: the documents the eehrxf suites emit, each held
//! to the data set of its category's annex of the draft `EEHRxF`
//! implementing act (initiative 17512, 2026-11-23).
//!
//! Every annex element of the category is followed through the draft
//! crosswalk to its Xt-EHR element and through the category crosswalk to the
//! HL7 Europe element that carries it. An element whose path ends in no
//! element is unmapped and counted apart. A mapped element is one case per
//! document: it fails as missing when the annex requires it under required
//! parents and the document holds none, and as wrong in cardinality when the
//! annex allows one under single parents and the document holds more. The
//! suite runs over the directory `EEHRXF_DRAFT_CORPUS_DIR` names, after the
//! other suites wrote it, and writes one result per category for
//! `scripts/checks/conformance.sh`; HL7's FHIR Validator result for the same
//! documents is the `fhir-validator` job's. No specification governs how the
//! documents are measured: our own design.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use eehrxf::crosswalk::Crosswalk;
use eehrxf::crosswalk::discharge::DISCHARGE;
use eehrxf::crosswalk::dispensation::DISPENSATION;
use eehrxf::crosswalk::imaging::{IMAGING_REPORT, IMAGING_STUDY};
use eehrxf::crosswalk::laboratory::LABORATORY;
use eehrxf::crosswalk::patient_summary::PATIENT_SUMMARY;
use eehrxf::crosswalk::prescription::PRESCRIPTION;
use eehrxf::dataset::Max;
use eehrxf::document::discharge::HDR_SLOTS;
use eehrxf::document::form::{SLOTS, Slot};
use eehrxf::document::imaging::IMAGING_SLOTS;
use eehrxf::document::laboratory::LAB_SLOTS;
use eehrxf::draft::{Annex, AnnexCrosswalk, Row, SHIPPED};
use serde_json::{Value, json};

/// The variable naming the directory of the documents the suites emitted.
const CORPUS_DIR: &str = "EEHRXF_DRAFT_CORPUS_DIR";

/// One category of the corpus: its corpus name, its annex, the crosswalks
/// that carry its elements and the prefixes of its documents' names.
struct Category {
    corpus: &'static str,
    annex: Annex,
    crosswalks: &'static [&'static Crosswalk<'static>],
    prefixes: &'static [&'static str],
}

/// The categories, one per annex of a priority category.
const CATEGORIES: [Category; 6] = [
    Category {
        corpus: "eehrxf-draft-patient-summary",
        annex: Annex::Iv,
        crosswalks: &[&PATIENT_SUMMARY],
        prefixes: &["eehrxf-summary-", "eehrxf-eps-summary-"],
    },
    Category {
        corpus: "eehrxf-draft-eprescription",
        annex: Annex::V,
        crosswalks: &[&PRESCRIPTION],
        prefixes: &["eehrxf-mpd-prescription"],
    },
    Category {
        corpus: "eehrxf-draft-edispensation",
        annex: Annex::Vi,
        crosswalks: &[&DISPENSATION],
        prefixes: &["eehrxf-mpd-dispense"],
    },
    Category {
        corpus: "eehrxf-draft-imaging",
        annex: Annex::Vii,
        crosswalks: &[&IMAGING_REPORT, &IMAGING_STUDY],
        prefixes: &["eehrxf-imaging-"],
    },
    Category {
        corpus: "eehrxf-draft-medical-test-report",
        annex: Annex::Viii,
        crosswalks: &[&LABORATORY],
        prefixes: &["eehrxf-laboratory-", "eehrxf-test-report-"],
    },
    Category {
        corpus: "eehrxf-draft-discharge",
        annex: Annex::Ix,
        crosswalks: &[&DISCHARGE],
        prefixes: &["eehrxf-discharge-"],
    },
];

/// The LOINC code of the section slice `slice` of any document form.
fn section_code(slice: &str) -> Option<&'static str> {
    SLOTS
        .iter()
        .chain(IMAGING_SLOTS.iter())
        .chain(LAB_SLOTS.iter())
        .chain(HDR_SLOTS.iter())
        .find(|slot: &&Slot| slot.slice == slice)
        .map(|slot| slot.code)
}

/// The values `element`, an element id of a profile, holds in `bundle`: the
/// `Composition` of a document for a `Composition` element, every resource
/// of the type otherwise.
fn values<'a>(bundle: &'a Value, element: &str) -> Vec<&'a Value> {
    let mut segments = element.split('.');
    let Some(kind) = segments.next() else {
        return Vec::new();
    };
    let resources = bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == kind);
    let mut held: Vec<&Value> = if kind == "Composition" {
        resources.take(1).collect()
    } else {
        resources.collect()
    };
    for segment in segments {
        let (name, slice) = segment
            .split_once(':')
            .map_or((segment, None), |(name, slice)| (name, Some(slice)));
        let mut next = Vec::new();
        for value in held {
            let found: Vec<&Value> = match name.strip_suffix("[x]") {
                Some(stem) => value
                    .as_object()
                    .into_iter()
                    .flatten()
                    .filter(|(key, _)| {
                        key.strip_prefix(stem)
                            .is_some_and(|rest| rest.starts_with(char::is_uppercase))
                    })
                    .map(|(_, held)| held)
                    .collect(),
                None => value.get(name).into_iter().collect(),
            };
            for one in found {
                match one {
                    Value::Array(items) => next.extend(items),
                    other => next.push(other),
                }
            }
        }
        if let (Some(slice), "section") = (slice, name) {
            let code = section_code(slice);
            next.retain(|section| {
                section["code"]["coding"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|coding| code.is_some_and(|code| coding["code"] == code))
            });
        }
        held = next;
    }
    held
}

/// The HL7 Europe element that carries the annex `row`, through the Xt-EHR
/// element the draft crosswalk sets beside it, and whether that element is
/// the one whose slices are the sections, which a single annex element
/// groups.
fn carried(row: &Row, crosswalks: &[&Crosswalk<'_>]) -> Option<(&'static str, bool)> {
    row.xtehr_element()?;
    let path = row.xtehr_path();
    crosswalks
        .iter()
        .find_map(|crosswalk| {
            crosswalk
                .row(&path)
                .map(|carried| (carried.eps, carried.eps == crosswalk.sliced))
        })
        .filter(|(eps, _)| !eps.is_empty())
}

/// Whether every parent of the annex element `element` among `rows` passes
/// `test`.
fn parents(rows: &[&Row], element: &str, test: impl Fn(&Row) -> bool) -> bool {
    let mut path = element;
    while let Some((parent, _)) = path.rsplit_once('.') {
        if let Some(row) = rows.iter().find(|row| row.draft_element() == Some(parent))
            && !test(row)
        {
            return false;
        }
        path = parent;
    }
    true
}

/// The result of one category over the documents under `dir`.
fn measure(category: &Category, dir: &Path) -> Result<Value, Box<dyn Error>> {
    let crosswalk: AnnexCrosswalk = SHIPPED.parse()?;
    let rows: Vec<&Row> = crosswalk
        .annex(category.annex)
        .filter(|row| row.draft_element().is_some())
        .collect();
    let mut documents: Vec<PathBuf> = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    documents.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                category
                    .prefixes
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "json")
            })
    });
    documents.sort();
    let mut cases = Vec::new();
    let mut unmapped = 0_usize;
    for document in &documents {
        let name = document
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or("a document name")?;
        let bundle: Value = serde_json::from_str(&std::fs::read_to_string(document)?)?;
        for row in &rows {
            let (Some(element), Some(cardinality)) = (row.draft_element(), row.draft_cardinality())
            else {
                continue;
            };
            let Some((eps, grouping)) = carried(row, category.crosswalks) else {
                unmapped += 1;
                continue;
            };
            let found = values(&bundle, eps).len();
            let required = cardinality.is_required()
                && parents(&rows, element, |parent| {
                    parent
                        .draft_cardinality()
                        .is_some_and(|held| held.is_required())
                });
            let single = !grouping
                && cardinality.upper() == Max::Bounded(1)
                && parents(&rows, element, |parent| {
                    parent
                        .draft_cardinality()
                        .is_some_and(|held| held.upper() == Max::Bounded(1))
                });
            let id = format!("{name}:{element}");
            cases.push(if required && found == 0 {
                json!({"id": id, "passed": false, "failure": format!("missing: {eps} holds nothing")})
            } else if single && found > 1 {
                json!({"id": id, "passed": false, "failure": format!("wrong cardinality: {eps} holds {found}")})
            } else {
                json!({"id": id, "passed": true})
            });
        }
    }
    Ok(json!({
        "corpus": category.corpus,
        "total": cases.len(),
        "unmapped": unmapped,
        "cases": cases,
    }))
}

#[test]
fn conformance_the_eehrxf_draft_corpus_measures_the_emitted_documents() -> Result<(), Box<dyn Error>>
{
    let Some(dir) = std::env::var_os(CORPUS_DIR) else {
        return Ok(());
    };
    let out = std::env::var_os("FERROBRIDGE_CONFORMANCE_OUT").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/conformance"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&out)?;
    let mut counted = BTreeMap::new();
    for category in &CATEGORIES {
        let result = measure(category, Path::new(&dir))?;
        counted.insert(category.corpus, result["total"].clone());
        std::fs::write(
            out.join(format!("{}.json", category.corpus)),
            serde_json::to_string(&result)?,
        )?;
    }
    assert!(
        counted
            .values()
            .any(|total| total.as_u64().is_some_and(|n| n > 0)),
        "the directory holds no document of any category: {counted:?}"
    );
    Ok(())
}

#[test]
fn an_element_id_finds_its_section_slice() {
    let bundle = json!({"entry": [{"resource": {
        "resourceType": "Composition",
        "section": [
            {"code": {"coding": [{"system": "http://loinc.org", "code": "48765-2"}]}, "entry": [{"reference": "a"}]},
            {"code": {"coding": [{"system": "http://loinc.org", "code": "11450-4"}]}}
        ]
    }}]});
    assert_eq!(
        values(&bundle, "Composition.section:sectionAllergies").len(),
        1
    );
    assert_eq!(
        values(&bundle, "Composition.section:sectionAllergies.entry").len(),
        1
    );
    assert_eq!(
        values(&bundle, "Composition.section:sectionMedications").len(),
        0
    );
    assert_eq!(values(&bundle, "Composition.section").len(), 2);
}
