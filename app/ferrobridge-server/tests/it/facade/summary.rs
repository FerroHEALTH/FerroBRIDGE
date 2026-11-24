// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `Patient/[id]/$summary` over a `wiremock` CDR and terminology server: the
//! patient read through the reference CDR's patient `PERSON`, the record
//! read as the patient summary compositions of the patient's EHR, the
//! document written in the IPS 2.0.1 form or the HL7 Europe Patient Summary
//! form and validated against its `Bundle` profile before it is served
//! (HL7 IPS 2.0.1, `OperationDefinition/summary`).

use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;

use axum::body::Body;
use eehrxf::crosswalk::patient_summary::EPS_COMPOSITION;
use eehrxf::document::EPS_BUNDLE;
use eehrxf::document::IPS_BUNDLE;
use ferrobridge_server::cdr::CdrClient;
use ferrobridge_server::cdr::config::CdrConfig;
use ferrobridge_server::facade::Facade;
use ferrobridge_server::facade::Settings;
use ferrobridge_server::facade::identity::store::MemoryStore;
use ferrobridge_server::facade::programs;
use ferrobridge_server::facade::subject::IdForm;
use ferrobridge_server::facade::subject::SubjectLink;
use ferrobridge_server::facade::summary::Summary;
use ferrobridge_server::terminology::Translator;
use ferrobridge_testkit::fixtures::CKM_MEDICATION_ARCHETYPES;
use ferrobridge_testkit::fixtures::CKM_PATIENT_SUMMARY_ARCHETYPES;
use ferrobridge_testkit::fixtures::FERROBRIDGE_PATIENT_SUMMARY_COMPOSITION;
use ferrobridge_testkit::fixtures::FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE;
use ferrobridge_testkit::fixtures::FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE_ID;
use ferrobridge_testkit::fixtures::FERROEHR_PATIENT_PERSON_ADL2;
use ferrobridge_testkit::fixtures::FERROEHR_PATIENT_PERSON_ID;
use ferrobridge_testkit::fixtures::PATIENT_SUMMARY_PRODUCTS;
use ferrobridge_validator::set::PackageSet;
use http::Request;
use http::StatusCode;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::assemble::parse_artefact;
use openehr_adl::opt::create_opt;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::template::TemplateSource;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::EHR_ID;
use super::Harness;
use super::app_over;
use super::auth::Issuer;
use super::auth::authority;
use super::auth::bearing;
use super::auth::ec_key;
use super::auth::mint;
use super::call;
use super::ehr_body;
use super::handle;
use crate::support::translator;

/// The patient summary fixture mapping set: the `Patient` party context.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/it/fixtures_summary");

/// The object id of the synthetic patient.
pub(crate) const UID: &str = "a1b2c3d4-0000-4000-8000-000000002014";

/// The namespace the CDR links each EHR to its patient's party under.
pub(crate) const NAMESPACE: &str = "fhir_facade";

/// The ATC code the terminology server maps the synthetic medicine to.
const ATC_CODE: &str = "N02BE01";

/// The ATC system (eHN Guidelines on Patient Summary §4).
const ATC: &str = "http://www.whocc.no/atc";

/// The profile packages the documents are validated over.
fn packages() -> Result<Option<PackageSet>, Box<dyn Error>> {
    static FOUND: OnceLock<Option<Vec<PathBuf>>> = OnceLock::new();
    let found = FOUND.get_or_init(|| {
        let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut paths = vec![workspace.join("tools/fhir-codegen/vendor/hl7.fhir.r4.core")];
        let mut fetched: Vec<PathBuf> =
            std::fs::read_dir(workspace.join("docs/specs/fhir-packages"))
                .ok()?
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.join("PROVENANCE.md").is_file())
                .collect();
        fetched.sort();
        if fetched
            .iter()
            .any(|path| !path.join("package/package.json").is_file())
        {
            return None;
        }
        paths.extend(fetched);
        Some(paths)
    });
    match found {
        Some(paths) => Ok(Some(PackageSet::load(paths.clone())?)),
        None if std::env::var("CI").is_ok_and(|value| value == "true") => Err(
            "the pinned profile packages are not fetched; run scripts/vendor/profile-packages.sh"
                .into(),
        ),
        None => Ok(None),
    }
}

