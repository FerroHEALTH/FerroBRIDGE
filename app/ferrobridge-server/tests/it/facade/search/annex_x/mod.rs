// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The draft `EEHRxF` Annex X data groups as individual facade resources (draft
//! act Art 7(2)), through the `eehrxf` Annex X project: each group's resource
//! answers the IHE `QEDm` 3.0.0 PCC-44 searches its option asks for and reads
//! back by the id the search answers, with the profile its context names.

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
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
use ferrobridge_testkit::fixtures::FERROEHR_PATIENT_PERSON_ADL2;
use ferrobridge_testkit::fixtures::FERROEHR_PATIENT_PERSON_ID;
use ferrobridge_testkit::fixtures::FERROEHR_PROFESSIONAL_PERSON_ADL2;
use ferrobridge_testkit::fixtures::FERROEHR_PROFESSIONAL_PERSON_ID;
use ferrobridge_testkit::fixtures::ORGANISATION_OPT;
use ferrobridge_testkit::fixtures::ORGANISATION_TEMPLATE_ID;
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
use super::super::handle;
use super::super::settings;
use super::super::stub::mount_read;
use super::SUBJECT;
use super::get;
use super::matches;
use super::mount_rows;

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

/// Returns the canonical JSON of the operational template of the ADL 2
/// demographic definition `source`, as the reference CDR serves it.
fn party_template(source: &str) -> Result<String, Box<dyn Error>> {
    let archetype =
        parse_artefact(source, Dialect::Adl2).map_err(|errors| format!("{errors:?}"))?;
    let raw = create_opt(&archetype, &ArchetypeRepository::default())?;
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
    for (id, opt) in [
        (
            "KDS_Laborbericht",
            ferrobridge_testkit::fixtures::KDS_LABORBERICHT_OPT,
        ),
        (
            "KDS_Laborauftrag",
            ferrobridge_testkit::fixtures::KDS_LABORAUFTRAG_OPT,
        ),
    ] {
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("/definition/template/adl1.4/{id}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Content-Type", "application/xml")
                    .set_body_string(opt),
            )
            .mount(&cdr)
            .await;
    }
    for (id, source) in [
        (FERROEHR_PATIENT_PERSON_ID, FERROEHR_PATIENT_PERSON_ADL2),
        (
            FERROEHR_PROFESSIONAL_PERSON_ID,
            FERROEHR_PROFESSIONAL_PERSON_ADL2,
        ),
    ] {
        mount_template(&cdr, id, party_template(source)?).await;
    }
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "/definition/template/adl1.4/{ORGANISATION_TEMPLATE_ID}"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/xml")
                .set_body_string(ORGANISATION_OPT),
        )
        .mount(&cdr)
        .await;
    let directory = Path::new(ANNEX_X);
    let set = programs::read_set(directory)?;
    let base = format!("{}/", cdr.uri()).parse()?;
    let client = CdrClient::new(&CdrConfig::new(base).with_demographic(true))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    let store = Arc::new(MemoryStore::default());
    programs::check_parties(&compiled, true)?;
    let built = Facade::new(compiled, handle(&store), client, settings());
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

mod endpoint;
mod groups;
mod party;
mod queries;

use endpoint::serve_endpoint;
use groups::groups;
use party::party_groups;
use party::patient;
use party::serve_party;

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

/// The data-type groups asserted in the served `Patient`, by name, with the
/// JSON pointer of the element.
const PATIENT_PARTS: [(&str, &str); 2] = [
    ("EHDSHumanName", "/name/0/family"),
    ("EHDSAddress", "/address/0/city"),
];

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
/// Returns the verdict on the Annex X group `name`.
async fn verdict(name: &str) -> Result<(), Box<dyn Error>> {
    if name == "EHDSEndpoint" {
        return serve_endpoint().await;
    }
    if let Some((_, party)) = party_groups().into_iter().find(|(held, _)| *held == name) {
        return serve_party(&party).await.map(drop);
    }
    if let Some((_, pointer)) = PATIENT_PARTS.iter().find(|(held, _)| *held == name) {
        let read = serve_party(&patient()).await?;
        return ensure(read.pointer(pointer).is_some(), || {
            format!("EHDSPatient answered no {pointer}: {read}")
        });
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
