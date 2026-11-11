// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a received imaging report carries into no structured element: each
//! section and element the run declares in its outcome, the performing
//! organization the imaging service slot carries, and a performer of another
//! type declared.

use std::error::Error;

use eehrxf::mapping::Mapping;
use eehrxf::mappings::IMAGING;
use fhir_types::r4::practitioner::Practitioner;
use serde_json::Value;

use super::{
    CONTEXT, expressions_by, mapping, only, received_text, received_text_with, report, template,
};

/// The expressions of the issues the run over the received `text` declares
/// not carried, in the order the run declares them.
fn not_carried(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    not_carried_by(&mapping()?, text)
}

/// The expressions of [`not_carried`], the run made by `mapping`.
fn not_carried_by(mapping: &Mapping, text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    expressions_by(mapping, text, |issue| {
        issue["details"]["text"]
            .as_str()
            .is_some_and(|text| text.contains("no structured element"))
    })
}

// NOTE: HL7 Europe Imaging Report 1.0.0-ballot DiagnosticReportEuImaging.performer admits a
// Practitioner beside the organization slice; the imaging service slot holds an organisation alone.
#[test]
fn a_performer_that_is_no_organization_is_declared_and_the_organization_carried()
-> Result<(), Box<dyn Error>> {
    let mut performed = report();
    performed["performer"] = serde_json::json!([
        {"reference": "Organization/synthetic-radiology"},
        {"reference": "Practitioner/synthetic-radiologist"}
    ]);
    let text = received_text_with(
        &performed,
        Some(Practitioner {
            id: Some(String::from("synthetic-radiologist")),
            ..Practitioner::default()
        }),
    )?;
    let declared = not_carried(&text)?;
    assert!(
        declared.contains(&String::from("DiagnosticReport.performer[1]")),
        "the practitioner is declared not carried: {declared:?}"
    );
    assert!(
        !declared.contains(&String::from("DiagnosticReport.performer[0]")),
        "the organization is carried: {declared:?}"
    );
    Ok(())
}

// NOTE: no specification governs this: our own design; a gateway that compiles the shipped files
// from disk, as FerroFED's receive path does, declares what the compiled project declares.
#[test]
fn a_report_mapped_by_the_shipped_files_declares_what_the_project_declares()
-> Result<(), Box<dyn Error>> {
    let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("eehrxf-imaging-files");
    std::fs::create_dir_all(&directory)?;
    let files = IMAGING.write_to(&directory)?;
    let from_files = Mapping::compile_template(&template()?, &files, &[CONTEXT])?;
    let text = received_text()?;
    let declared = not_carried_by(&from_files, &text)?;
    assert!(
        !declared.is_empty(),
        "the files declare the report's losses"
    );
    assert_eq!(
        declared,
        not_carried(&text)?,
        "the files declare what the compiled project declares, and nothing the composition carries"
    );
    Ok(())
}

/// The path of the section of the received `document` coded `code`.
fn section_at(document: &Value, code: &str) -> Result<String, Box<dyn Error>> {
    let index = only(document, "Composition")?["section"]
        .as_array()
        .ok_or("the sections")?
        .iter()
        .position(|section| section["code"]["coding"][0]["code"] == code)
        .ok_or_else(|| format!("the section {code}"))?;
    Ok(format!("Composition.section[{index}]"))
}

/// The path of the element of the received report's `member` list whose
/// `field` is `value`.
fn report_member_at(
    document: &Value,
    member: &str,
    field: &str,
    value: &str,
) -> Result<String, Box<dyn Error>> {
    let index = only(document, "DiagnosticReport")?[member]
        .as_array()
        .ok_or_else(|| format!("the report's {member}"))?
        .iter()
        .position(|held| held.pointer(field).and_then(Value::as_str) == Some(value))
        .ok_or_else(|| format!("the report's {member} {value}"))?;
    Ok(format!("DiagnosticReport.{member}[{index}]"))
}

// NOTE: CompositionEuImaging requires the order, history and procedure sections, which the
// template holds no section for, and DiagnosticReportEuImaging the composition link and two categories.
#[test]
fn a_received_report_declares_each_section_and_element_it_does_not_carry()
-> Result<(), Box<dyn Error>> {
    let text = received_text()?;
    let document: Value = serde_json::from_str(&text)?;
    let declared = not_carried(&text)?;
    assert_eq!(
        declared,
        [
            section_at(&document, "55115-0")?,
            section_at(&document, "11329-0")?,
            section_at(&document, "55111-9")?,
            report_member_at(
                &document,
                "extension",
                "/url",
                "http://hl7.org/fhir/5.0/StructureDefinition/extension-DiagnosticReport.composition",
            )?,
            report_member_at(&document, "category", "/coding/0/code", "85430-7")?,
            report_member_at(&document, "category", "/coding/0/code", "Medical-Imaging")?,
        ],
        "each required section with no entry, the report's link and categories, and nothing the \
         template carries"
    );
    assert!(
        !declared.contains(&section_at(&document, "18726-0")?),
        "the imaging study section lists the carried study"
    );
    Ok(())
}

#[test]
fn each_report_and_study_element_and_narrative_section_is_declared() -> Result<(), Box<dyn Error>> {
    let mut document: Value = serde_json::from_str(&received_text()?)?;
    let entries = document["entry"].as_array_mut().ok_or("the entries")?;
    for entry in entries.iter_mut() {
        let resource = &mut entry["resource"];
        match resource["resourceType"].as_str() {
            Some("Composition") => resource["section"]
                .as_array_mut()
                .ok_or("the sections")?
                .push(serde_json::json!({
                    "title": "Findings",
                    "code": {"coding": [{"system": "http://loinc.org", "code": "59776-5"}]},
                    "text": {
                        "status": "generated",
                        "div": "<div xmlns=\"http://www.w3.org/1999/xhtml\">Synthetic findings.</div>"
                    }
                })),
            Some("DiagnosticReport") => {
                resource["issued"] = serde_json::json!("2026-03-02T09:00:00+01:00");
                resource["resultsInterpreter"] =
                    serde_json::json!([{"reference": "Organization/synthetic-radiology"}]);
            }
            Some("ImagingStudy") => {
                resource["numberOfSeries"] = serde_json::json!(1);
                resource["note"] = serde_json::json!([{"text": "A synthetic study note."}]);
            }
            _ => {}
        }
    }
    let findings = section_at(&document, "59776-5")?;
    let declared = not_carried(&serde_json::to_string(&document)?)?;
    for expression in [
        findings.as_str(),
        "DiagnosticReport.issued",
        "DiagnosticReport.resultsInterpreter[0]",
        "ImagingStudy.numberOfSeries",
        "ImagingStudy.note[0]",
    ] {
        assert!(
            declared.iter().any(|held| held == expression),
            "{expression} is declared: {declared:?}"
        );
    }
    for carried in [
        "DiagnosticReport.status",
        "DiagnosticReport.code",
        "DiagnosticReport.conclusion",
        "ImagingStudy.identifier",
        "ImagingStudy.status",
        "ImagingStudy.modality",
        "ImagingStudy.started",
        "ImagingStudy.description",
    ] {
        assert!(
            !declared.iter().any(|held| held.starts_with(carried)),
            "{carried} is carried, not declared: {declared:?}"
        );
    }
    Ok(())
}
