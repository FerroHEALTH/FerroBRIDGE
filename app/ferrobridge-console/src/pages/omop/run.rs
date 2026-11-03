// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The OMOP ETL's past runs: the Runs tab's listing and one run's report, a
//! detail page (T2) at `/omop/runs/{run_id}`.
//!
//! The listing reads `GET /operator/etl/runs` and names each run with its
//! job, when it started and finished, and its counts, linked to its report.
//! The report reads `GET /operator/etl/runs/{run_id}` and draws when it ran,
//! the compositions it read, the rows it wrote by table, what its deletion
//! check found, and every record it refused with the reason. A run names a
//! composition by its versioned object id, never a clinical value.

use ferrobridge_operator_api::etl::{Run, RunKind, RunSummary, Runs};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::surface::{Crumb, Header, definitions, panel_sections};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::app::{Drawn, OMOP};
use crate::locale::messages::Msg;
use crate::locale::{tr, tr_with};
use crate::markup::{code, h, t};
use crate::model::{Loaded, Outcome};
use crate::pages::parts::{faulted, quiet, refused, section};
use crate::visit::Visit;

/// The address of the report of the run `run_id`.
fn run_href(run_id: &str) -> String {
    format!("{OMOP}/runs/{run_id}")
}

/// A count, in the locale's digits.
fn count(l: &Locale, value: u64) -> Html {
    t(tr_with(l, Msg::ValueCount, &[("count", value.into())]))
}

/// The words of `kind`.
const fn kind_words(kind: RunKind) -> Msg {
    match kind {
        RunKind::Compositions => Msg::RunKindCompositions,
        RunKind::Fhir => Msg::RunKindFhir,
    }
}

/// The Runs tab over the listing `runs`.
pub(crate) fn listing(l: &Locale, runs: &Loaded<Runs>) -> Html {
    let runs = match runs {
        Ok(Outcome::Shown(runs)) => runs,
        Ok(Outcome::Refused(refusal)) if refusal.not_served() => {
            return quiet(tr(l, Msg::RunsUnavailable), None);
        }
        Ok(Outcome::Refused(refusal)) => return refused(l, refusal),
        Err(fault) => return faulted(l, fault, &super::tab_href("runs")),
    };
    let body = if runs.runs.is_empty() {
        quiet(tr(l, Msg::RunsNone), None)
    } else {
        run_table(l, &runs.runs)
    };
    panel_sections(section(
        l,
        tr(l, Msg::Runs),
        Some(u64::try_from(runs.runs.len()).unwrap_or(u64::MAX)),
        body,
    ))
}

