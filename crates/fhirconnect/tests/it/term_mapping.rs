// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `TERM_MAPPING` whose `match` FHIR cannot carry, through a whole run.
//!
//! `match` is "hard coded to `=`, other operators not mappable to FHIR"
//! (`docs/specs/fhirconnect/modules/ROOT/pages/types-of-mappings/data-type/TERM_MAPPING.adoc`),
//! so such a mapping writes no `Coding`, the run declares it as a located
//! `Loss::Skipped`, and the value's own text and every `=` mapping still
//! convert. The composition is the synthetic diagnosis template's problem
//! name, annotated with invented codes.

use core::error::Error;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Location;
use fhirconnect::engine::outcome::Loss;
use fhirconnect::engine::outcome::SkipReason;
use fhirconnect::engine::outcome::Warning;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::to_fhir;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::composition::NodeValue;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::index::paths::AqlPath;

use crate::laws;
use crate::support::compiled;
use crate::support::template;

/// The archetype path of the problem name the composition annotates.
pub(crate) const PROBLEM: &str =
    "/content[openEHR-EHR-EVALUATION.problem_diagnosis.v1]/data[at0001]/items[at0002]/value";

/// The synthetic terminology of the withheld term.
pub(crate) const BROADER_SYSTEM: &str = "http://example.org/fhir/CodeSystem/synthetic-broader";

/// The synthetic code of the withheld term.
pub(crate) const BROADER_CODE: &str = "SYN-BROAD-7";

/// Returns one canonical `TERM_MAPPING`.
fn term(relation: char, system: &str, code: &str) -> serde_json::Value {
    serde_json::json!({
        "_type": "TERM_MAPPING",
        "match": relation.to_string(),
        "target": {
            "_type": "CODE_PHRASE",
            "terminology_id": { "_type": "TERMINOLOGY_ID", "value": system },
            "code_string": code,
        },
    })
}

