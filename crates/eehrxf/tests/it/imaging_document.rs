// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging report document: the section slots and categories held to the
//! vendored HL7 Europe Imaging Report 1.0.0-ballot profiles, and a document
//! assembled from synthetic resources, written as the `BundleReportEuImaging`
//! the profiles constrain, its composition's `Organization` authors the
//! report's performers (`dr-comp-author-org`) and the operator its custodian.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::io::Read;

use eehrxf::document::form::SectionRule;
use eehrxf::document::imaging::{
    IMAGING_BUNDLE, IMAGING_CATEGORIES, IMAGING_REPORT, IMAGING_REPORT_FORM, IMAGING_SLOTS,
};
use eehrxf::document::section::{EmptyReason, Section};
use eehrxf::document::{Document, DocumentError, LOINC};
use eehrxf::receive::ReceivedDocument;
use ferrobridge_term::client::Client;
use fhir_types::r4::bundle::{Bundle, BundleEntry};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::composition::{Composition, CompositionSection};
use fhir_types::r4::device::Device;
use fhir_types::r4::diagnostic_report::DiagnosticReport;
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::imaging_study::ImagingStudy;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use flate2::read::GzDecoder;
use serde_json::Value;

use super::support::{imaging_package, profile_set};

/// The base the test documents name their entries under, in the
/// `example.org` domain RFC 6761 §6.5 reserves.
const BASE: &str = "https://gateway.example.org/fhir";

/// The synthetic document identifier.
const IDENTIFIER: &str = "urn:uuid:5b0c3f7e-0000-4000-8000-00000000c001";

/// The LOINC code of the synthetic report, an MR of the brain.
const MR_BRAIN: &str = "24802-1";

/// Reads the JSON member `path` of the vendored Imaging Report package.
fn package_member(path: &str) -> Result<Value, Box<dyn Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(imaging_package())?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.to_string_lossy() == path {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            return Ok(serde_json::from_str(&text)?);
        }
    }
    Err(format!("the package holds no {path}").into())
}

/// The snapshot element `id` of `profile`.
fn element<'a>(profile: &'a Value, id: &str) -> Result<&'a Value, Box<dyn Error>> {
    profile["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?
        .iter()
        .find(|element| element["id"] == id)
        .ok_or_else(|| format!("the element {id}").into())
}