/// The ADL 2 operational template of the patient summary template.
fn summary_template() -> Result<TemplateSource, Box<dyn Error>> {
    let mut repository = ArchetypeRepository::default();
    for (file, source) in CKM_MEDICATION_ARCHETYPES
        .iter()
        .chain(CKM_PATIENT_SUMMARY_ARCHETYPES.iter())
    {
        let archetype = parse_artefact(source, Dialect::Adl14)
            .map_err(|errors| format!("{file}: {errors:?}"))?;
        repository.insert(archetype);
    }
    let template = parse_artefact(FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE, Dialect::Adl2)
        .map_err(|errors| format!("the template: {errors:?}"))?;
    let raw = create_opt(&template, &repository)
        .map_err(|errors| format!("the operational template: {errors:?}"))?;
    Ok(TemplateSource::Opt2 {
        template: Box::new(raw.template),
        prohibited_attributes: raw.prohibited_attributes,
        resolved_id: String::from(FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE_ID),
    })
}

/// The patient `PERSON` template as the reference CDR serves it, in AOM2
/// canonical JSON.
fn person_template() -> Result<String, Box<dyn Error>> {
    let archetype = parse_artefact(FERROEHR_PATIENT_PERSON_ADL2, Dialect::Adl2)
        .map_err(|errors| format!("{errors:?}"))?;
    let raw = create_opt(&archetype, &ArchetypeRepository::default())?;
    Ok(openehr_its::json::to_canonical_json(&raw.template))
}

/// Returns the facade settings of the suite: patients linked to their EHR by
/// their party id.
pub(crate) fn linked() -> Result<Settings, Box<dyn Error>> {
    Ok(Settings {
        subject: SubjectLink::new(NAMESPACE, IdForm::LogicalId, "PERSON")?,
        ..super::settings()
    })
}

/// Mounts the patient `PERSON` template on the `adl2` route alone.
async fn mount_person_template(cdr: &MockServer) -> Result<(), Box<dyn Error>> {
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "/definition/template/adl1.4/{FERROEHR_PATIENT_PERSON_ID}"
        )))
        .respond_with(ResponseTemplate::new(404))
        .mount(cdr)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "/definition/template/adl2/{FERROEHR_PATIENT_PERSON_ID}"
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{FERROEHR_PATIENT_PERSON_ID}\""))
                .set_body_string(person_template()?),
        )
        .mount(cdr)
        .await;
    Ok(())
}

/// Returns a facade over `client`, the stub CDR's, serving the patient
/// summary, its codes translated through `terminology`; `None` when the
/// profile packages are not fetched.
pub(crate) async fn summary_facade(
    cdr: &MockServer,
    client: CdrClient,
    terminology: Translator,
    store: &Arc<MemoryStore>,
) -> Result<Option<Arc<Facade>>, Box<dyn Error>> {
    let Some(packages) = packages()? else {
        return Ok(None);
    };
    mount_person_template(cdr).await?;
    let set = programs::read_set(std::path::Path::new(FIXTURES))?;
    let templates = programs::fetch_templates(&set, &client).await?;
    let compiled = programs::compile_set(&set, &templates)?;
    programs::check_parties(&compiled, true)?;
    let summary = Summary::new(
        &summary_template()?,
        packages,
        String::from("Synthetic operator"),
    )?;
    Ok(Some(Arc::new(
        Facade::new(compiled, handle(store), client, linked()?)
            .with_translator(terminology)
            .with_summary(Some(summary)),
    )))
}

