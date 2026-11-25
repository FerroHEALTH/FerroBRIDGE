// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Annex X groups served from a party context: the `PARTY` the CDR holds
//! and the resource it reads back as.

use std::error::Error;

use axum::body::Body;
use ferrobridge_testkit::fixtures::FERROBRIDGE_RELATED_PERSON_ID;
use ferrobridge_testkit::fixtures::FERROEHR_PATIENT_PERSON_ID;
use ferrobridge_testkit::fixtures::FERROEHR_PROFESSIONAL_PERSON_ID;
use ferrobridge_testkit::fixtures::ORGANISATION_TEMPLATE_ID;
use http::Request;
use http::StatusCode;
use http::header;
use wiremock::Mock;
use wiremock::ResponseTemplate;
use wiremock::matchers;

use super::super::super::call;
use super::super::get;
use super::BASE;
use super::annex_x;
use super::ensure;

/// The `FerroTERM` code system of the Annex X identifier types HL7 v2-0203
/// lacks.
pub(super) const EHDS_IDENTIFIER_TYPE: &str =
    "https://ferroterm.eu/fhir/CodeSystem/ehds-identifier-type";

/// The object id of the synthetic patient `PERSON`.
pub(super) const PATIENT_UID: &str = "a1b2c3d4-0000-4000-8000-0000000000e1";