#[test]
fn every_slot_and_category_is_the_profile_s() -> Result<(), Box<dyn Error>> {
    let composition = package_member("package/StructureDefinition-CompositionEuImaging.json")?;
    assert_eq!(composition["url"], IMAGING_REPORT_FORM.composition);
    let slices: Vec<&str> = composition["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?
        .iter()
        .filter_map(|element| {
            let slice = element["id"]
                .as_str()?
                .strip_prefix("Composition.section:")?;
            (!slice.contains('.')).then_some(slice)
        })
        .collect();
    let slots: Vec<&str> = IMAGING_SLOTS.iter().map(|slot| slot.slice).collect();
    assert_eq!(slices, slots, "one slot per section slice, in order");
    for slot in &IMAGING_SLOTS {
        let code = element(
            &composition,
            &format!("Composition.section:{}.code", slot.slice),
        )?;
        let coding = &code["patternCodeableConcept"]["coding"][0];
        assert_eq!(coding["code"], slot.code, "{}", slot.slice);
    }
    for (slice, category) in ["imaging-report", "imaging"]
        .iter()
        .zip(&IMAGING_CATEGORIES)
    {
        for (profile, path) in [
            (&composition, "Composition"),
            (
                &package_member("package/StructureDefinition-DiagnosticReportEuImaging.json")?,
                "DiagnosticReport",
            ),
        ] {
            let pattern = &element(profile, &format!("{path}.category:{slice}"))?["patternCodeableConcept"]
                ["coding"][0];
            assert_eq!(pattern["system"], category.system, "{path} {slice}");
            assert_eq!(pattern["code"], category.code, "{path} {slice}");
        }
    }
    let bundle = package_member("package/StructureDefinition-BundleReportEuImaging.json")?;
    assert_eq!(bundle["url"], IMAGING_BUNDLE);
    let set = package_member("package/ValueSet-ImagingReportTypesEuVSEuImaging.json")?;
    let filter = &set["compose"]["include"][0]["filter"][0];
    assert_eq!(
        (&filter["op"], &filter["value"]),
        (&Value::from("is-a"), &Value::from(IMAGING_REPORT)),
        "the fallback type is the concept the report types descend from"
    );
    Ok(())
}

/// A literal reference to `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}

/// A synthetic imaging report document, its report coded `code` and
/// performed by the radiology department.
fn document(code: Option<&str>) -> Result<Document, Box<dyn Error>> {
    let mut document = Document::of(
        &IMAGING_REPORT_FORM,
        BASE,
        IDENTIFIER.to_owned(),
        String::from("2026-10-25T12:00:00Z"),
        String::from("Imaging report"),
    );
    let subject = document.with_patient(Patient {
        id: Some(String::from("patient-1")),
        name: vec![HumanName {
            family: Some("Synthetic".into()),
            ..HumanName::default()
        }],
        birth_date: Some("1970-01-01".into()),
        ..Patient::default()
    })?;
    document.with_author(&Resource::Device(Box::new(Device {
        id: Some(String::from("gateway")),
        owner: Some(reference("Organization/operator")),
        ..Device::default()
    })))?;
    document.with_author(&Resource::Organization(Box::new(Organization {
        id: Some(String::from("operator")),
        name: Some("Synthetic Operator".into()),
        ..Organization::default()
    })))?;
    let added = document.add(&[
        Resource::Organization(Box::new(Organization {
            id: Some(String::from("radiology")),
            name: Some("Synthetic Radiology".into()),
            ..Organization::default()
        })),
        Resource::ImagingStudy(Box::new(ImagingStudy {
            id: Some(String::from("study-1")),
            meta: Some(Meta {
                profile: vec![
                    "http://hl7.eu/fhir/imaging/StructureDefinition/ImagingStudyEuImaging".into(),
                ],
                ..Meta::default()
            }),
            // NOTE: ImagingStudyEuImaging `identifier:studyInstanceUid` is 1..1; the UID is a
            // synthetic one under the ITU-T X.667 `2.25` arc.
            identifier: vec![Identifier {
                r#type: Some(CodeableConcept {
                    coding: vec![Coding {
                        system: Some("http://dicom.nema.org/resources/ontology/DCM".into()),
                        code: Some("110180".into()),
                        ..Coding::default()
                    }],
                    ..CodeableConcept::default()
                }),
                system: Some("urn:dicom:uid".into()),
                value: Some("urn:oid:2.25.1819".into()),
                ..Identifier::default()
            }],
            status: "available".into(),
            subject: reference(&subject),
            ..ImagingStudy::default()
        })),
    ])?;
    let study = added.get(1).ok_or("the study's fullUrl")?.clone();
    for slot in IMAGING_SLOTS.iter().take(4) {
        let mut section = Section::new(slot, vec![format!("{}.", slot.title)]);
        section = if slot.slice == "imagingstudy" {
            section.with_entry(study.clone())
        } else {
            section.empty_because(EmptyReason::NilKnown)
        };
        document.with_section(section)?;
    }
    document.with_report(DiagnosticReport {
        id: Some(String::from("imaging-report")),
        status: "final".into(),
        code: CodeableConcept {
            coding: code
                .map(|code| Coding {
                    system: Some(LOINC.into()),
                    code: Some(code.into()),
                    ..Coding::default()
                })
                .into_iter()
                .collect(),
            ..CodeableConcept::default()
        },
        performer: vec![reference("Organization/radiology")],
        ..DiagnosticReport::default()
    })?;
    Ok(document)
}

/// The one resource of `bundle` of type `kind`.
fn the<'a>(bundle: &'a Value, kind: &str) -> Result<&'a Value, Box<dyn Error>> {
    let found: Vec<&Value> = bundle["entry"]
        .as_array()
        .ok_or("entries")?
        .iter()
        .map(|entry| &entry["resource"])
        .filter(|resource| resource["resourceType"] == kind)
        .collect();
    match found.as_slice() {
        [one] => Ok(one),
        _ => Err(format!("one {kind}, found {}", found.len()).into()),
    }
}