/// Returns a harness over a stub CDR serving the patient summary, its codes
/// translated through `term`; `None` when the profile packages are not
/// fetched.
pub(super) async fn harness(term: &MockServer) -> Result<Option<Harness>, Box<dyn Error>> {
    let cdr = MockServer::start().await;
    let base = format!("{}/", cdr.uri()).parse()?;
    let client = CdrClient::new(&CdrConfig::new(base).with_demographic(true))?;
    let store = Arc::new(MemoryStore::default());
    let Some(facade) = summary_facade(&cdr, client, translator(term)?, &store).await? else {
        return Ok(None);
    };
    Ok(Some(Harness { cdr, store, facade }))
}

/// Returns the synthetic patient as the CDR holds it, its sex the local code
/// `sex`.
pub(crate) fn person(sex: &str) -> serde_json::Value {
    let text = |value: &str| serde_json::json!({"_type": "DV_TEXT", "value": value});
    let element = |node: &str, name: &str, value: serde_json::Value| serde_json::json!({"_type": "ELEMENT", "name": text(name), "archetype_node_id": node, "value": value});
    serde_json::json!({
        "_type": "PERSON",
        "name": text("Patient"),
        "archetype_node_id": "openEHR-DEMOGRAPHIC-PERSON.patient.v1",
        "uid": {"_type": "OBJECT_VERSION_ID", "value": format!("{UID}::synthetic.example::1")},
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": {"_type": "ARCHETYPE_ID", "value": "openEHR-DEMOGRAPHIC-PERSON.patient.v1"},
            "template_id": {"_type": "TEMPLATE_ID", "value": FERROEHR_PATIENT_PERSON_ID},
            "rm_version": "1.1.0"
        },
        "details": {
            "_type": "ITEM_TREE",
            "name": text("Details"),
            "archetype_node_id": "id2",
            "items": [
                element("id3", "Patient identifier", serde_json::json!({"_type": "DV_IDENTIFIER", "issuer": "http://example.org/fhir/sid/patients", "assigner": "Synthetic assigner", "id": "SYN-PAT-2014", "type": "patient"})),
                element("id4", "Date of birth", serde_json::json!({"_type": "DV_DATE", "value": "1970-01-01"})),
                element("id5", "Sex", serde_json::json!({"_type": "DV_CODED_TEXT", "value": "Female", "defining_code": {"_type": "CODE_PHRASE", "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "local"}, "code_string": sex}}))
            ]
        },
        "identities": [{
            "_type": "PARTY_IDENTITY",
            "name": text("Name"),
            "archetype_node_id": "id6",
            "details": {
                "_type": "ITEM_TREE",
                "name": text("Name structure"),
                "archetype_node_id": "id7",
                "items": [
                    element("id8", "Full name", text("Synthetic Subject")),
                    element("id9", "Given name", text("Synthetic")),
                    element("id10", "Family name", text("Subject"))
                ]
            }
        }]
    })
}

/// Mounts the synthetic patient, its EHR and `compositions` as the patient
/// summary compositions the EHR holds.
pub(crate) async fn mount_record(cdr: &MockServer, compositions: &[serde_json::Value]) {
    mount_patient(cdr, &person("at11")).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .and(matchers::query_param("subject_id", UID))
        .and(matchers::query_param("subject_namespace", NAMESPACE))
        .respond_with(ResponseTemplate::new(200).set_body_string(ehr_body()))
        .mount(cdr)
        .await;
    let rows: Vec<serde_json::Value> = compositions
        .iter()
        .map(|composition| serde_json::json!([composition]))
        .collect();
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/query/aql"))
        .and(matchers::body_string_contains(EHR_ID))
        .and(matchers::body_string_contains(
            FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE_ID,
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "q": "synthetic",
            "columns": [{ "name": "c", "path": "c" }],
            "rows": rows
        })))
        .mount(cdr)
        .await;
}

/// Mounts `person` as the synthetic patient's `PERSON`.
async fn mount_patient(cdr: &MockServer, person: &serde_json::Value) {
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("/demographic/person/{UID}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{UID}::synthetic.example::1\""))
                .set_body_json(person),
        )
        .mount(cdr)
        .await;
}

