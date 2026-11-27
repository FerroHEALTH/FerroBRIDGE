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
//! annex allows one under single parents and the document holds more. A
//! narrative-only document (Art 3(1)(b)) is one case per data group, held
//! when the section slice of [`groups::GROUPS`] carries its narrative, and a
//! PDF document (Art 3(1)(a)) is one case for the embedded PDF and one per
//! Annex III Table 1 element on the `DocumentReference` element of
//! [`described::DESCRIBED`]. The suite runs over the directory `EEHRXF_DRAFT_CORPUS_DIR` names, after the
//! other suites wrote it, and writes one result per category for
//! `scripts/checks/conformance.sh`; HL7's FHIR Validator result for the same
//! documents is the `fhir-validator` job's. The `xtehr-crosswalk` corpus is
//! measured here as well, since a divergent annex element counts as resolved
//! only when the category crosswalks, [`paths::PATHS`] or
//! [`described::DESCRIBED`] record where it goes. No specification governs
//! how the documents are measured: our own design.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
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
use eehrxf::document::metadata::{COMPOSITION_VERSION, DOCUMENT_REFERENCE_VERSION, PDF};
use eehrxf::draft::{Annex, AnnexCrosswalk, Row, SHIPPED, Verdict};
use ferrobridge_testkit::conformance::{self, Case, Corpus, Count};
use serde_json::{Value, json};

use described::DESCRIBED;
use groups::GROUPS;
use paths::{PATHS, Path as Tabled};

mod described;
mod groups;
mod paths;

/// The variable naming the directory of the documents the suites emitted.
const CORPUS_DIR: &str = "EEHRXF_DRAFT_CORPUS_DIR";

/// One category of the corpus: its corpus name, its annex, the crosswalks
/// that carry its elements, the prefixes of its documents' names, the
/// Xt-EHR models a document of its own carries, with that document's
/// prefixes, and the prefixes of its narrative-only and PDF documents
/// (draft `EEHRxF` Art 3(1)(b) and (a)).
struct Category {
    corpus: &'static str,
    annex: Annex,
    crosswalks: &'static [&'static Crosswalk<'static>],
    prefixes: &'static [&'static str],
    models: &'static [(&'static str, &'static [&'static str])],
    narrative: &'static [&'static str],
    pdf: &'static [&'static str],
}

impl Category {
    /// Returns the prefixes of the documents the rows of `model` are
    /// measured against.
    fn prefixes_of(&self, model: &str) -> &'static [&'static str] {
        self.models
            .iter()
            .find(|(held, _)| *held == model)
            .map_or(self.prefixes, |(_, prefixes)| prefixes)
    }

    /// Returns every prefix of the category's documents.
    fn every_prefix(&self) -> impl Iterator<Item = &'static str> {
        self.prefixes
            .iter()
            .chain(self.models.iter().flat_map(|(_, prefixes)| prefixes.iter()))
            .chain(self.narrative.iter())
            .chain(self.pdf.iter())
            .copied()
    }
}

