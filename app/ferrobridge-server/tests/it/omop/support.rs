// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the OMOP end-to-end cases share: the three synthetic laboratory
//! compositions and their canonical JSON, the template upload and the
//! commits over ITS-REST 1.1.0, the OMOCL laboratory files copied verbatim
//! from `docs/specs/omocl`, the CDM built from the vendored DDL with the
//! synthetic vocabulary loaded, the configuration and the `etl run` call.

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::Prefer;
use ferrobridge_server::cdr::commit::CommitContext;
use ferrobridge_server::cdr::ids::{EhrId, template_id, versioned_object_uid};
use ferrobridge_server::etl::RunOptions;
use ferrobridge_server::etl::report::RunReport;
use ferrobridge_testkit::containers::{Cdr, Postgres};
use ferrobridge_testkit::fixtures::{
    LABORATORY_REPORT_FLAT, LABORATORY_REPORT_OPT, LABORATORY_REPORT_TEMPLATE_ID,
};
use omop_cdm::database::{self, CdmPool};
use omop_cdm::ddl::SchemaName;
use omop_cdm::writer::{CdmWriter, input::PersonPolicy};
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::generated::ehr::client::EhrCreateOutcome;
use openehr_its::rest::generated::ehr::client::{
    CompositionCreateOutcome, CompositionUpdateOutcome,
};
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_rm::v1_2::composition::composition::Composition;
use serde_json::{Map, Value, json};
use std::error::Error;
use std::path::Path;

/// The two published laboratory files the acceptance test names, and the
/// specimen file the result file includes.
const LABORATORY_FILES: [&str; 3] = [
    "medical_data/observation/Laboratory_test_result_v1.yml",
    "medical_data/cluster/Laboratory_test_analyte_v1.yml",
    "medical_data/cluster/Specimen_v1.yml",
];

/// The composition query: each composition's latest version, whole.
const COMPOSITIONS: &str = "SELECT e/ehr_id/value AS ehr_id, \
    v/uid/value AS version_uid, c AS composition \
    FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
    ORDER BY c/context/start_time/value";

/// The visit query: one visit per EHR and health care facility, spanning the
/// contexts of the latest version of each composition recorded there, so a
/// superseded or deleted version shapes no visit.
const VISITS: &str = "SELECT e/ehr_id/value AS ehr_id, \
    c/context/health_care_facility/name AS visit_source, \
    c/context/start_time/value AS visit_start, c/context/end_time/value AS visit_end \
    FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
    ORDER BY c/context/start_time/value";

/// The `*_type_concept_id` the run writes, a synthetic record type.
const TYPE_CONCEPT: i32 = 5101;

/// The `period_type_concept_id` of every observation period.
const PERIOD_TYPE_CONCEPT: i32 = 5102;

/// The `visit_concept_id` of every visit.
const VISIT_CONCEPT: i32 = 5001;

/// The FLAT path of the one event of the laboratory result.
const EVENT: &str = "synthetic_laboratory_report/synthetic_laboratory_result:0/any_event:0";

/// One synthetic analyte.
#[derive(Debug, Clone, Copy)]
pub(super) struct Analyte {
    /// The analyte code, under the `LOINC` terminology.
    code: &'static str,
    /// The measured magnitude.
    pub(super) magnitude: f64,
    /// The units.
    units: &'static str,
    /// The analyte's own result time, when it carries one.
    time: Option<&'static str>,
    /// The normal range, in the value's units.
    range: Option<(f64, f64)>,
    /// The `magnitude_status`, when the value is not a point value.
    status: Option<&'static str>,
}

/// One synthetic laboratory composition.
#[derive(Debug, Clone, Copy)]
pub(super) struct Laboratory {
    /// The name the snapshot shows for it.
    pub(super) label: &'static str,
    /// Which of the two EHRs holds it.
    pub(super) ehr: usize,
    /// The health care facility of its context, the visit it belongs to.
    facility: &'static str,
    /// The context start and end times.
    context: (&'static str, &'static str),
    /// The event time.
    event: &'static str,
    /// The two analytes.
    pub(super) analytes: [Analyte; 2],
    /// Whether it carries the specimen.
    specimen: bool,
}