/// Mounts the terminology server: the synthetic medicine to [`ATC_CODE`],
/// and no match for any other question.
pub(crate) async fn mount_terminology(term: &MockServer) {
    let matched = |system: &str, code: &str| {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Parameters",
            "parameter": [
                {"name": "result", "valueBoolean": true},
                {"name": "match", "part": [
                    {"name": "equivalence", "valueCode": "equivalent"},
                    {"name": "concept", "valueCoding": {"system": system, "code": code}}
                ]}
            ]
        }))
    };
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .and(matchers::body_string_contains("SYN-MEDICINE-0001"))
        .and(matchers::body_string_contains(ATC))
        .respond_with(matched(ATC, ATC_CODE))
        .with_priority(1)
        .mount(term)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/r4/ConceptMap/$translate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "resourceType": "Parameters",
            "parameter": [{"name": "result", "valueBoolean": false}]
        })))
        .mount(term)
        .await;
}

/// Writes `body`, a document the bridge served, as `name` under the
/// directory `EEHRXF_EMIT_DIR` names, for a run of HL7's FHIR Validator.
pub(super) fn emit(name: &str, body: &serde_json::Value) -> Result<(), Box<dyn Error>> {
    let Some(dir) = std::env::var_os("EEHRXF_EMIT_DIR") else {
        return Ok(());
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join(format!("{name}.json")),
        serde_json::to_string(body)?,
    )?;
    Ok(())
}

/// The synthetic patient summary composition.
pub(crate) fn composition() -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(serde_json::from_str(
        FERROBRIDGE_PATIENT_SUMMARY_COMPOSITION,
    )?)
}

/// `GET` of the summary of `path`.
fn get(path: &str) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::get(path).body(Body::empty())?)
}

/// The entries of `bundle` whose resource is of `kind`.
fn of_kind<'a>(bundle: &'a serde_json::Value, kind: &str) -> Vec<&'a serde_json::Value> {
    bundle["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["resource"]["resourceType"] == kind)
        .map(|entry| &entry["resource"])
        .collect()
}

/// The section of `bundle`'s composition coded `code`.
fn section<'a>(bundle: &'a serde_json::Value, code: &str) -> Option<&'a serde_json::Value> {
    bundle["entry"][0]["resource"]["section"]
        .as_array()?
        .iter()
        .find(|section| section["code"]["coding"][0]["code"] == code)
}

