// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The draft `EEHRxF` Annex X data groups as individual facade resources (draft
//! act Art 7(2)), through the `eehrxf` Annex X project: each group's resource
//! answers the IHE `QEDm` 3.0.0 PCC-44 searches its option asks for and reads
//! back by the id the search answers, with the profile its context names.

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use ferrobridge_server::cdr::ids::EhrId;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::endpoint::Endpoints;
use ferrobridge_server::facade::identity::PersonId;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::identity::store::Store;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::search::Limits;
use ferrobridge_testkit::conformance::Case;
use ferrobridge_testkit::conformance::Corpus;
use ferrobridge_testkit::conformance::record;
use ferrobridge_testkit::fixtures::CKM_EHDS_ARCHETYPES;
use ferrobridge_testkit::fixtures::CKM_MEDICATION_ARCHETYPES;
use ferrobridge_testkit::fixtures::CKM_PATIENT_SUMMARY_ARCHETYPES;
use ferrobridge_testkit::fixtures::FERROBRIDGE_EHDS_RECORD_TEMPLATE;
use ferrobridge_testkit::fixtures::FERROBRIDGE_EHDS_RECORD_TEMPLATE_ID;
use ferrobridge_testkit::fixtures::FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE;
use ferrobridge_testkit::fixtures::FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE_ID;
use http::StatusCode;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::assemble::parse_artefact;
use openehr_adl::opt::create_opt;
use openehr_adl::parse::Dialect;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::EHR_ID;
use super::super::Harness;
use super::super::VERSION_ONE;
use super::super::client;
use super::super::handle;
use super::super::settings;
use super::super::stub::mount_read;
use super::SUBJECT;
use super::get;
use super::matches;
use super::mount_rows;
use super::queries;
use super::row;

/// The shipped Annex X project the facade loads as its mapping directory.
const ANNEX_X: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/eehrxf/mappings/annex-x"
);

/// HL7 Europe Base and Core 2.0.1.
const BASE: &str = "http://hl7.eu/fhir/base/StructureDefinition";

/// Returns the canonical JSON of the ADL 2 operational template `template`
/// built over `archetypes`, as the reference CDR serves it.
fn operational<'a>(
    template: &str,
    archetypes: impl Iterator<Item = &'a (&'a str, &'a str)>,
) -> Result<String, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    for (file, source) in archetypes {
        let archetype = parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    let parsed = parse_artefact(template, Dialect::Adl2)
        .map_err(|errors| format!("the template: {errors:?}"))?;
    let raw = create_opt(&parsed, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(openehr_its::json::to_canonical_json(&raw.template))
}

/// Mounts `template` as the CDR serves an ADL 2 template: on the `adl2`
/// route alone.
async fn mount_template(cdr: &MockServer, id: &str, template: String) {
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("/definition/template/adl1.4/{id}")))
        .respond_with(ResponseTemplate::new(404))
        .mount(cdr)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("/definition/template/adl2/{id}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{id}\""))
                .set_body_string(template),
        )
        .mount(cdr)
        .await;
}

/// Returns a harness whose facade runs the Annex X project, its subject
/// bound to the suite's EHR.
async fn annex_x() -> Result<Harness, Box<dyn Error>> {
    annex_x_with(Endpoints::default()).await
}

/// Returns the Annex X harness serving `endpoints` beside its FHIR base.
async fn annex_x_with(endpoints: Endpoints) -> Result<Harness, Box<dyn Error>> {
    let cdr = MockServer::start().await;
    mount_template(
        &cdr,
        FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE_ID,
        operational(
            FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE,
            CKM_MEDICATION_ARCHETYPES
                .iter()
                .chain(CKM_PATIENT_SUMMARY_ARCHETYPES.iter()),
        )?,
    )
    .await;
    mount_template(
        &cdr,
        FERROBRIDGE_EHDS_RECORD_TEMPLATE_ID,
        operational(
            FERROBRIDGE_EHDS_RECORD_TEMPLATE,
            CKM_MEDICATION_ARCHETYPES
                .iter()
                .chain(CKM_EHDS_ARCHETYPES.iter()),
        )?,
    )
    .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(
            "/definition/template/adl1.4/KDS_Laborbericht",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/xml")
                .set_body_string(ferrobridge_testkit::fixtures::KDS_LABORBERICHT_OPT),
        )
        .mount(&cdr)
        .await;
    let directory = Path::new(ANNEX_X);
    let set = programs::read_set(directory)?;
    let templates = programs::fetch_templates(&set, &client(&cdr)).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let store = Arc::new(MemoryStore::default());
    let built = Facade::new(compiled, handle(&store), client(&cdr), settings());
    let catalog = ferrobridge_server::facade::search::load(directory, &built.programs())?;
    let facade = Arc::new(
        built
            .with_search(catalog, Limits::default())
            .with_endpoints(Some(endpoints)),
    );
    let person = PersonId::new("http://example.org/fhir", SUBJECT)?;
    store.record_ehr(&person, &EhrId::new(EHR_ID)?)?;
    Ok(Harness { cdr, store, facade })
}

