// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `etl fhir`: FHIR R4 resources into the OMOP CDM through an in-memory
//! openEHR composition, with no CDR (`docs/architecture.md` §5.6).
//!
//! Each resource goes alone, as a Bundle of one entry, through the
//! FHIRconnect `$toopenehr` run of the configured mapping set, which selects
//! the program its type and profiles answer. The composition it answers is
//! mapped by the OMOCL engine and committed through the CDM writer as a
//! composition the CDR served would be, and is never stored anywhere else.
//!
//! A composition built in memory has no identifiers a CDR issued, so they are
//! derived: the versioned composition is a name-based UUID of the resource's
//! type and id, the version tree id is `meta.versionId` when it is a positive
//! integer and `1` otherwise, and the EHR is a name-based UUID of the
//! reference the resource names its subject by (a Patient names itself). A
//! re-run over the same input therefore commits the same compositions, and
//! each resource keeps its rows across runs. No specification governs the
//! chaining or the identifiers: our own design.

use std::path::Path;
use std::path::PathBuf;

use fhir_types::codec::Json;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::library::Library;
use fhirconnect::engine::seam::Seams;
use fhirconnect::operations::contract::ToOpenehrRequest;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::programs::ProgramSet;
use fhirconnect::resolve::program::binding::TemplateId;
use omop_cdm::graph::key::Source;
use omop_cdm::graph::report::Refusal;
use omop_cdm::writer::CdmWriter;
use omop_cdm::writer::input::RunId;
use omop_cdm::writer::origin::ResourceOrigin;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::etl::EtlSettings;
use crate::etl::Mapper;
use crate::etl::MappingContext;
use crate::etl::RunError;
use crate::etl::SourceComposition;
use crate::etl::report::RunReport;

/// The `creating_system_id` of every composition version the run derives.
pub const SYSTEM_ID: &str = "ferrobridge.fhir";

/// The namespace of the name-based UUIDs the run derives (RFC 9562 §5.5).
///
/// A fixed random value of the bridge's own, so the derived identifiers are
/// stable across runs and installations.
const NAMESPACE: Uuid = Uuid::from_u128(0x6f2b_9c4e_1a73_4d58_b0e2_8c91_7d3a_5f16);

/// The resources a run read, and what each came to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Resources {
    /// The resources read from the input.
    pub read: u64,
    /// The resources a FHIRconnect program mapped into a composition.
    pub mapped: u64,
    /// The resources no FHIRconnect program maps.
    pub unmapped: u64,
    /// The resources the FHIRconnect run refused, or that carry no id or no
    /// subject.
    pub refused: u64,
    /// The losses the FHIRconnect run declared over every mapped resource.
    pub losses: u64,
}

impl std::fmt::Display for Resources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "resources: {} read, {} mapped, {} unmapped, {} refused, {} FHIRconnect losses",
            self.read, self.mapped, self.unmapped, self.refused, self.losses
        )
    }
}

/// An input the run cannot read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InputError {
    /// The file could not be read.
    #[error("{path} could not be read")]
    Read {
        /// The file.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A line of an NDJSON file is no JSON.
    #[error("line {line} of {path} is no JSON")]
    Json {
        /// The file.
        path: PathBuf,
        /// The line, counted from 1.
        line: usize,
        /// What the parser reported.
        #[source]
        source: serde_json::Error,
    },
    /// A document of the input is no FHIR resource.
    #[error("item {item} of {path} is no FHIR resource: it carries no `resourceType`")]
    NotAResource {
        /// The file.
        path: PathBuf,
        /// The resource, counted from 1.
        item: usize,
    },
}