#[tokio::test]
async fn a_patient_s_summary_is_an_ips_document_with_coded_entries() -> Result<(), Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(());
    };
    mount_terminology(&term).await;
    mount_record(&harness.cdr, &[composition()?]).await;
    let (status, body) = call(
        app_over(&harness.facade),
        get(&format!("/fhir/Patient/{UID}/$summary"))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(body["type"], "document");
    assert_eq!(body["meta"]["profile"][0], IPS_BUNDLE);
    let composition = &body["entry"][0]["resource"];
    assert_eq!(composition["resourceType"], "Composition");
    assert_eq!(composition["type"]["coding"][0]["code"], "60591-5");
    let patients = of_kind(&body, "Patient");
    let [patient] = patients.as_slice() else {
        return Err(format!("{} patients", patients.len()).into());
    };
    assert_eq!(patient["id"], UID);
    assert_eq!(patient["identifier"][0]["value"], "SYN-PAT-2014");
    assert_eq!(
        patient["identifier"][0]["assigner"]["display"],
        "Synthetic assigner"
    );
    assert_eq!(
        patient["identifier"][0]["type"],
        serde_json::json!({"text": "patient"}),
        "{patient}"
    );
    assert_eq!(patient["name"][0]["family"], "Subject", "{patient}");
    assert_eq!(patient["birthDate"], "1970-01-01");
    assert_eq!(patient["gender"], "female");
    let statements = of_kind(&body, "MedicationStatement");
    let [statement] = statements.as_slice() else {
        return Err(format!("{} statements", statements.len()).into());
    };
    let codings = statement["medicationCodeableConcept"]["coding"]
        .as_array()
        .ok_or("the medicine's codings")?;
    assert!(
        codings
            .iter()
            .any(|coding| coding["system"] == PATIENT_SUMMARY_PRODUCTS),
        "the recorded code stays: {codings:?}"
    );
    assert!(
        codings
            .iter()
            .any(|coding| coding["system"] == ATC && coding["code"] == ATC_CODE),
        "FerroTERM's ATC translation is added: {codings:?}"
    );
    assert_eq!(1, of_kind(&body, "Immunization").len());
    emit("server-summary-ips", &body)?;
    let medications = section(&body, "10160-0").ok_or("the medication summary")?;
    assert_eq!(1, medications["entry"].as_array().map_or(0, Vec::len));
    for (code, kind, snomed) in [
        ("11450-4", "Condition", "38341003"),
        ("48765-2", "AllergyIntolerance", "764146007"),
        ("47519-4", "Procedure", "80146002"),
    ] {
        let held = section(&body, code).ok_or(code)?;
        assert_eq!(1, held["entry"].as_array().map_or(0, Vec::len), "{held}");
        let entries = of_kind(&body, kind);
        let [entry] = entries.as_slice() else {
            return Err(format!("{} {kind} entries", entries.len()).into());
        };
        assert_eq!(
            entry["code"]["coding"][0]["system"],
            "http://snomed.info/sct"
        );
        assert_eq!(entry["code"]["coding"][0]["code"], snomed, "{entry}");
    }
    Ok(())
}

#[tokio::test]
async fn the_eps_profile_answers_the_hl7_europe_form() -> Result<(), Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(());
    };
    mount_terminology(&term).await;
    mount_record(&harness.cdr, &[composition()?]).await;
    let (status, body) = call(
        app_over(&harness.facade),
        get(&format!(
            "/fhir/Patient/{UID}/$summary?profile={EPS_COMPOSITION}"
        ))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    assert_eq!(body["meta"]["profile"][0], EPS_BUNDLE);
    assert_eq!(
        body["entry"][0]["resource"]["meta"]["profile"][0],
        EPS_COMPOSITION
    );
    emit("server-summary-eps", &body)?;
    for code in ["47519-4", "46264-8"] {
        assert!(
            section(&body, code).is_some(),
            "the EPS form requires {code}"
        );
    }
    let devices = section(&body, "46264-8").ok_or("the medical devices")?;
    assert_eq!(
        1,
        devices["entry"].as_array().map_or(0, Vec::len),
        "{devices}"
    );
    let statements = of_kind(&body, "DeviceUseStatement");
    let [statement] = statements.as_slice() else {
        return Err(format!("{} device use statements", statements.len()).into());
    };
    assert_eq!(statement["status"], "active", "{statement}");
    let devices = of_kind(&body, "Device");
    let device = devices
        .iter()
        .find(|device| device["type"]["coding"][0]["code"] == "14106009")
        .ok_or("the pacemaker Device")?;
    assert_eq!(
        device["type"]["coding"][0]["system"], "http://snomed.info/sct",
        "{device}"
    );
    Ok(())
}