/// Returns the synthetic patient `PERSON` the CDR holds, its nodes named by
/// the reference CDR's patient definition.
pub(super) fn patient_person() -> serde_json::Value {
    let sex = serde_json::json!({
        "_type": "DV_CODED_TEXT",
        "value": "Female",
        "defining_code": {"_type": "CODE_PHRASE", "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "local"}, "code_string": "at11"}
    });
    party_of(
        "PERSON",
        "openEHR-DEMOGRAPHIC-PERSON.patient.v1",
        FERROEHR_PATIENT_PERSON_ID,
        PATIENT_UID,
        serde_json::json!({
            "details": tree("id2", "Details", serde_json::json!([
                element("id3", "Patient identifier", identifier(
                    "http://example.org/fhir/sid/synthetic-national-id",
                    "SYN-NI-0001",
                    "http://terminology.hl7.org/CodeSystem/v2-0203::NI"
                )),
                element("id3", "Patient identifier", identifier(
                    "http://example.org/fhir/sid/synthetic-health-id",
                    "SYN-HI-0001",
                    &format!("{EHDS_IDENTIFIER_TYPE}::HI")
                )),
                element("id4", "Date of birth", serde_json::json!({"_type": "DV_DATE", "value": "1970-01-01"})),
                element("id5", "Sex", sex)
            ])),
            "identities": [{
                "_type": "PARTY_IDENTITY",
                "name": {"_type": "DV_TEXT", "value": "Name"},
                "archetype_node_id": "id6",
                "details": tree("id7", "Name structure", serde_json::json!([
                    element("id8", "Full name", text("Synthetic Patient")),
                    element("id9", "Given name", text("Synthetic")),
                    element("id10", "Family name", text("Patient"))
                ]))
            }],
            "contacts": [{
                "_type": "CONTACT",
                "name": {"_type": "DV_TEXT", "value": "Contact"},
                "archetype_node_id": "id11",
                "addresses": [{
                    "_type": "ADDRESS",
                    "name": {"_type": "DV_TEXT", "value": "Postal address"},
                    "archetype_node_id": "id12",
                    "details": tree("id13", "Address details", serde_json::json!([
                        element("id14", "Address line", text("1 Synthetic Street")),
                        element("id15", "Postal code", text("0000 AA")),
                        element("id16", "Suburb/town/locality", text("Synthetic Town")),
                        element("id18", "Country", text("NL"))
                    ]))
                }, {
                    "_type": "ADDRESS",
                    "name": {
                        "_type": "DV_CODED_TEXT",
                        "value": "E-mail",
                        "defining_code": {"_type": "CODE_PHRASE", "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "local"}, "code_string": "at44"}
                    },
                    "archetype_node_id": "id19",
                    "details": tree("id20", "Components", serde_json::json!([
                        element("id22", "Details", text("patient@synthetic.example"))
                    ]))
                }]
            }]
        }),
    )
}

/// One Annex X group served from a party context: the `PARTY` the CDR holds
/// and the resource it reads back as.
pub(super) struct Party {
    /// The resource type.
    resource_type: &'static str,
    /// The Demographic API path segment of the party class.
    class: &'static str,
    /// The object id of the party.
    uid: &'static str,
    /// The party as the CDR holds it.
    held: serde_json::Value,
    /// The profile the context names.
    profile: String,
    /// A JSON pointer every read must answer.
    pointer: &'static str,
}

/// Returns a `PARTY` of `class` and `archetype` with the `template` and
/// object id `uid`, and the members `body` adds.
pub(super) fn party_of(
    class: &str,
    archetype: &str,
    template: &str,
    uid: &str,
    body: serde_json::Value,
) -> serde_json::Value {
    let mut party = serde_json::json!({
        "_type": class,
        "name": {"_type": "DV_TEXT", "value": "Synthetic party"},
        "archetype_node_id": archetype,
        "uid": {"_type": "OBJECT_VERSION_ID", "value": format!("{uid}::synthetic.example::1")},
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": {"_type": "ARCHETYPE_ID", "value": archetype},
            "template_id": {"_type": "TEMPLATE_ID", "value": template},
            "rm_version": "1.1.0"
        }
    });
    if let (Some(party), serde_json::Value::Object(members)) = (party.as_object_mut(), body) {
        party.extend(members);
    }
    party
}

/// Returns an `ELEMENT` named `name` at `id` holding `value`.
pub(super) fn element(id: &str, name: &str, value: serde_json::Value) -> serde_json::Value {
    let mut node = serde_json::json!({
        "_type": "ELEMENT",
        "name": {"_type": "DV_TEXT", "value": name},
        "archetype_node_id": id
    });
    node["value"] = value;
    node
}

/// Returns a `DV_TEXT` of `value`.
pub(super) fn text(value: &str) -> serde_json::Value {
    serde_json::json!({"_type": "DV_TEXT", "value": value})
}

/// Returns an `ITEM_TREE` named `name` at `id` holding `items`.
pub(super) fn tree(id: &str, name: &str, items: serde_json::Value) -> serde_json::Value {
    let mut node = serde_json::json!({
        "_type": "ITEM_TREE",
        "name": {"_type": "DV_TEXT", "value": name},
        "archetype_node_id": id
    });
    node["items"] = items;
    node
}

/// Returns a `DV_IDENTIFIER` of `id` issued under `issuer`, typed `kind`.
pub(super) fn identifier(issuer: &str, id: &str, kind: &str) -> serde_json::Value {
    serde_json::json!({
        "_type": "DV_IDENTIFIER",
        "issuer": issuer,
        "assigner": "Synthetic registry",
        "id": id,
        "type": kind
    })
}

/// Returns the `EHDSHealthProfessional` party.
pub(super) fn health_professional() -> Party {
    let uid = "a1b2c3d4-0000-4000-8000-0000000000e2";
    Party {
        resource_type: "Practitioner",
        class: "person",
        uid,
        held: party_of(
            "PERSON",
            "openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1",
            FERROEHR_PROFESSIONAL_PERSON_ID,
            uid,
            serde_json::json!({
                "details": tree("id2", "Person identifiers", serde_json::json!([
                    element("id3", "Identifier main data", identifier(
                        "http://example.org/fhir/sid/synthetic-professional-id",
                        "SYN-PRO-0002",
                        "http://terminology.hl7.org/CodeSystem/v2-0203::PRN"
                    ))
                ])),
                "identities": [{
                    "_type": "PARTY_IDENTITY",
                    "name": {"_type": "DV_TEXT", "value": "Person name"},
                    "archetype_node_id": "id4",
                    "details": tree("id5", "Name", serde_json::json!([
                        element("id6", "Full name", text("Synthetic Professional")),
                        element("id8", "Family name", text("Professional"))
                    ]))
                }]
            }),
        ),
        profile: format!("{BASE}/practitioner-eu-core"),
        pointer: "/name/0/family",
    }
}

/// Returns the `EHDSOrganisation` party.
pub(super) fn organisation() -> Party {
    let uid = "a1b2c3d4-0000-4000-8000-0000000000e3";
    Party {
        resource_type: "Organization",
        class: "organisation",
        uid,
        held: party_of(
            "ORGANISATION",
            "openEHR-DEMOGRAPHIC-ORGANISATION.ferrobridge_organisation.v1",
            ORGANISATION_TEMPLATE_ID,
            uid,
            serde_json::json!({
                "details": tree("at0001", "Details", serde_json::json!([
                    element("at0002", "Identifier", identifier(
                        "http://example.org/fhir/sid/synthetic-organisation-id",
                        "SYN-ORG-0001",
                        "http://terminology.hl7.org/CodeSystem/v2-0203::XX"
                    ))
                ])),
                "identities": [{
                    "_type": "PARTY_IDENTITY",
                    "name": {"_type": "DV_TEXT", "value": "Identity"},
                    "archetype_node_id": "at0003",
                    "details": tree("at0004", "Identity details", serde_json::json!([
                        element("at0005", "Name", text("Synthetic Hospital"))
                    ]))
                }]
            }),
        ),
        profile: format!("{BASE}/organization-eu-core"),
        pointer: "/name",
    }
}

/// Returns the `EHDSPatient` party.
pub(super) fn patient() -> Party {
    Party {
        resource_type: "Patient",
        class: "person",
        uid: PATIENT_UID,
        held: patient_person(),
        profile: format!("{BASE}/patient-eu-core"),
        pointer: "/identifier/0/type/coding/0/code",
    }
}

/// The object id of the synthetic related person `PERSON`.
const RELATED_UID: &str = "a1b2c3d4-0000-4000-8000-0000000000e4";

/// Returns the `PARTY_REF` of the party `uid` names, of `class`.
fn party_ref(uid: &str, class: &str) -> serde_json::Value {
    serde_json::json!({
        "_type": "PARTY_REF",
        "id": {"_type": "HIER_OBJECT_ID", "value": uid},
        "namespace": "demographic",
        "type": class
    })
}

/// Returns the coded kind of relationship `mother`, HL7 v3 `RoleCode` `MTH`.
fn mother() -> serde_json::Value {
    serde_json::json!({
        "_type": "DV_CODED_TEXT",
        "value": "mother",
        "defining_code": {
            "_type": "CODE_PHRASE",
            "terminology_id": {"_type": "TERMINOLOGY_ID", "value": "http://terminology.hl7.org/CodeSystem/v3-RoleCode"},
            "code_string": "MTH"
        }
    })
}

/// Returns the `EHDSRelatedPerson` party: a `PERSON` whose relationship
/// names the patient as its target and the person itself as its source.
pub(super) fn related_person() -> Party {
    Party {
        resource_type: "RelatedPerson",
        class: "person",
        uid: RELATED_UID,
        held: party_of(
            "PERSON",
            "openEHR-DEMOGRAPHIC-PERSON.ferrobridge_related_person.v1",
            FERROBRIDGE_RELATED_PERSON_ID,
            RELATED_UID,
            serde_json::json!({
                "identities": [{
                    "_type": "PARTY_IDENTITY",
                    "name": {"_type": "DV_TEXT", "value": "Name"},
                    "archetype_node_id": "id5",
                    "details": tree("id6", "Name structure", serde_json::json!([
                        element("id9", "Family name", text("Relative"))
                    ]))
                }],
                "relationships": [{
                    "_type": "PARTY_RELATIONSHIP",
                    "name": {"_type": "DV_TEXT", "value": "Relationship to the patient"},
                    "archetype_node_id": "id10",
                    "details": tree("id11", "Relationship details", serde_json::json!([
                        element("id12", "Kind of relationship", mother())
                    ])),
                    "source": party_ref(RELATED_UID, "PERSON"),
                    "target": party_ref(PATIENT_UID, "ACTOR")
                }]
            }),
        ),
        profile: String::from("http://hl7.org/fhir/StructureDefinition/RelatedPerson"),
        pointer: "/patient/reference",
    }
}

/// Holds the read of `party` to its profile and its pointer, and returns the
/// resource.
pub(super) async fn serve_party(party: &Party) -> Result<serde_json::Value, Box<dyn Error>> {
    let harness = annex_x().await?;
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!(
            "/demographic/{}/{}",
            party.class, party.uid
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{}::synthetic.example::1\"", party.uid))
                .set_body_json(&party.held),
        )
        .mount(&harness.cdr)
        .await;
    let (status, read) = get(&harness, &format!("{}/{}", party.resource_type, party.uid)).await?;
    ensure(status == StatusCode::OK, || {
        format!("the read answered {status}: {read}")
    })?;
    ensure(
        read["meta"]["profile"] == serde_json::json!([party.profile]),
        || format!("the read answered the profile {}", read["meta"]["profile"]),
    )?;
    ensure(read.pointer(party.pointer).is_some(), || {
        format!("the read answered no {}: {read}", party.pointer)
    })?;
    Ok(read)
}

