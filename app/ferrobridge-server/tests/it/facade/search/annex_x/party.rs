// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Annex X groups served from a party context: the `PARTY` the CDR holds
//! and the resource it reads back as.

use std::error::Error;

use axum::body::Body;
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
pub(super) fn party_groups() -> [(&'static str, Party); 3] {
    [
        ("EHDSPatient", patient()),
        ("EHDSHealthProfessional", health_professional()),
        ("EHDSOrganisation", organisation()),
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
