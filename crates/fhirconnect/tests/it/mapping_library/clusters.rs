// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The library's cluster models no library file slots, each slotted by a
//! FerroBRIDGE model and compiled verbatim.
//!
//! The observation status, organization, medication status, entry category,
//! FHIR identifier and case identification clusters compile against the
//! library clusters template built from the CKM archetypes. No CKM mirror
//! holds `CLUSTER.kontakttyp.v0` or `CLUSTER.diagnosetyp.v0`, so the contact
//! type and diagnosis type clusters compile against the published
//! `KDS_Fall_einfach` template, which carries both.
//!
//! Four of them compile and do not run (#101), so the suite pins how each
//! fails and counts none as reached: the two status clusters map a `code`
//! onto a `DV_CODED_TEXT`, which `DV_CODED_TEXT.adoc` pairs with `Coding`
//! and `CodeableConcept` only, the organization cluster maps its identifier
//! text onto the slotted element itself, and the case identification
//! cluster maps an `Identifier` onto a `DV_TEXT`, which `DV_TEXT.adoc`
//! pairs with `String`, `Coding` and `CodeableConcept` only.

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

/// The entry category and FHIR identifier clusters, which FerroBRIDGE's
/// report entry model slots.
const ENTRY: &[&str] = &[
    "model/cluster/org.highmed/entry_category.v0.yml",
    "model/cluster/org.highmed/identifier_fhir.v0.yml",
];

/// The case identification cluster, which FerroBRIDGE's report case model
/// slots.
const CASE: &[&str] = &["model/cluster/org.openehr/case_identification.v0.yml"];

/// The diagnosis type cluster, which FerroBRIDGE's diagnosis use model
/// slots.
const DIAGNOSIS_USE: &[&str] = &["model/cluster/org.highmed/diagnoseTyp.yml"];

/// The context over FerroBRIDGE's report status model.
const STATUS_CONTEXT: &str = "ferrobridge_library_report_status.context";

/// The context over FerroBRIDGE's report organization model.
const ORGANIZATION_CONTEXT: &str = "ferrobridge_library_report_organization.context";

/// The context over FerroBRIDGE's medication status model.
const MEDICATION_STATUS_CONTEXT: &str = "ferrobridge_library_medication_status.context";

/// The context over FerroBRIDGE's encounter model.
const CONTACT_TYPE_CONTEXT: &str = "ferrobridge_library_contact_type.context";

/// The context over FerroBRIDGE's report entry model.
const ENTRY_CONTEXT: &str = "ferrobridge_library_report_entry.context";

/// The context over FerroBRIDGE's report case model.
const CASE_CONTEXT: &str = "ferrobridge_library_report_case.context";

/// The context over FerroBRIDGE's diagnosis use model.
const DIAGNOSIS_USE_CONTEXT: &str = "ferrobridge_library_diagnosis_use.context";

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

/// Compiles the report entry context against the library clusters
/// template.
fn entry() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    program(
        CLUSTERS,
        ENTRY_CONTEXT,
        ENTRY,
        &["ferrobridge_report_entry.yml"],
    )
}

/// Compiles the report case context against the library clusters template.
fn case() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    program(
        CLUSTERS,
        CASE_CONTEXT,
        CASE,
        &["ferrobridge_report_case.yml"],
    )
}

/// Compiles the FerroBRIDGE context `context` over the published `files`
/// and the FerroBRIDGE `model` against the published `KDS_Fall_einfach`
/// template.
fn kds_fall(
    context: &str,
    files: &[&str],
    model: &str,
) -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    let opt = openehr_its::opt14::from_xml(KDS_FALL_EINFACH)?;
    let index = WebTemplateIndex::build(&TemplateSource::Opt14(Box::new(opt)))?;
    let file = format!("{context}.yml");
    let set = set(files, &[model, file.as_str()])?;
    let program = compile(
        &set,
        &MappingName::new(context)?,
        &index,
        &SCHEMAS,
        &Library,
    )
    .map_err(|diagnostics| render(&diagnostics))?;
    Ok((program, index))
}

/// Compiles the contact type context against `KDS_Fall_einfach`.
fn contact_type() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    kds_fall(
        CONTACT_TYPE_CONTEXT,
        CONTACT_TYPE,
        "ferrobridge_encounter_contact_type.yml",
    )
}

/// Compiles the diagnosis use context against `KDS_Fall_einfach`.
fn diagnosis_use() -> Result<(Arc<Program>, WebTemplateIndex), Box<dyn Error>> {
    kds_fall(
        DIAGNOSIS_USE_CONTEXT,
        DIAGNOSIS_USE,
        "ferrobridge_encounter_diagnosis_use.yml",
    )
}

/// Returns the synthetic `Encounter` whose contact type the contact type
/// cluster maps.
fn contact_type_encounter() -> serde_json::Value {
    serde_json::json!({
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
    })
}

/// Returns the synthetic `Observation` whose category and identifier the
/// entry clusters map.
fn observation_entry() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"text": "Synthetic observation"},
        "category": [{
            "coding": [{"system": "http://example.org/fhir/CodeSystem/synthetic-category", "code": "SYN-CAT", "display": "Synthetic category"}],
            "text": "A synthetic category"
        }],
        "identifier": [{"system": "http://example.org/fhir/sid/synthetic-observation", "value": "SYN-OBS-0001"}]
    })
}