/// Reads the resources of a FHIR R4 Bundle, of one resource, or of an NDJSON
/// file of resources at `path`, in input order.
///
/// A document that parses whole is a Bundle, whose entries' resources are
/// read, or one resource; anything else is read as NDJSON, one resource per
/// non-blank line (the FHIR Bulk Data format, <https://hl7.org/fhir/uv/bulkdata/>).
///
/// # Errors
///
/// Returns [`InputError`] naming the file and the item that does not read.
pub fn read_input(path: &Path) -> Result<Vec<Value>, InputError> {
    let text = std::fs::read_to_string(path).map_err(|source| InputError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let resources = match serde_json::from_str::<Value>(&text) {
        Ok(document) if resource_type(&document) == Some("Bundle") => document
            .get("entry")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("resource").cloned())
                    .collect()
            })
            .unwrap_or_default(),
        Ok(document) => vec![document],
        Err(_whole) => {
            let mut resources = Vec::new();
            for (index, line) in text.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let resource = serde_json::from_str(line).map_err(|source| InputError::Json {
                    path: path.to_path_buf(),
                    line: index.saturating_add(1),
                    source,
                })?;
                resources.push(resource);
            }
            resources
        }
    };
    for (index, resource) in resources.iter().enumerate() {
        if resource_type(resource).is_none() {
            return Err(InputError::NotAResource {
                path: path.to_path_buf(),
                item: index.saturating_add(1),
            });
        }
    }
    Ok(resources)
}

/// Returns the `resourceType` of `resource`.
fn resource_type(resource: &Value) -> Option<&str> {
    resource.get("resourceType").and_then(Value::as_str)
}

/// The FHIRconnect half of a run: the mapping set and the composition
/// defaults the operations lane fills in.
#[derive(Debug)]
pub struct FhirSide<'a> {
    /// The compiled FHIRconnect mapping set.
    pub programs: &'a ProgramSet,
    /// The engine's own `Device` reference.
    pub device: &'a str,
    /// The composer a run fills in when no mapping does.
    pub composer: &'a str,
    /// The composition language, when configured.
    pub language: Option<&'a str>,
    /// The composition territory, when configured.
    pub territory: Option<&'a str>,
    /// The time a resource without `meta.lastUpdated` is mapped at.
    pub now: &'a str,
}

impl FhirSide<'_> {
    /// Returns the run settings for `resource`, stamped with its
    /// `meta.lastUpdated` when it carries one, so a re-run maps it alike.
    fn settings(&self, resource: &Value) -> fhirconnect::operations::run::Settings {
        let now = resource
            .pointer("/meta/lastUpdated")
            .and_then(Value::as_str)
            .unwrap_or(self.now);
        crate::state::run_settings(
            self.device,
            self.composer,
            self.language,
            self.territory,
            now,
        )
    }
}

/// Returns the FHIR resource `resource` names, of type `kind`, as its rows'
/// lineage records it, or `None` when it carries no id.
fn origin_of(resource: &Value, kind: &str) -> Option<ResourceOrigin> {
    let text = |pointer: &str| {
        resource
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    Some(ResourceOrigin {
        resource_type: kind.to_owned(),
        id: text("/id")?,
        version: text("/meta/versionId"),
    })
}

/// Returns the composition version `resource` becomes, or why it cannot be
/// named.
fn source_of(resource: &Value, kind: &str) -> Result<Source, String> {
    let id = resource
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| format!("the {kind} resource carries no id"))?;
    let subject = if kind == "Patient" {
        format!("Patient/{id}")
    } else {
        ["subject", "patient"]
            .into_iter()
            .find_map(|name| resource.pointer(&format!("/{name}/reference")))
            .and_then(Value::as_str)
            .filter(|reference| !reference.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("the {kind} resource {kind}/{id} names no subject"))?
    };
    let ehr = Uuid::new_v5(&NAMESPACE, subject.as_bytes()).to_string();
    let composition = Uuid::new_v5(&NAMESPACE, format!("{kind}/{id}").as_bytes()).to_string();
    let version = resource
        .pointer("/meta/versionId")
        .and_then(Value::as_str)
        .and_then(|version| version.parse::<u32>().ok())
        .filter(|version| *version > 0)
        .unwrap_or(1);
    let ehr_id = HierObjectId::new(ehr.as_str()).map_err(|error| error.to_string())?;
    let version_uid = ObjectVersionId::new(format!("{composition}::{SYSTEM_ID}::{version}"))
        .map_err(|error| error.to_string())?;
    Ok(Source::new(ehr_id, version_uid))
}