/// One Annex X group served as a resource: its context, a resource it maps,
/// the `QEDm` searches its option asks for, and the profile it answers with.
struct Group {
    /// The Annex X data group.
    name: &'static str,
    /// The context mapping that maps its resource into a composition.
    context: &'static str,
    /// The resource type the group is served as.
    resource_type: &'static str,
    /// The archetype of the entry the context starts at.
    archetype: &'static str,
    /// The profile every resource of the group claims.
    profile: String,
    /// A resource the context maps, claiming the group's profile unless it
    /// states its own.
    resource: serde_json::Value,
    /// The searches, relative to `[type]?`, each answering the resource.
    searches: &'static [&'static str],
}

/// Returns the subject reference every resource carries.
fn subject() -> serde_json::Value {
    serde_json::json!({ "reference": SUBJECT })
}

/// Returns the `EHDSAllergyIntolerance` group.
fn group_allergy_intolerance() -> Group {
    Group {
        name: "EHDSAllergyIntolerance",
        context: "ferrobridge_ehds_allergy_intolerance.context",
        resource_type: "AllergyIntolerance",
        archetype: "openEHR-EHR-EVALUATION.adverse_reaction_risk.v2",
        profile: format!("{BASE}/allergyIntolerance-eu-core"),
        resource: serde_json::json!({
            "resourceType": "AllergyIntolerance",
            "clinicalStatus": { "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical",
                "code": "active"
            }]},
            "code": { "coding": [{
                "system": "http://snomed.info/sct", "code": "227493005", "display": "Cashew nuts"
            }]},
            "patient": subject()
        }),
        // PCC-44 2:3.44.4.1.2.2.
        searches: &["patient=Patient/p-1"],
    }
}