#[tokio::test]
async fn an_empty_record_answers_the_eps_medical_devices_nilknown() -> Result<(), Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(());
    };
    mount_terminology(&term).await;
    mount_record(&harness.cdr, &[]).await;
    let (status, body) = call(
        app_over(&harness.facade),
        get(&format!(
            "/fhir/Patient/{UID}/$summary?profile={EPS_COMPOSITION}"
        ))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let devices = section(&body, "46264-8").ok_or("the medical devices")?;
    assert_eq!(
        devices["emptyReason"]["coding"][0]["code"], "nilknown",
        "the medical devices are fed by the project and empty in the record: {devices}"
    );
    assert_eq!(0, of_kind(&body, "DeviceUseStatement").len());
    Ok(())
}

#[tokio::test]
async fn an_empty_record_answers_every_required_section_with_its_empty_reason()
-> Result<(), Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(());
    };
    mount_terminology(&term).await;
    mount_record(&harness.cdr, &[]).await;
    let (status, body) = call(
        app_over(&harness.facade),
        get(&format!("/fhir/Patient/{UID}/$summary"))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let medications = section(&body, "10160-0").ok_or("the medication summary")?;
    assert_eq!(medications["emptyReason"]["coding"][0]["code"], "nilknown");
    assert!(medications.get("entry").is_none(), "{medications}");
    for code in ["11450-4", "48765-2"] {
        let empty = section(&body, code).ok_or(code)?;
        assert_eq!(
            empty["emptyReason"]["coding"][0]["code"], "nilknown",
            "{code} is fed by the project and empty in the record: {empty}"
        );
    }
    assert_eq!(0, of_kind(&body, "MedicationStatement").len());
    emit("server-summary-ips-empty", &body)?;
    Ok(())
}

#[tokio::test]
async fn a_patient_with_no_ehr_has_an_empty_record() -> Result<(), Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(());
    };
    mount_terminology(&term).await;
    mount_patient(&harness.cdr, &person("at11")).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&harness.cdr)
        .await;
    let (status, body) = call(
        app_over(&harness.facade),
        get(&format!("/fhir/Patient/{UID}/$summary"))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let medications = section(&body, "10160-0").ok_or("the medication summary")?;
    assert_eq!(medications["emptyReason"]["coding"][0]["code"], "nilknown");
    Ok(())
}

#[tokio::test]
async fn a_document_that_breaks_its_profile_is_refused_with_422() -> Result<(), Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(());
    };
    mount_terminology(&term).await;
    mount_record(&harness.cdr, &[]).await;
    let mut undated = person("at11");
    if let Some(items) = undated["details"]["items"].as_array_mut() {
        items.retain(|item| item["archetype_node_id"] != "id4");
    }
    harness.cdr.reset().await;
    mount_person_template(&harness.cdr).await?;
    mount_patient(&harness.cdr, &undated).await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/ehr"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&harness.cdr)
        .await;
    let (status, body) = call(
        app_over(&harness.facade),
        get(&format!("/fhir/Patient/{UID}/$summary"))?,
    )
    .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert!(
        body.to_string().contains("birthDate"),
        "the issue names the element Patient-uv-ips requires: {body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_profile_this_server_does_not_write_is_refused() -> Result<(), Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(());
    };
    for query in [
        "profile=http://example.org/fhir/StructureDefinition/other",
        "profile=http://hl7.org/fhir/uv/ips/StructureDefinition/Composition-uv-ips|1.1.0",
        "identifier=http://example.org/fhir/sid/patients|SYN-PAT-2014",
    ] {
        let (status, body) = call(
            app_over(&harness.facade),
            get(&format!("/fhir/Patient/{UID}/$summary?{query}"))?,
        )
        .await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {body}");
        assert_eq!(body["issue"][0]["code"], "not-supported", "{query}");
    }
    let (status, _) = call(
        app_over(&harness.facade),
        get("/fhir/Condition/c-1/$summary")?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    Ok(())
}

