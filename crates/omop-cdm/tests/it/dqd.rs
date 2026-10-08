// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Data Quality Dashboard's v5.4 field thresholds against the generated
//! metadata: the same field set, and exactly the recorded disagreements on
//! the facts the conformance checks read (`isRequired`, `isPrimaryKey`,
//! `isForeignKey`, `fkDomain`, `fkClass`).
//!
//! The port takes its field set from the CDM v5.4.3 definitions, which the
//! vendored DDL agrees with, and its thresholds from the Dashboard
//! (`docs/architecture.md` section 2), so a disagreement is a fact the port
//! must know about, never one it absorbs.

use std::collections::BTreeMap;
use std::error::Error;

/// The Dashboard's vendored field thresholds, read from the repository.
const FIELD_LEVEL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/dqd/inst/csv/OMOP_CDMv5.4_Field_Level.csv"
);

/// What the conformance checks read about one field.
#[derive(Debug, Default, PartialEq, Eq)]
struct Facts {
    required: bool,
    primary_key: bool,
    foreign_key: Option<(String, String)>,
    fk_domain: Option<String>,
    fk_class: Option<String>,
}

/// The facts of every field the Dashboard thresholds, keyed by table and
/// field as the generated metadata spells them.
fn dashboard() -> Result<BTreeMap<(String, String), Facts>, Box<dyn Error>> {
    let mut reader = csv::Reader::from_path(FIELD_LEVEL)?;
    let headers = reader.headers()?.clone();
    let index = |name: &str| {
        headers
            .iter()
            .position(|header| header == name)
            .ok_or_else(|| format!("the field thresholds have no {name} column"))
    };
    let table = index("cdmTableName")?;
    let field = index("cdmFieldName")?;
    let required = index("isRequired")?;
    let primary_key = index("isPrimaryKey")?;
    let foreign_key = index("isForeignKey")?;
    let fk_table = index("fkTableName")?;
    let fk_field = index("fkFieldName")?;
    let fk_domain = index("fkDomain")?;
    let fk_class = index("fkClass")?;
    let mut facts = BTreeMap::new();
    for record in reader.records() {
        let record = record?;
        let cell = |at: usize| record.get(at).unwrap_or_default();
        let yes = |at: usize| cell(at) == "Yes";
        let key = (cell(table).to_lowercase(), cell(field).to_lowercase());
        let row = Facts {
            required: yes(required),
            primary_key: yes(primary_key),
            foreign_key: yes(foreign_key)
                .then(|| (cell(fk_table).to_lowercase(), cell(fk_field).to_lowercase())),
            fk_domain: Some(cell(fk_domain))
                .filter(|domain| !domain.is_empty())
                .map(str::to_owned),
            fk_class: Some(cell(fk_class))
                .filter(|class| !class.is_empty())
                .map(str::to_owned),
        };
        assert!(
            facts.insert(key.clone(), row).is_none(),
            "the field thresholds carry {key:?} twice"
        );
    }
    Ok(facts)
}

/// The same facts from the generated metadata.
fn definitions() -> BTreeMap<(String, String), Facts> {
    omop_cdm::generated::TABLES
        .iter()
        .flat_map(|table| {
            table.columns.iter().map(|column| {
                (
                    (table.name.to_owned(), column.name.to_owned()),
                    Facts {
                        required: column.required,
                        primary_key: column.primary_key,
                        foreign_key: column
                            .foreign_key
                            .map(|(table, field)| (table.to_owned(), field.to_owned())),
                        fk_domain: column.fk_domain.map(str::to_owned),
                        fk_class: column.fk_class.map(str::to_owned),
                    },
                )
            })
        })
        .collect()
}

#[test]
fn the_dashboard_thresholds_cover_the_generated_field_set() -> Result<(), Box<dyn Error>> {
    let dashboard: Vec<_> = dashboard()?.into_keys().collect();
    let definitions: Vec<_> = definitions().into_keys().collect();
    assert_eq!(
        definitions, dashboard,
        "the Dashboard thresholds and the CDM definitions name different fields"
    );
    Ok(())
}

#[test]
fn the_dashboard_and_the_definitions_disagree_exactly_where_recorded() -> Result<(), Box<dyn Error>>
{
    let dashboard = dashboard()?;
    let definitions = definitions();
    let mut found = Vec::new();
    for (key, cdm) in &definitions {
        let dqd = dashboard
            .get(key)
            .ok_or_else(|| format!("the Dashboard thresholds have no {key:?}"))?;
        let place = format!("{}.{}", key.0, key.1);
        if dqd.required != cdm.required {
            found.push(format!(
                "{place} isRequired: {} / {}",
                dqd.required, cdm.required
            ));
        }
        if dqd.primary_key != cdm.primary_key {
            found.push(format!(
                "{place} isPrimaryKey: {} / {}",
                dqd.primary_key, cdm.primary_key
            ));
        }
        if dqd.foreign_key != cdm.foreign_key {
            found.push(format!(
                "{place} isForeignKey: {:?} / {:?}",
                dqd.foreign_key, cdm.foreign_key
            ));
        }
        if dqd.fk_domain != cdm.fk_domain {
            found.push(format!(
                "{place} fkDomain: {:?} / {:?}",
                dqd.fk_domain, cdm.fk_domain
            ));
        }
        if dqd.fk_class != cdm.fk_class {
            found.push(format!(
                "{place} fkClass: {:?} / {:?}",
                dqd.fk_class, cdm.fk_class
            ));
        }
    }
    // NOTE: DQD v2.9.0 `inst/csv/OMOP_CDMv5.4_Field_Level.csv` against CDM v5.4.3
    // `OMOP_CDMv5.4_Field_Level.csv`; each pair is Dashboard / definitions.
    let recorded = [
        "cdm_source.cdm_version_concept_id fkDomain: Some(\"Metadata\") / None",
        "cohort_definition.cohort_definition_id isForeignKey: Some((\"cohort\", \"cohort_definition_id\")) / None",
        "death.person_id isPrimaryKey: true / false",
        "drug_strength.drug_concept_id fkDomain: Some(\"Drug\") / None",
        "drug_strength.ingredient_concept_id fkClass: Some(\"Ingredient\") / None",
        "episode.episode_object_concept_id fkDomain: None / Some(\"Procedure, Regimen\")",
        "episode.episode_parent_id isForeignKey: None / Some((\"episode\", \"episode_id\"))",
        "location.country_concept_id fkDomain: Some(\"Geography\") / None",
        "measurement.operator_concept_id fkDomain: Some(\"Meas Value Operator\") / None",
        "note_nlp.note_id isForeignKey: None / Some((\"note\", \"note_id\"))",
        "vocabulary.vocabulary_id isPrimaryKey: true / false",
    ];
    assert_eq!(
        recorded.as_slice(),
        found.as_slice(),
        "the Dashboard thresholds and the CDM definitions disagree somewhere new"
    );
    Ok(())
}
