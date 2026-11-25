// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The crosswalk of the draft `EEHRxF` annexes (initiative 17512, 2026-11-23)
//! to the Xt-EHR models 1.0.0, held to both vendored sources.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::path::PathBuf;

use eehrxf::draft::Annex;
use eehrxf::draft::AnnexCrosswalk;
use eehrxf::draft::DraftError;
use eehrxf::draft::SHIPPED;
use eehrxf::draft::Verdict;

use ferrobridge_testkit::conformance::{self, Case, Corpus};

use super::support::xtehr;

/// The extracted text of the vendored Annexes I to X.
fn annexes() -> Result<String, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/ehds-eehrxf-draft/annexes.txt");
    Ok(std::fs::read_to_string(path)?)
}

fn shipped() -> Result<AnnexCrosswalk, DraftError> {
    SHIPPED.parse()
}

#[test]
fn every_xtehr_element_of_a_crosswalked_model_has_a_row_at_its_cardinality()
-> Result<(), Box<dyn Error>> {
    let found = shipped()?.check_xtehr(&xtehr()?);
    let lines: Vec<String> = found.iter().map(ToString::to_string).collect();
    assert!(lines.is_empty(), "{}", lines.join("\n"));
    Ok(())
}

#[test]
fn every_annex_table_holds_exactly_the_rows_with_a_draft_cardinality() -> Result<(), Box<dyn Error>>
{
    let found = shipped()?.check_annexes(&annexes()?);
    let lines: Vec<String> = found.iter().map(ToString::to_string).collect();
    assert!(lines.is_empty(), "{}", lines.join("\n"));
    Ok(())
}

#[test]
fn every_annex_with_a_data_set_has_rows() -> Result<(), Box<dyn Error>> {
    let crosswalk = shipped()?;
    for annex in Annex::all() {
        assert!(crosswalk.annex(annex).next().is_some(), "Annex {annex}");
    }
    // Annex X, Tables 1 to 33: the common data groups of Art 7.
    let mut groups: Vec<u8> = crosswalk
        .annex(Annex::X)
        .filter_map(eehrxf::draft::Row::table)
        .collect();
    groups.dedup();
    assert_eq!(groups, (1..=33).collect::<Vec<u8>>());
    Ok(())
}

#[test]
fn the_annex_iii_document_metadata_records_where_the_draft_leads() -> Result<(), Box<dyn Error>> {
    // Annex III, Table 1: documentCategory is new, version is 1..1 against
    // Xt-EHR's 0..1, and eventCategory is Xt-EHR's eventType.
    let crosswalk = shipped()?;
    let row = |element| {
        crosswalk
            .draft("Document metadata", element)
            .ok_or(format!("no row for {element}"))
    };
    assert_eq!(row("documentCategory")?.verdict(), Verdict::MissingInXtEhr);
    assert_eq!(row("version")?.verdict(), Verdict::CardinalityDiffers);
    assert_eq!(row("version")?.xtehr_path(), "EHDSDocument.header.version");
    let event = row("eventCategory")?;
    assert_eq!(event.verdict(), Verdict::Renamed);
    assert_eq!(event.xtehr_element(), Some("header.eventType"));
    Ok(())
}

#[test]
fn the_medical_test_report_is_the_xtehr_laboratory_report() -> Result<(), Box<dyn Error>> {
    let crosswalk = shipped()?;
    let root = crosswalk
        .annex(Annex::Viii)
        .find(|row| row.is_root())
        .ok_or("Annex VIII has no root row")?;
    assert_eq!(root.draft_model(), "EHDSMedicalTestReport");
    assert_eq!(root.xtehr_model(), "EHDSLaboratoryReport");
    assert_eq!(root.verdict(), Verdict::Renamed);
    Ok(())
}

#[test]
fn a_row_whose_verdict_its_cells_contradict_is_refused() {
    let text = SHIPPED.replacen(
        "header.subject\t1..1\tsame",
        "header.subject\t1..1\tcardinality differs",
        1,
    );
    assert!(matches!(
        text.parse::<AnnexCrosswalk>(),
        Err(DraftError::Line(_, inner)) if matches!(*inner, DraftError::Contradicted { .. })
    ));
}

#[test]
fn a_crosswalk_without_its_header_is_refused() {
    let text = SHIPPED.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert!(matches!(
        text.parse::<AnnexCrosswalk>(),
        Err(DraftError::Header)
    ));
}

#[test]
fn a_row_missing_from_the_crosswalk_fails_the_xtehr_check() -> Result<(), Box<dyn Error>> {
    let text: Vec<&str> = SHIPPED
        .lines()
        .filter(|line| !line.contains("\tEHDSPatient\tidentifier\t"))
        .collect();
    let crosswalk: AnnexCrosswalk = text.join("\n").parse()?;
    let found = crosswalk.check_xtehr(&xtehr()?);
    assert!(
        found
            .iter()
            .any(|d| d.to_string() == "EHDSPatient.identifier is in the package and in no row"),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn a_row_missing_from_an_annex_table_fails_the_text_check() -> Result<(), Box<dyn Error>> {
    let text: Vec<&str> = SHIPPED
        .lines()
        .filter(|line| !line.starts_with("X\t26\tEHDSPatient\tdateOfBirth\t"))
        .collect();
    let crosswalk: AnnexCrosswalk = text.join("\n").parse()?;
    let found = crosswalk.check_annexes(&annexes()?);
    assert!(
        found
            .iter()
            .any(|d| d.to_string().starts_with("Annex X Table 26:")),
        "{found:?}"
    );
    Ok(())
}

// NOTE: no specification governs this: our own design; an annex element counts as aligned
// when the crosswalk finds it in the Xt-EHR models 1.0.0 under the same name and cardinality.
#[test]
fn conformance_the_xtehr_crosswalk_holds_its_pass_list() -> Result<(), Box<dyn Error>> {
    let cases: Vec<Case> = shipped()?
        .rows()
        .iter()
        .filter_map(|row| {
            let element = row.draft_element()?;
            let id = format!(
                "{}:{}:{element}",
                row.annex().numeral(),
                row.draft_model().replace(' ', "_")
            );
            Some(if row.verdict() == Verdict::Same {
                Case::pass(id)
            } else {
                Case::fail(id, row.verdict().as_str())
            })
        })
        .collect();
    let outcome = conformance::record(Corpus::XtehrCrosswalk, &cases)?;
    assert!(
        outcome.regressed.is_empty(),
        "cases the pass list records that fail: {:?}",
        outcome.regressed
    );
    Ok(())
}