/// Returns the groups served from a party context, by name.
pub(super) fn party_groups() -> [(&'static str, Party); 4] {
    [
        ("EHDSPatient", patient()),
        ("EHDSHealthProfessional", health_professional()),
        ("EHDSOrganisation", organisation()),
        ("EHDSRelatedPerson", related_person()),
    ]
}

#[tokio::test]
async fn each_patient_identifier_reads_with_its_annex_x_type() -> Result<(), Box<dyn Error>> {
    let read = serve_party(&patient()).await?;
    let types: Vec<(Option<&str>, Option<&str>)> = read["identifier"]
        .as_array()
        .ok_or("identifiers")?
        .iter()
        .map(|identifier| {
            let coding = &identifier["type"]["coding"][0];
            (coding["system"].as_str(), coding["code"].as_str())
        })
        .collect();
    assert_eq!(
        vec![
            (
                Some("http://terminology.hl7.org/CodeSystem/v2-0203"),
                Some("NI")
            ),
            (Some(EHDS_IDENTIFIER_TYPE), Some("HI")),
        ],
        types,
        "{read}"
    );
    Ok(())
}

#[tokio::test]
async fn a_patient_identifier_without_a_type_is_refused() -> Result<(), Box<dyn Error>> {
    // Draft Annex X EHDSPatient.identifier.type 1..1.
    let harness = annex_x().await?;
    let patient = serde_json::json!({
        "resourceType": "Patient",
        "meta": { "profile": [format!("{BASE}/patient-eu-core")] },
        "identifier": [
            {
                "type": { "coding": [{
                    "system": "http://terminology.hl7.org/CodeSystem/v2-0203", "code": "NI"
                }]},
                "system": "http://example.org/fhir/sid/synthetic-national-id",
                "value": "SYN-NI-0002"
            },
            { "system": "http://example.org/fhir/sid/synthetic-health-id", "value": "SYN-HI-0002" }
        ],
        "name": [{ "family": "Patient", "given": ["Synthetic"] }],
        "birthDate": "1970-01-01"
    });
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Patient")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(patient.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{body}");
    let issue = &body["issue"][0];
    assert_eq!(Some("required"), issue["code"].as_str(), "{body}");
    assert_eq!(
        Some("Patient.identifier[1].type"),
        issue["location"][0].as_str(),
        "{body}"
    );
    Ok(())
}

