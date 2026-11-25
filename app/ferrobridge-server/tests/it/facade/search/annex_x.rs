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
    let directory = Path::new(ANNEX_X);
    let set = programs::read_set(directory)?;
    let templates = programs::fetch_templates(&set, &client(&cdr)).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let store = Arc::new(MemoryStore::default());
    let built = Facade::new(compiled, handle(&store), client(&cdr), settings());
    let catalog = ferrobridge_server::facade::search::load(directory, &built.programs())?;
    let facade = Arc::new(built.with_search(catalog, Limits::default()));
    let person = PersonId::new("http://example.org/fhir", super::PSEUDONYM)?;
    store.record_ehr(&person, &EhrId::new(EHR_ID)?).await?;
    Ok(Harness { cdr, store, facade })
}

/// One Annex X group served as a resource: its context, a resource it maps,
/// the `QEDm` searches its option asks for, and the profile it answers with.
struct Group {
    /// The Annex X data group.
    name: &'static str,
    /// The context mapping that serves it.
    context: &'static str,
    /// The archetype of the entry the context starts at.
    archetype: &'static str,
    /// The profile every resource of the group claims.
    profile: String,
    /// A resource the context maps.
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

/// Returns the groups this suite serves.
fn groups() -> Vec<Group> {
    vec![
        group_allergy_intolerance(),
        group_condition(),
        group_procedure(),
        group_medication_use(),
        group_immunisation(),
        group_observation(),
    ]
}

/// Returns the composition `group`'s context maps its resource to.
fn composition_of(harness: &Harness, group: &Group) -> serde_json::Value {
    let mut resource = group.resource.clone();
    resource["meta"] = serde_json::json!({ "profile": [group.profile] });
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
fn type_of(group: &Group) -> &str {
    group.resource["resourceType"].as_str().unwrap_or_default()
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

/// Returns the one id every search of `group` answers, after holding each
/// answer to the group's profile, and reads that resource back.
async fn serve(group: &Group) -> Result<String, Box<dyn Error>> {
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
    Ok(id)
}

/// Serves `EHDSMedication`: the Medication a medication statement
/// references, included by its search and read back by its id.
async fn serve_medication() -> Result<(), Box<dyn Error>> {
    let group = groups()
        .into_iter()
        .find(|group| group.name == "EHDSMedicationUse")
        .ok_or("the medication use group")?;
    let harness = annex_x().await?;
    let composition = composition_of(&harness, &group);
    mount_rows(&harness.cdr, &[entry_row(&composition, group.archetype)?]).await;
    mount_read(&harness.cdr, VERSION_ONE, &composition).await;
    let (status, body) = get(
        &harness,
        &format!("MedicationStatement?patient={SUBJECT}&_include=MedicationStatement:medication"),
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
        .ok_or("the search includes no Medication")?
        .to_owned();
    let (status, read) = get(&harness, &format!("Medication/{id}")).await?;
    ensure(status == StatusCode::OK, || {
        format!("the read answered {status}: {read}")
    })?;
    let profile = serde_json::json!([format!("{BASE}/medication-eu-core")]);
    ensure(read["meta"]["profile"] == profile, || {
        format!("the read answered the profile {}", read["meta"]["profile"])
    })
}

#[tokio::test]
async fn conformance_the_annex_x_groups_hold_their_pass_list() -> Result<(), Box<dyn Error>> {
    let served = groups();
    let mut cases = Vec::with_capacity(ANNEX_X_GROUPS.len());
    for name in ANNEX_X_GROUPS {
        let verdict = match served.iter().find(|group| group.name == name) {
            Some(group) => serve(group).await.map(drop),
            None if name == "EHDSMedication" => serve_medication().await,
            None => Err("the facade serves no resource of this group yet".into()),
        };
        cases.push(match verdict {
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
    // The body weight mapping writes the category and the LOINC code as
    // constants, so any other value matches no resource of the context.
    let harness = annex_x().await?;
    for search in [
        "category=laboratory",
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