/// Returns the `EHDSCondition` group.
fn group_condition() -> Group {
    Group {
        name: "EHDSCondition",
        context: "ferrobridge_ehds_condition.context",
        resource_type: "Condition",
        archetype: "openEHR-EHR-EVALUATION.problem_diagnosis.v1",
        profile: format!("{BASE}/condition-eu-core"),
        resource: serde_json::json!({
            "resourceType": "Condition",
            "code": { "coding": [{
                "system": "http://snomed.info/sct", "code": "38341003", "display": "Hypertension"
            }]},
            "onsetDateTime": "2026-01-10",
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.3.
        searches: &[
            "patient=Patient/p-1",
            "patient=Patient/p-1&onset-date=ge2026-01-01",
        ],
    }
}

/// Returns the `EHDSProcedure` group.
fn group_procedure() -> Group {
    Group {
        name: "EHDSProcedure",
        context: "ferrobridge_ehds_procedure.context",
        resource_type: "Procedure",
        archetype: "openEHR-EHR-ACTION.procedure.v1",
        profile: format!("{BASE}/procedure-eu-core"),
        resource: serde_json::json!({
            "resourceType": "Procedure",
            "status": "completed",
            "code": { "coding": [{
                "system": "http://snomed.info/sct", "code": "80146002", "display": "Appendectomy"
            }]},
            "performedDateTime": "2026-02-01T10:00:00Z",
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.7.
        searches: &[
            "patient=Patient/p-1",
            "patient=Patient/p-1&date=ge2026-01-01&date=lt2026-03-01",
        ],
    }
}

/// Returns the `EHDSMedicationUse` group.
fn group_medication_use() -> Group {
    Group {
        name: "EHDSMedicationUse",
        context: "ferrobridge_ehds_medication_use.context",
        resource_type: "MedicationStatement",
        archetype: "openEHR-EHR-INSTRUCTION.medication_order.v3",
        profile: format!("{BASE}/medicationStatement-eu-core"),
        resource: serde_json::json!({
            "resourceType": "MedicationStatement",
            "status": "active",
            "contained": [{
                "resourceType": "Medication",
                "id": "medicine",
                "code": { "coding": [{
                    "system": "http://snomed.info/sct", "code": "387517004", "display": "Paracetamol"
                }]}
            }],
            "medicationReference": { "reference": "#medicine" },
            "effectivePeriod": { "start": "2026-03-01" },
            "dosage": [{ "text": "One tablet twice a day" }],
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.5.
        searches: &["patient=Patient/p-1&_include=MedicationStatement:medication"],
    }
}

/// Returns the `EHDSImmunisation` group.
fn group_immunisation() -> Group {
    Group {
        name: "EHDSImmunisation",
        context: "ferrobridge_ehds_immunisation.context",
        resource_type: "Immunization",
        archetype: "openEHR-EHR-ACTION.medication.v1",
        profile: format!("{BASE}/immunization-eu-core"),
        resource: serde_json::json!({
            "resourceType": "Immunization",
            "status": "completed",
            "vaccineCode": { "coding": [{
                "system": "http://snomed.info/sct", "code": "1119349007", "display": "SARS-CoV-2 mRNA vaccine"
            }]},
            "occurrenceDateTime": "2026-04-01T09:00:00Z",
            "patient": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.6.
        searches: &["patient=Patient/p-1", "patient=Patient/p-1&date=2026-04"],
    }
}

/// Returns the `EHDSObservation` group.
fn group_observation() -> Group {
    Group {
        name: "EHDSObservation",
        context: "ferrobridge_ehds_observation.context",
        resource_type: "Observation",
        archetype: "openEHR-EHR-OBSERVATION.body_weight.v2",
        profile: String::from("http://hl7.org/fhir/StructureDefinition/bodyweight"),
        resource: serde_json::json!({
            "resourceType": "Observation",
            "status": "final",
            "category": [{ "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/observation-category",
                "code": "vital-signs"
            }]}],
            "code": { "coding": [{ "system": "http://loinc.org", "code": "29463-7" }]},
            "effectiveDateTime": "2026-05-01T08:00:00Z",
            "valueQuantity": {
                "value": 72.5, "unit": "kg", "system": "http://unitsofmeasure.org", "code": "kg"
            },
            "subject": subject()
        }),
        // PCC-44 2:3.44.4.1.2.1.1.
        searches: &[
            "patient=Patient/p-1&category=vital-signs",
            "patient=Patient/p-1&category=vital-signs&code=http://loinc.org|29463-7",
            "patient=Patient/p-1&category=vital-signs&date=ge2026-05-01",
            "patient=Patient/p-1&category=vital-signs&code=29463-7&date=lt2026-06-01",
        ],
    }
}

/// HL7 Europe Laboratory Report 2.0.0.
const LAB: &str = "http://hl7.eu/fhir/laboratory/StructureDefinition";

/// Returns a laboratory report whose result and specimen it contains, in
/// the shapes HL7 Europe Laboratory Report 2.0.0 constrains.
fn laboratory_report() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "DiagnosticReport",
        "meta": { "profile": [format!("{LAB}/DiagnosticReport-eu-lab")] },
        "contained": [
            {
                "resourceType": "Specimen",
                "id": "serum",
                "meta": { "profile": [format!("{LAB}/Specimen-eu-lab")] },
                "type": { "coding": [{
                    "system": "http://snomed.info/sct", "code": "119364003", "display": "Serum specimen"
                }]},
                "subject": subject(),
                "collection": { "collectedDateTime": "2026-03-02T07:05:00+01:00" }
            },
            {
                "resourceType": "Observation",
                "id": "cholesterol",
                "meta": { "profile": [format!("{LAB}/Observation-resultslab-eu-lab")] },
                "status": "final",
                "category": [{ "coding": [{
                    "system": "http://terminology.hl7.org/CodeSystem/observation-category",
                    "code": "laboratory"
                }]}],
                "code": { "coding": [{
                    "system": "http://loinc.org", "code": "2093-3",
                    "display": "Cholesterol [Mass/volume] in Serum or Plasma"
                }]},
                "subject": subject(),
                "effectiveDateTime": "2026-03-02T08:15:00+01:00",
                "valueQuantity": {
                    "value": 182, "unit": "mg/dL", "system": "http://unitsofmeasure.org", "code": "mg/dL"
                }
            }
        ],
        "identifier": [{
            "system": "http://example.org/fhir/sid/ferrobridge-lab-reports",
            "value": "synthetic-report-0001"
        }],
        "status": "final",
        "code": { "coding": [{
            "system": "http://loinc.org", "code": "24331-1", "display": "Lipid panel - Serum or Plasma"
        }]},
        "subject": subject(),
        "effectiveDateTime": "2026-03-02T08:15:00+01:00",
        "specimen": [{ "reference": "#serum" }],
        "result": [{ "reference": "#cholesterol" }]
    })
}

/// Returns the laboratory `DiagnosticReport`, the IHE `QEDm` Diagnostic
/// Reports option, which is no Annex X group.
fn diagnostic_report() -> Group {
    Group {
        name: "DiagnosticReport",
        context: "ferrobridge_ehds_diagnostic_report.context",
        resource_type: "DiagnosticReport",
        archetype: "openEHR-EHR-OBSERVATION.laboratory_test_result.v1",
        profile: format!("{LAB}/DiagnosticReport-eu-lab"),
        resource: laboratory_report(),
        // PCC-44 2:3.44.4.1.2.1.4.
        searches: &[
            "patient=Patient/p-1&category=LAB",
            "patient=Patient/p-1&category=LAB&date=ge2026-03-01",
            "patient=Patient/p-1&category=http://terminology.hl7.org/CodeSystem/v2-0074|LAB&date=lt2026-04-01",
            "patient=Patient/p-1&category=LAB&code=http://loinc.org|24331-1",
        ],
    }
}

/// Returns the `EHDSLaboratoryObservation` group: each analyte of a
/// laboratory report.
fn group_laboratory_observation() -> Group {
    Group {
        name: "EHDSLaboratoryObservation",
        context: "ferrobridge_ehds_diagnostic_report.context",
        resource_type: "Observation",
        archetype: "openEHR-EHR-CLUSTER.laboratory_test_analyte.v1",
        profile: format!("{LAB}/Observation-resultslab-eu-lab"),
        resource: laboratory_report(),
        // PCC-44 2:3.44.4.1.2.1.1.
        searches: &[
            "patient=Patient/p-1&category=laboratory",
            "patient=Patient/p-1&category=laboratory&code=http://loinc.org|2093-3",
            "patient=Patient/p-1&category=laboratory&date=ge2026-03-01",
            "patient=Patient/p-1&category=laboratory&code=2093-3&date=lt2026-04-01",
        ],
    }
}

/// Returns the `EHDSDeviceUse` group.
fn group_device_use() -> Group {
    Group {
        name: "EHDSDeviceUse",
        context: "ferrobridge_ehds_device_use.context",
        resource_type: "DeviceUseStatement",
        archetype: "openEHR-EHR-EVALUATION.device_summary.v0",
        profile: String::from(
            "http://hl7.eu/fhir/eps/StructureDefinition/deviceUseStatement-eu-eps",
        ),
        resource: serde_json::json!({
            "resourceType": "DeviceUseStatement",
            "status": "active",
            "contained": [{
                "resourceType": "Device",
                "id": "pacemaker",
                "type": { "coding": [{
                    "system": "http://snomed.info/sct", "code": "14106009", "display": "Cardiac pacemaker"
                }]}
            }],
            "device": { "reference": "#pacemaker" },
            "timingPeriod": { "start": "2025-06-01" },
            "subject": subject()
        }),
        // R4 DeviceUseStatement search parameter `patient`.
        searches: &["patient=Patient/p-1"],
    }
}

/// Returns the `EHDSSpecimen` group: each specimen of a laboratory report.
fn group_specimen() -> Group {
    Group {
        name: "EHDSSpecimen",
        context: "ferrobridge_ehds_diagnostic_report.context",
        resource_type: "Specimen",
        archetype: "openEHR-EHR-CLUSTER.specimen.v1",
        profile: format!("{LAB}/Specimen-eu-lab"),
        resource: laboratory_report(),
        // R4 Specimen search parameter `patient`.
        searches: &["patient=Patient/p-1"],
    }
}

/// Returns the groups this suite serves.
fn groups() -> Vec<Group> {
    vec![
        group_allergy_intolerance(),
        group_condition(),
        group_procedure(),
        group_medication_use(),
        group_immunisation(),
        group_observation(),
        group_laboratory_observation(),
        group_device_use(),
        group_specimen(),
    ]
}

/// Returns the composition `group`'s context maps its resource to.
fn composition_of(harness: &Harness, group: &Group) -> serde_json::Value {
    let mut resource = group.resource.clone();
    if resource.get("meta").is_none() {
        resource["meta"] = serde_json::json!({ "profile": [group.profile] });
    }
    harness.composition_under(group.context, &resource)
}

/// Returns the first node of `archetype` in `value`, at any depth.
fn node_of<'a>(value: &'a serde_json::Value, archetype: &str) -> Option<&'a serde_json::Value> {
    // NOTE: an ADL 2 template's data names the archetype with its full version,
    // `<id>.v2.0.0`, which the major version `<id>.v2` identifies.
    if value["archetype_node_id"]
        .as_str()
        .and_then(|id| id.strip_prefix(archetype))
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    {
        return Some(value);
    }
    match value {
        serde_json::Value::Object(members) => {
            members.values().find_map(|child| node_of(child, archetype))
        }
        serde_json::Value::Array(items) => items.iter().find_map(|child| node_of(child, archetype)),
        _ => None,
    }
}

/// Returns the row of `composition` whose bound nodes, as many as a search
/// of the suite binds, are each its entry of `archetype`.
fn entry_row(
    composition: &serde_json::Value,
    archetype: &str,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let entry =
        node_of(composition, archetype).ok_or(format!("no {archetype} entry in {composition}"))?;
    Ok(serde_json::json!([
        EHR_ID,
        VERSION_ONE,
        composition,
        entry,
        entry,
        entry
    ]))
}

/// Returns the resource type of `group`.
const fn type_of(group: &Group) -> &'static str {
    group.resource_type
}

