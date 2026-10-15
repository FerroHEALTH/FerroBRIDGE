// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ETL's AQL against the reference CDR, behind the `FERROBRIDGE_E2E` gate.
//!
//! Each case commits synthetic laboratory compositions over ITS-REST 1.1.0
//! and reads them back through the query shapes the bridge sends, paged the
//! way the runner pages them, so a CDR that answers a known AQL defect for one
//! of those shapes fails the case. The expectations come from QUERY AQL 1.1.0
//! (<https://specifications.openehr.org/releases/QUERY/Release-1.1.0/AQL.html>)
//! and the logical deletion of openEHR RM Common 1.1.0, never from what the
//! reference server answers.

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::Prefer;
use ferrobridge_server::cdr::commit::CommitContext;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::cdr::ids::{EhrId, template_id, versioned_object_uid};
use ferrobridge_server::cdr::query::PageSize;
use ferrobridge_server::etl::aql::CheckedQuery;
use ferrobridge_testkit::containers::{self, Cdr};
use ferrobridge_testkit::fixtures::{
    LABORATORY_REPORT_FLAT, LABORATORY_REPORT_OPT, LABORATORY_REPORT_TEMPLATE_ID,
};
use futures_util::StreamExt;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::generated::ehr::client::{
    CompositionCreateOutcome, CompositionDeleteOutcome, CompositionUpdateOutcome, EhrCreateOutcome,
};
use openehr_its::rest::generated::query::AdhocQueryExecute;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;
use openehr_rm::v1_2::composition::composition::Composition;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::error::Error;

/// The composition query of the OMOP round trip: each composition's latest
/// version, selected whole beside its EHR and version.
const COMPOSITIONS: &str = "SELECT e/ehr_id/value AS ehr_id, \
    v/uid/value AS version_uid, c AS composition \
    FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
    ORDER BY c/context/start_time/value";

/// The visit query of the OMOP round trip: three leaves of the one
/// `EVENT_CONTEXT` of each composition's latest version.
const VISITS: &str = "SELECT e/ehr_id/value AS ehr_id, \
    c/context/health_care_facility/name AS visit_source, \
    c/context/start_time/value AS visit_start, c/context/end_time/value AS visit_end \
    FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
    ORDER BY c/context/start_time/value";

/// The FLAT path of the laboratory result the fixture fills.
const RESULT: &str = "synthetic_laboratory_report/synthetic_laboratory_result:0";

/// One event of a synthetic laboratory result, holding one analyte.
#[derive(Debug, Clone, Copy)]
struct Event {
    /// The event time.
    time: &'static str,
    /// The analyte code, under `LOINC`.
    code: &'static str,
    /// The measured magnitude.
    magnitude: f64,
    /// The units.
    units: &'static str,
}

/// One synthetic laboratory composition.
#[derive(Debug, Clone, Copy)]
struct Laboratory {
    /// The context start and end times.
    context: (&'static str, &'static str),
    /// The health care facility of the context.
    facility: &'static str,
    /// The events of the one laboratory result.
    events: &'static [Event],
}

/// A composition whose result holds two events, each with a different
/// analyte, so a row that pairs a value of one with the other is wrong.
const TWO_EVENTS: Laboratory = Laboratory {
    context: ("2026-10-01T07:30:00Z", "2026-10-01T10:00:00Z"),
    facility: "synthetic-ward-a",
    events: &[
        Event {
            time: "2026-10-01T08:00:00Z",
            code: "SYN-LAB-1",
            magnitude: 7.25,
            units: "mmol/L",
        },
        Event {
            time: "2026-10-01T09:00:00Z",
            code: "SYN-LAB-2",
            magnitude: 140.0,
            units: "g/L",
        },
    ],
};

/// One event, for the compositions whose content the case does not read.
const ONE_EVENT: &[Event] = &[Event {
    time: "2026-10-02T08:00:00Z",
    code: "SYN-LAB-1",
    magnitude: 5.1,
    units: "mmol/L",
}];

/// The first version of the composition the deletion cases keep.
const KEPT: Laboratory = Laboratory {
    context: ("2026-10-02T07:30:00Z", "2026-10-02T09:00:00Z"),
    facility: "synthetic-ward-a",
    events: ONE_EVENT,
};

