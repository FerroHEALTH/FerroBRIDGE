// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The library's cluster models no library file slots, each slotted by a
//! FerroBRIDGE model and compiled verbatim.
//!
//! The observation status, organization and medication status clusters
//! compile against the library clusters template built from the CKM
//! archetypes. No CKM mirror holds `CLUSTER.kontakttyp.v0`, so the contact
//! type cluster compiles against the published `KDS_Fall_einfach` template,
//! which carries it.
//!
//! Three of them compile and do not run (#101), so the suite pins how each
//! fails and counts none as reached: the two status clusters map a `code`
//! onto a `DV_CODED_TEXT`, which `DV_CODED_TEXT.adoc` pairs with `Coding`
//! and `CodeableConcept` only, and the organization cluster maps its
//! identifier text onto the slotted element itself.

use core::error::Error;
use std::sync::Arc;

use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::lens::LensError;
use fhirconnect::engine::library::Library;
use fhirconnect::engine::seam::Seams;
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::engine::traverse::to_fhir;
use fhirconnect::engine::traverse::to_openehr;
use fhirconnect::resolve::compile::compile;
use fhirconnect::resolve::program::Program;
use openehr_mapping_core::composition::CanonicalComposition;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::TemplateSource;

use super::models;
use super::program;
use super::set;
use crate::laws;
use crate::resolve::render;

/// The library clusters template.
const CLUSTERS: &str = "openEHR-EHR-COMPOSITION.ferrobridge_library_clusters.v1.0.0";

/// The observation status cluster, which FerroBRIDGE's report status model
/// slots.
const OBSERVATION_STATUS: &[&str] = &["model/cluster/org.highmed/observation_status_fhir.v1.yml"];

/// The organization cluster, which FerroBRIDGE's report organization model
/// slots.
const ORGANIZATION: &[&str] = &["model/cluster/org.highmed/organization.v0.yml"];

/// The medication status cluster, which FerroBRIDGE's medication status
/// model slots.
const MEDICATION_STATUS: &[&str] = &["model/cluster/org.highmed/medication_status_fhir.v0.yml"];

/// The contact type cluster, which FerroBRIDGE's encounter model slots.
const CONTACT_TYPE: &[&str] = &["model/cluster/org.highmed/kontaktTyp.yml"];

/// The context over FerroBRIDGE's report status model.
const STATUS_CONTEXT: &str = "ferrobridge_library_report_status.context";

/// The context over FerroBRIDGE's report organization model.
const ORGANIZATION_CONTEXT: &str = "ferrobridge_library_report_organization.context";

/// The context over FerroBRIDGE's medication status model.
const MEDICATION_STATUS_CONTEXT: &str = "ferrobridge_library_medication_status.context";

/// The context over FerroBRIDGE's encounter model.
const CONTACT_TYPE_CONTEXT: &str = "ferrobridge_library_contact_type.context";

/// The published `KDS_Fall_einfach` operational template.
const KDS_FALL_EINFACH: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/ferrobridge-testkit/fixtures/opt/kds-fall-einfach/KDS_Fall_einfach.opt"
));

/// Compiles the report status context against the library clusters
/// template.
fn status() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    program(
        CLUSTERS,
        STATUS_CONTEXT,
        OBSERVATION_STATUS,
        &["ferrobridge_report_status.yml"],
    )
}

/// Compiles the report organization context against the library clusters
/// template.
fn organization() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    program(
        CLUSTERS,
        ORGANIZATION_CONTEXT,
        ORGANIZATION,
        &["ferrobridge_report_organization.yml"],
    )
}

/// Compiles the medication status context against the library clusters
/// template.
fn medication_status() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    program(
        CLUSTERS,
        MEDICATION_STATUS_CONTEXT,
        MEDICATION_STATUS,
        &["ferrobridge_medication_status.yml"],
    )
}