/// Merges `fields` into the value of the element `node_id` names.
fn annotate(
    document: &mut serde_json::Value,
    node_id: &str,
    fields: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    match *document {
        serde_json::Value::Object(ref mut members) => {
            if members
                .get("archetype_node_id")
                .and_then(serde_json::Value::as_str)
                == Some(node_id)
                && let Some(serde_json::Value::Object(value)) = members.get_mut("value")
            {
                value.extend(fields.clone());
                return true;
            }
            members
                .values_mut()
                .any(|member| annotate(member, node_id, fields))
        }
        serde_json::Value::Array(ref mut items) => {
            items.iter_mut().any(|item| annotate(item, node_id, fields))
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => false,
    }
}

/// Returns the mappings the problem name carries: one with `relation` to the
/// withheld term, then an `=` mapping to `SYN-001`.
fn problem_mappings(relation: char) -> serde_json::Value {
    serde_json::json!([
        term(relation, BROADER_SYSTEM, BROADER_CODE),
        term(
            '=',
            "http://example.org/fhir/CodeSystem/synthetic",
            "SYN-001"
        ),
    ])
}

/// Builds the synthetic composition whose problem name value carries
/// `fields` beside its text.
fn composition_with(
    index: &WebTemplateIndex,
    fields: &serde_json::Map<String, serde_json::Value>,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let values = vec![
        NodeValue::new(
            index.node(&AqlPath::new("/composer"))?,
            "FerroBRIDGE".into(),
        )
        .with_datum("name"),
        NodeValue::new(index.node(&AqlPath::new("/language"))?, "de".into()).with_datum("code"),
        NodeValue::new(index.node(&AqlPath::new("/territory"))?, "DE".into()).with_datum("code"),
        NodeValue::new(
            index.node(&AqlPath::new(PROBLEM))?,
            "a synthetic problem name".into(),
        )
        .with_datum("value"),
    ];
    let mut document = index.build_composition(&values, laws::NOW)?.into_value();
    if !annotate(&mut document, "at0002", fields) {
        return Err("the composition holds no problem name".into());
    }
    Ok(index.accept(document)?)
}

/// Builds the synthetic composition whose problem name carries a mapping
/// with `relation` to the withheld term, then an `=` mapping to `SYN-001`.
pub(crate) fn annotated_composition(
    index: &WebTemplateIndex,
    relation: char,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let mut fields = serde_json::Map::new();
    fields.insert(String::from("mappings"), problem_mappings(relation));
    composition_with(index, &fields)
}

/// The synthetic terminology of the problem name's defining code.
pub(crate) const DEFINING_SYSTEM: &str = "http://example.org/fhir/CodeSystem/synthetic-problem";

/// The synthetic code of the problem name's defining code.
pub(crate) const DEFINING_CODE: &str = "SYN-PROB-3";

/// Builds the synthetic composition whose problem name is a `DV_CODED_TEXT`
/// coded as [`DEFINING_CODE`], carrying the mappings of
/// [`annotated_composition`].
pub(crate) fn coded_composition(
    index: &WebTemplateIndex,
    relation: char,
) -> Result<CanonicalComposition, Box<dyn Error>> {
    let mut fields = serde_json::Map::new();
    fields.insert(String::from("_type"), "DV_CODED_TEXT".into());
    fields.insert(
        String::from("defining_code"),
        serde_json::json!({
            "_type": "CODE_PHRASE",
            "terminology_id": { "_type": "TERMINOLOGY_ID", "value": DEFINING_SYSTEM },
            "code_string": DEFINING_CODE,
        }),
    );
    fields.insert(String::from("mappings"), problem_mappings(relation));
    composition_with(index, &fields)
}

/// Returns the `Condition.code` a run out of openEHR wrote.
fn code_of(resource: &serde_json::Value) -> serde_json::Value {
    resource
        .get("code")
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

#[test]
fn each_relation_fhir_cannot_carry_is_a_located_skip_and_the_rest_converts()
-> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    for relation in ['>', '<', '?'] {
        let outbound = to_fhir(
            &program,
            &SCHEMAS,
            &index,
            &annotated_composition(&index, relation)?,
            &Seams::default(),
            &CallContext::new(),
        )?;
        let resource = outbound
            .value()
            .to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?;
        assert_eq!(
            code_of(&resource),
            serde_json::json!({
                "coding": [{
                    "system": "http://example.org/fhir/CodeSystem/synthetic",
                    "code": "SYN-001",
                }],
                "text": "a synthetic problem name",
            }),
            "the text and the `=` mapping convert past `{relation}`"
        );
        assert_eq!(
            outbound.warnings(),
            [Warning::new(
                Loss::Skipped {
                    mapping: String::from("problemDiagnose"),
                    reason: SkipReason::TermMappingMatch {
                        match_code: relation,
                        terminology: String::from(BROADER_SYSTEM),
                        code: String::from(BROADER_CODE),
                    },
                },
                Location::default()
                    .with_fhir("Condition.code")
                    .with_openehr(format!("{PROBLEM}/mappings")),
            )],
            "the `{relation}` mapping is the one declared loss"
        );
    }
    Ok(())
}

#[test]
fn equivalent_mappings_all_convert_and_declare_nothing() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &annotated_composition(&index, '=')?,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let resource = outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Condition"))?;
    assert_eq!(
        code_of(&resource),
        serde_json::json!({
            "coding": [
                { "system": BROADER_SYSTEM, "code": BROADER_CODE },
                {
                    "system": "http://example.org/fhir/CodeSystem/synthetic",
                    "code": "SYN-001",
                },
            ],
            "text": "a synthetic problem name",
        }),
        "every `=` mapping is a coding"
    );
    assert!(outbound.warnings().is_empty(), "`=` declares no loss");
    Ok(())
}

// The declared set of a GetPut over a withheld mapping: the first leg names the
// skip, and the second leg writes the `=` mapping back alone, so the withheld one
// is the mapping the composition loses.
#[test]
fn getput_declares_the_withheld_mapping() -> Result<(), Box<dyn Error>> {
    let program = compiled("ferrobridge_diagnose_minimal")?;
    let index = template()?;
    let declared = laws::getput(
        &program,
        &index,
        &annotated_composition(&index, '<')?,
        &laws::untouched,
    )?;
    insta::assert_json_snapshot!("term_mapping_getput", declared);
    Ok(())
}