#[tokio::test]
async fn a_telecom_writes_its_system_as_the_coded_name_of_its_address() -> Result<(), Box<dyn Error>>
{
    // R4 cpt-2: a ContactPoint with a value carries its system, which the
    // patient PERSON records as the coded name of the electronic ADDRESS.
    let harness = annex_x().await?;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/demographic/person"))
        .respond_with(
            ResponseTemplate::new(204)
                .insert_header("ETag", format!("\"{PATIENT_UID}::synthetic.example::1\"")),
        )
        .mount(&harness.cdr)
        .await;
    let patient = serde_json::json!({
        "resourceType": "Patient",
        "meta": { "profile": [format!("{BASE}/patient-eu-core")] },
        "identifier": [{
            "type": { "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/v2-0203", "code": "NI"
            }]},
            "system": "http://example.org/fhir/sid/synthetic-national-id",
            "value": "SYN-NI-0003"
        }],
        "name": [{ "family": "Patient", "given": ["Synthetic"] }],
        "birthDate": "1970-01-01",
        "telecom": [{ "system": "email", "value": "patient@synthetic.example" }]
    });
    let (status, body) = call(
        harness.app(),
        Request::post("/fhir/Patient")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(patient.to_string()))?,
    )
    .await?;
    assert!(status.is_success(), "{status}: {body}");
    let requests = harness.cdr.received_requests().await.unwrap_or_default();
    let sent = requests
        .iter()
        .find(|request| {
            request.method.as_str() == "POST" && request.url.path() == "/demographic/person"
        })
        .ok_or("the CDR received the person")?;
    let sent: serde_json::Value = serde_json::from_slice(&sent.body)?;
    let address = sent["contacts"][0]["addresses"]
        .as_array()
        .and_then(|addresses| {
            addresses
                .iter()
                .find(|address| address["archetype_node_id"] == "id19")
        })
        .ok_or_else(|| format!("no electronic address in {sent}"))?;
    assert_eq!("DV_CODED_TEXT", address["name"]["_type"], "{sent}");
    assert_eq!(
        "at44", address["name"]["defining_code"]["code_string"],
        "{sent}"
    );
    assert_eq!(
        "patient@synthetic.example", address["details"]["items"][0]["value"]["value"],
        "{sent}"
    );
    Ok(())
}

