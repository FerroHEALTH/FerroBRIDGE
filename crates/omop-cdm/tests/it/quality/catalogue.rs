// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The generated Data Quality Dashboard catalogue and settings, read back
//! through `quality::definitions` and `quality::check`.

use std::error::Error;

use omop_cdm::generated::dqd;
use omop_cdm::quality::check::CheckName;
use omop_cdm::quality::definitions::{self, CheckLevel, KahnCategory, Percentage};

/// The Dashboard's vendored provenance note, which names the pinned tag.
const PROVENANCE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/dqd/PROVENANCE.md"
);

#[test]
fn the_catalogue_carries_every_check_of_the_description_file() {
    assert_eq!(27, dqd::CHECK_DESCRIPTIONS.len());
}

#[test]
fn the_ported_checks_are_the_conformance_checks_of_the_catalogue() {
    let conformance: Vec<&str> = dqd::CHECK_DESCRIPTIONS
        .iter()
        .filter(|check| check.kahn_category == KahnCategory::Conformance)
        .map(|check| check.name)
        .collect();
    let ported: Vec<&str> = CheckName::ALL.iter().map(|name| name.as_str()).collect();
    assert_eq!(conformance, ported);
}

#[test]
fn a_check_name_reads_its_level_from_the_catalogue() {
    assert_eq!(CheckLevel::Table, CheckName::CdmTable.level());
    assert_eq!(CheckLevel::Field, CheckName::FkClass.level());
    assert_eq!("field_fk_class.sql", CheckName::FkClass.sql_file());
}

#[test]
fn every_generated_threshold_is_a_percentage() {
    let thresholds = dqd::TABLE_THRESHOLDS
        .iter()
        .flat_map(|table| table.checks)
        .chain(dqd::FIELD_THRESHOLDS.iter().flat_map(|field| field.checks))
        .filter_map(|setting| setting.threshold);
    for threshold in thresholds {
        assert_eq!(Percentage::new(threshold.value()), Some(threshold));
    }
}

#[test]
fn a_percentage_above_100_is_refused() {
    assert_eq!(None, Percentage::new(101));
    assert_eq!(Some(100), Percentage::new(100).map(Percentage::value));
}

#[test]
fn the_settings_are_keyed_by_the_generated_names() -> Result<(), Box<dyn Error>> {
    let offset = definitions::field_thresholds("note_nlp", "offset").ok_or("no note_nlp.offset")?;
    assert_eq!(
        Some("Yes"),
        offset
            .setting("measureValueCompleteness")
            .and_then(|s| s.value)
    );
    let person = definitions::table_thresholds("person").ok_or("no person table")?;
    assert_eq!(Some("Yes"), person.parameter("isRequired"));
    Ok(())
}

#[test]
fn a_field_keeps_its_parameter_cells() -> Result<(), Box<dyn Error>> {
    let field = definitions::field_thresholds("person", "gender_concept_id")
        .ok_or("no person.gender_concept_id")?;
    assert_eq!(Some("CONCEPT"), field.parameter("fkTableName"));
    assert_eq!(
        None,
        field.parameter("userGuidance"),
        "documentation is left out"
    );
    Ok(())
}

#[test]
fn the_tag_constant_matches_the_vendored_pin() -> Result<(), Box<dyn Error>> {
    let provenance = std::fs::read_to_string(PROVENANCE)?;
    let pin = format!("Pin: tag `{}`", omop_cdm::quality::DQD_TAG);
    assert!(
        provenance.contains(&pin),
        "docs/specs/dqd/PROVENANCE.md does not pin {}",
        omop_cdm::quality::DQD_TAG
    );
    Ok(())
}