/// The second version of the kept composition, with its context corrected.
const KEPT_CORRECTED: Laboratory = Laboratory {
    context: ("2026-10-02T07:45:00Z", "2026-10-02T09:15:00Z"),
    facility: "synthetic-ward-a",
    events: ONE_EVENT,
};

/// The composition the deletion cases delete.
const DELETED: Laboratory = Laboratory {
    context: ("2026-10-03T07:30:00Z", "2026-10-03T09:00:00Z"),
    facility: "synthetic-ward-b",
    events: ONE_EVENT,
};

/// A CDR with the laboratory template and one EHR.
struct Harness {
    /// The container, held for the case's lifetime.
    _cdr: Cdr,
    /// The bridge's client.
    client: CdrClient,
    /// The EHR every composition is committed into.
    ehr: EhrId,
    /// The index of the laboratory template, which builds the compositions.
    index: WebTemplateIndex,
}

impl Harness {
    /// Starts the CDR, uploads the template and creates the EHR.
    async fn start() -> Result<Self, Box<dyn Error>> {
        let cdr = containers::cdr().await?;
        let response = reqwest::Client::new()
            .post(format!("{}/definition/template/adl1.4", cdr.base_url()))
            .header("Content-Type", "application/xml")
            .body(LABORATORY_REPORT_OPT)
            .send()
            .await?;
        assert_eq!(
            reqwest::StatusCode::CREATED,
            response.status(),
            "the CDR refused the synthetic template"
        );
        let client = CdrClient::new(&CdrConfig::new(format!("{}/", cdr.base_url()).parse()?))?;
        let ehr = match client.create_ehr(None, Prefer::Minimal).await?.outcome {
            EhrCreateOutcome::Created { headers, .. } => ferrobridge_server::cdr::ehr_id_from_etag(
                http::StatusCode::CREATED,
                headers.etag.as_deref(),
            )?,
            EhrCreateOutcome::NoContent { headers } => ferrobridge_server::cdr::ehr_id_from_etag(
                http::StatusCode::NO_CONTENT,
                headers.etag.as_deref(),
            )?,
            other => return Err(format!("the CDR refused the EHR: {other:?}").into()),
        };
        let index = WebTemplateIndex::build(&TemplateSource::opt14(LABORATORY_REPORT_OPT)?)?;
        Ok(Self {
            _cdr: cdr,
            client,
            ehr,
            index,
        })
    }

    /// Returns `laboratory` as the RM composition the client commits.
    fn composition(&self, laboratory: &Laboratory) -> Result<Composition, Box<dyn Error>> {
        Ok(openehr_its::json::from_canonical_json::<Composition>(
            &canonical(&self.index, laboratory)?.to_string(),
        )?)
    }

    /// Returns the commit context of a laboratory composition.
    fn commit_context() -> Result<CommitContext, Box<dyn Error>> {
        Ok(CommitContext {
            template_id: Some(template_id(LABORATORY_REPORT_TEMPLATE_ID)?),
            ..CommitContext::default()
        })
    }
}

/// Returns the canonical JSON of `laboratory`, built from the fixture's FLAT
/// keys with one analyte per event, and the context's end time and facility.
fn canonical(index: &WebTemplateIndex, laboratory: &Laboratory) -> Result<Value, Box<dyn Error>> {
    let mut flat: Map<String, Value> = serde_json::from_str(LABORATORY_REPORT_FLAT)?;
    flat.retain(|key, _| !key.contains("/any_event:"));
    flat.insert(
        String::from("synthetic_laboratory_report/context/start_time"),
        json!(laboratory.context.0),
    );
    for (position, event) in laboratory.events.iter().enumerate() {
        let at = format!("{RESULT}/any_event:{position}");
        let analyte = format!("{at}/synthetic_analyte:0");
        flat.insert(format!("{at}/time"), json!(event.time));
        flat.insert(format!("{analyte}/analyte_name|code"), json!(event.code));
        flat.insert(
            format!("{analyte}/analyte_name|value"),
            json!(format!("Synthetic analyte {}", event.code)),
        );
        flat.insert(
            format!("{analyte}/analyte_name|terminology"),
            json!("LOINC"),
        );
        flat.insert(
            format!("{analyte}/result_value|magnitude"),
            json!(event.magnitude),
        );
        flat.insert(format!("{analyte}/result_value|unit"), json!(event.units));
    }
    let mut value = index
        .build_from_flat(&flat, "2026-10-04T00:00:00Z")?
        .into_value();
    let context = value
        .get_mut("context")
        .and_then(Value::as_object_mut)
        .ok_or("the composition carries a context")?;
    context.insert(
        String::from("end_time"),
        json!({"_type": "DV_DATE_TIME", "value": laboratory.context.1}),
    );
    context.insert(
        String::from("health_care_facility"),
        json!({"_type": "PARTY_IDENTIFIED", "name": laboratory.facility}),
    );
    Ok(value)
}