#[tokio::test]
async fn a_related_person_reads_its_patient_and_relationship() -> Result<(), Box<dyn Error>> {
    let read = serve_party(&related_person()).await?;
    assert_eq!(
        Some(format!("Patient/{PATIENT_UID}").as_str()),
        read["patient"]["reference"].as_str(),
        "{read}"
    );
    let coding = &read["relationship"][0]["coding"][0];
    assert_eq!(
        (
            Some("http://terminology.hl7.org/CodeSystem/v3-RoleCode"),
            Some("MTH")
        ),
        (coding["system"].as_str(), coding["code"].as_str()),
        "{read}"
    );
    Ok(())
}

/// A synthetic `RelatedPerson`: the patient's mother.
fn related_resource() -> serde_json::Value {
    serde_json::json!({
        "resourceType": "RelatedPerson",
        "patient": { "reference": format!("Patient/{PATIENT_UID}") },
        "relationship": [{ "coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/v3-RoleCode", "code": "MTH"
        }]}],
        "name": [{ "family": "Relative" }]
    })
}

/// Mounts the CDR's create of a `PERSON`, answering its first version.
async fn mount_created(harness: &super::super::super::Harness) {
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/demographic/person"))
        .respond_with(
            ResponseTemplate::new(204)
                .insert_header("ETag", format!("\"{RELATED_UID}::synthetic.example::1\"")),
        )
        .mount(&harness.cdr)
        .await;
}

/// Returns the bodies the CDR received under `method`, in order.
async fn sent(
    harness: &super::super::super::Harness,
    method: &str,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let requests = harness.cdr.received_requests().await.unwrap_or_default();
    requests
        .iter()
        .filter(|request| request.method.as_str() == method)
        .map(|request| Ok(serde_json::from_slice(&request.body)?))
        .collect()
}

/// RM Demographic IM, the `PARTY` class, invariant `Relationships_validity`:
/// the source of a party's relationship is the party, whose uid the CDR
/// assigns when it creates it.
#[tokio::test]
async fn a_related_person_create_sources_its_relationship_in_a_second_version()
-> Result<(), Box<dyn Error>> {
    let harness = annex_x().await?;
    mount_created(&harness).await;
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(format!("/demographic/person/{RELATED_UID}")))
        .and(matchers::header(
            "If-Match",
            format!("\"{RELATED_UID}::synthetic.example::1\"").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(204)
                .insert_header("ETag", format!("\"{RELATED_UID}::synthetic.example::2\"")),
        )
        .mount(&harness.cdr)
        .await;
    let (status, body) = create_related(&harness).await?;
    assert_eq!(StatusCode::CREATED, status, "{body}");
    assert_eq!(Some("2"), body["meta"]["versionId"].as_str(), "{body}");
    assert_eq!(
        Some(format!("Patient/{PATIENT_UID}").as_str()),
        body["patient"]["reference"].as_str(),
        "{body}"
    );
    let created = sent(&harness, "POST").await?;
    let created = created.first().ok_or("the CDR received the person")?;
    assert_eq!(None, created.get("relationships"), "{created}");
    let updated = sent(&harness, "PUT").await?;
    let relationship = updated
        .first()
        .map(|person| &person["relationships"][0])
        .ok_or("the CDR received the second version")?;
    assert_eq!(party_ref(RELATED_UID, "PERSON"), relationship["source"]);
    assert_eq!(party_ref(PATIENT_UID, "ACTOR"), relationship["target"]);
    Ok(())
}