/// Returns the Bundle of the one entry `resource`.
fn bundle_of(resource: &Value) -> Result<fhir_types::r4::bundle::Bundle, String> {
    let document = serde_json::json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [{ "resource": resource }]
    });
    let fhir_types::codec::Value::Object(object) =
        fhir_types::codec::Value::from_serde_json(document)
    else {
        return Err(String::from("the Bundle is no JSON object"));
    };
    fhir_types::r4::bundle::Bundle::from_json(&object, &mut fhir_types::codec::Path::root("Bundle"))
        .map_err(|error| {
            format!(
                "the resource does not read as FHIR R4: {}",
                crate::chain(&error)
            )
        })
}

/// What the FHIRconnect half made of one resource.
#[derive(Debug)]
pub enum Outcome {
    /// A composition, its version and its template.
    Mapped {
        /// The canonical composition.
        composition: Value,
        /// The template it was built against.
        template: TemplateId,
        /// The losses the run declared.
        losses: u64,
    },
    /// No program maps the resource.
    Unmapped(String),
    /// The run refused it.
    Refused(String),
}

/// Runs the FHIRconnect half over one resource: the program its profile
/// selects maps it into a canonical composition, held in memory.
#[must_use]
pub fn composition_of(side: &FhirSide<'_>, resource: &Value) -> Outcome {
    let bundle = match bundle_of(resource) {
        Ok(bundle) => bundle,
        Err(reason) => return Outcome::Refused(reason),
    };
    let request = ToOpenehrRequest::new(bundle);
    let seams = Seams::default().with_functions(&Library);
    let transformed = match fhirconnect::operations::run::to_openehr_with_warnings(
        side.programs,
        &SCHEMAS,
        &seams,
        &side.settings(resource),
        &request,
    ) {
        Ok(transformed) => transformed,
        Err(error @ OperationError::Select { .. }) => {
            return Outcome::Unmapped(crate::chain(&error));
        }
        Err(error) => return Outcome::Refused(crate::chain(&error)),
    };
    let losses = u64::try_from(transformed.warnings().len()).unwrap_or(u64::MAX);
    let response = transformed.into_response();
    let composition: Value = match serde_json::from_str(response.composition()) {
        Ok(composition) => composition,
        Err(error) => {
            return Outcome::Refused(format!("the composition is no JSON: {error}"));
        }
    };
    let template = composition
        .pointer("/archetype_details/template_id/value")
        .and_then(Value::as_str)
        .map(TemplateId::new);
    match template {
        Some(template) => Outcome::Mapped {
            composition,
            template,
            losses,
        },
        None => Outcome::Refused(String::from("the composition names no template")),
    }
}