/// The three compositions the round trip commits.
pub(super) const LABORATORIES: [Laboratory; 3] = [
    Laboratory {
        label: "lab-1",
        ehr: 0,
        facility: "synthetic-ward-a",
        context: ("2026-09-20T07:30:00Z", "2026-09-20T12:00:00Z"),
        event: "2026-09-20T08:00:00Z",
        analytes: [
            Analyte {
                code: "SYN-LAB-1",
                magnitude: 7.25,
                units: "mmol/L",
                time: Some("2026-09-20T09:15:00Z"),
                range: Some((3.5, 6.5)),
                status: None,
            },
            Analyte {
                code: "SYN-LAB-2",
                magnitude: 140.0,
                units: "g/L",
                time: None,
                range: Some((120.0, 160.0)),
                status: Some("<"),
            },
        ],
        specimen: true,
    },
    Laboratory {
        label: "lab-2",
        ehr: 0,
        facility: "synthetic-ward-a",
        context: ("2026-09-21T08:00:00Z", "2026-09-21T09:00:00Z"),
        event: "2026-09-21T08:10:00Z",
        analytes: [
            Analyte {
                code: "SYN-LAB-1",
                magnitude: 5.1,
                units: "mmol/L",
                time: None,
                range: None,
                status: Some("="),
            },
            Analyte {
                code: "SYN-LAB-9",
                magnitude: 2.2,
                units: "mmol/L",
                time: None,
                range: None,
                status: Some(">="),
            },
        ],
        specimen: false,
    },
    Laboratory {
        label: "lab-3",
        ehr: 1,
        facility: "synthetic-ward-b",
        context: ("2026-09-22T10:00:00Z", "2026-09-22T11:00:00Z"),
        event: "2026-09-22T10:05:00Z",
        analytes: [
            Analyte {
                code: "SYN-LAB-2",
                magnitude: 150.0,
                units: "g/L",
                time: None,
                range: None,
                status: None,
            },
            Analyte {
                code: "SYN-LAB-1",
                magnitude: 6.0,
                units: "mmol/L",
                time: Some("2026-09-22T10:30:00Z"),
                range: Some((3.5, 6.5)),
                status: None,
            },
        ],
        specimen: false,
    },
];

/// Returns the FLAT form of `laboratory`, from the fixture's FLAT keys.
fn flat(laboratory: &Laboratory) -> Result<Map<String, Value>, Box<dyn Error>> {
    let mut flat: Map<String, Value> = serde_json::from_str(LABORATORY_REPORT_FLAT)?;
    flat.retain(|key, _| {
        !key.contains("/synthetic_analyte:") && (laboratory.specimen || !key.contains("specimen"))
    });
    flat.insert(
        String::from("synthetic_laboratory_report/context/start_time"),
        json!(laboratory.context.0),
    );
    flat.insert(format!("{EVENT}/time"), json!(laboratory.event));
    for (index, analyte) in laboratory.analytes.iter().enumerate() {
        let at = format!("{EVENT}/synthetic_analyte:{index}");
        flat.insert(format!("{at}/analyte_name|code"), json!(analyte.code));
        flat.insert(
            format!("{at}/analyte_name|value"),
            json!(format!("Synthetic analyte {}", analyte.code)),
        );
        flat.insert(format!("{at}/analyte_name|terminology"), json!("LOINC"));
        flat.insert(
            format!("{at}/result_value|magnitude"),
            json!(analyte.magnitude),
        );
        flat.insert(format!("{at}/result_value|unit"), json!(analyte.units));
        if let Some(time) = analyte.time {
            flat.insert(format!("{at}/result_time"), json!(time));
        }
    }
    Ok(flat)
}