impl Harness {
    /// Commits the first version of `laboratory`.
    async fn create(&self, laboratory: &Laboratory) -> Result<ObjectVersionId, Box<dyn Error>> {
        let (status, etag) = match self
            .client
            .create_composition(
                &self.ehr,
                &self.composition(laboratory)?,
                &Self::commit_context()?,
                Prefer::Minimal,
            )
            .await?
            .outcome
        {
            CompositionCreateOutcome::Created { headers, .. } => {
                (http::StatusCode::CREATED, headers.etag)
            }
            CompositionCreateOutcome::NoContent { headers } => {
                (http::StatusCode::NO_CONTENT, headers.etag)
            }
            other => return Err(format!("the CDR refused the composition: {other:?}").into()),
        };
        Ok(ferrobridge_server::cdr::version_from_etag(
            "composition_create",
            status,
            etag.as_deref(),
        )?)
    }

    /// Commits `laboratory` as the version after `preceding`.
    async fn update(
        &self,
        preceding: &ObjectVersionId,
        laboratory: &Laboratory,
    ) -> Result<ObjectVersionId, Box<dyn Error>> {
        let (status, etag) = match self
            .client
            .update_composition(
                &self.ehr,
                &versioned_object_uid(preceding),
                preceding,
                &self.composition(laboratory)?,
                &Self::commit_context()?,
                Prefer::Minimal,
            )
            .await?
            .outcome
        {
            CompositionUpdateOutcome::Ok { headers, .. } => (http::StatusCode::OK, headers.etag),
            CompositionUpdateOutcome::NoContent { headers } => {
                (http::StatusCode::NO_CONTENT, headers.etag)
            }
            other => return Err(format!("the CDR refused the new version: {other:?}").into()),
        };
        Ok(ferrobridge_server::cdr::version_from_etag(
            "composition_update",
            status,
            etag.as_deref(),
        )?)
    }

    /// Deletes the composition whose latest version is `latest`, and returns
    /// the deleting version the `ETag` names.
    async fn delete(&self, latest: &ObjectVersionId) -> Result<ObjectVersionId, Box<dyn Error>> {
        match self
            .client
            .delete_composition(&self.ehr, latest)
            .await?
            .outcome
        {
            CompositionDeleteOutcome::NoContent { headers } => {
                Ok(ferrobridge_server::cdr::optional_version_from_etag(
                    "composition_delete",
                    headers.etag.as_deref(),
                )?
                .ok_or("the delete names no deleting version")?)
            }
            other => Err(format!("the CDR refused the delete: {other:?}").into()),
        }
    }

    /// Commits the kept composition in two versions and deletes a second
    /// one, returning the kept composition's latest version and the deleting
    /// version of the other.
    async fn kept_and_deleted(&self) -> Result<(ObjectVersionId, ObjectVersionId), Box<dyn Error>> {
        let first = self.create(&KEPT).await?;
        let kept = self.update(&first, &KEPT_CORRECTED).await?;
        let doomed = self.create(&DELETED).await?;
        let deleting = self.delete(&doomed).await?;
        Ok((kept, deleting))
    }

