// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document compositions of one EHR, or of one CONTRIBUTION: the latest
//! version of every composition of a template a Composition context maps,
//! each bound to its Composition's logical id.

use std::collections::BTreeMap;

use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_its::rest::generated::query::AdhocQueryExecute;
use openehr_its::rest::generated::query::ResultSetRow;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_rm::v1_2::composition::composition::Composition;

use crate::cdr::CdrClient;
use crate::cdr::ids::EhrId;
use crate::cdr::query::PageSize;
use crate::facade::Facade;
use crate::facade::handlers::read::COMPOSITION;
use crate::facade::handlers::write::refuse;
use crate::facade::identity::FhirResourceId;
use crate::facade::identity::record::CompositionBinding;
use crate::facade::programs::Loaded;
use crate::facade::reply::Refusal;
use crate::facade::status;

/// The latest version of every composition of one template in one EHR.
// NOTE: AQL 1.1.0 §6 `versionPredicate`: under `VERSION [LATEST_VERSION]` no superseded or deleted
// version is read.
const BY_EHR: &str = "SELECT e/ehr_id/value, v/uid/value, v/contribution/id/value, \
v/commit_audit/time_committed/value, v/commit_audit/system_id, c \
FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
WHERE e/ehr_id/value = $ehr_id AND c/archetype_details/template_id/value = $template_id \
ORDER BY v/uid/value";

/// The latest version of every composition of one template one CONTRIBUTION
/// committed, in whichever EHR.
const BY_CONTRIBUTION: &str = "SELECT e/ehr_id/value, v/uid/value, v/contribution/id/value, \
v/commit_audit/time_committed/value, v/commit_audit/system_id, c \
FROM EHR e CONTAINS VERSION v[LATEST_VERSION] CONTAINS COMPOSITION c \
WHERE v/contribution/id/value = $contribution AND c/archetype_details/template_id/value = $template_id \
ORDER BY v/uid/value";

/// The rows one page of a listing asks for.
const PAGE: u32 = 50;

/// One listed document composition.
#[derive(Debug, Clone)]
pub(super) struct Listed {
    /// The program of the Composition context.
    pub(super) program: Loaded,
    /// The EHR the composition is in.
    pub(super) ehr_id: EhrId,
    /// The composition version.
    pub(super) version: ObjectVersionId,
    /// The CONTRIBUTION that committed the version.
    pub(super) contribution: String,
    /// When the version was committed, a FHIR `dateTime`.
    pub(super) committed: String,
    /// The system that committed the version.
    pub(super) system_id: String,
    /// The composition, canonical.
    pub(super) composition: CanonicalComposition,
    /// The logical id of its Composition.
    pub(super) id: FhirResourceId,
    /// What the identity map binds to the id.
    pub(super) binding: CompositionBinding,
}

/// What a listing is held to.
#[derive(Debug, Clone, Copy)]
pub(super) enum Scope<'a> {
    /// One EHR.
    Ehr(&'a EhrId),
    /// One CONTRIBUTION.
    Contribution(&'a str),
}

/// Returns the program of each template a Composition context maps, the
/// first context by name where two map one template.
fn document_programs(facade: &Facade) -> BTreeMap<String, Loaded> {
    let mut programs = BTreeMap::new();
    for loaded in facade.programs().loaded() {
        if loaded.program().resource().as_str() == COMPOSITION {
            programs
                .entry(loaded.index().template_id().to_owned())
                .or_insert_with(|| loaded.clone());
        }
    }
    programs
}

/// Returns the document compositions `scope` holds, template by template,
/// each bound to its Composition's logical id.
///
/// # Errors
///
/// The refusal a CDR answer or an identity-store failure renders as, and
/// the `502` of a row this server cannot read.
pub(super) async fn listed(
    facade: &Facade,
    client: &CdrClient,
    scope: Scope<'_>,
) -> Result<Vec<Listed>, Refusal> {
    let mut listed = Vec::new();
    for (template_id, program) in document_programs(facade) {
        let (query, name, value) = match scope {
            Scope::Ehr(ehr_id) => (BY_EHR, "ehr_id", ehr_id.as_str()),
            Scope::Contribution(contribution) => (BY_CONTRIBUTION, "contribution", contribution),
        };
        let request = AdhocQueryExecute {
            q: query.to_owned(),
            offset: Some(0),
            fetch: None,
            query_parameters: Some(BTreeMap::from([
                (
                    String::from(name),
                    serde_json::Value::String(value.to_owned()),
                ),
                (
                    String::from("template_id"),
                    serde_json::Value::String(template_id.clone()),
                ),
            ])),
            additional_properties: BTreeMap::new(),
        };
        let page = PageSize::new(PAGE).map_err(|error| super::internal(error.to_string()))?;
        let rows = crate::facade::handlers::search::every_row(client, request, page).await?;
        for (position, row) in rows.iter().enumerate() {
            listed.push(read_row(facade, client, &program, row, position).await?);
        }
    }
    Ok(listed)
}

/// Reads one row and binds its composition.
async fn read_row(
    facade: &Facade,
    client: &CdrClient,
    program: &Loaded,
    row: &ResultSetRow,
    position: usize,
) -> Result<Listed, Refusal> {
    let text = |column: usize, name: &str| {
        row.get(column)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| malformed(position, name))
    };
    let ehr_id = EhrId::new(&text(0, "ehr_id")?).map_err(|_bad| malformed(position, "ehr_id"))?;
    let version = ObjectVersionId::new(&text(1, "version uid")?)
        .map_err(|_bad| malformed(position, "version uid"))?;
    let contribution = text(2, "contribution")?;
    let committed = text(3, "time_committed")?;
    let system_id = text(4, "system_id")?;
    let composition = row
        .get(5)
        .and_then(|value| serde_json::to_string(value).ok())
        .and_then(|value| openehr_its::json::from_canonical_json::<Composition>(&value).ok())
        .ok_or_else(|| malformed(position, "composition"))?;
    let composition = crate::facade::handlers::read::version::canonical(
        facade,
        &ehr_id,
        &composition,
        program.index().template_id(),
    )?;
    let (id, binding) = facade
        .ingest(client.clone())
        .bind(program, COMPOSITION, &ehr_id, &version, &composition)
        .await?;
    Ok(Listed {
        program: program.clone(),
        ehr_id,
        version,
        contribution,
        committed,
        system_id,
        composition,
        id,
        binding,
    })
}

/// Returns the `502` a row the bridge cannot read answers; the row is
/// clinical content, so the refusal names the column alone.
fn malformed(position: usize, column: &str) -> Refusal {
    refuse(&status::Answer::new(
        status::UPSTREAM,
        format!(
            "the CDR answered row {position} of the document listing with no readable {column}"
        ),
    ))
}