/// The 33 data groups of draft `EEHRxF` Annex X, in the order of its tables.
const ANNEX_X_GROUPS: [&str; 33] = [
    "EHDSAddress",
    "EHDSAdministeredDosage",
    "EHDSAdvanceDirective",
    "EHDSAlert",
    "EHDSAllergyIntolerance",
    "EHDSAttachment",
    "EHDSBodyStructure",
    "EHDSCarePlan",
    "EHDSCondition",
    "EHDSCurrentPregnancyStatus",
    "EHDSDevice",
    "EHDSDeviceUse",
    "EHDSDosage",
    "EHDSEncounter",
    "EHDSEndpoint",
    "EHDSHealthProfessional",
    "EHDSHumanName",
    "EHDSImmunisation",
    "EHDSLaboratoryObservation",
    "EHDSLocation",
    "EHDSMedicationAdministration",
    "EHDSMedication",
    "EHDSMedicationUse",
    "EHDSObservation",
    "EHDSOrganisation",
    "EHDSPatient",
    "EHDSPregnancyHistory",
    "EHDSProcedure",
    "EHDSRelatedPerson",
    "EHDSServiceRequest",
    "EHDSSpecimen",
    "EHDSTelecom",
    "EHDSTravelHistory",
];

/// Fails with `reason` unless `holds`.
fn ensure(holds: bool, reason: impl FnOnce() -> String) -> Result<(), Box<dyn Error>> {
    if holds { Ok(()) } else { Err(reason().into()) }
}