/// The categories, one per annex of a priority category.
const CATEGORIES: [Category; 6] = [
    Category {
        corpus: "eehrxf-draft-patient-summary",
        annex: Annex::Iv,
        crosswalks: &[&PATIENT_SUMMARY],
        prefixes: &["eehrxf-summary-", "eehrxf-eps-summary-"],
        models: &[],
        narrative: &[],
        pdf: &[],
    },
    Category {
        corpus: "eehrxf-draft-eprescription",
        annex: Annex::V,
        crosswalks: &[&PRESCRIPTION],
        prefixes: &["eehrxf-mpd-prescription"],
        models: &[],
        narrative: &[],
        pdf: &[],
    },
    Category {
        corpus: "eehrxf-draft-edispensation",
        annex: Annex::Vi,
        crosswalks: &[&DISPENSATION],
        prefixes: &["eehrxf-mpd-dispense"],
        models: &[],
        narrative: &[],
        pdf: &[],
    },
    Category {
        corpus: "eehrxf-draft-imaging",
        annex: Annex::Vii,
        crosswalks: &[&IMAGING_REPORT, &IMAGING_STUDY],
        prefixes: &["eehrxf-imaging-"],
        // NOTE: draft EEHRxF Annex II(d) and Annex VII Table 2: the study is the manifest
        // beside the report.
        models: &[("EHDSImagingStudy", &["eehrxf-manifest-"])],
        narrative: &[],
        pdf: &[],
    },
    Category {
        corpus: "eehrxf-draft-medical-test-report",
        annex: Annex::Viii,
        crosswalks: &[&LABORATORY],
        prefixes: &["eehrxf-laboratory-", "eehrxf-test-report-"],
        models: &[],
        narrative: &[],
        pdf: &["eehrxf-pdf-laboratory"],
    },
    Category {
        corpus: "eehrxf-draft-discharge",
        annex: Annex::Ix,
        crosswalks: &[&DISCHARGE],
        prefixes: &["eehrxf-discharge-"],
        models: &[],
        narrative: &["eehrxf-narrative-discharge"],
        pdf: &[],
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

/// The extension URL an annex `element` carried at the extension element
/// `eps` is written under, when the element has one: the R5
/// `Composition.version` cross-version extension for `header.version`, so
/// another extension of the composition is never counted as the version.
fn extension_of(element: &str, eps: &str) -> Option<&'static str> {
    (element == "header.version" && eps.ends_with(".extension")).then_some(COMPOSITION_VERSION)
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

/// The row of [`paths::PATHS`] that names the annex `row`.
fn tabled(row: &Row) -> Option<&'static Tabled> {
    PATHS.iter().find(|path| {
        path.annex == row.annex()
            && path.model == row.xtehr_model()
            && Some(path.element) == row.draft_element()
    })
}

/// Whether the annex `row` has a recorded decision: the crosswalk of its
/// category carries it to an HL7 Europe element, [`paths::PATHS`] names the
/// element that carries it or records that none does, or, for an Annex III
/// Table 1 element, [`DESCRIBED`] names its `DocumentReference` element or
/// why it has none.
fn resolved(row: &Row) -> bool {
    if row.annex() == Annex::Iii && row.draft_model() == "Document metadata" {
        return DESCRIBED
            .iter()
            .any(|(element, _)| Some(*element) == row.draft_element());
    }
    CATEGORIES
        .iter()
        .find(|category| category.annex == row.annex())
        .is_some_and(|category| {
            carried(row, category.crosswalks).is_some() || tabled(row).is_some()
        })
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

/// The cases of a narrative-only document `bundle` named `name`: one per data
/// group of the category, held when the section slice [`GROUPS`] names for
/// it is in the document with its narrative.
fn narrated(
    category: &Category,
    rows: &[&Row],
    name: &str,
    bundle: &Value,
    cases: &mut Vec<Value>,
    unmapped: &mut usize,
) {
    for group in GROUPS.iter().filter(|group| group.annex == category.annex) {
        let declared = rows.iter().any(|row| {
            row.xtehr_model() == group.model && row.draft_element() == Some(group.element)
        });
        if !declared {
            continue;
        }
        let Some(slice) = group.slice else {
            *unmapped += 1;
            continue;
        };
        let id = format!("{name}:{}", group.element);
        let narrated = values(bundle, &format!("Composition.section:{slice}"))
            .into_iter()
            .any(|section| section["text"]["div"].is_string());
        cases.push(if narrated {
            json!({"id": id, "passed": true})
        } else {
            json!({"id": id, "passed": false, "failure": format!("missing: no {slice} section with its narrative")})
        });
    }
}

/// The cases of a PDF document `document`, a `DocumentReference` named
/// `name`: the embedded PDF, and each Annex III Table 1 element on the
/// element [`DESCRIBED`] names for it.
fn described_pdf(
    crosswalk: &AnnexCrosswalk,
    name: &str,
    document: &Value,
    cases: &mut Vec<Value>,
    unmapped: &mut usize,
) {
    let attachment = &document["content"][0]["attachment"];
    let embedded = attachment["contentType"] == PDF
        && attachment["data"].as_str().is_some_and(|data| {
            STANDARD
                .decode(data)
                .is_ok_and(|bytes| bytes.starts_with(b"%PDF-"))
        });
    let id = format!("{name}:attachment");
    cases.push(if embedded {
        json!({"id": id, "passed": true})
    } else {
        json!({"id": id, "passed": false, "failure": "missing: no embedded application/pdf data"})
    });
    let wrapped = json!({"entry": [{"resource": document}]});
    let rows: Vec<&Row> = crosswalk
        .annex(Annex::Iii)
        .filter(|row| row.draft_model() == "Document metadata" && row.draft_element().is_some())
        .collect();
    for row in &rows {
        let (Some(element), Some(cardinality)) = (row.draft_element(), row.draft_cardinality())
        else {
            continue;
        };
        let Some(path) = DESCRIBED
            .iter()
            .find(|(held, _)| *held == element)
            .and_then(|(_, path)| path.ok())
        else {
            *unmapped += 1;
            continue;
        };
        let found = values(&wrapped, path)
            .into_iter()
            .filter(|value| element != "version" || value["url"] == DOCUMENT_REFERENCE_VERSION)
            .count();
        let required = cardinality.is_required()
            && parents(&rows, element, |parent| {
                parent
                    .draft_cardinality()
                    .is_some_and(|held| held.is_required())
            });
        let single = cardinality.upper() == Max::Bounded(1)
            && parents(&rows, element, |parent| {
                parent
                    .draft_cardinality()
                    .is_some_and(|held| held.upper() == Max::Bounded(1))
            });
        let id = format!("{name}:{element}");
        cases.push(if required && found == 0 {
            json!({"id": id, "passed": false, "failure": format!("missing: {path} holds nothing")})
        } else if single && found > 1 {
            json!({"id": id, "passed": false, "failure": format!("wrong cardinality: {path} holds {found}")})
        } else {
            json!({"id": id, "passed": true})
        });
    }
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
                    .every_prefix()
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
        if category
            .narrative
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            narrated(category, &rows, name, &bundle, &mut cases, &mut unmapped);
            continue;
        }
        if category.pdf.iter().any(|prefix| name.starts_with(prefix)) {
            described_pdf(&crosswalk, name, &bundle, &mut cases, &mut unmapped);
            continue;
        }
        for row in &rows {
            if !category
                .prefixes_of(row.xtehr_model())
                .iter()
                .any(|prefix| name.starts_with(prefix))
            {
                continue;
            }
            let (Some(element), Some(cardinality)) = (row.draft_element(), row.draft_cardinality())
            else {
                continue;
            };
            let Some((eps, grouping)) = carried(row, category.crosswalks).or_else(|| {
                tabled(row)
                    .and_then(|path| path.eps)
                    .map(|eps| (eps, eps == "Composition.section"))
            }) else {
                unmapped += 1;
                continue;
            };
            let found = values(&bundle, eps)
                .into_iter()
                .filter(|value| extension_of(element, eps).is_none_or(|url| value["url"] == url))
                .count();
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

// NOTE: no specification governs this: our own design; an annex element counts as resolved
// when it matches the Xt-EHR models 1.0.0, or its divergence has a recorded decision; every
// divergence is counted as `differ`.
#[test]
fn conformance_the_xtehr_crosswalk_holds_its_pass_list() -> Result<(), Box<dyn Error>> {
    let crosswalk: AnnexCrosswalk = SHIPPED.parse()?;
    let mut differ = 0_usize;
    let mut cases = Vec::new();
    for row in crosswalk.rows() {
        let Some(element) = row.draft_element() else {
            continue;
        };
        let id = format!(
            "{}:{}:{element}",
            row.annex().numeral(),
            row.draft_model().replace(' ', "_")
        );
        if row.verdict() == Verdict::Same {
            cases.push(Case::pass(id));
            continue;
        }
        differ = differ.saturating_add(1);
        if resolved(row) {
            cases.push(Case::pass(id));
        } else {
            cases.push(Case::fail(
                id,
                format!(
                    "{}, and no crosswalk or path records where the element goes",
                    row.verdict()
                ),
            ));
        }
    }
    let counts = BTreeMap::from([(Count::Differ, differ)]);
    let outcome = conformance::record_counted(Corpus::XtehrCrosswalk, &cases, &counts)?;
    assert!(
        outcome.regressed.is_empty(),
        "cases the pass list records that fail: {:?}",
        outcome.regressed
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

/// The JSON snapshot of the profile `url` among the fetched profile packages.
fn snapshot(url: &str) -> Result<Value, Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/specs/fhir-packages");
    for package in std::fs::read_dir(&root)? {
        let package = package?.path();
        let guide = package
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("hl7.fhir.eu."));
        if !guide {
            continue;
        }
        let directory = package.join("package");
        let Ok(files) = std::fs::read_dir(&directory) else {
            continue;
        };
        for file in files {
            let file = file?.path();
            let is_definition = file
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("StructureDefinition-"));
            if !is_definition {
                continue;
            }
            let text = std::fs::read_to_string(&file)?;
            if !text.contains(url) {
                continue;
            }
            let definition: Value = serde_json::from_str(&text)?;
            if definition["url"] == url {
                return Ok(definition);
            }
        }
    }
    Err(format!("no fetched package defines {url}").into())
}

/// The profile of the category of `annex` whose root is `resource`.
fn profile_of(annex: Annex, resource: &str) -> Option<&'static str> {
    Some(match (annex, resource) {
        (Annex::Iv, "Composition") => {
            "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps"
        }
        (Annex::V, "MedicationRequest") => {
            "http://hl7.eu/fhir/mpd/StructureDefinition/MedicationRequest-eu-mpd"
        }
        (Annex::Vi, "MedicationDispense") => {
            "http://hl7.eu/fhir/mpd/StructureDefinition/MedicationDispense-eu-mpd"
        }
        (Annex::Vii, "Composition") => {
            "http://hl7.eu/fhir/imaging/StructureDefinition/CompositionEuImaging"
        }
        (Annex::Vii, "Observation") => {
            "http://hl7.eu/fhir/imaging/StructureDefinition/ObservationRadiationDoseEuImaging"
        }
        (Annex::Vii, "ImagingStudy") => {
            "http://hl7.eu/fhir/imaging/StructureDefinition/ImagingStudyEuImaging"
        }
        (Annex::Viii, "Composition") => {
            "http://hl7.eu/fhir/laboratory/StructureDefinition/Composition-eu-lab"
        }
        (Annex::Ix, "Composition") => {
            "http://hl7.eu/fhir/hdr/StructureDefinition/composition-eu-hdr"
        }
        _ => return None,
    })
}

#[test]
fn every_path_is_an_element_of_its_profile() -> Result<(), Box<dyn Error>> {
    if std::env::var("FERROBRIDGE_SKIP_PROFILE_PACKAGES").is_ok_and(|value| value == "1") {
        return Ok(());
    }
    let mut definitions: BTreeMap<&str, Value> = BTreeMap::new();
    for path in PATHS {
        let Some(eps) = path.eps else { continue };
        let resource = eps.split('.').next().ok_or("an element id")?;
        let url = profile_of(path.annex, resource)
            .ok_or_else(|| format!("{eps}: no profile for its resource in its annex"))?;
        if !definitions.contains_key(url) {
            definitions.insert(url, snapshot(url)?);
        }
        let definition = definitions.get(url).ok_or("a read definition")?;
        let held = definition["snapshot"]["element"]
            .as_array()
            .ok_or("a snapshot")?
            .iter()
            .any(|element| element["id"] == eps);
        assert!(held, "{}: {eps} is no element of {url}", path.element);
    }
    Ok(())
}

#[test]
fn every_group_slice_is_a_section_slice_of_its_profile() -> Result<(), Box<dyn Error>> {
    if std::env::var("FERROBRIDGE_SKIP_PROFILE_PACKAGES").is_ok_and(|value| value == "1") {
        return Ok(());
    }
    let mut definitions: BTreeMap<&str, Value> = BTreeMap::new();
    for group in GROUPS {
        let Some(slice) = group.slice else { continue };
        let url = profile_of(group.annex, "Composition")
            .ok_or_else(|| format!("{}: no Composition profile in its annex", group.element))?;
        if !definitions.contains_key(url) {
            definitions.insert(url, snapshot(url)?);
        }
        let definition = definitions.get(url).ok_or("a read definition")?;
        let id = format!("Composition.section:{slice}");
        let held = definition["snapshot"]["element"]
            .as_array()
            .ok_or("a snapshot")?
            .iter()
            .any(|element| element["id"] == id.as_str());
        assert!(held, "{}: {id} is no element of {url}", group.element);
        assert!(
            section_code(slice).is_some(),
            "{}: the document forms code no {slice} section",
            group.element
        );
    }
    Ok(())
}

#[test]
fn every_data_group_has_a_slice_row() -> Result<(), Box<dyn Error>> {
    let crosswalk: AnnexCrosswalk = SHIPPED.parse()?;
    let mut missing = Vec::new();
    for (annex, model) in [
        (Annex::Iv, "EHDSPatientSummary"),
        (Annex::Vii, "EHDSImagingReport"),
        (Annex::Viii, "EHDSLaboratoryReport"),
        (Annex::Ix, "EHDSDischargeReport"),
    ] {
        let elements: Vec<&str> = crosswalk
            .annex(annex)
            .filter(|row| row.xtehr_model() == model)
            .filter_map(Row::draft_element)
            .collect();
        for element in &elements {
            let top = element.strip_prefix("body.").unwrap_or(element);
            let grouping = !top.contains('.')
                && *element != "body"
                && !element.starts_with("header")
                && *element != "presentedForm"
                && elements
                    .iter()
                    .any(|other| other.starts_with(&format!("{element}.")));
            let tabled = GROUPS.iter().any(|group| {
                group.annex == annex && group.model == model && group.element == *element
            });
            if grouping && !tabled {
                missing.push(format!("{} {model} {element}", annex.numeral()));
            }
        }
    }
    assert!(missing.is_empty(), "data groups with no row: {missing:?}");
    Ok(())
}

#[test]
fn every_described_path_is_a_document_reference_element() {
    for (element, path) in DESCRIBED {
        let Ok(path) = path else { continue };
        assert!(
            fhir_types::r4::schema::SCHEMAS.element(path).is_some(),
            "{element}: {path} is no element of the R4 DocumentReference"
        );
    }
}

#[test]
fn every_row_the_crosswalks_miss_has_a_path() -> Result<(), Box<dyn Error>> {
    let crosswalk: AnnexCrosswalk = SHIPPED.parse()?;
    let mut missing = Vec::new();
    for category in &CATEGORIES {
        for row in crosswalk.annex(category.annex) {
            let Some(element) = row.draft_element() else {
                continue;
            };
            if carried(row, category.crosswalks).is_none() && tabled(row).is_none() {
                missing.push(format!(
                    "{} {} {element}",
                    row.annex().numeral(),
                    row.xtehr_model()
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "annex elements with no path: {missing:?}"
    );
    Ok(())
}