#[test]
fn the_document_is_a_bundle_the_imaging_report_profiles_constrain() -> Result<(), Box<dyn Error>> {
    let bundle = serde_json::to_value(document(Some(MR_BRAIN))?.into_bundle()?)?;
    assert_eq!(bundle["meta"]["profile"][0], IMAGING_BUNDLE);
    let entries = bundle["entry"].as_array().ok_or("entries")?;
    assert!(entries.len() >= 4, "Bundle.entry min 4");
    assert_eq!(entries[0]["resource"]["resourceType"], "Composition");
    assert_eq!(entries[1]["resource"]["resourceType"], "DiagnosticReport");
    let composition = the(&bundle, "Composition")?;
    let report = the(&bundle, "DiagnosticReport")?;
    the(&bundle, "ImagingStudy")?;

    assert_eq!(
        composition["type"]["coding"][0]["code"], MR_BRAIN,
        "the composition's type is the report's code"
    );
    for category in &IMAGING_CATEGORIES {
        for (name, resource) in [("Composition", composition), ("DiagnosticReport", report)] {
            assert!(
                resource["category"]
                    .as_array()
                    .ok_or("categories")?
                    .iter()
                    .any(|held| held["coding"][0]["code"] == category.code),
                "{name} carries {}",
                category.code
            );
        }
    }
    assert!(
        report["identifier"][0].is_object(),
        "DiagnosticReportEuImaging.identifier is 1..*"
    );

    let organizations = |references: &Value| -> Vec<Value> {
        references
            .as_array()
            .into_iter()
            .flatten()
            .filter(|held| {
                held["reference"]
                    .as_str()
                    .is_some_and(|text| text.contains("Organization/"))
            })
            .cloned()
            .collect()
    };
    assert_eq!(
        organizations(&composition["author"]),
        organizations(&report["performer"]),
        "dr-comp-author-org: the composition's Organization authors are the report's performers"
    );
    assert_eq!(
        composition["custodian"]["reference"],
        format!("{BASE}/Organization/operator"),
        "A109: the operator is the custodian"
    );
    assert!(
        composition["author"]
            .as_array()
            .ok_or("authors")?
            .iter()
            .any(|author| author["reference"] == format!("{BASE}/Device/gateway")),
        "the gateway Device stays an author"
    );
    let codes: Vec<&Value> = composition["section"]
        .as_array()
        .ok_or("sections")?
        .iter()
        .map(|section| &section["code"]["coding"][0]["code"])
        .collect();
    for required in ["18726-0", "55115-0", "11329-0", "55111-9"] {
        assert!(codes.iter().any(|code| *code == required), "{required}");
    }
    Ok(())
}

#[test]
fn a_report_with_no_code_is_typed_as_an_imaging_report() -> Result<(), Box<dyn Error>> {
    let bundle = serde_json::to_value(document(None)?.into_bundle()?)?;
    let composition = the(&bundle, "Composition")?;
    let report = the(&bundle, "DiagnosticReport")?;
    assert_eq!(composition["type"]["coding"][0]["code"], IMAGING_REPORT);
    assert_eq!(report["code"]["coding"][0]["code"], IMAGING_REPORT);
    Ok(())
}

#[test]
fn the_form_holds_every_section_to_eu_imaging_composition_1() -> Result<(), Box<dyn Error>> {
    let composition = package_member("package/StructureDefinition-CompositionEuImaging.json")?;
    let section = element(&composition, "Composition.section")?;
    let invariant = section["constraint"]
        .as_array()
        .ok_or("the section's constraints")?
        .iter()
        .find(|constraint| constraint["key"] == "eu-imaging-composition-1")
        .ok_or("eu-imaging-composition-1")?;
    assert_eq!(invariant["severity"], "error");
    let ruled: Vec<&str> = IMAGING_REPORT_FORM
        .rules
        .iter()
        .filter_map(|rule| match rule {
            SectionRule::Reasoned {
                invariant: "eu-imaging-composition-1",
                slices,
            } => Some(slices.iter().copied()),
            _ => None,
        })
        .flatten()
        .collect();
    let slots: Vec<&str> = IMAGING_SLOTS.iter().map(|slot| slot.slice).collect();
    assert_eq!(ruled, slots, "the invariant holds every section");
    Ok(())
}