/// Compiles the contact type context against the published
/// `KDS_Fall_einfach` template.
fn contact_type() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(KDS_FALL_EINFACH)?;
    let index = WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(opt)))?;
    let set = set(
        CONTACT_TYPE,
        &[
            "ferrobridge_encounter_contact_type.yml",
            "ferrobridge_library_contact_type.context.yml",
        ],
    )?;
    let program = compile(
        &set,
        &MappingName::new(CONTACT_TYPE_CONTEXT)?,
        &index,
        &SCHEMAS,
        &Library,
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    Ok((program, index))
}

/// Compiles the context of this module whose cluster runs and returns it
/// with the published file it reaches.
pub(super) fn chains() -> Result<Vec<crate::support::Chain>, Box<dyn Error>> {
    Ok(vec![(CONTACT_TYPE, contact_type()?.0)])
}

/// Returns a synthetic report composition of the library clusters template:
/// its context holds the observation status `Final` (`at0026`) and one
/// organization.
fn report_composition(index: &WebTemplateIndex) -> Result<CanonicalComposition, Box<dyn Error>> {
    let flat = serde_json::json!({
        "library_clusters/language|code": "en",
        "library_clusters/language|terminology": "ISO_639-1",
        "library_clusters/territory|code": "NL",
        "library_clusters/territory|terminology": "ISO_3166-1",
        "library_clusters/composer|name": "FerroBRIDGE synthetic composer",
        "library_clusters/category|code": "433",
        "library_clusters/category|value": "event",
        "library_clusters/category|terminology": "openehr",
        "library_clusters/context/start_time": "2026-10-01T09:00:00Z",
        "library_clusters/context/setting|code": "238",
        "library_clusters/context/setting|value": "other care",
        "library_clusters/context/setting|terminology": "openehr",
        "library_clusters/context/observation_status/value|code": "at0026",
        "library_clusters/context/observation_status/value|value": "Final",
        "library_clusters/context/observation_status/value|terminology": "local",
        "library_clusters/context/observation_status/value_1": "Synthetic observation",
        "library_clusters/context/organization/type": "Synthetic laboratory",
        "library_clusters/context/organization/identifier": "SYN-ORG-0001",
        "library_clusters/context/organization/name": "Synthetic Organization"
    });
    let serde_json::Value::Object(flat) = flat else {
        return Err("the FLAT composition is no object".into());
    };
    Ok(index.build_from_flat(&flat, laws::NOW)?)
}

/// Asserts that `outcome` is the refusal of the status mapping going into
/// openEHR because no data-type cell pairs `DV_CODED_TEXT` with the `code`.
fn assert_status_refused<T>(outcome: Result<T, EngineError>) -> Result<(), Box<dyn Error>> {
    let Err(EngineError::Cell {
        mapping, source, ..
    }) = outcome
    else {
        return Err("the status went into openEHR".into());
    };
    assert_eq!(mapping, "status", "the refused mapping");
    assert!(
        matches!(*source, LensError::NoCell { ref rm_type, kind: "string", .. } if rm_type == "DV_CODED_TEXT"),
        "{source}"
    );
    Ok(())
}