/// Mounts the CDR's refusal of the second version of the created `PERSON`.
async fn mount_refused_relationships(harness: &super::super::super::Harness) {
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(format!("/demographic/person/{RELATED_UID}")))
        .respond_with(
            ResponseTemplate::new(412)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{RELATED_UID}::synthetic.example::2\""))
                .set_body_json(serde_json::json!({"message": "synthetic refusal"})),
        )
        .mount(&harness.cdr)
        .await;
}

/// Mounts the CDR's answer `status` to the deletion of the created `PERSON`'s
/// first version.
async fn mount_withdrawal(harness: &super::super::super::Harness, status: u16) {
    Mock::given(matchers::method("DELETE"))
        .and(matchers::path(format!(
            "/demographic/person/{RELATED_UID}::synthetic.example::1"
        )))
        .respond_with(ResponseTemplate::new(status))
        .expect(1)
        .mount(&harness.cdr)
        .await;
}

/// Posts the synthetic `RelatedPerson` to the facade.
async fn create_related(
    harness: &super::super::super::Harness,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    call(
        harness.app(),
        Request::post("/fhir/RelatedPerson")
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(related_resource().to_string()))?,
    )
    .await
}

#[tokio::test]
async fn a_refused_second_version_deletes_the_person_and_answers_the_refusal()
-> Result<(), Box<dyn Error>> {
    let harness = annex_x().await?;
    mount_created(&harness).await;
    mount_refused_relationships(&harness).await;
    mount_withdrawal(&harness, 204).await;
    let (status, body) = create_related(&harness).await?;
    assert_eq!(StatusCode::PRECONDITION_FAILED, status, "{body}");
    harness.cdr.verify().await;
    Ok(())
}

#[tokio::test]
async fn a_refused_deletion_after_a_refused_second_version_names_both_refusals()
-> Result<(), Box<dyn Error>> {
    let harness = annex_x().await?;
    mount_created(&harness).await;
    mount_refused_relationships(&harness).await;
    mount_withdrawal(&harness, 409).await;
    let (status, body) = create_related(&harness).await?;
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, status, "{body}");
    let issues = body["issue"]
        .as_array()
        .ok_or_else(|| format!("no issues in {body}"))?;
    assert!(
        issues.len() >= 3,
        "the stranded person, the relationships' refusal and the deletion's refusal: {body}"
    );
    assert!(
        issues[0]["diagnostics"].as_str().is_some_and(|text| text
            .contains(&format!("{RELATED_UID}::synthetic.example::1"))
            && text.contains("refused its deletion")),
        "{body}"
    );
    harness.cdr.verify().await;
    Ok(())
}

#[tokio::test]
async fn a_related_person_update_sources_its_relationship_in_its_one_version()
-> Result<(), Box<dyn Error>> {
    let harness = annex_x().await?;
    let held = related_person();
    Mock::given(matchers::method("GET"))
        .and(matchers::path(format!("/demographic/person/{RELATED_UID}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "application/json")
                .insert_header("ETag", format!("\"{RELATED_UID}::synthetic.example::1\""))
                .set_body_json(&held.held),
        )
        .mount(&harness.cdr)
        .await;
    Mock::given(matchers::method("PUT"))
        .and(matchers::path(format!("/demographic/person/{RELATED_UID}")))
        .respond_with(
            ResponseTemplate::new(204)
                .insert_header("ETag", format!("\"{RELATED_UID}::synthetic.example::2\"")),
        )
        .mount(&harness.cdr)
        .await;
    let mut resource = related_resource();
    resource["id"] = serde_json::Value::from(RELATED_UID);
    let (status, body) = call(
        harness.app(),
        Request::put(format!("/fhir/RelatedPerson/{RELATED_UID}"))
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(resource.to_string()))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{body}");
    let updated = sent(&harness, "PUT").await?;
    assert_eq!(
        1,
        updated.len(),
        "the update wrote {} versions",
        updated.len()
    );
    let relationship = &updated[0]["relationships"][0];
    assert_eq!(party_ref(RELATED_UID, "PERSON"), relationship["source"]);
    Ok(())
}
