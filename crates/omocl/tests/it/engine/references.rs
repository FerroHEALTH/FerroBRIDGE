// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `provider_id` and `visit_detail_id`: a column that references a
//! `PROVIDER` or a `VISIT_DETAIL` takes the text the record carries as the
//! row's source, which the writer resolves (our own design over the CDM's
//! `PROVIDER` and `VISIT_DETAIL` definitions).
//!
//! The mapping is synthetic: each analyte's name stands in for the provider
//! and the visit detail, so the case needs no other template.

use std::error::Error;

use omocl::engine::RefusalKind;
use omocl::engine::concept::VocabularyAliases;
use omocl::engine::run;
use omop_cdm::graph::row::Cell;
use omop_cdm::graph::row::Reference;

use crate::lab::StubSource;
use crate::lab::compiled;
use crate::lab::composition;
use crate::lab::flat;
use crate::lab::seams;
use crate::lab::set;
use crate::lab::source;
use crate::lab::template;
use crate::resolve::ANALYTE;
use crate::resolve::mapping;

use crate::engine::outcome;
use crate::engine::rows;

/// A condition per analyte whose name names `columns`, with concept 0 so the
/// case needs no vocabulary row.
fn named(columns: &[&str]) -> String {
    let references: String = columns
        .iter()
        .map(|column| {
            [
                "    ",
                column,
                ":\n      alternatives:\n        - path: \"/items[at0024]\"\n",
            ]
            .concat()
        })
        .collect();
    mapping(
        "Synthetic_references_v1",
        ANALYTE,
        &format!(
            "  - type: \"ConditionOccurrence\"\n    concept_id:\n      alternatives:\n        - code: 0\n    \
             condition_start_date:\n      alternatives:\n        - path: \"../../\"\n{references}"
        ),
    )
}

#[tokio::test]
async fn a_provider_and_a_visit_detail_are_named_by_the_text_the_record_carries()
-> Result<(), Box<dyn Error>> {
    let stub = StubSource::laboratory()?;
    let index = template()?;
    let composition = composition(&index, &flat(&[], &[])?)?;
    let file = named(&["provider_id", "visit_detail_id"]);
    let (graph, refusals) = outcome(
        &[("Synthetic_references_v1.yml", &file)],
        &index,
        &composition,
        &stub,
    )
    .await?;
    assert!(refusals.is_empty(), "{refusals:?}");
    let rows = rows(&graph, "condition_occurrence");
    assert_eq!(2, rows.len());
    for row in rows {
        let Some(Cell::Reference(Reference::Provider(provider))) = row.cell("provider_id") else {
            panic!("a provider reference: {:?}", row.cell("provider_id"));
        };
        let Some(Cell::Reference(Reference::VisitDetail(detail))) = row.cell("visit_detail_id")
        else {
            panic!(
                "a visit detail reference: {:?}",
                row.cell("visit_detail_id")
            );
        };
        assert_eq!(provider.as_str(), detail.source().as_str());
        let Some(Cell::Reference(Reference::Visit(visit))) = row.cell("visit_occurrence_id") else {
            panic!("the composition's visit");
        };
        assert_eq!(
            visit,
            detail.visit(),
            "the detail lies in the composition's visit"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_visit_detail_without_the_compositions_visit_refuses_the_record()
-> Result<(), Box<dyn Error>> {
    let stub = StubSource::laboratory()?;
    let index = template()?;
    let composition = composition(&index, &flat(&[], &[])?)?;
    let file = named(&["visit_detail_id"]);
    let program = compiled(
        &set(&[], &[("Synthetic_references_v1.yml", &file)])?,
        &index,
    )?;
    let aliases = VocabularyAliases::default();
    let (graph, refusals) = run(
        &program,
        &composition,
        &index,
        &source()?,
        None,
        &seams(&stub, &aliases),
    )
    .await?
    .into_parts();
    assert_eq!(0, rows(&graph, "condition_occurrence").len());
    assert_eq!(2, refusals.len(), "{refusals:?}");
    assert!(
        refusals.iter().all(|refusal| refusal
            .message
            .contains("visit detail needs the composition's visit")),
        "{refusals:?}"
    );
    assert!(
        refusals
            .iter()
            .all(|refusal| refusal.kind != RefusalKind::MissingColumn)
    );
    Ok(())
}