/// Holds every search of `group` to one answer with the group's profile, and
/// returns that resource as it reads back by its id.
async fn serve(group: &Group) -> Result<serde_json::Value, Box<dyn Error>> {
    let harness = annex_x().await?;
    let composition = composition_of(&harness, group);
    mount_rows(&harness.cdr, &[entry_row(&composition, group.archetype)?]).await;
    mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    let resource_type = type_of(group);
    let mut ids = Vec::new();
    for search in group.searches {
        let (status, body) = get(&harness, &format!("{resource_type}?{search}")).await?;
        ensure(status == StatusCode::OK, || {
            format!("{search} answered {status}: {body}")
        })?;
        let found = matches(&body);
        ensure(found.len() == 1, || {
            format!("{search} answered {} matches", found.len())
        })?;
        let profile = &found[0]["resource"]["meta"]["profile"];
        ensure(*profile == serde_json::json!([group.profile]), || {
            format!("{search} answered the profile {profile}")
        })?;
        ids.push(
            found[0]["resource"]["id"]
                .as_str()
                .ok_or("an id")?
                .to_owned(),
        );
    }
    ids.dedup();
    ensure(ids.len() == 1, || {
        format!("the searches answered the ids {ids:?}")
    })?;
    let id = ids.first().ok_or("an id")?.clone();
    let (status, read) = get(&harness, &format!("{resource_type}/{id}")).await?;
    ensure(status == StatusCode::OK, || {
        format!("the read answered {status}: {read}")
    })?;
    ensure(
        read["meta"]["profile"] == serde_json::json!([group.profile]),
        || format!("the read answered the profile {}", read["meta"]["profile"]),
    )?;
    Ok(read)
}

/// A group served as the resource another group's resource references and
/// its run creates: included by that group's search and read by its id.
struct Included {
    /// The group whose resource references it.
    by: &'static str,
    /// The `_include` value, `[type]:[parameter]`.
    include: &'static str,
    /// The resource type it is served as.
    resource_type: &'static str,
    /// The profile it answers with.
    profile: String,
}

/// Returns the groups served as included resources, by name.
fn included_groups() -> Vec<(&'static str, Included)> {
    vec![
        (
            "EHDSMedication",
            Included {
                by: "EHDSMedicationUse",
                include: "MedicationStatement:medication",
                resource_type: "Medication",
                profile: format!("{BASE}/medication-eu-core"),
            },
        ),
        (
            "EHDSDevice",
            Included {
                by: "EHDSDeviceUse",
                include: "DeviceUseStatement:device",
                resource_type: "Device",
                profile: String::from("http://hl7.eu/fhir/eps/StructureDefinition/device-eu-eps"),
            },
        ),
    ]
}

/// A data-type group, asserted in a served resource that carries it.
struct Embedded {
    /// The group whose resource carries it.
    by: &'static str,
    /// The JSON pointer of the element in that resource.
    pointer: &'static str,
}