/// Calls `edit` on every `DV_QUANTITY` below `value`, in document order.
fn each_quantity(value: &mut Value, edit: &mut dyn FnMut(&mut Map<String, Value>)) {
    match value {
        Value::Object(object) => {
            if object.get("_type").and_then(Value::as_str) == Some("DV_QUANTITY") {
                edit(object);
            } else {
                for member in object.values_mut() {
                    each_quantity(member, edit);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                each_quantity(item, edit);
            }
        }
        _ => {}
    }
}

/// Returns the canonical JSON of `laboratory`: the FLAT build, with the
/// context's end time and facility, and each analyte's range and status.
pub(super) fn canonical(
    index: &WebTemplateIndex,
    laboratory: &Laboratory,
) -> Result<Value, Box<dyn Error>> {
    let mut value = index
        .build_from_flat(&flat(laboratory)?, "2026-09-25T00:00:00Z")?
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
    let mut analytes = laboratory.analytes.iter();
    each_quantity(&mut value, &mut |quantity| {
        let Some(analyte) = analytes.next() else {
            return;
        };
        if let Some(status) = analyte.status {
            quantity.insert(String::from("magnitude_status"), json!(status));
        }
        if let Some((low, high)) = analyte.range {
            quantity.insert(
                String::from("normal_range"),
                json!({
                    "_type": "DV_INTERVAL",
                    "lower": {"_type": "DV_QUANTITY", "magnitude": low, "units": analyte.units},
                    "upper": {"_type": "DV_QUANTITY", "magnitude": high, "units": analyte.units},
                    "lower_unbounded": false,
                    "upper_unbounded": false,
                    "lower_included": true,
                    "upper_included": true
                }),
            );
        }
    });
    Ok(value)
}

/// Returns `value` as the RM composition the client commits.
fn composition(value: &Value) -> Result<Composition, Box<dyn Error>> {
    Ok(openehr_its::json::from_canonical_json::<Composition>(
        &value.to_string(),
    )?)
}

/// Returns the commit context of a laboratory composition.
fn commit() -> Result<CommitContext, Box<dyn Error>> {
    Ok(CommitContext {
        template_id: Some(template_id(LABORATORY_REPORT_TEMPLATE_ID)?),
        ..CommitContext::default()
    })
}

/// Uploads the laboratory template through the CDR's own route.
pub(super) async fn upload_template(cdr: &Cdr) -> Result<(), Box<dyn Error>> {
    let response = reqwest::Client::new()
        .post(format!("{}/definition/template/adl1.4", cdr.base_url()))
        .header("Content-Type", "application/xml")
        .body(LABORATORY_REPORT_OPT)
        .send()
        .await?;
    let status = response.status();
    if status != reqwest::StatusCode::CREATED {
        return Err(format!(
            "the CDR refused the template: {status} {}",
            response.text().await?
        )
        .into());
    }
    Ok(())
}

/// Creates an EHR and returns its identifier.
pub(super) async fn create_ehr(client: &CdrClient) -> Result<EhrId, Box<dyn Error>> {
    match client.create_ehr(None, Prefer::Minimal).await?.outcome {
        EhrCreateOutcome::Created { headers, .. } => Ok(ferrobridge_server::cdr::ehr_id_from_etag(
            http::StatusCode::CREATED,
            headers.etag.as_deref(),
        )?),
        EhrCreateOutcome::NoContent { headers } => Ok(ferrobridge_server::cdr::ehr_id_from_etag(
            http::StatusCode::NO_CONTENT,
            headers.etag.as_deref(),
        )?),
        other => Err(format!("the CDR refused the EHR: {other:?}").into()),
    }
}

/// Commits the first version of `value` into `ehr`.
pub(super) async fn create(
    client: &CdrClient,
    ehr: &EhrId,
    value: &Value,
) -> Result<ObjectVersionId, Box<dyn Error>> {
    match client
        .create_composition(ehr, &composition(value)?, &commit()?, Prefer::Minimal)
        .await?
        .outcome
    {
        CompositionCreateOutcome::Created { headers, .. } => {
            Ok(ferrobridge_server::cdr::version_from_etag(
                "composition_create",
                http::StatusCode::CREATED,
                headers.etag.as_deref(),
            )?)
        }
        CompositionCreateOutcome::NoContent { headers } => {
            Ok(ferrobridge_server::cdr::version_from_etag(
                "composition_create",
                http::StatusCode::NO_CONTENT,
                headers.etag.as_deref(),
            )?)
        }
        other => Err(format!("the CDR refused the composition: {other:?}").into()),
    }
}

/// Commits `value` as the next version after `preceding`.
pub(super) async fn update(
    client: &CdrClient,
    ehr: &EhrId,
    preceding: &ObjectVersionId,
    value: &Value,
) -> Result<ObjectVersionId, Box<dyn Error>> {
    match client
        .update_composition(
            ehr,
            &versioned_object_uid(preceding),
            preceding,
            &composition(value)?,
            &commit()?,
            Prefer::Minimal,
        )
        .await?
        .outcome
    {
        CompositionUpdateOutcome::Ok { headers, .. } => {
            Ok(ferrobridge_server::cdr::version_from_etag(
                "composition_update",
                http::StatusCode::OK,
                headers.etag.as_deref(),
            )?)
        }
        CompositionUpdateOutcome::NoContent { headers } => {
            Ok(ferrobridge_server::cdr::version_from_etag(
                "composition_update",
                http::StatusCode::NO_CONTENT,
                headers.etag.as_deref(),
            )?)
        }
        other => Err(format!("the CDR refused the new version: {other:?}").into()),
    }
}

/// Copies the laboratory files from the vendored corpus into `directory`,
/// checking that every copied byte equals the vendored one.
pub(super) fn copy_mappings(directory: &Path) -> Result<(), Box<dyn Error>> {
    let corpus = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/specs/omocl"
    ));
    for file in LABORATORY_FILES {
        let from = corpus.join(file);
        let to = directory.join(from.file_name().ok_or("a corpus file name")?);
        std::fs::copy(&from, &to)?;
        assert_eq!(
            std::fs::read(&from)?,
            std::fs::read(&to)?,
            "{file} is copied verbatim"
        );
    }
    Ok(())
}