#[tokio::test]
async fn the_statement_declares_the_operation_on_patient() -> Result<(), Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(());
    };
    let (status, body) = call(app_over(&harness.facade), get("/fhir/metadata")?).await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let patient = body["rest"][0]["resource"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|resource| resource["type"] == "Patient")
        .ok_or("the Patient entry")?;
    assert!(
        patient["operation"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|operation| {
                operation["name"] == "summary"
                    && operation["definition"]
                        == "http://hl7.org/fhir/uv/ips/OperationDefinition/summary"
            }),
        "{patient}"
    );
    Ok(())
}

/// The router over `harness` behind `issuer`, recording each access in `log`
/// when one is given.
async fn guarded(
    harness: &Harness,
    issuer: &Issuer,
    log: Option<Arc<ferrobridge_server::audit::AccessLog>>,
) -> axum::Router {
    let state = ferrobridge_server::state::AppState::with_health(
        ferrohealth_health::registry::Registry::default(),
    )
    .with_facade(Arc::clone(&harness.facade))
    .with_auth(authority(issuer.settings()).await);
    let state = match log {
        Some(log) => state.with_access_log(log),
        None => state,
    };
    ferrobridge_server::router(Arc::new(state), &super::server_settings())
}

/// The claims of a patient's token launched for `patient`.
pub(super) fn patient_claims(issuer: &Issuer, patient: &str) -> serde_json::Value {
    let mut claims = issuer.claims("patient/*.rs");
    claims["patient"] = serde_json::json!(patient);
    claims
}

#[tokio::test]
async fn a_patient_token_for_another_patient_answers_404() -> Result<(), Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(());
    };
    mount_record(&harness.cdr, &[composition()?]).await;
    let key = ec_key("k1", jsonwebtoken::Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let token = mint(
        &key,
        &patient_claims(&issuer, "a1b2c3d4-0000-4000-8000-0000000020ff"),
    );
    let (status, body) = call(
        guarded(&harness, &issuer, None).await,
        bearing(get(&format!("/fhir/Patient/{UID}/$summary"))?, &token),
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{body}");
    assert!(
        !harness
            .cdr
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .any(|request| request.url.path().starts_with("/demographic/")),
        "the CDR is never asked for another patient"
    );
    Ok(())
}

#[tokio::test]
async fn a_patient_s_own_summary_writes_one_access_record_naming_the_person()
-> Result<(), Box<dyn Error>> {
    let term = MockServer::start().await;
    let Some(harness) = harness(&term).await? else {
        return Ok(());
    };
    mount_terminology(&term).await;
    mount_record(&harness.cdr, &[composition()?]).await;
    let arr = crate::audit::repository().await;
    let categories = ehds_logging::map::CategoryMap::declare_with(
        ehds_logging::map::Matching::OpenEhr,
        &[],
        &std::collections::BTreeMap::from([(
            String::from(FERROBRIDGE_PATIENT_SUMMARY_TEMPLATE_ID),
            ehds_logging::map::Declared::Codes(vec![String::from("Patient-Summaries")]),
        )]),
        &std::collections::BTreeMap::new(),
    )?;
    let spool = tempfile::tempdir()?;
    let log = crate::audit::access_log(
        &arr,
        spool.path(),
        harness.facade.client().base_url(),
        categories,
        64 << 20,
    )?;
    let key = ec_key("k1", jsonwebtoken::Algorithm::ES256);
    let issuer = Issuer::start(&[&key]).await;
    let token = mint(&key, &patient_claims(&issuer, UID));
    let (status, body) = call(
        guarded(&harness, &issuer, Some(log)).await,
        bearing(get(&format!("/fhir/Patient/{UID}/$summary"))?, &token),
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let records = crate::audit::records(&arr, 1).await?;
    let [record] = records.as_slice() else {
        return Err(format!("{} records", records.len()).into());
    };
    let text = record.to_string();
    assert_eq!(Some("R"), record["action"].as_str(), "{record}");
    for named in ["synthetic-client-sub", UID, EHR_ID, "Patient-Summaries"] {
        assert!(text.contains(named), "the record names {named}: {record}");
    }
    Ok(())
}