    /// Returns every row of `query`, one row per page, as the runner pages a
    /// query (`etl::run` over `CdrClient::query_aql_rows`).
    async fn rows(&self, query: &CheckedQuery) -> Result<Vec<Vec<Value>>, Box<dyn Error>> {
        let request = AdhocQueryExecute {
            q: query.text().to_owned(),
            offset: None,
            fetch: None,
            query_parameters: None,
            additional_properties: BTreeMap::new(),
        };
        let stream = self.client.query_aql_rows(request, PageSize::new(1)?);
        let mut stream = std::pin::pin!(stream);
        let mut rows = Vec::new();
        while let Some(row) = stream.next().await {
            rows.push(row?);
        }
        Ok(rows)
    }
}

/// Returns the cell `projection` of `row`.
fn cell<'r>(query: &CheckedQuery, row: &'r [Value], projection: &str) -> &'r Value {
    query
        .column(projection)
        .and_then(|position| row.get(position))
        .unwrap_or(&Value::Null)
}

/// Calls `visit` on every object below `value`, in document order.
fn each_object(value: &Value, visit: &mut dyn FnMut(&Value)) {
    match value {
        Value::Object(object) => {
            visit(value);
            for member in object.values() {
                each_object(member, visit);
            }
        }
        Value::Array(items) => {
            for item in items {
                each_object(item, visit);
            }
        }
        _ => {}
    }
}

/// Returns, for every event of `composition`, its time and the analyte codes
/// and magnitudes found below it.
fn events(composition: &Value) -> Vec<(String, Vec<String>, Vec<String>)> {
    let mut found = Vec::new();
    each_object(composition, &mut |object| {
        let kind = object.get("_type").and_then(Value::as_str).unwrap_or("");
        if !matches!(kind, "POINT_EVENT" | "INTERVAL_EVENT" | "EVENT") {
            return;
        }
        let time = object["time"]["value"].as_str().unwrap_or("").to_owned();
        let mut codes = Vec::new();
        let mut magnitudes = Vec::new();
        each_object(object, &mut |inner| {
            let defining = &inner["defining_code"];
            if defining["terminology_id"]["value"] == "LOINC" {
                codes.push(defining["code_string"].as_str().unwrap_or("").to_owned());
            }
            if inner.get("_type").and_then(Value::as_str) == Some("DV_QUANTITY") {
                magnitudes.push(format!(
                    "{}",
                    inner["magnitude"].as_f64().unwrap_or(f64::NAN)
                ));
            }
        });
        found.push((time, codes, magnitudes));
    });
    found.sort();
    found
}

#[test]
fn the_queries_the_cases_send_check_as_the_runner_checks_them() {
    // The cases below read these texts through the runner's own checks, so a
    // text the runner would refuse cannot pass for a shape the bridge sends.
    CheckedQuery::compositions(COMPOSITIONS).expect("the composition query checks");
    CheckedQuery::visits(VISITS).expect("the visit query checks");
}

/// Returns what [`events`] must read from [`TWO_EVENTS`]: each event's
/// time with its own analyte.
fn two_events_expected() -> Vec<(String, Vec<String>, Vec<String>)> {
    TWO_EVENTS
        .events
        .iter()
        .map(|event| {
            (
                event.time.to_owned(),
                vec![event.code.to_owned()],
                vec![format!("{}", event.magnitude)],
            )
        })
        .collect()
}

#[test]
fn the_two_event_composition_holds_each_analyte_under_its_own_event() -> Result<(), Box<dyn Error>>
{
    // The container case compares the CDR's answer with this reading, so the
    // fixture and the reader are checked offline first.
    let index = WebTemplateIndex::build(&TemplateSource::opt14(LABORATORY_REPORT_OPT)?)?;
    let value = canonical(&index, &TWO_EVENTS)?;
    openehr_its::json::from_canonical_json::<Composition>(&value.to_string())?;
    assert_eq!(two_events_expected(), events(&value));
    Ok(())
}