/// Returns the data-type groups the suite asserts, by name.
const fn embedded_groups() -> [(&'static str, Embedded); 1] {
    [(
        "EHDSDosage",
        Embedded {
            by: "EHDSMedicationUse",
            pointer: "/dosage/0/text",
        },
    )]
}

/// Returns the served group named `name`.
fn group_named(name: &str) -> Result<Group, Box<dyn Error>> {
    groups()
        .into_iter()
        .find(|group| group.name == name)
        .ok_or_else(|| format!("no served group {name}").into())
}

/// Serves an included group: the search of the group that references it
/// includes it, and it reads back by its id with its profile.
async fn serve_included(included: &Included) -> Result<(), Box<dyn Error>> {
    let group = group_named(included.by)?;
    let harness = annex_x().await?;
    let composition = composition_of(&harness, &group);
    mount_rows(&harness.cdr, &[entry_row(&composition, group.archetype)?]).await;
    mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    let (status, body) = get(
        &harness,
        &format!(
            "{}?patient={SUBJECT}&_include={}",
            group.resource_type, included.include
        ),
    )
    .await?;
    ensure(status == StatusCode::OK, || {
        format!("the search answered {status}: {body}")
    })?;
    let id = body["entry"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["search"]["mode"] == "include")
        })
        .and_then(|entry| entry["resource"]["id"].as_str())
        .ok_or_else(|| format!("the search includes no {}", included.resource_type))?
        .to_owned();
    let (status, read) = get(&harness, &format!("{}/{id}", included.resource_type)).await?;
    ensure(status == StatusCode::OK, || {
        format!("the read answered {status}: {read}")
    })?;
    ensure(
        read["meta"]["profile"] == serde_json::json!([included.profile]),
        || format!("the read answered the profile {}", read["meta"]["profile"]),
    )
}

/// Serves a data-type group: the served resource that carries it reads back
/// with the element.
async fn serve_embedded(embedded: &Embedded) -> Result<(), Box<dyn Error>> {
    let group = group_named(embedded.by)?;
    let read = serve(&group).await?;
    ensure(read.pointer(embedded.pointer).is_some(), || {
        format!("{} answered no {}: {read}", group.name, embedded.pointer)
    })
}

/// Serves `EHDSEndpoint`: the facade's own FHIR base, searched by its
/// connection type and read by its id.
async fn serve_endpoint() -> Result<(), Box<dyn Error>> {
    let harness = annex_x().await?;
    let (status, body) = get(&harness, "Endpoint?connection-type=hl7-fhir-rest").await?;
    ensure(status == StatusCode::OK, || {
        format!("the search answered {status}: {body}")
    })?;
    let found = matches(&body);
    ensure(found.len() == 1, || {
        format!("the search answered {} matches", found.len())
    })?;
    let id = found[0]["resource"]["id"]
        .as_str()
        .ok_or("an id")?
        .to_owned();
    let (status, read) = get(&harness, &format!("Endpoint/{id}")).await?;
    ensure(status == StatusCode::OK, || {
        format!("the read answered {status}: {read}")
    })?;
    ensure(read["address"] == found[0]["resource"]["address"], || {
        format!("the read answered {read}")
    })
}

/// Returns the verdict on the Annex X group `name`.
async fn verdict(name: &str) -> Result<(), Box<dyn Error>> {
    if name == "EHDSEndpoint" {
        return serve_endpoint().await;
    }
    if let Ok(group) = group_named(name) {
        return serve(&group).await.map(drop);
    }
    if let Some((_, included)) = included_groups()
        .into_iter()
        .find(|(held, _)| *held == name)
    {
        return serve_included(&included).await;
    }
    if let Some((_, embedded)) = embedded_groups()
        .into_iter()
        .find(|(held, _)| *held == name)
    {
        return serve_embedded(&embedded).await;
    }
    Err("the facade serves no resource of this group yet".into())
}