/// The table of `runs`: each one's id and job, when it ran, and its counts.
fn run_table(l: &Locale, runs: &[RunSummary]) -> Html {
    let columns = [
        Column::text("run", tr(l, Msg::ColRun)).role(Role::Title),
        Column::text("started", tr(l, Msg::ColStarted)),
        Column::text("finished", tr(l, Msg::ColFinished)),
        Column::text("read", tr(l, Msg::ColRead)).numeric(),
        Column::text("committed", tr(l, Msg::ColCommitted)).numeric(),
        Column::text("refusals", tr(l, Msg::ColRefusals)).numeric(),
        Column::text("rows", tr(l, Msg::ColRowsWritten)).numeric(),
        Column::text("removed", tr(l, Msg::ColRemoved)).numeric(),
    ];
    let [
        _,
        started,
        finished,
        read,
        committed,
        refusals,
        rows,
        removed,
    ] = &columns;
    let body = runs
        .iter()
        .map(|run| {
            h(format!(
                "<tr>{}{}{}{}{}{}{}{}</tr>",
                title_cell(
                    Some(&run_href(&run.id)),
                    code(&run.id),
                    Some(t(tr(l, kind_words(run.kind)))),
                ),
                cell(started, code(&run.started)),
                cell(finished, code(&run.finished)),
                cell(read, count(l, run.compositions.read)),
                cell(committed, count(l, run.compositions.committed)),
                cell(refusals, count(l, run.refusals)),
                cell(rows, count(l, run.rows)),
                cell(removed, count(l, run.removed)),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        caption: &tr(l, Msg::Runs),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(body),
    })
}

/// The report of the run the route names.
pub async fn page(visit: &Visit) -> Drawn {
    let Some(run_id) = visit.segment(2).map(str::to_owned) else {
        return crate::pages::errors::not_found(visit);
    };
    let run = visit.reads.etl_run(&run_id).await;
    if let Ok(Outcome::Refused(refusal)) = &run
        && refusal.not_served()
    {
        return crate::pages::errors::not_found(visit);
    }
    Drawn::shell(visit.tr(Msg::RunTitle), drawn(&visit.locale, &run_id, &run))
}

/// The detail page of the run `run_id`.
fn drawn(l: &Locale, run_id: &str, run: &Loaded<Run>) -> Html {
    let body = match run {
        Ok(Outcome::Shown(run)) => report(l, run),
        Ok(Outcome::Refused(refusal)) => refused(l, refusal),
        Err(fault) => faulted(l, fault, &run_href(run_id)),
    };
    detail_page(
        l,
        DetailPage {
            header: Header {
                title: run_id.to_owned().into(),
                subtitle: Some(tr(l, Msg::RunTitle).into()),
                subtitle_view: None,
                crumbs: vec![Crumb::new(tr(l, Msg::OmopEtl), super::tab_href("runs"))],
                mono: true,
                badge: None,
            },
            actions: ActionGroup::none(),
            stepper: None,
            tabs: None,
            body,
        },
    )
}

/// The sections of the report of `run`.
fn report(l: &Locale, run: &Run) -> Html {
    let compositions = &run.compositions;
    let facts = definitions(vec![
        (tr(l, Msg::FactJob), t(tr(l, kind_words(run.kind)))),
        (tr(l, Msg::ColStarted), code(&run.started)),
        (tr(l, Msg::ColFinished), code(&run.finished)),
        (tr(l, Msg::ColRead), count(l, compositions.read)),
        (tr(l, Msg::ColCommitted), count(l, compositions.committed)),
        (tr(l, Msg::FactSkipped), count(l, compositions.skipped)),
        (tr(l, Msg::FactRefusedWhole), count(l, compositions.refused)),
        (tr(l, Msg::FactConceptZero), count(l, run.concept_zero)),
    ]);
    let deletions = &run.deletions;
    let ids = |ids: &[String]| {
        if ids.is_empty() {
            t("–")
        } else {
            ids.iter().map(code).collect::<Html>()
        }
    };
    let checked = definitions(vec![
        (tr(l, Msg::FactChecked), count(l, deletions.checked)),
        (tr(l, Msg::FactExtant), count(l, deletions.extant)),
        (tr(l, Msg::ColRemoved), ids(&deletions.removed)),
        (tr(l, Msg::FactKept), ids(&deletions.kept)),
        (tr(l, Msg::FactUnknown), ids(&deletions.unknown)),
    ]);
    panel_sections(
        section(l, tr(l, Msg::RunWhen), None, facts)
            .then(section(
                l,
                tr(l, Msg::RunRows),
                Some(u64::try_from(run.rows.len()).unwrap_or(u64::MAX)),
                rows(l, run),
            ))
            .then(section(l, tr(l, Msg::RunDeletions), None, checked))
            .then(section(
                l,
                tr(l, Msg::RunRefusals),
                Some(u64::try_from(run.refusals.len()).unwrap_or(u64::MAX)),
                refusals(l, run),
            )),
    )
}

/// The rows `run` wrote, one line per CDM table.
fn rows(l: &Locale, run: &Run) -> Html {
    if run.rows.is_empty() {
        return quiet(tr(l, Msg::RunNoRows), None);
    }
    definitions(
        run.rows
            .iter()
            .map(|(table, rows)| (table.clone(), count(l, *rows)))
            .collect(),
    )
}

/// The records `run` refused, each with where and why.
fn refusals(l: &Locale, run: &Run) -> Html {
    if run.refusals.is_empty() {
        return quiet(tr(l, Msg::RunNoRefusals), None);
    }
    let columns = [
        Column::text("reason", tr(l, Msg::ColReason)).role(Role::Title),
        Column::text("composition", tr(l, Msg::ColComposition)),
        Column::text("mapping", tr(l, Msg::ColMapping)),
        Column::text("column", tr(l, Msg::ColColumn)),
        Column::text("element", tr(l, Msg::ColElement)),
    ];
    let [_, composition, mapping, column, element] = &columns;
    let shown = |value: Option<&str>| value.map_or_else(|| t("–"), code);
    let body = run
        .refusals
        .iter()
        .map(|refusal| {
            let place = match (refusal.table.as_deref(), refusal.column.as_deref()) {
                (Some(table), Some(column)) => Some(format!("{table}.{column}")),
                (Some(table), None) => Some(table.to_owned()),
                (None, column) => column.map(str::to_owned),
            };
            h(format!(
                "<tr>{}{}{}{}{}</tr>",
                title_cell(None, t(&refusal.reason), None),
                cell(composition, shown(refusal.composition.as_deref())),
                cell(mapping, shown(refusal.mapping.as_deref())),
                cell(column, shown(place.as_deref())),
                cell(element, shown(refusal.element.as_deref())),
            ))
        })
        .collect::<Html>();
    data_table(DataTable {
        caption: &tr(l, Msg::RunRefusals),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(body),
    })
}