#[test]
fn an_empty_imaging_section_without_a_reason_is_refused() -> Result<(), Box<dyn Error>> {
    for slice in ["findings", "communication"] {
        let mut document = document(Some(MR_BRAIN))?;
        let slot = IMAGING_SLOTS
            .iter()
            .find(|slot| slot.slice == slice)
            .ok_or("the slot")?;
        document.with_section(Section::new(slot, vec![format!("{}.", slot.title)]))?;
        let refused = document.into_bundle();
        assert!(
            matches!(
                refused,
                Err(DocumentError::Unreasoned {
                    slice: refused_slice,
                    invariant: "eu-imaging-composition-1",
                }) if refused_slice == slice
            ),
            "{slice}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn an_empty_required_imaging_section_without_a_reason_is_refused() -> Result<(), Box<dyn Error>> {
    let mut document = Document::of(
        &IMAGING_REPORT_FORM,
        BASE,
        IDENTIFIER.to_owned(),
        String::from("2026-10-25T12:00:00Z"),
        String::from("Imaging report"),
    );
    document.with_patient(Patient {
        id: Some(String::from("patient-1")),
        ..Patient::default()
    })?;
    let order = IMAGING_SLOTS
        .iter()
        .find(|slot| slot.slice == "order")
        .ok_or("the order slot")?;
    document.with_section(Section::new(order, vec![String::from("Order.")]))?;
    document.with_report(DiagnosticReport {
        id: Some(String::from("imaging-report")),
        status: "final".into(),
        ..DiagnosticReport::default()
    })?;
    let refused = document.into_bundle();
    assert!(
        matches!(
            refused,
            Err(DocumentError::Unreasoned {
                slice: "order",
                invariant: "eu-imaging-composition-1",
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn the_form_requires_every_section_slice_the_profile_requires() -> Result<(), Box<dyn Error>> {
    let composition = package_member("package/StructureDefinition-CompositionEuImaging.json")?;
    let required: Vec<&str> = composition["snapshot"]["element"]
        .as_array()
        .ok_or("a snapshot")?
        .iter()
        .filter(|element| element["min"].as_u64() >= Some(1))
        .filter_map(|element| {
            let slice = element["id"]
                .as_str()?
                .strip_prefix("Composition.section:")?;
            (!slice.contains('.')).then_some(slice)
        })
        .collect();
    let ruled: Vec<&str> = IMAGING_REPORT_FORM
        .rules
        .iter()
        .filter_map(|rule| match rule {
            SectionRule::Required { slices } => Some(slices.iter().copied()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(ruled, required);
    assert_eq!(required.len(), 4, "{required:?}");
    Ok(())
}

#[test]
fn an_imaging_report_lacking_a_required_section_is_refused() -> Result<(), Box<dyn Error>> {
    let mut document = Document::of(
        &IMAGING_REPORT_FORM,
        BASE,
        IDENTIFIER.to_owned(),
        String::from("2026-10-25T12:00:00Z"),
        String::from("Imaging report"),
    );
    document.with_patient(Patient {
        id: Some(String::from("patient-1")),
        ..Patient::default()
    })?;
    for slot in IMAGING_SLOTS.iter().take(3) {
        document.with_section(
            Section::new(slot, vec![format!("{}.", slot.title)])
                .empty_because(EmptyReason::Unavailable),
        )?;
    }
    document.with_report(DiagnosticReport {
        id: Some(String::from("imaging-report")),
        status: "final".into(),
        ..DiagnosticReport::default()
    })?;
    let refused = document.into_bundle();
    assert!(
        matches!(refused, Err(DocumentError::Lacking { slice: "procedure" })),
        "{refused:?}"
    );
    Ok(())
}

/// The LOINC coding `code`.
fn loinc(code: &str) -> Coding {
    Coding {
        system: Some(LOINC.into()),
        code: Some(code.into()),
        ..Coding::default()
    }
}

#[test]
fn the_empty_reason_a_mapping_run_writes_reaches_the_document() -> Result<(), Box<dyn Error>> {
    let mut document = document(Some(MR_BRAIN))?;
    // NOTE: SectionEmptyReasonEuImaging includes LOINC LA4720-4 beside list-empty-reason.
    let reason = CodeableConcept {
        coding: vec![loinc("LA4720-4")],
        ..CodeableConcept::default()
    };
    let findings = IMAGING_SLOTS
        .iter()
        .find(|slot| slot.slice == "findings")
        .ok_or("the findings slot")?;
    let run = Bundle {
        r#type: "collection".into(),
        entry: vec![BundleEntry {
            resource: Some(Resource::Composition(Box::new(Composition {
                section: vec![CompositionSection {
                    code: Some(CodeableConcept {
                        coding: vec![loinc(findings.code)],
                        ..CodeableConcept::default()
                    }),
                    empty_reason: Some(reason.clone()),
                    ..CompositionSection::default()
                }],
                ..Composition::default()
            }))),
            ..BundleEntry::default()
        }],
        ..Bundle::default()
    };
    for section in document.with_mapped_composition(&run)? {
        assert_eq!(section.empty_reason.as_ref(), Some(&reason));
        document.with_section(section.into_section(vec![String::from("No findings.")]))?;
    }
    let bundle = serde_json::to_value(document.into_bundle()?)?;
    let written = bundle["entry"][0]["resource"]["section"]
        .as_array()
        .ok_or("sections")?
        .iter()
        .find(|section| section["code"]["coding"][0]["code"] == findings.code)
        .ok_or("the findings section")?;
    assert_eq!(written["emptyReason"]["coding"][0]["code"], "LA4720-4");
    assert_eq!(written["emptyReason"]["coding"][0]["system"], LOINC);
    Ok(())
}

/// The imaging report of [`document`] naming the member it was read at as
/// its custodian, returning the custodian's `fullUrl` with it.
fn custodied() -> Result<(Document, String), Box<dyn Error>> {
    let mut document = document(Some(MR_BRAIN))?;
    let custodian = document.with_custodian(Organization {
        id: Some(String::from("member")),
        name: Some("Synthetic Member".into()),
        ..Organization::default()
    })?;
    Ok((document, custodian))
}

#[test]
fn a_named_custodian_replaces_the_first_organization_author() -> Result<(), Box<dyn Error>> {
    let composition = package_member("package/StructureDefinition-CompositionEuImaging.json")?;
    let custodian = element(&composition, "Composition.custodian")?;
    assert_eq!(
        (&custodian["min"], &custodian["max"]),
        (&Value::from(0), &Value::from("1"))
    );
    assert_eq!(
        custodian["type"][0]["targetProfile"][0],
        "http://hl7.eu/fhir/base/StructureDefinition/organization-eu-core"
    );

    let (document, full_url) = custodied()?;
    assert_eq!(full_url, format!("{BASE}/Organization/member"));
    let bundle = serde_json::to_value(document.into_bundle()?)?;
    let composition = the(&bundle, "Composition")?;
    assert_eq!(composition["custodian"]["reference"], full_url.as_str());
    let authors: Vec<&str> = composition["author"]
        .as_array()
        .ok_or("authors")?
        .iter()
        .filter_map(|author| author["reference"].as_str())
        .collect();
    assert_eq!(
        authors,
        [
            format!("{BASE}/Device/gateway").as_str(),
            "Organization/radiology"
        ],
        "dr-comp-author-org: the given Organization authors give way to the report's performers"
    );
    Ok(())
}

#[test]
fn an_imaging_report_naming_its_custodian_validates_against_bundle_report_eu_imaging()
-> Result<(), Box<dyn Error>> {
    let Some(set) = profile_set()? else {
        return Ok(());
    };
    let (document, _) = custodied()?;
    let text = serde_json::to_string(&document.into_bundle()?)?;
    let received = ReceivedDocument::read(&text)?;
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    runtime
        .block_on(received.validate(set, IMAGING_BUNDLE, None::<&Client>))
        .map_err(|error| format!("{error:?}"))?;
    Ok(())
}