/// Writes the configuration `etl run` reads: both containers, the two
/// queries, the configured concepts, and the OMOCL directory.
pub(super) fn configuration(
    directory: &Path,
    cdr: &Cdr,
    postgres: &Postgres,
    mappings: &Path,
) -> Result<std::path::PathBuf, Box<dyn Error>> {
    let text = format!(
        "[cdr]\nbase_url = \"{cdr}/\"\n\n\
         [cdm]\nurl = \"{cdm}\"\n\n\
         [etl]\naql = \"{COMPOSITIONS}\"\ntype_concept_id = {TYPE_CONCEPT}\n\
         observation_period_type_concept_id = {PERIOD_TYPE_CONCEPT}\n\n\
         [etl.visits]\naql = \"{VISITS}\"\nvisit_concept_id = {VISIT_CONCEPT}\n\
         visit_type_concept_id = {TYPE_CONCEPT}\n\n\
         [mappings]\nomocl = \"{omocl}\"\n",
        cdr = cdr.base_url(),
        cdm = postgres.url(),
        omocl = mappings.display(),
    );
    let path = directory.join("ferrobridge.toml");
    std::fs::write(&path, text)?;
    Ok(path)
}

/// Builds the CDM schema and the bridge schema, and loads the synthetic
/// vocabulary.
pub(super) async fn build_cdm(postgres: &Postgres) -> Result<(), Box<dyn Error>> {
    let pool = CdmPool::connect(
        sqlx::postgres::PgPoolOptions::new().max_connections(1),
        postgres.url().parse()?,
        SchemaName::new("cdm")?,
    )
    .await?;
    database::init(&pool).await?;
    let download = omop_cdm::athena::Export::read(&ferrobridge_testkit::vocabulary::directory())?;
    omop_cdm::athena::load(&pool, &download).await?;
    pool.pool().close().await;
    let mut writer = CdmWriter::connect(
        postgres.url(),
        SchemaName::new("cdm")?,
        SchemaName::new("ferrobridge")?,
        PersonPolicy::CreateOnFirstSight,
    )
    .await?;
    writer.init().await?;
    Ok(())
}

/// Runs `etl run` once through the job the binary calls.
pub(super) async fn etl_run(config: &Path) -> Result<RunReport, Box<dyn Error>> {
    let settings = ferrobridge_server::config::Config::load(Some(config))?.resolve()?;
    Ok(ferrobridge_server::etl::job::run(&settings, &RunOptions::default()).await?)
}

/// Returns every row `query` answers, each cell rendered as text or `NULL`.
pub(super) async fn rows(
    postgres: &Postgres,
    query: &str,
) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    let (client, connection) =
        tokio_postgres::connect(postgres.url(), tokio_postgres::NoTls).await?;
    tokio::spawn(connection);
    let answered = client.simple_query(query).await?;
    let mut rows = Vec::new();
    for message in answered {
        if let tokio_postgres::SimpleQueryMessage::Row(row) = message {
            rows.push(
                (0..row.len())
                    .map(|column| row.get(column).unwrap_or("NULL").to_owned())
                    .collect(),
            );
        }
    }
    Ok(rows)
}