#[test]
fn the_observation_status_cluster_compiles_onto_observation() -> Result<(), Box<dyn Error>> {
    let (program, _) = status()?;
    assert_eq!(program.resource().as_str(), "Observation");
    assert_eq!(
        models(&program),
        [
            "ferrobridge_report_status",
            "CLUSTER.observation_status_fhir.v1"
        ]
    );
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

#[test]
fn the_organization_cluster_compiles_onto_the_performer() -> Result<(), Box<dyn Error>> {
    let (program, _) = organization()?;
    assert_eq!(program.resource().as_str(), "Observation");
    assert_eq!(
        models(&program),
        ["ferrobridge_report_organization", "CLUSTER.organization.v0"]
    );
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

#[test]
fn the_medication_status_cluster_compiles_onto_medication_administration()
-> Result<(), Box<dyn Error>> {
    let (program, _) = medication_status()?;
    assert_eq!(program.resource().as_str(), "MedicationAdministration");
    assert_eq!(
        models(&program),
        [
            "ferrobridge_medication_status",
            "CLUSTER.medication_status_fhir.v0"
        ]
    );
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

#[test]
fn the_contact_type_cluster_compiles_onto_encounter() -> Result<(), Box<dyn Error>> {
    let (program, _) = contact_type()?;
    assert_eq!(program.resource().as_str(), "Encounter");
    assert_eq!(
        models(&program),
        [
            "ferrobridge_encounter_contact_type",
            "CLUSTER.kontakttyp.v0"
        ]
    );
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

/// `organization.v0.yml:21` maps the identifier text onto `$fhirRoot`, the
/// `Organization` the performer references, which no data-type cell writes,
/// so going out the run refuses (#101).
#[test]
fn the_organization_identifier_refuses_going_out() -> Result<(), Box<dyn Error>> {
    let (program, index) = organization()?;
    let composition = report_composition(&index)?;
    let outcome = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    );
    let Err(EngineError::UnknownElement { mapping, element }) = outcome else {
        return Err("the organization went out of openEHR".into());
    };
    assert_eq!(
        (mapping.as_str(), element.as_str()),
        ("orgaKey", "Organization")
    );
    Ok(())
}

/// `observation_status_fhir.v1.yml:17` maps the archetype's local status
/// term onto `Observation.status`, a `code`, so going out the cell writes the
/// term text, which is no FHIR observation status (#101).
#[test]
fn the_observation_status_goes_out_as_its_term_text() -> Result<(), Box<dyn Error>> {
    let (program, index) = status()?;
    let composition = report_composition(&index)?;
    let outbound = to_fhir(
        &program,
        &SCHEMAS,
        &index,
        &composition,
        &Seams::default(),
        &CallContext::new(),
    )?;
    let observation = outbound
        .value()
        .to_serde_json(&mut fhir_types::codec::Path::root("Observation"))?;
    assert_eq!(observation.get("status"), Some(&serde_json::json!("Final")));
    Ok(())
}

/// `observation_status_fhir.v1.yml:17` going in: no cell pairs the
/// `DV_CODED_TEXT` with a `code` (`DV_CODED_TEXT.adoc`), so the run refuses
/// (#101).
#[test]
fn the_observation_status_refuses_going_in() -> Result<(), Box<dyn Error>> {
    let (program, index) = status()?;
    let observation = serde_json::json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"text": "Synthetic observation"}
    });
    assert_status_refused(to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(observation),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    ))
}

/// `medication_status_fhir.v0.yml:19` going in: no cell pairs the
/// `DV_CODED_TEXT` with a `code` (`DV_CODED_TEXT.adoc`), so the run refuses
/// (#101).
#[test]
fn the_medication_status_refuses_going_in() -> Result<(), Box<dyn Error>> {
    let (program, index) = medication_status()?;
    let administration = serde_json::json!({
        "resourceType": "MedicationAdministration",
        "status": "completed",
        "statusReason": [{"text": "A synthetic status reason."}]
    });
    assert_status_refused(to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(administration),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    ))
}

/// `kontaktTyp.yml:24` selects the contact level coding by the system
/// `http://fhir.de/CodeSystem/Kontaktebene` and `:34` writes it back as
/// `http://fhir.de/CodeSystem/kontaktebene`, so the system comes back as
/// another string than the one the condition selected (#101).
#[test]
fn putget_holds_on_a_synthetic_contact_type() -> Result<(), Box<dyn Error>> {
    let (program, index) = contact_type()?;
    let encounter = serde_json::json!({
        "resourceType": "Encounter",
        "class": {
            "system": "http://terminology.hl7.org/CodeSystem/v3-ActCode",
            "code": "IMP"
        },
        "type": [{
            "coding": [
                {"system": "http://fhir.de/CodeSystem/Kontaktebene", "code": "abteilungskontakt"},
                {"system": "http://fhir.de/CodeSystem/kontaktart-de", "code": "normalstationaer"}
            ]
        }]
    });
    let declared = laws::putget(&program, &index, &encounter, &laws::untouched)?;
    insta::assert_json_snapshot!("library_contact_type_putget", declared);
    Ok(())
}