#[tokio::test]
async fn conformance_the_annex_x_groups_hold_their_pass_list() -> Result<(), Box<dyn Error>> {
    let served = groups();
    let mut cases = Vec::with_capacity(ANNEX_X_GROUPS.len());
    for name in ANNEX_X_GROUPS {
        cases.push(match verdict(name).await {
            Ok(()) => Case::pass(name),
            Err(reason) => Case::fail(name, reason.to_string()),
        });
    }
    let outcome = record(Corpus::EehrxfAnnexX, &cases)?;
    assert!(
        outcome.regressed.is_empty(),
        "groups the {} pass list records are no longer served: {:?}",
        Corpus::EehrxfAnnexX,
        outcome.regressed
    );
    for group in &served {
        assert!(
            cases
                .iter()
                .any(|case| case.id() == group.name && case.failure().is_none()),
            "{} is not served: {:?}",
            group.name,
            cases.iter().find(|case| case.id() == group.name)
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_medication_statement_includes_the_medication_it_references() -> Result<(), Box<dyn Error>>
{
    // PCC-44 2:3.44.4.1.2.1.5: MedicationStatement with
    // _include=MedicationStatement:medication; draft Annex X Table 22 makes
    // the medication a group of its own.
    let harness = annex_x().await?;
    let group = groups()
        .into_iter()
        .find(|group| group.name == "EHDSMedicationUse")
        .ok_or("the medication use group")?;
    let composition = composition_of(&harness, &group);
    mount_rows(&harness.cdr, &[row(VERSION_ONE, &composition)]).await;
    let (status, body) = get(
        &harness,
        &format!("MedicationStatement?patient={SUBJECT}&_include=MedicationStatement:medication"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let included: Vec<&serde_json::Value> = body["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["search"]["mode"] == "include")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(1, included.len(), "{body}");
    let medication = &included[0]["resource"];
    assert_eq!(Some("Medication"), medication["resourceType"].as_str());
    assert_eq!(
        Some("387517004"),
        medication["code"]["coding"][0]["code"].as_str()
    );
    assert_eq!(
        serde_json::json!([format!("{BASE}/medication-eu-core")]),
        medication["meta"]["profile"]
    );
    let reference = matches(&body)[0]["resource"]["medicationReference"]["reference"]
        .as_str()
        .ok_or("the statement references its medication")?
        .to_owned();
    assert_eq!(
        Some(reference.as_str()),
        medication["id"]
            .as_str()
            .map(|id| format!("Medication/{id}"))
            .as_deref()
    );
    Ok(())
}

#[tokio::test]
async fn an_observation_of_another_category_or_code_answers_no_query() -> Result<(), Box<dyn Error>>
{
    // Each Observation context writes its category as a constant and the body
    // weight its LOINC code too, so a value neither carries matches no
    // resource and queries nothing.
    let harness = annex_x().await?;
    for search in [
        "category=exam",
        "category=vital-signs&code=http://loinc.org|8302-2",
    ] {
        let (status, body) =
            get(&harness, &format!("Observation?patient={SUBJECT}&{search}")).await?;
        assert_eq!(StatusCode::OK, status, "{search}: {body}");
        assert!(matches(&body).is_empty(), "{search}: {body}");
    }
    assert_eq!(0, queries(&harness).await.len());
    Ok(())
}

#[tokio::test]
async fn the_statement_lists_each_group_s_searches() -> Result<(), Box<dyn Error>> {
    let harness = annex_x().await?;
    let (status, body) = get(&harness, "metadata").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let resources = body["rest"][0]["resource"]
        .as_array()
        .ok_or("the statement names resources")?;
    for (resource_type, parameters) in [
        ("AllergyIntolerance", &["patient"][..]),
        ("Condition", &["patient", "onset-date"][..]),
        ("Procedure", &["patient", "date"][..]),
        ("MedicationStatement", &["patient"][..]),
        ("Immunization", &["patient", "date"][..]),
        ("Observation", &["patient", "category", "code", "date"][..]),
        (
            "DiagnosticReport",
            &["patient", "category", "code", "date"][..],
        ),
        ("DeviceUseStatement", &["patient"][..]),
        ("Specimen", &["patient"][..]),
    ] {
        let resource = resources
            .iter()
            .find(|resource| resource["type"] == resource_type)
            .ok_or(resource_type)?;
        let names: Vec<&str> = resource["searchParam"]
            .as_array()
            .map(|params| {
                params
                    .iter()
                    .filter_map(|param| param["name"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        for parameter in parameters {
            assert!(names.contains(parameter), "{resource_type}: {names:?}");
        }
    }
    let statement = resources
        .iter()
        .find(|resource| resource["type"] == "MedicationStatement")
        .ok_or("MedicationStatement")?;
    assert_eq!(
        serde_json::json!(["MedicationStatement:medication"]),
        statement["searchInclude"]
    );
    Ok(())
}

#[tokio::test]
async fn each_date_search_compares_the_instant_the_mapping_writes() -> Result<(), Box<dyn Error>> {
    // A date search reads the RM attribute the mapping fills from the
    // resource's own date, so the stub's rows and a CDR's WHERE agree.
    let harness = annex_x().await?;
    for (name, pointer, path, date) in [
        ("EHDSProcedure", "/time/value", "/time/value", "2026-02-01"),
        (
            "EHDSImmunisation",
            "/time/value",
            "/time/value",
            "2026-04-01",
        ),
        (
            "EHDSObservation",
            "/data/origin/value",
            "/data[at0002]/origin/value",
            "2026-05-01",
        ),
    ] {
        let group = groups()
            .into_iter()
            .find(|group| group.name == name)
            .ok_or(name)?;
        let composition = composition_of(&harness, &group);
        let entry = node_of(&composition, group.archetype).ok_or(name)?;
        assert!(
            entry
                .pointer(pointer)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value.starts_with(date)),
            "{name}: {entry}"
        );
        mount_rows(&harness.cdr, &[entry_row(&composition, group.archetype)?]).await;
        let resource_type = type_of(&group);
        let (status, body) = get(
            &harness,
            &format!("{resource_type}?patient={SUBJECT}&date=ge{date}"),
        )
        .await?;
        assert_eq!(StatusCode::OK, status, "{name}: {body}");
        let sent = queries(&harness).await;
        assert!(
            sent.iter()
                .filter_map(|query| query["q"].as_str())
                .any(|text| text.contains(&format!("n0{path}"))),
            "{name}: {sent:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_laboratory_report_answers_its_qedm_searches_and_includes_its_results()
-> Result<(), Box<dyn Error>> {
    // PCC-44 2:3.44.4.1.2.1.4; R4 search.html §3.1.1.5.4 for the include.
    let report = diagnostic_report();
    serve(&report).await?;
    let harness = annex_x().await?;
    let composition = composition_of(&harness, &report);
    mount_rows(&harness.cdr, &[entry_row(&composition, report.archetype)?]).await;
    let (status, body) = get(
        &harness,
        &format!("DiagnosticReport?patient={SUBJECT}&_include=DiagnosticReport:result"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    assert_eq!(
        Some("LAB"),
        found[0]["resource"]["category"][0]["coding"][0]["code"].as_str(),
        "{body}"
    );
    let included: Vec<&serde_json::Value> = body["entry"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry["search"]["mode"] == "include")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(1, included.len(), "{body}");
    assert_eq!(
        Some("Observation"),
        included[0]["resource"]["resourceType"].as_str()
    );
    Ok(())
}

#[tokio::test]
async fn a_report_code_matches_the_test_name_under_any_event() -> Result<(), Box<dyn Error>> {
    // PCC-44 2:3.44.4.1.2.1.4 patient+category+code: KDS_Laborbericht holds
    // the test name under its repeating event, so the query binds the event
    // inside the result and a report matches when any of its events does.
    let report = diagnostic_report();
    let harness = annex_x().await?;
    let composition = composition_of(&harness, &report);
    mount_rows(&harness.cdr, &[entry_row(&composition, report.archetype)?]).await;
    let (status, body) = get(
        &harness,
        &format!("DiagnosticReport?patient={SUBJECT}&category=LAB&code=http://loinc.org|24331-1"),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(1, matches(&body).len(), "{body}");
    let sent = queries(&harness).await;
    let text = sent
        .iter()
        .find_map(|query| query["q"].as_str())
        .ok_or("the search queried the CDR")?;
    assert!(
        text.contains(
            "OBSERVATION[openEHR-EHR-OBSERVATION.laboratory_test_result.v1] CONTAINS EVENT n0[at0002]"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "n0/data[at0003]/items[at0005,'Labortest-Bezeichnung']/value/defining_code/code_string"
        ),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn the_endpoints_are_the_fhir_base_and_the_configured_wado_rs() -> Result<(), Box<dyn Error>>
{
    // R4 endpoint.html; the connection types of
    // http://terminology.hl7.org/CodeSystem/endpoint-connection-type.
    let harness = annex_x_with(Endpoints {
        wado_rs: Some(String::from("https://pacs.example.org/dicom-web")),
    })
    .await?;
    let (status, body) = get(&harness, "Endpoint").await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let found = matches(&body);
    assert_eq!(2, found.len(), "{body}");
    assert_eq!(
        Some("hl7-fhir-rest"),
        found[0]["resource"]["connectionType"]["code"].as_str()
    );
    let (status, wado) = get(&harness, "Endpoint/dicom-wado-rs").await?;
    assert_eq!(StatusCode::OK, status, "{wado}");
    assert_eq!(
        Some("dicom-wado-rs"),
        wado["connectionType"]["code"].as_str()
    );
    assert_eq!(
        Some("https://pacs.example.org/dicom-web"),
        wado["address"].as_str()
    );
    let (status, none) = get(&harness, "Endpoint?connection-type=dicom-wado-rs&_id=fhir").await?;
    assert_eq!(StatusCode::OK, status, "{none}");
    assert!(matches(&none).is_empty(), "{none}");
    let (status, refused) = get(&harness, "Endpoint?address=x").await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{refused}");
    let (status, missing) = get(&harness, "Endpoint/other").await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{missing}");
    let (status, statement) = get(&harness, "metadata").await?;
    assert_eq!(StatusCode::OK, status, "{statement}");
    assert!(
        statement["rest"][0]["resource"]
            .as_array()
            .is_some_and(|resources| resources
                .iter()
                .any(|resource| resource["type"] == "Endpoint")),
        "{statement}"
    );
    assert_eq!(0, queries(&harness).await.len());
    Ok(())
}