/// Loads `resources` into the CDM through `side`, `mapper` and `writer`,
/// rebuilds the derived tables, and returns the run's report.
///
/// A resource no program maps, one the FHIRconnect run refuses, and a
/// composition OMOCL refuses are each counted in the report and the run goes
/// on; the visit derivation needs a CDR, so the run derives no visit.
///
/// # Errors
///
/// Returns [`RunError::Database`] when the CDM database stops the run.
pub async fn run<M: Mapper>(
    resources: &[Value],
    side: &FhirSide<'_>,
    settings: &EtlSettings,
    writer: &mut CdmWriter,
    mapper: &M,
    run_id: &RunId,
) -> Result<RunReport, RunError> {
    let mut report = RunReport::new(run_id.as_str());
    let mut counts = Resources::default();
    let context = MappingContext {
        type_concept_id: settings.type_concept_id,
    };
    for resource in resources {
        counts.read = counts.read.saturating_add(1);
        let kind = resource_type(resource).unwrap_or_default();
        let source = match source_of(resource, kind) {
            Ok(source) => source,
            Err(reason) => {
                counts.refused = counts.refused.saturating_add(1);
                report.composition_refused(None, &Refusal::new(reason));
                continue;
            }
        };
        let (composition, template, losses) = match composition_of(side, resource) {
            Outcome::Mapped {
                composition,
                template,
                losses,
            } => (composition, template, losses),
            Outcome::Unmapped(reason) => {
                counts.unmapped = counts.unmapped.saturating_add(1);
                report.composition_refused(Some(&source), &Refusal::new(reason));
                continue;
            }
            Outcome::Refused(reason) => {
                counts.refused = counts.refused.saturating_add(1);
                report.composition_refused(Some(&source), &Refusal::new(reason));
                continue;
            }
        };
        counts.mapped = counts.mapped.saturating_add(1);
        counts.losses = counts.losses.saturating_add(losses);
        report.compositions.read = report.compositions.read.saturating_add(1);
        let Some(index) = side.programs.template(&template) else {
            report.composition_refused(
                Some(&source),
                &Refusal::new(format!("no template {} is loaded", template.as_str())),
            );
            continue;
        };
        let canonical = match index.accept(composition) {
            Ok(canonical) => canonical,
            Err(error) => {
                report.composition_refused(Some(&source), &Refusal::new(crate::chain(&error)));
                continue;
            }
        };
        let graph = match mapper
            .map(&SourceComposition {
                source: source.clone(),
                composition: &canonical,
                template: index,
                visit: None,
                context,
            })
            .await
        {
            Ok(graph) => graph,
            Err(refusal) => {
                report.composition_refused(Some(&source), &refusal);
                continue;
            }
        };
        let Some(origin) = origin_of(resource, kind) else {
            report.composition_refused(Some(&source), &Refusal::new("the resource carries no id"));
            continue;
        };
        match writer.commit_from(&graph, run_id, Some(&origin)).await {
            Ok(written) => report.committed(&source, &written),
            Err(error) if writer.is_closed() => return Err(RunError::Database(error)),
            Err(error) => {
                report.engine_outcomes(Some(&source), graph.report());
                report.composition_refused(Some(&source), &Refusal::new(crate::chain(&error)));
            }
        }
    }
    report.resources = Some(counts);
    crate::etl::derive(settings, writer, &mut report, None).await?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::source_of;
    use serde_json::json;

    #[test]
    fn a_resource_and_its_patient_share_the_derived_ehr() {
        let patient = source_of(&json!({"resourceType": "Patient", "id": "p-1"}), "Patient")
            .expect("a patient names itself");
        let condition = source_of(
            &json!({"resourceType": "Condition", "id": "c-1",
                    "subject": {"reference": "Patient/p-1"},
                    "meta": {"versionId": "3"}}),
            "Condition",
        )
        .expect("a condition names its subject");
        assert_eq!(patient.ehr_id(), condition.ehr_id());
        assert_ne!(
            patient.versioned_object_uid(),
            condition.versioned_object_uid()
        );
        assert!(
            condition
                .version_uid()
                .value()
                .ends_with("::ferrobridge.fhir::3")
        );
    }

    #[test]
    fn the_derived_identifiers_are_stable_and_a_text_version_is_one() {
        let resource = json!({"resourceType": "Procedure", "id": "x",
                              "subject": {"reference": "Patient/p-1"},
                              "meta": {"versionId": "W/\"abc\""}});
        let first = source_of(&resource, "Procedure").expect("the procedure is named");
        let second = source_of(&resource, "Procedure").expect("the procedure is named");
        assert_eq!(first, second);
        assert!(
            first
                .version_uid()
                .value()
                .ends_with("::ferrobridge.fhir::1")
        );
    }

    #[test]
    fn a_resource_without_an_id_or_a_subject_is_refused() {
        assert!(source_of(&json!({"resourceType": "Patient"}), "Patient").is_err());
        assert!(
            source_of(
                &json!({"resourceType": "Condition", "id": "c"}),
                "Condition"
            )
            .is_err()
        );
    }
}
