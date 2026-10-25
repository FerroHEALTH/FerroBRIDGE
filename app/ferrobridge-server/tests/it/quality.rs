// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `dq check` renderings: the Dashboard's result fields in JSON (DQD
//! v2.9.0 `R/recordResult.R`, `R/summarizeResults.R`) and the text summary,
//! over results built from planned checks and synthetic counts.

use std::error::Error;

use ferrobridge_server::quality::{self, QualityRun};
use omop_cdm::quality::check::CheckName;
use omop_cdm::quality::evaluate::{self, Counts, Outcome};

/// A run of two checks: one passing, one failing.
fn run() -> Result<QualityRun, Box<dyn Error>> {
    let plan = quality::plan(None)?;
    let find = |name: CheckName, field: &str| {
        plan.checks()
            .iter()
            .find(|check| {
                check.name() == name
                    && check.table().name == "person"
                    && check.column().map(|column| column.name) == Some(field)
            })
            .cloned()
            .ok_or("a planned check")
    };
    let required = find(CheckName::IsRequired, "year_of_birth")?;
    let datatype = find(CheckName::CdmDatatype, "year_of_birth")?;
    Ok(QualityRun {
        results: vec![
            evaluate::evaluate(
                &required,
                Outcome::Counted(Counts {
                    violated: 0,
                    denominator: 4,
                }),
                None,
            ),
            evaluate::evaluate(
                &datatype,
                Outcome::Counted(Counts {
                    violated: 1,
                    denominator: 4,
                }),
                None,
            ),
        ],
        started: String::from("2026-10-25T10:00:00Z"),
        ended: String::from("2026-10-25T10:00:01Z"),
        seconds: 1.0,
    })
}

#[test]
fn the_json_keeps_the_dashboards_field_names() -> Result<(), Box<dyn Error>> {
    let json = run()?.dashboard_json();
    let results = json["CheckResults"].as_array().ok_or("a result list")?;
    assert_eq!(2, results.len());
    let failing = results.get(1).ok_or("a second result")?;
    assert_eq!("cdmDatatype", failing["checkName"]);
    assert_eq!("field_cdmdatatype_person_year_of_birth", failing["checkId"]);
    assert_eq!(1, failing["numViolatedRows"]);
    assert_eq!(4, failing["numDenominatorRows"]);
    assert_eq!(0.25, failing["pctViolatedRows"]);
    assert_eq!(1, failing["failed"]);
    assert_eq!(0, failing["passed"]);
    assert_eq!("PERSON", failing["cdmTableName"]);
    assert_eq!(2, json["Overview"]["countTotal"]);
    assert_eq!(1, json["Overview"]["countOverallFailed"]);
    assert_eq!("2026-10-25T10:00:00Z", json["startTimestamp"]);
    Ok(())
}

#[test]
fn the_text_names_each_failing_check_and_the_verdicts() -> Result<(), Box<dyn Error>> {
    let run = run()?;
    let text = run.to_string();
    assert!(
        text.contains("failed field_cdmdatatype_person_year_of_birth (1 of 4)"),
        "{text}"
    );
    assert!(
        text.contains("checks: 2 run, 1 passed, 1 failed, 0 errors, 0 not applicable"),
        "{text}"
    );
    assert!(!run.clean());
    let verdicts = run.verdicts();
    assert_eq!(
        vec![String::from("field_cdmdatatype_person_year_of_birth")],
        verdicts.failing
    );
    Ok(())
}

#[test]
fn the_post_load_plan_checks_the_written_tables_and_their_person() -> Result<(), Box<dyn Error>> {
    let written = [String::from("measurement")].into_iter().collect();
    let plan = quality::plan(Some(&written))?;
    let tables: std::collections::BTreeSet<&str> = plan
        .checks()
        .iter()
        .map(|check| check.table().name)
        .collect();
    assert!(tables.contains("measurement") && tables.contains("person"));
    assert!(!tables.contains("drug_exposure"), "{tables:?}");
    Ok(())
}