/// Returns the synthetic `Encounter` whose diagnosis use the diagnosis type
/// cluster maps.
fn diagnosis_encounter() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "Encounter",
        "reasonCode": [{
            "coding": [{"system": "http://example.org/fhir/CodeSystem/synthetic-problem", "code": "SYN-PROBLEM", "display": "A synthetic problem"}]
        }],
        "diagnosis": [{
            "use": {
                "coding": [
                    {"system": "http://terminology.hl7.org/CodeSystem/diagnosis-role", "code": "AD"},
                    {"system": "http://fhir.de/CodeSystem/DiagnoseTyp", "code": "surgery-diagnosis"}
                ]
            }
        }]
    })
}

/// Returns `program` with the published `files` it reaches when `PutGet`
/// over `resource` changes no value it carries back, and nothing when it
/// does: a run that rewrites a value is no pass.
fn counted(
    files: &'static [&'static str],
    (program, index): (Arc<Program>, WebTemplateIndex),
    resource: &serde_json::Value,
) -> Result<Option<crate::support::Chain>, Box<dyn Error>> {
    let declared = laws::putget(&program, &index, resource, &laws::untouched)?;
    let unchanged = declared
        .get("changed")
        .and_then(serde_json::Value::as_array)
        .is_some_and(Vec::is_empty);
    Ok(unchanged.then_some((files, program)))
}

/// Compiles the contexts of this module whose clusters run and returns each
/// whose `PutGet` changes nothing, with the published files it reaches.
pub(super) fn chains() -> Result<Vec<crate::support::Chain>, Box<dyn Error>> {
    Ok([
        counted(CONTACT_TYPE, contact_type()?, &contact_type_encounter())?,
        counted(ENTRY, entry()?, &observation_entry())?,
        counted(DIAGNOSIS_USE, diagnosis_use()?, &diagnosis_encounter())?,
    ]
    .into_iter()
    .flatten()
    .collect())
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
/// another string than the one the condition selected (#101), and the corpus
/// does not count the file.
#[test]
fn putget_holds_on_a_synthetic_contact_type() -> Result<(), Box<dyn Error>> {
    let (program, index) = contact_type()?;
    let encounter = contact_type_encounter();
    let declared = laws::putget(&program, &index, &encounter, &laws::untouched)?;
    insta::assert_json_snapshot!("library_contact_type_putget", declared);
    Ok(())
}

#[test]
fn the_entry_clusters_compile_onto_observation() -> Result<(), Box<dyn Error>> {
    let (program, _) = entry()?;
    assert_eq!(program.resource().as_str(), "Observation");
    assert_eq!(
        models(&program),
        [
            "ferrobridge_report_entry",
            "CLUSTER.entry_category.v0",
            "CLUSTER.identifier_fhir.v0"
        ]
    );
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

#[test]
fn putget_holds_on_a_synthetic_observation_entry() -> Result<(), Box<dyn Error>> {
    let (program, index) = entry()?;
    let observation = observation_entry();
    let declared = laws::putget(&program, &index, &observation, &laws::untouched)?;
    insta::assert_json_snapshot!("library_observation_entry_putget", declared);
    Ok(())
}

#[test]
fn the_case_identification_cluster_compiles_onto_the_encounter_identifier()
-> Result<(), Box<dyn Error>> {
    let (program, _) = case()?;
    assert_eq!(
        models(&program),
        ["ferrobridge_report_case", "CLUSTER.case_identification.v0"]
    );
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

/// `case_identification.v0.yml:16` maps the `Identifier` it is slotted at
/// onto the `DV_TEXT` at `items[at0001]`, a pair `DV_TEXT.adoc` does not
/// write, so going in the run refuses (#101).
#[test]
fn the_case_identifier_refuses_going_in() -> Result<(), Box<dyn Error>> {
    let (program, index) = case()?;
    let observation = serde_json::json!({
        "resourceType": "Observation",
        "status": "final",
        "code": {"text": "Synthetic observation"},
        "encounter": {"identifier": {"value": "SYN-CASE-0001"}}
    });
    let outcome = to_openehr(
        &program,
        &SCHEMAS,
        &index,
        &Value::from_serde_json(observation),
        &Seams::default(),
        &laws::defaults(),
        &CallContext::new(),
    );
    let Err(EngineError::Cell {
        mapping, source, ..
    }) = outcome
    else {
        return Err("the case identifier went into openEHR".into());
    };
    assert_eq!(mapping, "identifierCaseParent", "the refused mapping");
    assert!(
        matches!(*source, LensError::NoCell { ref rm_type, kind: "Identifier", .. } if rm_type == "DV_TEXT"),
        "{source}"
    );
    Ok(())
}

#[test]
fn the_diagnosis_type_cluster_compiles_onto_encounter() -> Result<(), Box<dyn Error>> {
    let (program, _) = diagnosis_use()?;
    assert_eq!(program.resource().as_str(), "Encounter");
    assert_eq!(
        models(&program),
        [
            "ferrobridge_encounter_diagnosis_use",
            "CLUSTER.diagnosetyp.v0"
        ]
    );
    assert!(program.warnings().is_empty(), "{:?}", program.warnings());
    Ok(())
}

#[test]
fn putget_holds_on_a_synthetic_diagnosis_use() -> Result<(), Box<dyn Error>> {
    let (program, index) = diagnosis_use()?;
    let encounter = diagnosis_encounter();
    let declared = laws::putget(&program, &index, &encounter, &laws::untouched)?;
    insta::assert_json_snapshot!("library_diagnosis_use_putget", declared);
    Ok(())
}

#[test]
fn the_contact_type_is_not_counted_while_its_system_changes() -> Result<(), Box<dyn Error>> {
    let chain = counted(CONTACT_TYPE, contact_type()?, &contact_type_encounter())?;
    assert!(
        chain.is_none(),
        "the contact type counted with a changed system"
    );
    Ok(())
}