#[tokio::test]
async fn the_composition_query_answers_repeated_events_whole_and_each_value_with_its_own_event()
-> Result<(), Box<dyn Error>> {
    // NOTE: QUERY AQL 1.1.0 §3.13.4.2 returns "all data items selected" as rows; the
    // runner selects the composition whole, so no row pairs leaves of two events.
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let harness = Harness::start().await?;
    let version = harness.create(&TWO_EVENTS).await?;
    let query = CheckedQuery::compositions(COMPOSITIONS)?;
    let rows = harness.rows(&query).await?;
    assert_eq!(1, rows.len(), "one composition, one row: {rows:?}");
    let row = rows.first().ok_or("one row")?;
    assert_eq!(json!(harness.ehr.as_str()), *cell(&query, row, "ehr_id"));
    assert_eq!(json!(version.value()), *cell(&query, row, "version_uid"));
    assert_eq!(
        two_events_expected(),
        events(cell(&query, row, "composition")),
        "each analyte comes back under the event it was measured in"
    );
    Ok(())
}

#[tokio::test]
async fn the_visit_query_reads_the_three_context_leaves_of_one_composition_in_one_row()
-> Result<(), Box<dyn Error>> {
    // NOTE: openEHR RM EHR 1.1.0 §5.2.3.1 makes the event context optional and single,
    // so the three leaves of one row name one EVENT_CONTEXT (QUERY AQL 1.1.0 §3.13.4).
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let harness = Harness::start().await?;
    harness.create(&TWO_EVENTS).await?;
    let query = CheckedQuery::visits(VISITS)?;
    let rows = harness.rows(&query).await?;
    assert_eq!(1, rows.len(), "one composition, one visit row: {rows:?}");
    let row = rows.first().ok_or("one row")?;
    assert_eq!(json!(harness.ehr.as_str()), *cell(&query, row, "ehr_id"));
    assert_eq!(
        json!(TWO_EVENTS.facility),
        *cell(&query, row, "visit_source")
    );
    assert_eq!(
        json!(TWO_EVENTS.context.0),
        *cell(&query, row, "visit_start")
    );
    assert_eq!(json!(TWO_EVENTS.context.1), *cell(&query, row, "visit_end"));
    Ok(())
}

#[tokio::test]
async fn the_composition_query_answers_the_latest_version_and_leaves_out_a_deleted_composition()
-> Result<(), Box<dyn Error>> {
    // NOTE: openEHR RM Common 1.1.0 §6.2.5 records a deletion as a version whose data is
    // Void, so the newest version (`LATEST_VERSION`, QUERY AQL 1.1.0 §6) holds no COMPOSITION.
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let harness = Harness::start().await?;
    let (kept, _deleting) = harness.kept_and_deleted().await?;
    let query = CheckedQuery::compositions(COMPOSITIONS)?;
    let rows = harness.rows(&query).await?;
    let versions: Vec<&Value> = rows
        .iter()
        .map(|row| cell(&query, row, "version_uid"))
        .collect();
    assert_eq!(
        vec![&json!(kept.value())],
        versions,
        "only the kept composition, at its latest version"
    );
    let row = rows.first().ok_or("one row")?;
    assert!(
        cell(&query, row, "composition").is_object(),
        "the composition comes back whole"
    );
    Ok(())
}

#[tokio::test]
async fn the_visit_query_answers_the_latest_context_and_leaves_out_a_deleted_composition()
-> Result<(), Box<dyn Error>> {
    // NOTE: openEHR RM Common 1.1.0 §6.2.5 (deletion leaves Void data); a superseded or
    // deleted composition's context must not shape a visit under `LATEST_VERSION`.
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let harness = Harness::start().await?;
    harness.kept_and_deleted().await?;
    let query = CheckedQuery::visits(VISITS)?;
    let rows = harness.rows(&query).await?;
    let spans: Vec<(&Value, &Value, &Value)> = rows
        .iter()
        .map(|row| {
            (
                cell(&query, row, "visit_source"),
                cell(&query, row, "visit_start"),
                cell(&query, row, "visit_end"),
            )
        })
        .collect();
    assert_eq!(
        vec![(
            &json!(KEPT_CORRECTED.facility),
            &json!(KEPT_CORRECTED.context.0),
            &json!(KEPT_CORRECTED.context.1),
        )],
        spans,
        "only the corrected context of the kept composition"
    );
    Ok(())
}
